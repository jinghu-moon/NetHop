use std::collections::BTreeMap;

use thiserror::Error;

use crate::{
    capability::CapabilityMatrix,
    diagnostics::{DiagnosticCode, NodeDiagnostic, Severity, SourceLocation},
    protocol::{
        AnyTlsOptions, BoundedText, Capabilities, Credentials, DisplayName, Endpoint, HttpOptions,
        Hysteria2Obfs, Hysteria2Options, PluginSpec, ProtocolOptions, ProxyNode, ProxyProtocol,
        RealityOptions, SocksOptions, SourceRef, TlsOptions, TransportKind, TransportOptions,
        TuicOptions, UdpOverTcpOptions, UuidValue,
    },
    secret::SecretString,
    uri::{UriNodeCandidate, UriScheme, percent_decode_field},
};

/// Container-neutral node fields. Adapters may decode aliases, but semantic policy lives here.
#[derive(Clone, PartialEq, Eq)]
pub struct NodeSpec {
    pub display_name: Option<String>,
    pub protocol: String,
    pub server: String,
    pub port: u16,
    pub uuid: Option<String>,
    pub password: Option<String>,
    pub username: Option<String>,
    pub method: Option<String>,
    pub vmess_security: Option<String>,
    pub alter_id: Option<u16>,
    pub plugin: Option<String>,
    pub plugin_options: BTreeMap<String, String>,
    pub tls: bool,
    pub insecure: bool,
    pub server_name: Option<String>,
    pub alpn: Vec<String>,
    pub client_fingerprint: Option<String>,
    pub reality_public_key: Option<String>,
    pub reality_short_id: Option<String>,
    pub flow: Option<String>,
    pub transport: Option<String>,
    pub path: Option<String>,
    pub hosts: Vec<String>,
    pub service_name: Option<String>,
    pub headers: BTreeMap<String, String>,
    pub udp: bool,
    pub udp_over_tcp: Option<UdpOverTcpOptions>,
    pub obfs: Option<String>,
    pub obfs_password: Option<String>,
    pub congestion_control: Option<String>,
    pub server_ports: Vec<String>,
    pub hop_interval: Option<String>,
    pub up_mbps: Option<u32>,
    pub down_mbps: Option<u32>,
    pub udp_relay_mode: Option<String>,
    pub udp_over_stream: bool,
    pub zero_rtt_handshake: bool,
    pub heartbeat: Option<String>,
    pub idle_session_check_interval: Option<String>,
    pub idle_session_timeout: Option<String>,
    pub min_idle_session: Option<u32>,
    pub http_path: Option<String>,
    pub http_headers: BTreeMap<String, String>,
    pub socks_version: Option<String>,
    pub source_ref: Option<SourceRef>,
    pub location: Option<SourceLocation>,
    pub unknown_critical_field: Option<String>,
    pub unknown_harmless_fields: usize,
}

