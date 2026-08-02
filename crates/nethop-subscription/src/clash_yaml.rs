use std::collections::BTreeMap;

use serde::Deserialize;
use serde_saphyr::{DuplicateKeyPolicy, Error as YamlError, MergeKeyPolicy, Options};

use crate::{
    adapter::{AdapterNodeResult, AdapterOutput},
    capability::CapabilityMatrix,
    diagnostics::{DiagnosticCode, NodeDiagnostic, Severity, SourceLocation},
    limits::ParserLimits,
    normalize::normalize_bytes,
    payload::{FormatHint, SourceId},
    protocol::SourceRef,
    semantic::{NodeSpec, semantic_diagnostic, validate_node_spec},
};

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
    let documents: Vec<ClashDocument> =
        serde_saphyr::from_multiple_with_options(payload.as_str(), yaml_options(limits)).map_err(
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
    obfs: Option<String>,
    #[serde(rename = "grpc-opts")]
    grpc_options: Option<GrpcOptions>,
    #[serde(rename = "ws-opts")]
    ws_options: Option<WsOptions>,
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

fn clash_node_spec(
    node: ClashNode,
    source_id: Option<&SourceId>,
    item_index: u32,
    location: Option<SourceLocation>,
) -> Result<NodeSpec, DiagnosticCode> {
    let protocol = node.protocol.ok_or(DiagnosticCode::MissingRequiredField)?;
    let server = node.server.ok_or(DiagnosticCode::MissingRequiredField)?;
    let port = node.port.ok_or(DiagnosticCode::MissingRequiredField)?;
    let mut spec = NodeSpec::minimal(protocol, server, port);
    spec.display_name = node.name;
    spec.uuid = node.uuid;
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
    spec.plugin = node.plugin;
    spec.obfs = node.obfs;
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
    if let Some(field) = node
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

fn is_critical_unknown_field(field: &str) -> bool {
    matches!(
        field,
        "xhttp-opts" | "reality-opts" | "plugin-opts" | "port-range" | "tfo" | "smux"
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
