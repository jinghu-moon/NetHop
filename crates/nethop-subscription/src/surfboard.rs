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
pub struct SurfboardIniError {
    pub code: DiagnosticCode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProxyLine<'a> {
    line: u32,
    name: &'a str,
    fields: Vec<String>,
}

pub fn parse_surfboard_ini(
    bytes: &[u8],
    source_id: Option<&SourceId>,
    limits: &ParserLimits,
    matrix: &CapabilityMatrix,
) -> Result<AdapterOutput, SurfboardIniError> {
    let payload =
        normalize_bytes(bytes, limits).map_err(|error| SurfboardIniError { code: error.code() })?;
    let mut section = String::new();
    let mut output = AdapterOutput::default();
    let mut item_index = 0_u32;

    for line in payload.lines() {
        if line.text().len() > limits.max_line_bytes() {
            return Err(SurfboardIniError {
                code: DiagnosticCode::InvalidIni,
            });
        }
        let trimmed = line.text().trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            section.clear();
            section.push_str(trimmed[1..trimmed.len() - 1].trim());
            continue;
        }
        let Some((name, value)) = trimmed.split_once('=') else {
            if section.eq_ignore_ascii_case("proxy") {
                output.nodes.push(AdapterNodeResult::rejected(
                    item_index,
                    diagnostic(
                        DiagnosticCode::InvalidIni,
                        source_id,
                        item_index,
                        line.number(),
                    ),
                ));
                item_index = item_index.saturating_add(1);
            }
            continue;
        };
        if !section.eq_ignore_ascii_case("proxy") {
            output.diagnostics.push(NodeDiagnostic::new(
                DiagnosticCode::NonNodeSectionIgnored,
                Severity::Warning,
            ));
            continue;
        }
        let fields = split_fields(value).map_err(|_| SurfboardIniError {
            code: DiagnosticCode::InvalidIni,
        })?;
        if fields.len() == 1 && fields[0].eq_ignore_ascii_case("direct") {
            output.diagnostics.push(NodeDiagnostic::new(
                DiagnosticCode::NonNodeSectionIgnored,
                Severity::Warning,
            ));
            continue;
        }
        if item_index as usize >= limits.max_nodes() {
            return Err(SurfboardIniError {
                code: DiagnosticCode::NodeLimitExceeded,
            });
        }
        if fields.len() < 3 {
            output.nodes.push(AdapterNodeResult::rejected(
                item_index,
                diagnostic(
                    DiagnosticCode::MissingRequiredField,
                    source_id,
                    item_index,
                    line.number(),
                ),
            ));
        } else {
            let proxy_line = ProxyLine {
                line: line.number(),
                name: name.trim(),
                fields,
            };
            let location = SourceLocation::new(
                item_index,
                Some(proxy_line.line),
                Some(1),
                Some("[Proxy]".into()),
            )
            .ok();
            match surfboard_node_spec(proxy_line, source_id, item_index, location.clone()) {
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
                    diagnostic(code, source_id, item_index, line.number()),
                )),
            }
        }
        item_index = item_index.saturating_add(1);
    }
    Ok(output)
}

