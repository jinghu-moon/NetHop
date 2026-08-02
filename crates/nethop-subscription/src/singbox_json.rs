use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::value::RawValue;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SingboxJsonError {
    pub code: DiagnosticCode,
}

pub fn parse_singbox_json(
    bytes: &[u8],
    source_id: Option<&SourceId>,
    limits: &ParserLimits,
    matrix: &CapabilityMatrix,
) -> Result<AdapterOutput, SingboxJsonError> {
    let payload =
        normalize_bytes(bytes, limits).map_err(|error| SingboxJsonError { code: error.code() })?;
    check_json_structure(payload.as_bytes(), limits)?;
    let (outbounds, boundary_count) = extract_outbounds(payload.as_str())?;
    if outbounds.len() > limits.max_nodes() {
        return Err(SingboxJsonError {
            code: DiagnosticCode::InputTooLarge,
        });
    }
    let mut output = AdapterOutput::default();
    let mut skipped = 0usize;
    for (item_index, raw) in outbounds.into_iter().enumerate() {
        let item_index = u32::try_from(item_index).unwrap_or(u32::MAX);
        let location = SourceLocation::new(
            item_index,
            None,
            None,
            Some(format!("outbounds[{item_index}]")),
        )
        .ok();
        let node = match serde_json::from_str::<SingboxOutbound>(raw.get()) {
            Ok(node) => node,
            Err(error) => {
                output.nodes.push(AdapterNodeResult::rejected(
                    item_index,
                    diagnostic(json_node_error_code(&error), location),
                ));
                continue;
            }
        };
        let Some(protocol) = node.protocol.as_deref() else {
            output.nodes.push(AdapterNodeResult::rejected(
                item_index,
                diagnostic(DiagnosticCode::MissingRequiredField, location),
            ));
            continue;
        };
        if is_non_terminal(protocol) {
            skipped = skipped.saturating_add(1);
            continue;
        }
        match singbox_node_spec(node, source_id, item_index, location.clone()) {
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
    if boundary_count > 0 || skipped > 0 {
        output.diagnostics.push(
            source_diagnostic(DiagnosticCode::NonNodeSectionIgnored)
                .with_parameter("count", boundary_count.saturating_add(skipped).to_string()),
        );
    }
    Ok(output)
}

fn extract_outbounds(input: &str) -> Result<(Vec<Box<RawValue>>, usize), SingboxJsonError> {
    let first = input.as_bytes().first().copied().ok_or(SingboxJsonError {
        code: DiagnosticCode::InvalidJson,
    })?;
    match first {
        b'[' => serde_json::from_str::<Vec<Box<RawValue>>>(input)
            .map(|outbounds| (outbounds, 0))
            .map_err(|_| SingboxJsonError {
                code: DiagnosticCode::InvalidJson,
            }),
        b'{' => {
            let probe: SingboxRootProbe =
                serde_json::from_str(input).map_err(|_| SingboxJsonError {
                    code: DiagnosticCode::InvalidJson,
                })?;
            let boundary_count = probe.boundary_count();
            if let Some(outbounds) = probe.outbounds {
                Ok((outbounds, boundary_count))
            } else if probe.protocol.is_some() {
                let raw =
                    RawValue::from_string(input.to_owned()).map_err(|_| SingboxJsonError {
                        code: DiagnosticCode::InvalidJson,
                    })?;
                Ok((vec![raw], 0))
            } else {
                Err(SingboxJsonError {
                    code: DiagnosticCode::InvalidJson,
                })
            }
        }
        _ => Err(SingboxJsonError {
            code: DiagnosticCode::InvalidJson,
        }),
    }
}

#[derive(Deserialize)]
struct SingboxRootProbe {
    #[serde(default)]
    outbounds: Option<Vec<Box<RawValue>>>,
    #[serde(rename = "type")]
    protocol: Option<serde::de::IgnoredAny>,
    log: Option<serde::de::IgnoredAny>,
    dns: Option<serde::de::IgnoredAny>,
    inbounds: Option<serde::de::IgnoredAny>,
    route: Option<serde::de::IgnoredAny>,
    services: Option<serde::de::IgnoredAny>,
    experimental: Option<serde::de::IgnoredAny>,
}

impl SingboxRootProbe {
    fn boundary_count(&self) -> usize {
        [
            self.log.is_some(),
            self.dns.is_some(),
            self.inbounds.is_some(),
            self.route.is_some(),
            self.services.is_some(),
            self.experimental.is_some(),
        ]
        .into_iter()
        .filter(|present| *present)
        .count()
    }
}

#[derive(Deserialize)]
struct SingboxOutbound {
    #[serde(rename = "type")]
    protocol: Option<String>,
    tag: Option<String>,
    server: Option<String>,
    server_port: Option<u16>,
    uuid: Option<String>,
    password: Option<String>,
    method: Option<String>,
    security: Option<String>,
    alter_id: Option<u16>,
    flow: Option<String>,
    plugin: Option<String>,
    plugin_opts: Option<String>,
    congestion_control: Option<String>,
    tls: Option<SingboxTls>,
    transport: Option<SingboxTransport>,
    obfs: Option<SingboxObfs>,
    #[serde(default)]
    udp: bool,
    #[serde(flatten)]
    unknown: BTreeMap<String, serde::de::IgnoredAny>,
}

#[derive(Deserialize)]
struct SingboxTls {
    #[serde(default)]
    enabled: bool,
    server_name: Option<String>,
    #[serde(default)]
    insecure: bool,
    #[serde(default)]
    alpn: Vec<String>,
    utls: Option<SingboxUtls>,
    reality: Option<SingboxReality>,
    #[serde(flatten)]
    unknown: BTreeMap<String, serde::de::IgnoredAny>,
}

#[derive(Deserialize)]
struct SingboxUtls {
    fingerprint: Option<String>,
}

#[derive(Deserialize)]
struct SingboxReality {
    public_key: Option<String>,
    short_id: Option<String>,
}

#[derive(Deserialize)]
struct SingboxTransport {
    #[serde(rename = "type")]
    kind: Option<String>,
    path: Option<String>,
    service_name: Option<String>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    #[serde(flatten)]
    unknown: BTreeMap<String, serde::de::IgnoredAny>,
}

#[derive(Deserialize)]
struct SingboxObfs {
    #[serde(rename = "type")]
    kind: Option<String>,
    password: Option<String>,
}

fn singbox_node_spec(
    node: SingboxOutbound,
    source_id: Option<&SourceId>,
    item_index: u32,
    location: Option<SourceLocation>,
) -> Result<NodeSpec, DiagnosticCode> {
    let protocol = node.protocol.ok_or(DiagnosticCode::MissingRequiredField)?;
    if protocol.parse::<ProxyProtocol>().is_err() {
        return Err(DiagnosticCode::UnsupportedProtocol);
    }
    let server = node.server.ok_or(DiagnosticCode::MissingRequiredField)?;
    let port = node
        .server_port
        .ok_or(DiagnosticCode::MissingRequiredField)?;
    let mut spec = NodeSpec::minimal(protocol, server, port);
    spec.display_name = node.tag;
    spec.uuid = node.uuid;
    spec.password = node.password;
    spec.method = node.method;
    spec.vmess_security = node.security;
    spec.alter_id = node.alter_id;
    spec.flow = node.flow;
    spec.plugin = node.plugin;
    if let Some(plugin_options) = node.plugin_opts {
        match parse_plugin_options(&plugin_options) {
            Some(options) => spec.plugin_options = options,
            None => spec.unknown_critical_field = Some("plugin_opts".into()),
        }
    }
    spec.congestion_control = node.congestion_control;
    spec.udp = node.udp;
    if let Some(obfs) = node.obfs {
        spec.obfs = obfs.kind;
        spec.obfs_password = obfs.password;
    }
    if let Some(tls) = node.tls {
        spec.tls = tls.enabled;
        spec.insecure = tls.insecure;
        spec.server_name = tls.server_name;
        spec.alpn = tls.alpn;
        if let Some(utls) = tls.utls {
            spec.client_fingerprint = utls.fingerprint;
        }
        if let Some(reality) = tls.reality {
            spec.reality_public_key = reality.public_key;
            spec.reality_short_id = reality.short_id;
        }
        if let Some(field) = tls
            .unknown
            .keys()
            .find(|field| is_critical_tls_field(field))
        {
            spec.unknown_critical_field = Some(format!("tls.{field}"));
        }
        spec.unknown_harmless_fields = spec.unknown_harmless_fields.saturating_add(
            tls.unknown
                .keys()
                .filter(|field| !is_critical_tls_field(field))
                .count(),
        );
    }
    if let Some(transport) = node.transport {
        spec.transport = transport.kind;
        spec.path = transport.path;
        spec.service_name = transport.service_name;
        spec.headers = transport.headers;
        if let Some(field) = transport
            .unknown
            .keys()
            .find(|field| is_critical_transport_field(field))
        {
            spec.unknown_critical_field = Some(format!("transport.{field}"));
        }
        spec.unknown_harmless_fields = spec.unknown_harmless_fields.saturating_add(
            transport
                .unknown
                .keys()
                .filter(|field| !is_critical_transport_field(field))
                .count(),
        );
    }
    if let Some(field) = node
        .unknown
        .keys()
        .find(|field| is_critical_node_field(field))
    {
        spec.unknown_critical_field = Some(field.to_owned());
    }
    spec.unknown_harmless_fields = spec.unknown_harmless_fields.saturating_add(
        node.unknown
            .keys()
            .filter(|field| !is_critical_node_field(field))
            .count(),
    );
    spec.source_ref = source_id.cloned().map(|source_id| SourceRef {
        source_id,
        item_index,
        format: FormatHint::SingboxJson,
        line: None,
    });
    spec.location = location;
    Ok(spec)
}

fn is_non_terminal(protocol: &str) -> bool {
    matches!(
        protocol,
        "selector" | "urltest" | "direct" | "block" | "dns" | "shadowtls"
    )
}

fn parse_plugin_options(input: &str) -> Option<BTreeMap<String, String>> {
    if input.is_empty() {
        return Some(BTreeMap::new());
    }
    let mut options = BTreeMap::new();
    for part in input.split(';') {
        let (key, value) = part.split_once('=')?;
        if key.is_empty()
            || key.len() > 64
            || value.is_empty()
            || value.len() > 256
            || options.insert(key.to_owned(), value.to_owned()).is_some()
        {
            return None;
        }
    }
    Some(options)
}

fn is_critical_node_field(field: &str) -> bool {
    matches!(
        field,
        "detour"
            | "bind_interface"
            | "routing_mark"
            | "domain_resolver"
            | "multiplex"
            | "udp_over_tcp"
            | "certificate"
            | "certificate_path"
            | "xudp"
    )
}

fn is_critical_tls_field(field: &str) -> bool {
    matches!(
        field,
        "certificate" | "certificate_path" | "client_certificate" | "ech" | "acme"
    )
}

fn is_critical_transport_field(field: &str) -> bool {
    matches!(
        field,
        "host" | "method" | "max_early_data" | "early_data_header_name"
    )
}

fn json_node_error_code(error: &serde_json::Error) -> DiagnosticCode {
    let message = error.to_string();
    if message.contains("duplicate field") {
        if ["uuid", "password", "method", "security"]
            .iter()
            .any(|field| message.contains(field))
        {
            DiagnosticCode::DuplicateCredentialKey
        } else {
            DiagnosticCode::DuplicateKey
        }
    } else {
        DiagnosticCode::InvalidJson
    }
}

fn check_json_structure(bytes: &[u8], limits: &ParserLimits) -> Result<(), SingboxJsonError> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut string_bytes = 0usize;
    for byte in bytes.iter().copied() {
        if in_string {
            if escaped {
                escaped = false;
                string_bytes = string_bytes.saturating_add(1);
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            } else {
                string_bytes = string_bytes.saturating_add(1);
            }
            if string_bytes > limits.max_string_bytes() {
                return Err(SingboxJsonError {
                    code: DiagnosticCode::InputTooLarge,
                });
            }
            continue;
        }
        match byte {
            b'"' => {
                in_string = true;
                string_bytes = 0;
            }
            b'{' | b'[' => {
                depth = depth.saturating_add(1);
                if depth > limits.max_depth() {
                    return Err(SingboxJsonError {
                        code: DiagnosticCode::InputTooLarge,
                    });
                }
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    Ok(())
}

fn source_diagnostic(code: DiagnosticCode) -> NodeDiagnostic {
    NodeDiagnostic::new(code, Severity::Warning)
}

fn diagnostic(code: DiagnosticCode, location: Option<SourceLocation>) -> NodeDiagnostic {
    let mut diagnostic = NodeDiagnostic::new(code, Severity::Error);
    diagnostic.location = location;
    diagnostic
}
