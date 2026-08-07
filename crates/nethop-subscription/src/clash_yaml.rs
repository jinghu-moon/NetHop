use std::{collections::BTreeMap, thread};

use serde::Deserialize;
use serde_saphyr::{DuplicateKeyPolicy, Error as YamlError, MergeKeyPolicy, Options};

use crate::{
    adapter::{AdapterNodeResult, AdapterOutput},
    capability::CapabilityMatrix,
    diagnostics::{DiagnosticCode, NodeDiagnostic, Severity, SourceLocation},
    limits::ParserLimits,
    normalize::normalize_bytes,
    payload::{FormatHint, SourceId},
    protocol::{ProxyProtocol, SourceRef},
    semantic::{NodeSpec, semantic_diagnostic, validate_node_spec},
};

const YAML_PARSE_STACK_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClashYamlError {
    pub code: DiagnosticCode,
}

pub fn yaml_options(limits: &ParserLimits) -> Options {
    serde_saphyr::options! {
        budget: serde_saphyr::budget! {
            max_reader_input_bytes: Some(limits.max_body_bytes()),
            max_events: 200_000,
            max_aliases: 1_024,
            max_anchors: 1_024,
            max_depth: limits.max_depth(),
            max_inclusion_depth: 0,
            max_documents: 1,
            max_nodes: 200_000,
            max_total_scalar_bytes: limits.max_body_bytes(),
            max_total_comment_bytes: 1024 * 1024,
            max_merge_keys: 0,
            enforce_alias_anchor_ratio: true,
            alias_anchor_min_aliases: 64,
            alias_anchor_ratio_multiplier: 64,
        },
        alias_limits: serde_saphyr::alias_limits! {
            max_total_replayed_events: 200_000,
            max_replay_stack_depth: 32,
            max_alias_expansions_per_anchor: 1_024,
        },
        duplicate_keys: DuplicateKeyPolicy::Error,
        merge_keys: MergeKeyPolicy::Error,
        legacy_octal_numbers: false,
        strict_booleans: true,
        reject_non_finite_typeless_float: true,
        with_snippet: false,
        crop_radius: 0,
    }
}

pub fn parse_clash_yaml(
    bytes: &[u8],
    source_id: Option<&SourceId>,
    limits: &ParserLimits,
    matrix: &CapabilityMatrix,
) -> Result<AdapterOutput, ClashYamlError> {
    let payload =
        normalize_bytes(bytes, limits).map_err(|error| ClashYamlError { code: error.code() })?;
    if yaml_contains_unsupported_tag(payload.as_str()) {
        return Err(ClashYamlError {
            code: DiagnosticCode::InvalidYaml,
        });
    }
    if yaml_depth_exceeds_budget(payload.as_str(), limits.max_depth()) {
        return Err(ClashYamlError {
            code: DiagnosticCode::YamlNodeLimitExceeded,
        });
    }
    thread::scope(|scope| {
        let worker = thread::Builder::new()
            .name("nethop-yaml-parser".to_owned())
            .stack_size(YAML_PARSE_STACK_BYTES)
            .spawn_scoped(scope, || {
                parse_clash_payload(payload.as_str(), source_id, limits, matrix)
            })
            .map_err(|_| ClashYamlError {
                code: DiagnosticCode::InvalidYaml,
            })?;
        worker.join().unwrap_or_else(|_| {
            Err(ClashYamlError {
                code: DiagnosticCode::InvalidYaml,
            })
        })
    })
}