fn surfboard_node_spec(
    line: ProxyLine<'_>,
    source_id: Option<&SourceId>,
    item_index: u32,
    location: Option<SourceLocation>,
) -> Result<NodeSpec, DiagnosticCode> {
    let protocol = line.fields[0].trim().to_ascii_lowercase();
    if matches!(
        protocol.as_str(),
        "http" | "https" | "socks" | "socks5" | "socks5-tls"
    ) {
        return Err(DiagnosticCode::UnsupportedSemantics);
    }
    let server = line.fields[1].trim();
    let port = line.fields[2]
        .trim()
        .parse::<u16>()
        .map_err(|_| DiagnosticCode::InvalidEndpoint)?;
    let mut spec = NodeSpec::minimal(protocol.clone(), server, port);
    spec.display_name = Some(line.name.trim().to_owned());
    spec.location = location.clone();
    spec.source_ref = source_id.cloned().map(|source_id| SourceRef {
        source_id,
        item_index,
        format: FormatHint::SurfboardIni,
        line: Some(line.line),
    });
    let mut positional = Vec::new();
    for field in line.fields.iter().skip(3) {
        if let Some((key, value)) = field.split_once('=') {
            let key = key.trim().to_ascii_lowercase().replace('-', "_");
            let value = unquote(value.trim())?;
            match key.as_str() {
                "username" | "uuid" => spec.uuid = Some(value),
                "password" | "pass" => spec.password = Some(value),
                "cipher" | "method" | "encrypt_method" => spec.method = Some(value),
                "tls" => {
                    spec.tls = parse_bool(&value).ok_or(DiagnosticCode::UnsupportedSemantics)?
                }
                "sni" | "servername" | "server_name" => spec.server_name = Some(value),
                "network" | "type" | "transport" => spec.transport = Some(value),
                "ws" if parse_bool(&value) == Some(true) => spec.transport = Some("ws".into()),
                "ws_path" | "path" => spec.path = Some(value),
                "grpc_service_name" | "service_name" => spec.service_name = Some(value),
                "udp" => {
                    spec.udp = parse_bool(&value).ok_or(DiagnosticCode::UnsupportedSemantics)?
                }
                "alpn" => spec.alpn = value.split(',').map(str::to_owned).collect(),
                "obfs" if matches!(protocol.as_str(), "ss" | "shadowsocks") => {
                    spec.plugin = Some("obfs-local".into());
                    spec.plugin_options.insert("obfs".into(), value);
                }
                "obfs_host" if matches!(protocol.as_str(), "ss" | "shadowsocks") => {
                    spec.plugin = Some("obfs-local".into());
                    spec.plugin_options.insert("obfs-host".into(), value);
                }
                "obfs" => spec.obfs = Some(value),
                "obfs-password" | "obfs_password" => spec.obfs_password = Some(value),
                "plugin" | "plugin_opts" | "reality_opts" | "obfs_host" => {
                    spec.unknown_critical_field = Some(key)
                }
                _ => spec.unknown_harmless_fields = spec.unknown_harmless_fields.saturating_add(1),
            }
        } else {
            positional.push(unquote(field.trim())?);
        }
    }
    match protocol.as_str() {
        "ss" | "shadowsocks" => {
            if spec.method.is_none() {
                spec.method = positional.first().cloned();
            }
            if spec.password.is_none() {
                spec.password = positional.get(1).cloned();
            }
        }
        "tuic" => {
            if spec.uuid.is_none() {
                spec.uuid = positional.first().cloned();
            }
            if spec.password.is_none() {
                spec.password = positional.get(1).cloned();
            }
        }
        _ => {
            if spec.uuid.is_none() && matches!(protocol.as_str(), "vless" | "vmess") {
                spec.uuid = positional.first().cloned();
            }
            if spec.password.is_none()
                && matches!(protocol.as_str(), "trojan" | "hysteria2" | "hy2" | "anytls")
            {
                spec.password = positional.first().cloned();
            }
        }
    }
    Ok(spec)
}

fn split_fields(value: &str) -> Result<Vec<String>, ()> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;
    for byte in value.bytes() {
        if escaped {
            current.push(byte as char);
            escaped = false;
        } else if byte == b'\\' && quote.is_some() {
            escaped = true;
        } else if Some(byte) == quote {
            quote = None;
            current.push(byte as char);
        } else if quote.is_none() && (byte == b'\'' || byte == b'"') {
            quote = Some(byte);
            current.push(byte as char);
        } else if quote.is_none() && byte == b',' {
            fields.push(current.trim().to_owned());
            current.clear();
        } else {
            current.push(byte as char);
        }
    }
    if escaped || quote.is_some() {
        return Err(());
    }
    fields.push(current.trim().to_owned());
    Ok(fields)
}

fn unquote(value: &str) -> Result<String, DiagnosticCode> {
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        Ok(value[1..value.len() - 1].to_owned())
    } else if value.contains('"') || value.contains('\'') {
        Err(DiagnosticCode::InvalidIni)
    } else {
        Ok(value.to_owned())
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn diagnostic(
    code: DiagnosticCode,
    source_id: Option<&SourceId>,
    index: u32,
    line: u32,
) -> NodeDiagnostic {
    let mut diagnostic = NodeDiagnostic::new(code, Severity::Error);
    diagnostic.source_id = source_id.cloned();
    diagnostic.location =
        SourceLocation::new(index, Some(line), Some(1), Some("[Proxy]".into())).ok();
    diagnostic
}