impl NodeSpec {
    pub fn minimal(protocol: impl Into<String>, server: impl Into<String>, port: u16) -> Self {
        Self {
            display_name: None,
            protocol: protocol.into(),
            server: server.into(),
            port,
            uuid: None,
            password: None,
            username: None,
            method: None,
            vmess_security: None,
            alter_id: None,
            plugin: None,
            plugin_options: BTreeMap::new(),
            tls: false,
            insecure: false,
            server_name: None,
            alpn: Vec::new(),
            client_fingerprint: None,
            reality_public_key: None,
            reality_short_id: None,
            flow: None,
            transport: None,
            path: None,
            hosts: Vec::new(),
            service_name: None,
            headers: BTreeMap::new(),
            udp: false,
            udp_over_tcp: None,
            obfs: None,
            obfs_password: None,
            congestion_control: None,
            server_ports: Vec::new(),
            hop_interval: None,
            up_mbps: None,
            down_mbps: None,
            udp_relay_mode: None,
            udp_over_stream: false,
            zero_rtt_handshake: false,
            heartbeat: None,
            idle_session_check_interval: None,
            idle_session_timeout: None,
            min_idle_session: None,
            http_path: None,
            http_headers: BTreeMap::new(),
            socks_version: None,
            source_ref: None,
            location: None,
            unknown_critical_field: None,
            unknown_harmless_fields: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SemanticError {
    #[error("protocol is not in NetHop's allowlist")]
    UnsupportedProtocol,
    #[error("endpoint is invalid")]
    InvalidEndpoint,
    #[error("a required credential is missing or invalid")]
    InvalidCredential,
    #[error("transport is unsupported")]
    UnsupportedTransport,
    #[error("connection-affecting semantics are unsupported")]
    UnsupportedSemantics,
    #[error("TLS, Reality, or transport combination is invalid")]
    InvalidTlsCombination,
}

impl SemanticError {
    pub const fn code(&self) -> DiagnosticCode {
        match self {
            Self::UnsupportedProtocol => DiagnosticCode::UnsupportedProtocol,
            Self::InvalidEndpoint => DiagnosticCode::InvalidEndpoint,
            Self::InvalidCredential => DiagnosticCode::InvalidCredential,
            Self::UnsupportedTransport => DiagnosticCode::UnsupportedTransport,
            Self::UnsupportedSemantics => DiagnosticCode::UnsupportedSemantics,
            Self::InvalidTlsCombination => DiagnosticCode::InvalidTlsCombination,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticOutcome {
    pub node: ProxyNode,
    pub warnings: Vec<NodeDiagnostic>,
}

pub fn validate_node_spec(
    spec: NodeSpec,
    matrix: &CapabilityMatrix,
) -> Result<SemanticOutcome, SemanticError> {
    if spec.unknown_critical_field.is_some() {
        return Err(SemanticError::UnsupportedSemantics);
    }
    let protocol = spec
        .protocol
        .parse::<ProxyProtocol>()
        .map_err(|_| SemanticError::UnsupportedProtocol)?;
    if protocol != ProxyProtocol::Hysteria2
        && (!spec.server_ports.is_empty()
            || spec.hop_interval.is_some()
            || spec.up_mbps.is_some()
            || spec.down_mbps.is_some())
    {
        return Err(SemanticError::UnsupportedSemantics);
    }
    if protocol != ProxyProtocol::Tuic
        && (spec.congestion_control.is_some()
            || spec.udp_relay_mode.is_some()
            || spec.udp_over_stream
            || spec.zero_rtt_handshake
            || spec.heartbeat.is_some())
    {
        return Err(SemanticError::UnsupportedSemantics);
    }
    if protocol != ProxyProtocol::AnyTls
        && (spec.idle_session_check_interval.is_some()
            || spec.idle_session_timeout.is_some()
            || spec.min_idle_session.is_some())
    {
        return Err(SemanticError::UnsupportedSemantics);
    }
    if protocol != ProxyProtocol::Http
        && (spec.http_path.is_some() || !spec.http_headers.is_empty())
    {
        return Err(SemanticError::UnsupportedSemantics);
    }
    if protocol != ProxyProtocol::Socks && spec.socks_version.is_some() {
        return Err(SemanticError::UnsupportedSemantics);
    }
    if protocol == ProxyProtocol::Socks && spec.tls {
        return Err(SemanticError::InvalidTlsCombination);
    }
    if protocol == ProxyProtocol::Hysteria2 && !valid_server_ports(&spec.server_ports) {
        return Err(SemanticError::UnsupportedSemantics);
    }
    let tls = make_tls(&spec, protocol)?;
    let credentials = make_credentials(&spec, protocol)?;
    let endpoint =
        Endpoint::new(spec.server, spec.port).map_err(|_| SemanticError::InvalidEndpoint)?;
    let transport = make_transport(
        protocol,
        spec.transport.as_deref(),
        spec.path,
        spec.hosts,
        spec.service_name,
        spec.headers,
    )?;
    let protocol_options = match protocol {
        ProxyProtocol::Vless => ProtocolOptions::Vless {
            flow: spec
                .flow
                .map(|value| text(value, 256))
                .transpose()
                .map_err(|_| SemanticError::UnsupportedSemantics)?,
        },
        ProxyProtocol::Shadowsocks => ProtocolOptions::Shadowsocks {
            udp_over_tcp: spec.udp_over_tcp,
        },
        ProxyProtocol::Hysteria2 => ProtocolOptions::Hysteria2(Hysteria2Options {
            server_ports: spec
                .server_ports
                .into_iter()
                .map(|value| text(value, 64))
                .collect::<Result<_, _>>()
                .map_err(|_| SemanticError::UnsupportedSemantics)?,
            hop_interval: spec
                .hop_interval
                .map(|value| text(value, 64))
                .transpose()
                .map_err(|_| SemanticError::UnsupportedSemantics)?,
            up_mbps: spec.up_mbps,
            down_mbps: spec.down_mbps,
        }),
        ProxyProtocol::Tuic => ProtocolOptions::Tuic(TuicOptions {
            congestion_control: spec
                .congestion_control
                .as_deref()
                .map(|value| text(value, 64))
                .transpose()
                .map_err(|_| SemanticError::UnsupportedSemantics)?,
            udp_relay_mode: spec
                .udp_relay_mode
                .as_deref()
                .map(|value| text(value, 64))
                .transpose()
                .map_err(|_| SemanticError::UnsupportedSemantics)?,
            udp_over_stream: spec.udp_over_stream,
            zero_rtt_handshake: spec.zero_rtt_handshake,
            heartbeat: spec
                .heartbeat
                .map(|value| text(value, 64))
                .transpose()
                .map_err(|_| SemanticError::UnsupportedSemantics)?,
        }),
        ProxyProtocol::AnyTls => ProtocolOptions::AnyTls(AnyTlsOptions {
            idle_session_check_interval: spec
                .idle_session_check_interval
                .map(|value| text(value, 64))
                .transpose()
                .map_err(|_| SemanticError::UnsupportedSemantics)?,
            idle_session_timeout: spec
                .idle_session_timeout
                .map(|value| text(value, 64))
                .transpose()
                .map_err(|_| SemanticError::UnsupportedSemantics)?,
            min_idle_session: spec.min_idle_session,
        }),
        ProxyProtocol::Http => ProtocolOptions::Http(HttpOptions {
            path: spec
                .http_path
                .map(|value| text(value, 8 * 1024))
                .transpose()
                .map_err(|_| SemanticError::UnsupportedSemantics)?,
            headers: map_http_headers(spec.http_headers)?,
        }),
        ProxyProtocol::Socks => {
            let version = spec.socks_version.unwrap_or_else(|| "5".into());
            if !matches!(version.as_str(), "4" | "4a" | "5") {
                return Err(SemanticError::UnsupportedSemantics);
            }
            ProtocolOptions::Socks(SocksOptions {
                version: text(version, 8).map_err(|_| SemanticError::UnsupportedSemantics)?,
                udp_over_tcp: spec.udp_over_tcp,
            })
        }
        _ => ProtocolOptions::None,
    };
    if spec.udp_over_tcp.is_some()
        && !matches!(protocol, ProxyProtocol::Shadowsocks | ProxyProtocol::Socks)
    {
        return Err(SemanticError::UnsupportedSemantics);
    }
    if let Some(congestion) = spec.congestion_control.as_deref()
        && (protocol != ProxyProtocol::Tuic || !matches!(congestion, "bbr" | "cubic" | "new_reno"))
    {
        return Err(SemanticError::UnsupportedSemantics);
    }
    let display_name = spec.display_name.unwrap_or_else(|| {
        format!(
            "{}-{}:{}",
            protocol.as_str(),
            endpoint.server(),
            endpoint.port()
        )
    });
    let udp = spec.udp || matches!(protocol, ProxyProtocol::Hysteria2 | ProxyProtocol::Tuic);
    let node = crate::protocol::UnvalidatedNode {
        display_name: DisplayName::new(display_name)
            .map_err(|_| SemanticError::UnsupportedSemantics)?,
        protocol,
        endpoint,
        credentials,
        tls,
        transport,
        protocol_options,
        capabilities: Capabilities {
            tcp: !matches!(protocol, ProxyProtocol::Hysteria2 | ProxyProtocol::Tuic),
            udp,
            ipv6: false,
            quic: matches!(protocol, ProxyProtocol::Hysteria2 | ProxyProtocol::Tuic),
            tls: spec.tls,
        },
        source_refs: spec.source_ref.into_iter().collect(),
    };
    let proxy = ProxyNode::validate(node, matrix).map_err(map_validation_error)?;
    let mut warnings = if spec.insecure {
        vec![diagnostic(
            DiagnosticCode::InsecureTls,
            Severity::Warning,
            spec.location.clone(),
            Some(protocol),
        )]
    } else {
        Vec::new()
    };
    if spec.unknown_harmless_fields > 0 {
        warnings.push(
            diagnostic(
                DiagnosticCode::UnknownField,
                Severity::Warning,
                spec.location,
                Some(protocol),
            )
            .with_parameter("count", spec.unknown_harmless_fields.to_string()),
        );
    }
    Ok(SemanticOutcome {
        node: proxy,
        warnings,
    })
}

/// Converts a bounded URI container candidate into a container-neutral semantic input.
/// It deliberately does not construct a `ProxyNode` until `validate_node_spec` is called.
pub fn node_spec_from_uri(candidate: &UriNodeCandidate<'_>) -> Result<NodeSpec, SemanticError> {
    if candidate.scheme() == UriScheme::Vmess && candidate.raw_userinfo().is_none() {
        return node_spec_from_vmess_json(candidate);
    }
    let port = candidate.port().ok_or(SemanticError::InvalidEndpoint)?;
    let mut spec = NodeSpec::minimal(candidate.protocol().as_str(), candidate.server(), port);
    if let Some(port_spec) = candidate.port_spec() {
        if !matches!(candidate.protocol(), ProxyProtocol::Hysteria2) {
            return Err(SemanticError::UnsupportedSemantics);
        }
        spec.server_ports = port_spec
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect();
    }
    spec.display_name = candidate
        .display_name()
        .map_err(|_| SemanticError::UnsupportedSemantics)?;
    spec.uuid = candidate
        .raw_userinfo()
        .map(percent_decode_field)
        .transpose()
        .map_err(|_| SemanticError::InvalidCredential)?;
    let userinfo = spec.uuid.clone();
    match candidate.scheme() {
        UriScheme::Vless | UriScheme::Vmess => {}
        UriScheme::Shadowsocks => {
            let value = userinfo.ok_or(SemanticError::InvalidCredential)?;
            let (method, password) = value
                .split_once(':')
                .ok_or(SemanticError::InvalidCredential)?;
            spec.method = Some(method.into());
            spec.password = Some(password.into());
            spec.uuid = None;
        }
        UriScheme::Trojan
        | UriScheme::Hysteria2
        | UriScheme::Hysteria2Short
        | UriScheme::AnyTls => {
            spec.password = userinfo;
            spec.uuid = None;
        }
        UriScheme::Tuic => {
            let value = userinfo.ok_or(SemanticError::InvalidCredential)?;
            let (uuid, password) = value
                .split_once(':')
                .ok_or(SemanticError::InvalidCredential)?;
            spec.uuid = Some(uuid.into());
            spec.password = Some(password.into());
        }
    }
    for parameter in candidate.query() {
        let key = parameter
            .decoded_key()
            .map_err(|_| SemanticError::UnsupportedSemantics)?;
        let value = parameter
            .decoded_value()
            .map_err(|_| SemanticError::UnsupportedSemantics)?;
        match key.as_str() {
            "type" | "network" => spec.transport = Some(value),
            "security" if candidate.protocol() == ProxyProtocol::Vmess => {
                spec.vmess_security = Some(value);
            }
            "security" if value == "tls" || value == "reality" => spec.tls = true,
            "security" if value != "none" => spec.unknown_critical_field = Some(key),
            "tls" => spec.tls = matches!(value.as_str(), "1" | "true" | "tls" | "reality"),
            "sni" | "servername" => spec.server_name = Some(value),
            "alpn" => spec.alpn = value.split(',').map(str::to_owned).collect(),
            "fp" | "fingerprint" => spec.client_fingerprint = Some(value),
            "pbk" | "public-key" => spec.reality_public_key = Some(value),
            "sid" | "short-id" => spec.reality_short_id = Some(value),
            "flow" => spec.flow = Some(value),
            "path" => spec.path = Some(value),
            "host" => spec.hosts.extend(
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|host| !host.is_empty())
                    .map(str::to_owned),
            ),
            "serviceName" | "service_name" => spec.service_name = Some(value),
            "udp" => spec.udp = matches!(value.as_str(), "1" | "true"),
            "obfs" => spec.obfs = Some(value),
            "obfs-password" | "obfs_password" => spec.obfs_password = Some(value),
            "congestion_control" => spec.congestion_control = Some(value),
            "udp_relay_mode" => spec.udp_relay_mode = Some(value),
            "udp_over_stream" => spec.udp_over_stream = matches!(value.as_str(), "1" | "true"),
            "zero_rtt_handshake" => {
                spec.zero_rtt_handshake = matches!(value.as_str(), "1" | "true")
            }
            "heartbeat" => spec.heartbeat = Some(value),
            "hop_interval" => spec.hop_interval = Some(value),
            "server_ports" => {
                spec.server_ports = value
                    .split(',')
                    .map(str::trim)
                    .filter(|port| !port.is_empty())
                    .map(str::to_owned)
                    .collect()
            }
            "up_mbps" => spec.up_mbps = value.parse().ok(),
            "down_mbps" => spec.down_mbps = value.parse().ok(),
            "idle_session_check_interval" => spec.idle_session_check_interval = Some(value),
            "idle_session_timeout" => spec.idle_session_timeout = Some(value),
            "min_idle_session" => spec.min_idle_session = value.parse().ok(),
            "plugin" => {
                let (plugin, options) = parse_uri_plugin_value(&value)?;
                spec.plugin = Some(plugin);
                spec.plugin_options = options;
            }
            "allowInsecure" | "insecure" => spec.insecure = matches!(value.as_str(), "1" | "true"),
            _ => spec.unknown_harmless_fields = spec.unknown_harmless_fields.saturating_add(1),
        }
    }
    if matches!(
        candidate.protocol(),
        ProxyProtocol::Trojan
            | ProxyProtocol::Hysteria2
            | ProxyProtocol::Tuic
            | ProxyProtocol::AnyTls
    ) {
        spec.tls = true;
    }
    if matches!(
        candidate.protocol(),
        ProxyProtocol::Hysteria2 | ProxyProtocol::Tuic
    ) {
        spec.transport.get_or_insert_with(|| "quic".into());
        spec.udp = true;
    }
    Ok(spec)
}

fn node_spec_from_vmess_json(candidate: &UriNodeCandidate<'_>) -> Result<NodeSpec, SemanticError> {
    let bytes = candidate
        .vmess_inner_json()
        .map_err(|_| SemanticError::InvalidCredential)?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| SemanticError::InvalidCredential)?;
    let object = value.as_object().ok_or(SemanticError::InvalidCredential)?;
    let required = |key: &str| {
        object
            .get(key)
            .and_then(serde_json::Value::as_str)
            .ok_or(SemanticError::InvalidCredential)
    };
    let port = match object.get("port") {
        Some(serde_json::Value::Number(value)) => value
            .as_u64()
            .and_then(|value| u16::try_from(value).ok())
            .ok_or(SemanticError::InvalidEndpoint)?,
        Some(serde_json::Value::String(value)) => value
            .parse::<u16>()
            .map_err(|_| SemanticError::InvalidEndpoint)?,
        _ => return Err(SemanticError::InvalidEndpoint),
    };
    let mut spec = NodeSpec::minimal("vmess", required("add")?, port);
    spec.display_name = object
        .get("ps")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    spec.uuid = Some(required("id")?.into());
    spec.alter_id = object
        .get("aid")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u16::try_from(value).ok());
    spec.vmess_security = object
        .get("scy")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    spec.transport = object
        .get("net")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    spec.path = object
        .get("path")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    spec.hosts = object
        .get("host")
        .and_then(serde_json::Value::as_str)
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|host| !host.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    spec.tls = matches!(
        object.get("tls").and_then(serde_json::Value::as_str),
        Some("tls")
    );
    spec.server_name = object
        .get("sni")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    Ok(spec)
}

pub fn semantic_diagnostic(
    error: SemanticError,
    location: Option<SourceLocation>,
) -> NodeDiagnostic {
    diagnostic(error.code(), Severity::Error, location, None)
}

fn diagnostic(
    code: DiagnosticCode,
    severity: Severity,
    location: Option<SourceLocation>,
    protocol: Option<ProxyProtocol>,
) -> NodeDiagnostic {
    let mut diagnostic = NodeDiagnostic::new(code, severity);
    diagnostic.location = location;
    diagnostic.protocol = protocol;
    diagnostic
}

fn make_credentials(
    spec: &NodeSpec,
    protocol: ProxyProtocol,
) -> Result<Credentials, SemanticError> {
    if !matches!(protocol, ProxyProtocol::Http | ProxyProtocol::Socks) && spec.username.is_some() {
        return Err(SemanticError::UnsupportedSemantics);
    }
    if matches!(protocol, ProxyProtocol::Http | ProxyProtocol::Socks)
        && (spec.uuid.is_some()
            || spec.method.is_some()
            || spec.vmess_security.is_some()
            || spec.alter_id.is_some()
            || spec.plugin.is_some()
            || !spec.plugin_options.is_empty()
            || spec.obfs.is_some()
            || spec.obfs_password.is_some())
    {
        return Err(SemanticError::UnsupportedSemantics);
    }
    let password = || {
        spec.password
            .as_deref()
            .filter(|value| !value.is_empty())
            .map(SecretString::new)
            .ok_or(SemanticError::InvalidCredential)
    };
    let uuid = || {
        spec.uuid
            .as_deref()
            .ok_or(SemanticError::InvalidCredential)
            .and_then(|value| UuidValue::parse(value).map_err(|_| SemanticError::InvalidCredential))
    };
    match protocol {
        ProxyProtocol::Vless => Ok(Credentials::Vless { uuid: uuid()? }),
        ProxyProtocol::Vmess => {
            let security = spec.vmess_security.as_deref().unwrap_or("auto");
            if !matches!(
                security,
                "auto" | "none" | "aes-128-gcm" | "chacha20-poly1305" | "zero"
            ) {
                return Err(SemanticError::UnsupportedSemantics);
            }
            Ok(Credentials::Vmess {
                uuid: uuid()?,
                alter_id: spec.alter_id.unwrap_or(0),
                security: text(security, 64).map_err(|_| SemanticError::InvalidCredential)?,
            })
        }
        ProxyProtocol::Shadowsocks => {
            let method = spec
                .method
                .as_deref()
                .ok_or(SemanticError::InvalidCredential)?;
            if !matches!(
                method,
                "aes-128-gcm"
                    | "aes-256-gcm"
                    | "chacha20-ietf-poly1305"
                    | "2022-blake3-aes-128-gcm"
                    | "2022-blake3-aes-256-gcm"
                    | "2022-blake3-chacha20-poly1305"
            ) {
                return Err(SemanticError::UnsupportedSemantics);
            }
            let plugin = match spec.plugin.as_deref() {
                None if spec.plugin_options.is_empty() => None,
                Some("obfs-local") => {
                    if spec
                        .plugin_options
                        .keys()
                        .any(|key| !matches!(key.as_str(), "obfs" | "obfs-host"))
                        || !matches!(
                            spec.plugin_options.get("obfs").map(String::as_str),
                            Some("http" | "tls")
                        )
                    {
                        return Err(SemanticError::UnsupportedSemantics);
                    }
                    let options = spec
                        .plugin_options
                        .iter()
                        .map(|(key, value)| {
                            Ok((
                                key.clone(),
                                text(value, 256)
                                    .map_err(|_| SemanticError::UnsupportedSemantics)?,
                            ))
                        })
                        .collect::<Result<BTreeMap<_, _>, SemanticError>>()?;
                    Some(PluginSpec {
                        name: text("obfs-local", 64)
                            .map_err(|_| SemanticError::UnsupportedSemantics)?,
                        options,
                    })
                }
                Some("v2ray-plugin") => {
                    if spec.plugin_options.keys().any(|key| {
                        !matches!(key.as_str(), "mode" | "host" | "path" | "tls" | "mux")
                    }) || !matches!(
                        spec.plugin_options.get("mode").map(String::as_str),
                        Some("websocket" | "quic")
                    ) {
                        return Err(SemanticError::UnsupportedSemantics);
                    }
                    if let Some(tls) = spec.plugin_options.get("tls")
                        && tls != "true"
                        && tls != "false"
                    {
                        return Err(SemanticError::UnsupportedSemantics);
                    }
                    if let Some(mux) = spec.plugin_options.get("mux")
                        && mux.parse::<u16>().is_err()
                    {
                        return Err(SemanticError::UnsupportedSemantics);
                    }
                    let options = spec
                        .plugin_options
                        .iter()
                        .map(|(key, value)| {
                            Ok((
                                key.clone(),
                                text(value, 256)
                                    .map_err(|_| SemanticError::UnsupportedSemantics)?,
                            ))
                        })
                        .collect::<Result<BTreeMap<_, _>, SemanticError>>()?;
                    Some(PluginSpec {
                        name: text("v2ray-plugin", 64)
                            .map_err(|_| SemanticError::UnsupportedSemantics)?,
                        options,
                    })
                }
                _ => return Err(SemanticError::UnsupportedSemantics),
            };
            Ok(Credentials::Shadowsocks {
                method: text(method, 64).map_err(|_| SemanticError::InvalidCredential)?,
                password: password()?,
                plugin,
            })
        }
        ProxyProtocol::Trojan => Ok(Credentials::Trojan {
            password: password()?,
        }),
        ProxyProtocol::Hysteria2 => {
            let obfs = match (spec.obfs.as_deref(), spec.obfs_password.as_deref()) {
                (None, None) => None,
                (Some("salamander"), Some(password)) if !password.is_empty() => {
                    Some(Hysteria2Obfs {
                        kind: text("salamander", 64)
                            .map_err(|_| SemanticError::UnsupportedSemantics)?,
                        password: SecretString::new(password),
                    })
                }
                _ => return Err(SemanticError::UnsupportedSemantics),
            };
            Ok(Credentials::Hysteria2 {
                password: password()?,
                obfs,
            })
        }
        ProxyProtocol::Tuic => Ok(Credentials::Tuic {
            uuid: uuid()?,
            password: password()?,
        }),
        ProxyProtocol::AnyTls => Ok(Credentials::AnyTls {
            password: password()?,
        }),
        ProxyProtocol::Http => optional_user_password(spec)
            .map(|(username, password)| Credentials::Http { username, password }),
        ProxyProtocol::Socks => optional_user_password(spec)
            .map(|(username, password)| Credentials::Socks { username, password }),
    }
}

fn optional_user_password(
    spec: &NodeSpec,
) -> Result<(Option<SecretString>, Option<SecretString>), SemanticError> {
    match (spec.username.as_deref(), spec.password.as_deref()) {
        (None, None) => Ok((None, None)),
        (Some(username), Some(password)) if !username.is_empty() && !password.is_empty() => Ok((
            Some(SecretString::new(username)),
            Some(SecretString::new(password)),
        )),
        _ => Err(SemanticError::InvalidCredential),
    }
}

fn parse_uri_plugin_value(
    value: &str,
) -> Result<(String, BTreeMap<String, String>), SemanticError> {
    let mut parts = value.split(';');
    let name = parts
        .next()
        .filter(|name| !name.is_empty())
        .ok_or(SemanticError::UnsupportedSemantics)?
        .to_owned();
    let mut options = BTreeMap::new();
    for part in parts {
        let (key, value) = part
            .split_once('=')
            .filter(|(key, value)| !key.is_empty() && !value.is_empty())
            .ok_or(SemanticError::UnsupportedSemantics)?;
        if options.insert(key.to_owned(), value.to_owned()).is_some() {
            return Err(SemanticError::UnsupportedSemantics);
        }
    }
    Ok((name, options))
}

fn make_tls(spec: &NodeSpec, protocol: ProxyProtocol) -> Result<TlsOptions, SemanticError> {
    let reality = match spec.reality_public_key.as_deref() {
        Some(public_key)
            if protocol == ProxyProtocol::Vless && spec.tls && !public_key.is_empty() =>
        {
            Some(RealityOptions {
                public_key: SecretString::new(public_key),
                short_id: spec
                    .reality_short_id
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .map(SecretString::new),
                fingerprint: spec
                    .client_fingerprint
                    .clone()
                    .map(|value| text(value, 64))
                    .transpose()
                    .map_err(|_| SemanticError::InvalidTlsCombination)?,
            })
        }
        Some(_) => return Err(SemanticError::InvalidTlsCombination),
        None => None,
    };
    if !spec.tls
        && (spec.server_name.is_some()
            || !spec.alpn.is_empty()
            || spec.client_fingerprint.is_some())
    {
        return Err(SemanticError::InvalidTlsCombination);
    }
    Ok(TlsOptions {
        enabled: spec.tls,
        server_name: spec
            .server_name
            .clone()
            .map(|value| text(value, 256))
            .transpose()
            .map_err(|_| SemanticError::InvalidTlsCombination)?,
        insecure: spec.insecure,
        alpn: spec
            .alpn
            .iter()
            .cloned()
            .map(|value| text(value, 64))
            .collect::<Result<_, _>>()
            .map_err(|_| SemanticError::InvalidTlsCombination)?,
        client_fingerprint: spec
            .client_fingerprint
            .clone()
            .map(|value| text(value, 64))
            .transpose()
            .map_err(|_| SemanticError::InvalidTlsCombination)?,
        reality,
        certificate_pin: None,
    })
}

fn make_transport(
    protocol: ProxyProtocol,
    requested: Option<&str>,
    path: Option<String>,
    hosts: Vec<String>,
    service_name: Option<String>,
    headers: BTreeMap<String, String>,
) -> Result<TransportOptions, SemanticError> {
    if matches!(protocol, ProxyProtocol::Http | ProxyProtocol::Socks)
        && requested.is_some_and(|value| value != "tcp")
    {
        return Err(SemanticError::UnsupportedTransport);
    }
    let default = if matches!(protocol, ProxyProtocol::Hysteria2 | ProxyProtocol::Tuic) {
        "quic"
    } else {
        "tcp"
    };
    let kind = requested
        .unwrap_or(default)
        .parse::<TransportKind>()
        .map_err(|_| SemanticError::UnsupportedTransport)?;
    match kind {
        TransportKind::Tcp => {
            if path.is_some() || !hosts.is_empty() || service_name.is_some() || !headers.is_empty()
            {
                return Err(SemanticError::UnsupportedSemantics);
            }
            Ok(TransportOptions::Tcp)
        }
        TransportKind::WebSocket | TransportKind::HttpUpgrade => {
            let path = text(path.unwrap_or_else(|| "/".into()), 8 * 1024)
                .map_err(|_| SemanticError::UnsupportedSemantics)?;
            let mut headers = map_headers(headers)?;
            if let Some(host) = hosts.first()
                && !headers.keys().any(|key| key.eq_ignore_ascii_case("host"))
            {
                headers.insert(
                    "Host".into(),
                    text(host, 256).map_err(|_| SemanticError::UnsupportedSemantics)?,
                );
            }
            if hosts.len() > 1 {
                return Err(SemanticError::UnsupportedSemantics);
            }
            Ok(if kind == TransportKind::WebSocket {
                TransportOptions::WebSocket { path, headers }
            } else {
                TransportOptions::HttpUpgrade { path, headers }
            })
        }
        TransportKind::Http => Ok(TransportOptions::Http {
            path: text(path.unwrap_or_else(|| "/".into()), 8 * 1024)
                .map_err(|_| SemanticError::UnsupportedSemantics)?,
            hosts: hosts
                .into_iter()
                .map(|host| text(host, 256).map_err(|_| SemanticError::UnsupportedSemantics))
                .collect::<Result<_, _>>()?,
        }),
        TransportKind::Grpc => {
            // gRPC has no V2Ray host list in sing-box 1.13.15.
            // Host values are therefore rejected instead of silently dropped.
            if !hosts.is_empty() {
                return Err(SemanticError::UnsupportedSemantics);
            }
            Ok(TransportOptions::Grpc {
                service_name: text(service_name.unwrap_or_else(|| "grpc".into()), 256)
                    .map_err(|_| SemanticError::UnsupportedSemantics)?,
            })
        }
        TransportKind::Quic => {
            if path.is_some() || !hosts.is_empty() || service_name.is_some() || !headers.is_empty()
            {
                return Err(SemanticError::UnsupportedSemantics);
            }
            Ok(TransportOptions::Quic)
        }
    }
}

fn valid_server_ports(values: &[String]) -> bool {
    if values.is_empty() || values.len() > 64 {
        return values.is_empty();
    }
    values.iter().all(|value| {
        let value = value.trim();
        if value.is_empty() {
            return false;
        }
        if let Some((start, end)) = value.split_once('-') {
            if start.is_empty() || end.is_empty() || end.contains('-') {
                return false;
            }
            let Ok(start) = start.parse::<u16>() else {
                return false;
            };
            let Ok(end) = end.parse::<u16>() else {
                return false;
            };
            start != 0 && start <= end
        } else {
            value.parse::<u16>().is_ok_and(|port| port != 0)
        }
    })
}

fn map_headers(
    headers: BTreeMap<String, String>,
) -> Result<BTreeMap<String, BoundedText>, SemanticError> {
    headers
        .into_iter()
        .map(|(key, value)| {
            let key = text(key, 256).map_err(|_| SemanticError::UnsupportedSemantics)?;
            let value = text(value, 8 * 1024).map_err(|_| SemanticError::UnsupportedSemantics)?;
            Ok((key.as_str().to_owned(), value))
        })
        .collect()
}

fn map_http_headers(
    headers: BTreeMap<String, String>,
) -> Result<BTreeMap<String, SecretString>, SemanticError> {
    headers
        .into_iter()
        .map(|(key, value)| {
            let key = text(key, 256).map_err(|_| SemanticError::UnsupportedSemantics)?;
            text(&value, 8 * 1024).map_err(|_| SemanticError::UnsupportedSemantics)?;
            Ok((key.as_str().to_owned(), SecretString::new(value)))
        })
        .collect()
}

fn text(value: impl Into<String>, max: usize) -> Result<BoundedText, crate::protocol::TextError> {
    BoundedText::new(value, max)
}

fn map_validation_error(error: crate::protocol::NodeValidationError) -> SemanticError {
    match error {
        crate::protocol::NodeValidationError::InvalidTlsCombination => {
            SemanticError::InvalidTlsCombination
        }
        crate::protocol::NodeValidationError::EmptyCredential => SemanticError::InvalidCredential,
        crate::protocol::NodeValidationError::UnsupportedTransport => {
            SemanticError::UnsupportedTransport
        }
        crate::protocol::NodeValidationError::UnsupportedSemantics
        | crate::protocol::NodeValidationError::UnsupportedCapability(_) => {
            SemanticError::UnsupportedSemantics
        }
        crate::protocol::NodeValidationError::CredentialProtocolMismatch
        | crate::protocol::NodeValidationError::TooManySourceRefs => {
            SemanticError::InvalidCredential
        }
    }
}