fn parse_clash_payload(
    payload: &str,
    source_id: Option<&SourceId>,
    limits: &ParserLimits,
    matrix: &CapabilityMatrix,
) -> Result<AdapterOutput, ClashYamlError> {
    let documents: Vec<ClashDocument> =
        serde_saphyr::from_multiple_with_options(payload, yaml_options(limits)).map_err(
            |error| ClashYamlError {
                code: yaml_error_code(&error),
            },
        )?;
    if documents.len() != 1 {
        return Err(ClashYamlError {
            code: DiagnosticCode::InvalidYaml,
        });
    }
    let document = documents.into_iter().next().expect("one document checked");
    if document.proxies.len() > limits.max_nodes() {
        return Err(ClashYamlError {
            code: DiagnosticCode::YamlNodeLimitExceeded,
        });
    }
    let mut output = AdapterOutput::default();
    if document.proxies.is_empty() && document.proxy_providers.is_some() {
        output
            .diagnostics
            .push(source_diagnostic(DiagnosticCode::ClashInlineProxiesMissing));
        output.diagnostics.push(source_diagnostic(
            DiagnosticCode::ClashProxyProvidersNotImported,
        ));
    }
    for (item_index, node) in document.proxies.into_iter().enumerate() {
        let item_index = u32::try_from(item_index).unwrap_or(u32::MAX);
        let location = SourceLocation::new(item_index, None, None, Some("proxies".into())).ok();
        match clash_node_spec(node, source_id, item_index, location.clone()) {
            Ok(spec) => match validate_node_spec(spec, matrix) {
                Ok(outcome) => output.nodes.push(AdapterNodeResult::accepted(
                    item_index,
                    outcome.node,
                    outcome.warnings,
                )),
                Err(error) => output.nodes.push(AdapterNodeResult::rejected(
                    item_index,
                    semantic_diagnostic(error, location),
                )),
            },
            Err(code) => output.nodes.push(AdapterNodeResult::rejected(
                item_index,
                diagnostic(code, location),
            )),
        }
    }
    for ignored in [
        document.proxy_groups.is_some(),
        document.rules.is_some(),
        document.rule_providers.is_some(),
        document.script.is_some(),
    ] {
        if ignored {
            output
                .diagnostics
                .push(source_diagnostic(DiagnosticCode::NonNodeSectionIgnored));
        }
    }
    if document.proxy_providers.is_some() && !output.nodes.is_empty() {
        output.diagnostics.push(source_diagnostic(
            DiagnosticCode::ClashProxyProvidersNotImported,
        ));
    }
    Ok(output)
}

/// Reject pathological nesting before the YAML library can recurse through a
/// hostile scalar/container stream. This is deliberately conservative: it
/// only counts structural indentation and flow delimiters, never rewrites YAML.
fn yaml_depth_exceeds_budget(text: &str, max_depth: usize) -> bool {
    let mut flow_depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for line in text.lines() {
        let content = line.trim_end();
        if content.is_empty() || content.trim_start().starts_with('#') {
            continue;
        }
        let indent = content.len() - content.trim_start().len();
        let mut sequence_offset = 0usize;
        let mut sequence_text = content.trim_start();
        while sequence_text.starts_with('-')
            && sequence_text
                .as_bytes()
                .get(1)
                .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            sequence_offset = sequence_offset.saturating_add(1);
            sequence_text = sequence_text[1..].trim_start();
        }
        let block_depth = indent.saturating_add(sequence_offset);
        if block_depth > max_depth.saturating_mul(2) {
            return true;
        }
        for character in content.chars() {
            if let Some(delimiter) = quote {
                if escaped {
                    escaped = false;
                } else if character == '\\' && delimiter == '"' {
                    escaped = true;
                } else if character == delimiter {
                    quote = None;
                }
                continue;
            }
            if matches!(character, '\'' | '"') {
                quote = Some(character);
            } else if matches!(character, '[' | '{') {
                flow_depth = flow_depth.saturating_add(1);
            } else if matches!(character, ']' | '}') {
                flow_depth = flow_depth.saturating_sub(1);
            }
            if block_depth.saturating_add(flow_depth) > max_depth {
                return true;
            }
        }
    }
    false
}

fn yaml_contains_unsupported_tag(text: &str) -> bool {
    let mut quote = None;
    let mut escaped = false;
    let mut line_start = true;
    let mut previous: Option<char> = None;
    let mut comment = false;
    for character in text.chars() {
        if character == '\n' {
            line_start = true;
            previous = None;
            comment = false;
            continue;
        }
        if comment {
            continue;
        }
        if let Some(delimiter) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' && delimiter == '"' {
                escaped = true;
            } else if character == delimiter {
                quote = None;
            }
        } else if matches!(character, '\'' | '"') {
            quote = Some(character);
        } else if character == '#'
            && (line_start || previous.is_some_and(|value| value.is_ascii_whitespace()))
        {
            comment = true;
        } else if character == '!'
            && (line_start
                || previous.is_none_or(|value| value.is_ascii_whitespace())
                || previous.is_some_and(|value| matches!(value, ':' | ',' | '[' | '{')))
        {
            return true;
        }
        if !character.is_ascii_whitespace() {
            line_start = false;
        }
        previous = Some(character);
    }
    false
}

#[derive(Debug, Deserialize)]
struct ClashDocument {
    #[serde(default)]
    proxies: Vec<ClashNode>,
    #[serde(rename = "proxy-providers", default)]
    proxy_providers: Option<serde::de::IgnoredAny>,
    #[serde(rename = "proxy-groups", default)]
    proxy_groups: Option<serde::de::IgnoredAny>,
    #[serde(default)]
    rules: Option<serde::de::IgnoredAny>,
    #[serde(rename = "rule-providers", default)]
    rule_providers: Option<serde::de::IgnoredAny>,
    #[serde(default)]
    script: Option<serde::de::IgnoredAny>,
}

#[derive(Debug, Deserialize)]
struct ClashNode {
    name: Option<String>,
    #[serde(rename = "type")]
    protocol: Option<String>,
    server: Option<String>,
    port: Option<u16>,
    uuid: Option<String>,
    username: Option<String>,
    password: Option<String>,
    cipher: Option<String>,
    #[serde(default)]
    tls: bool,
    #[serde(alias = "sni")]
    servername: Option<String>,
    #[serde(rename = "skip-cert-verify", default)]
    insecure: bool,
    network: Option<String>,
    #[serde(default)]
    udp: bool,
    flow: Option<String>,
    #[serde(rename = "alterId", alias = "alter-id")]
    alter_id: Option<u16>,
    security: Option<String>,
    plugin: Option<String>,
    #[serde(rename = "plugin-opts")]
    plugin_options: Option<ClashPluginOptions>,
    #[serde(rename = "udp-over-tcp")]
    udp_over_tcp: Option<bool>,
    ports: Option<String>,
    #[serde(rename = "hop-interval")]
    hop_interval: Option<ClashScalar>,
    up: Option<ClashScalar>,
    down: Option<ClashScalar>,
    #[serde(rename = "congestion-controller")]
    congestion_controller: Option<String>,
    #[serde(rename = "udp-relay-mode")]
    udp_relay_mode: Option<String>,
    #[serde(rename = "udp-over-stream")]
    udp_over_stream: Option<bool>,
    #[serde(rename = "zero-rtt")]
    zero_rtt: Option<bool>,
    #[serde(rename = "heartbeat-interval")]
    heartbeat_interval: Option<ClashScalar>,
    #[serde(rename = "idle-session-check-interval")]
    idle_session_check_interval: Option<String>,
    #[serde(rename = "idle-session-timeout")]
    idle_session_timeout: Option<String>,
    #[serde(rename = "min-idle-session")]
    min_idle_session: Option<u32>,
    fingerprint: Option<String>,
    obfs: Option<String>,
    #[serde(rename = "obfs-password")]
    obfs_password: Option<String>,
    #[serde(rename = "grpc-opts")]
    grpc_options: Option<GrpcOptions>,
    #[serde(rename = "ws-opts")]
    ws_options: Option<WsOptions>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    #[serde(flatten)]
    unknown: BTreeMap<String, serde::de::IgnoredAny>,
}

#[derive(Debug, Deserialize)]
struct GrpcOptions {
    #[serde(rename = "grpc-service-name", alias = "serviceName")]
    service_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WsOptions {
    path: Option<String>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct ClashPluginOptions {
    mode: Option<String>,
    host: Option<String>,
    path: Option<String>,
    tls: Option<bool>,
    mux: Option<u16>,
    #[serde(flatten)]
    unknown: BTreeMap<String, serde::de::IgnoredAny>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ClashScalar {
    Text(String),
    Unsigned(u64),
    Signed(i64),
    Float(f64),
    Boolean(bool),
}

fn clash_node_spec(
    node: ClashNode,
    source_id: Option<&SourceId>,
    item_index: u32,
    location: Option<SourceLocation>,
) -> Result<NodeSpec, DiagnosticCode> {
    let protocol = node.protocol.ok_or(DiagnosticCode::MissingRequiredField)?;
    let protocol_kind = protocol.parse::<ProxyProtocol>().ok();
    let server = node.server.ok_or(DiagnosticCode::MissingRequiredField)?;
    let port = node.port.ok_or(DiagnosticCode::MissingRequiredField)?;
    let mut spec = NodeSpec::minimal(protocol, server, port);
    spec.display_name = node.name;
    spec.uuid = node.uuid;
    spec.username = node.username;
    spec.password = node.password;
    spec.method = node.cipher;
    spec.tls = node.tls;
    spec.insecure = node.insecure;
    spec.server_name = node.servername;
    spec.transport = node.network;
    spec.udp = node.udp;
    spec.flow = node.flow;
    spec.alter_id = node.alter_id;
    spec.vmess_security = node.security;
    spec.plugin = node.plugin.map(|plugin| match plugin.as_str() {
        "obfs" => "obfs-local".to_owned(),
        _ => plugin,
    });
    spec.udp_over_tcp = node.udp_over_tcp.map(|enabled| crate::UdpOverTcpOptions {
        enabled,
        version: 0,
    });
    if let Some(plugin_options) = node.plugin_options {
        if let Some(mode) = plugin_options.mode {
            let key = if spec.plugin.as_deref() == Some("obfs-local") {
                "obfs"
            } else {
                "mode"
            };
            spec.plugin_options.insert(key.into(), mode);
        }
        if let Some(host) = plugin_options.host {
            let key = if spec.plugin.as_deref() == Some("obfs-local") {
                "obfs-host"
            } else {
                "host"
            };
            spec.plugin_options.insert(key.into(), host);
        }
        if let Some(path) = plugin_options.path {
            spec.plugin_options.insert("path".into(), path);
        }
        if let Some(tls) = plugin_options.tls {
            spec.plugin_options.insert("tls".into(), tls.to_string());
        }
        if let Some(mux) = plugin_options.mux {
            spec.plugin_options.insert("mux".into(), mux.to_string());
        }
        if let Some(field) = plugin_options.unknown.keys().next() {
            spec.unknown_critical_field = Some(format!("plugin-opts.{field}"));
        }
    }
    spec.obfs = node.obfs;
    spec.obfs_password = node.obfs_password;
    spec.server_ports = node
        .ports
        .map(|ports| {
            ports
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    spec.hop_interval = node
        .hop_interval
        .as_ref()
        .map(normalize_hop_interval)
        .transpose()?;
    spec.up_mbps = node
        .up
        .as_ref()
        .map(ClashScalar::as_text)
        .transpose()?
        .as_deref()
        .map(parse_bandwidth_mbps)
        .transpose()?;
    spec.down_mbps = node
        .down
        .as_ref()
        .map(ClashScalar::as_text)
        .transpose()?
        .as_deref()
        .map(parse_bandwidth_mbps)
        .transpose()?;
    spec.congestion_control = node.congestion_controller;
    spec.udp_relay_mode = node.udp_relay_mode;
    spec.udp_over_stream = node.udp_over_stream.unwrap_or(false);
    spec.zero_rtt_handshake = node.zero_rtt.unwrap_or(false);
    spec.heartbeat = node
        .heartbeat_interval
        .as_ref()
        .map(normalize_heartbeat_interval)
        .transpose()?;
    spec.idle_session_check_interval = node.idle_session_check_interval;
    spec.idle_session_timeout = node.idle_session_timeout;
    spec.min_idle_session = node.min_idle_session;
    if protocol_kind == Some(ProxyProtocol::Http) {
        spec.http_headers = node.headers;
    } else if !node.headers.is_empty() {
        spec.unknown_critical_field = Some("headers".into());
    }
    if protocol_kind == Some(ProxyProtocol::Socks) {
        spec.socks_version = Some("5".into());
    }
    if node.fingerprint.is_some() {
        // Mihomo fingerprint pins the certificate DER hash, while sing-box
        // 1.13.15 accepts the public-key hash in a different encoding.
        spec.unknown_critical_field = Some("fingerprint".into());
    }
    if let Some(grpc) = node.grpc_options {
        spec.service_name = grpc.service_name;
    }
    if let Some(ws) = node.ws_options {
        spec.path = ws.path;
        spec.headers = ws.headers;
    }
    spec.source_ref = source_id.cloned().map(|source_id| SourceRef {
        source_id,
        item_index,
        format: FormatHint::ClashYaml,
        line: None,
    });
    spec.location = location;
    if spec.unknown_critical_field.is_none()
        && let Some(field) = node
            .unknown
            .keys()
            .find(|field| is_critical_unknown_field(field))
    {
        spec.unknown_critical_field = Some(field.to_owned());
    }
    spec.unknown_harmless_fields = node
        .unknown
        .keys()
        .filter(|field| !is_critical_unknown_field(field))
        .count();
    Ok(spec)
}

fn parse_bandwidth_mbps(value: &str) -> Result<u32, DiagnosticCode> {
    let normalized = value.trim().to_ascii_lowercase();
    let number = normalized
        .strip_suffix("mbps")
        .unwrap_or(&normalized)
        .trim();
    if number.is_empty() {
        return Err(DiagnosticCode::UnsupportedSemantics);
    }
    number
        .parse::<u32>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or(DiagnosticCode::UnsupportedSemantics)
}

impl ClashScalar {
    fn as_text(&self) -> Result<String, DiagnosticCode> {
        match self {
            Self::Text(value) => Ok(value.clone()),
            Self::Unsigned(value) => Ok(value.to_string()),
            Self::Signed(value) if *value >= 0 => Ok(value.to_string()),
            Self::Float(value) if value.is_finite() && *value >= 0.0 => Ok(value.to_string()),
            Self::Signed(_) | Self::Float(_) => Err(DiagnosticCode::UnsupportedSemantics),
            Self::Boolean(value) => {
                let _ = value;
                Err(DiagnosticCode::UnsupportedSemantics)
            }
        }
    }
}

fn normalize_hop_interval(value: &ClashScalar) -> Result<String, DiagnosticCode> {
    let text = value.as_text()?.trim().to_owned();
    if text.is_empty() || text.contains('-') {
        return Err(DiagnosticCode::UnsupportedSemantics);
    }
    if text.chars().all(|character| character.is_ascii_digit()) {
        return Ok(format!("{text}s"));
    }
    Ok(text)
}

fn normalize_heartbeat_interval(value: &ClashScalar) -> Result<String, DiagnosticCode> {
    let text = value.as_text()?.trim().to_owned();
    if text.is_empty() {
        return Err(DiagnosticCode::UnsupportedSemantics);
    }
    if text.chars().all(|character| character.is_ascii_digit()) {
        return Ok(format!("{text}ms"));
    }
    Ok(text)
}

fn is_critical_unknown_field(field: &str) -> bool {
    matches!(
        field,
        "xhttp-opts"
            | "reality-opts"
            | "port-range"
            | "tfo"
            | "smux"
            | "certificate"
            | "private-key"
            | "name-cert-verify"
            | "dialer-proxy"
            | "interface-name"
            | "routing-mark"
    )
}

fn yaml_error_code(error: &YamlError) -> DiagnosticCode {
    match error {
        YamlError::MergeKeyNotAllowed { .. } => DiagnosticCode::YamlMergeKeyUnsupported,
        YamlError::DuplicateMappingKey { .. } => DiagnosticCode::DuplicateKey,
        YamlError::AliasReplayLimitExceeded { .. }
        | YamlError::AliasExpansionLimitExceeded { .. }
        | YamlError::AliasReplayStackDepthExceeded { .. }
        | YamlError::AliasReplayCounterOverflow { .. }
        | YamlError::AliasError { .. } => DiagnosticCode::YamlAliasLimitExceeded,
        YamlError::Budget { breach, .. } => match breach {
            serde_saphyr::budget::BudgetBreach::Aliases { .. }
            | serde_saphyr::budget::BudgetBreach::Anchors { .. }
            | serde_saphyr::budget::BudgetBreach::AliasAnchorRatio { .. } => {
                DiagnosticCode::YamlAliasLimitExceeded
            }
            _ => DiagnosticCode::YamlNodeLimitExceeded,
        },
        _ => DiagnosticCode::InvalidYaml,
    }
}

fn source_diagnostic(code: DiagnosticCode) -> NodeDiagnostic {
    NodeDiagnostic::new(code, Severity::Warning)
}

fn diagnostic(code: DiagnosticCode, location: Option<SourceLocation>) -> NodeDiagnostic {
    let mut diagnostic = NodeDiagnostic::new(code, Severity::Error);
    diagnostic.location = location;
    diagnostic
}
