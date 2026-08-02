use std::{collections::BTreeMap, fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    capability::{CapabilityMatrix, CapabilityQuery},
    payload::{FormatHint, SourceId, SourceIdError},
    secret::SecretString,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyProtocol {
    Vless,
    Vmess,
    Shadowsocks,
    Trojan,
    Hysteria2,
    Tuic,
    AnyTls,
}

impl ProxyProtocol {
    pub const ALL: [Self; 7] = [
        Self::Vless,
        Self::Vmess,
        Self::Shadowsocks,
        Self::Trojan,
        Self::Hysteria2,
        Self::Tuic,
        Self::AnyTls,
    ];
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vless => "vless",
            Self::Vmess => "vmess",
            Self::Shadowsocks => "shadowsocks",
            Self::Trojan => "trojan",
            Self::Hysteria2 => "hysteria2",
            Self::Tuic => "tuic",
            Self::AnyTls => "anytls",
        }
    }
}
impl fmt::Display for ProxyProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("unsupported protocol: {0}")]
pub struct UnsupportedProtocol(pub String);
impl FromStr for ProxyProtocol {
    type Err = UnsupportedProtocol;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "vless" => Ok(Self::Vless),
            "vmess" => Ok(Self::Vmess),
            "ss" | "shadowsocks" => Ok(Self::Shadowsocks),
            "trojan" => Ok(Self::Trojan),
            "hysteria2" | "hy2" => Ok(Self::Hysteria2),
            "tuic" => Ok(Self::Tuic),
            "anytls" => Ok(Self::AnyTls),
            _ => Err(UnsupportedProtocol(value.to_owned())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EndpointError {
    #[error("endpoint server is empty")]
    EmptyServer,
    #[error("endpoint server exceeds 64 KiB")]
    ServerTooLong,
    #[error("endpoint server contains whitespace or control characters")]
    InvalidServer,
    #[error("endpoint port must be between 1 and 65535")]
    InvalidPort,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    server: String,
    port: u16,
}
impl Endpoint {
    pub fn new(server: impl Into<String>, port: u16) -> Result<Self, EndpointError> {
        let server = server.into();
        if server.is_empty() {
            return Err(EndpointError::EmptyServer);
        }
        if server.len() > 64 * 1024 {
            return Err(EndpointError::ServerTooLong);
        }
        if server.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(EndpointError::InvalidServer);
        }
        if port == 0 {
            return Err(EndpointError::InvalidPort);
        }
        Ok(Self { server, port })
    }
    pub fn server(&self) -> &str {
        &self.server
    }
    pub const fn port(&self) -> u16 {
        self.port
    }
}
impl fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Endpoint")
            .field("server", &self.server)
            .field("port", &self.port)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundedText(String);
impl BoundedText {
    pub fn new(value: impl Into<String>, max: usize) -> Result<Self, TextError> {
        let value = value.into();
        if value.is_empty() {
            return Err(TextError::Empty);
        }
        if value.len() > max {
            return Err(TextError::TooLong);
        }
        if value.chars().any(char::is_control) {
            return Err(TextError::Control);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TextError {
    #[error("text is empty")]
    Empty,
    #[error("text exceeds configured bound")]
    TooLong,
    #[error("text contains a control character")]
    Control,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UuidValue(SecretString);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum UuidError {
    #[error("UUID shape is not 32 or 36 bytes")]
    Shape,
    #[error("UUID is invalid or nil")]
    Invalid,
}
impl UuidValue {
    pub fn parse(value: &str) -> Result<Self, UuidError> {
        let simple = value.len() == 32 && value.bytes().all(|b| b.is_ascii_hexdigit());
        let hyphenated = value.len() == 36
            && value.chars().enumerate().all(|(i, c)| {
                if [8, 13, 18, 23].contains(&i) {
                    c == '-'
                } else {
                    c.is_ascii_hexdigit()
                }
            });
        if !simple && !hyphenated {
            return Err(UuidError::Shape);
        }
        let uuid = Uuid::parse_str(value).map_err(|_| UuidError::Invalid)?;
        if uuid.is_nil() {
            return Err(UuidError::Invalid);
        }
        Ok(Self(SecretString::new(uuid.hyphenated().to_string())))
    }
    pub fn expose(&self) -> &str {
        self.0.expose()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginSpec {
    pub name: BoundedText,
    pub options: BTreeMap<String, BoundedText>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Credentials {
    Vless {
        uuid: UuidValue,
    },
    Vmess {
        uuid: UuidValue,
        alter_id: u16,
        security: BoundedText,
    },
    Shadowsocks {
        method: BoundedText,
        password: SecretString,
        plugin: Option<PluginSpec>,
    },
    Trojan {
        password: SecretString,
    },
    Hysteria2 {
        password: SecretString,
        obfs: Option<BoundedText>,
    },
    Tuic {
        uuid: UuidValue,
        password: SecretString,
    },
    AnyTls {
        password: SecretString,
    },
}
impl Credentials {
    pub fn protocol(&self) -> ProxyProtocol {
        match self {
            Self::Vless { .. } => ProxyProtocol::Vless,
            Self::Vmess { .. } => ProxyProtocol::Vmess,
            Self::Shadowsocks { .. } => ProxyProtocol::Shadowsocks,
            Self::Trojan { .. } => ProxyProtocol::Trojan,
            Self::Hysteria2 { .. } => ProxyProtocol::Hysteria2,
            Self::Tuic { .. } => ProxyProtocol::Tuic,
            Self::AnyTls { .. } => ProxyProtocol::AnyTls,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealityOptions {
    pub public_key: SecretString,
    pub short_id: Option<SecretString>,
    pub fingerprint: Option<BoundedText>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TlsOptions {
    pub enabled: bool,
    pub server_name: Option<BoundedText>,
    pub insecure: bool,
    pub alpn: Vec<BoundedText>,
    pub client_fingerprint: Option<BoundedText>,
    pub reality: Option<RealityOptions>,
    pub certificate_pin: Option<BoundedText>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportKind {
    Tcp,
    WebSocket,
    Http,
    HttpUpgrade,
    Grpc,
    Quic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportOptions {
    Tcp,
    WebSocket {
        path: BoundedText,
        headers: BTreeMap<String, BoundedText>,
    },
    Http {
        path: BoundedText,
        hosts: Vec<BoundedText>,
    },
    HttpUpgrade {
        path: BoundedText,
        headers: BTreeMap<String, BoundedText>,
    },
    Grpc {
        service_name: BoundedText,
    },
    Quic,
}
impl TransportOptions {
    pub fn kind(&self) -> TransportKind {
        match self {
            Self::Tcp => TransportKind::Tcp,
            Self::WebSocket { .. } => TransportKind::WebSocket,
            Self::Http { .. } => TransportKind::Http,
            Self::HttpUpgrade { .. } => TransportKind::HttpUpgrade,
            Self::Grpc { .. } => TransportKind::Grpc,
            Self::Quic => TransportKind::Quic,
        }
    }
}
impl FromStr for TransportKind {
    type Err = ();
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "tcp" => Ok(Self::Tcp),
            "ws" | "websocket" => Ok(Self::WebSocket),
            "http" => Ok(Self::Http),
            "httpupgrade" | "http-upgrade" => Ok(Self::HttpUpgrade),
            "grpc" => Ok(Self::Grpc),
            "quic" => Ok(Self::Quic),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtocolOptions {
    None,
    Vless { flow: Option<BoundedText> },
    Vmess,
    Shadowsocks,
    Trojan,
    Hysteria2,
    Tuic,
    AnyTls,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub tcp: bool,
    pub udp: bool,
    pub ipv6: bool,
    pub quic: bool,
    pub tls: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRef {
    pub source_id: SourceId,
    pub item_index: u32,
    pub format: FormatHint,
    pub line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayName(String);
impl DisplayName {
    pub fn new(value: impl Into<String>) -> Result<Self, TextError> {
        let value = value.into();
        if value.is_empty() {
            return Err(TextError::Empty);
        }
        if value.len() > 256 {
            return Err(TextError::TooLong);
        }
        if value.chars().any(char::is_control) {
            return Err(TextError::Control);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnvalidatedNode {
    pub display_name: DisplayName,
    pub protocol: ProxyProtocol,
    pub endpoint: Endpoint,
    pub credentials: Credentials,
    pub tls: TlsOptions,
    pub transport: TransportOptions,
    pub protocol_options: ProtocolOptions,
    pub capabilities: Capabilities,
    pub source_refs: Vec<SourceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NodeValidationError {
    #[error("credentials do not match protocol")]
    CredentialProtocolMismatch,
    #[error("source references exceed 64")]
    TooManySourceRefs,
    #[error("transport/TLS combination is invalid")]
    InvalidTlsCombination,
    #[error("capability matrix rejected the combination: {0}")]
    UnsupportedCapability(String),
    #[error("credential is empty")]
    EmptyCredential,
    #[error("transport is not valid for this protocol")]
    UnsupportedTransport,
    #[error("protocol semantics are not supported")]
    UnsupportedSemantics,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeId(String);
impl NodeId {
    pub fn new(value: impl Into<String>) -> Result<Self, TextError> {
        let value = value.into();
        if value.is_empty() {
            return Err(TextError::Empty);
        }
        if value.len() > 128 {
            return Err(TextError::TooLong);
        }
        if value.chars().any(char::is_control) {
            return Err(TextError::Control);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyNode {
    node_id: NodeId,
    inner: UnvalidatedNode,
}
impl ProxyNode {
    pub fn validate(
        node: UnvalidatedNode,
        matrix: &CapabilityMatrix,
    ) -> Result<Self, NodeValidationError> {
        if node.protocol != node.credentials.protocol() {
            return Err(NodeValidationError::CredentialProtocolMismatch);
        }
        if node.source_refs.len() > 64 {
            return Err(NodeValidationError::TooManySourceRefs);
        }
        match &node.credentials {
            Credentials::Shadowsocks { password, .. }
            | Credentials::Trojan { password }
            | Credentials::Hysteria2 { password, .. }
            | Credentials::AnyTls { password }
            | Credentials::Tuic { password, .. }
                if password.is_empty() =>
            {
                return Err(NodeValidationError::EmptyCredential);
            }
            _ => {}
        }
        if matches!(node.transport.kind(), TransportKind::Quic) && !node.tls.enabled {
            return Err(NodeValidationError::InvalidTlsCombination);
        }
        if matches!(
            node.protocol,
            ProxyProtocol::Hysteria2 | ProxyProtocol::Tuic
        ) && node.transport.kind() != TransportKind::Quic
        {
            return Err(NodeValidationError::UnsupportedTransport);
        }
        if matches!(node.protocol, ProxyProtocol::Trojan | ProxyProtocol::AnyTls)
            && !node.tls.enabled
        {
            return Err(NodeValidationError::InvalidTlsCombination);
        }
        if node.tls.reality.is_some()
            && (node.protocol != ProxyProtocol::Vless || !node.tls.enabled)
        {
            return Err(NodeValidationError::InvalidTlsCombination);
        }
        if let Credentials::Shadowsocks { method, plugin, .. } = &node.credentials {
            if plugin.is_some()
                || !matches!(
                    method.as_str(),
                    "aes-128-gcm"
                        | "aes-256-gcm"
                        | "chacha20-ietf-poly1305"
                        | "2022-blake3-aes-128-gcm"
                        | "2022-blake3-aes-256-gcm"
                        | "2022-blake3-chacha20-poly1305"
                )
            {
                return Err(NodeValidationError::UnsupportedSemantics);
            }
        }
        if let Credentials::Vmess { security, .. } = &node.credentials
            && !matches!(
                security.as_str(),
                "auto" | "none" | "aes-128-gcm" | "chacha20-poly1305" | "zero"
            )
        {
            return Err(NodeValidationError::UnsupportedSemantics);
        }
        let query = CapabilityQuery::from_node(&node);
        if !matrix.supports(&query) {
            return Err(NodeValidationError::UnsupportedCapability(
                query.to_string(),
            ));
        }
        let node_id = NodeId::new(format!(
            "{}-{}",
            node.protocol.as_str(),
            node.endpoint.port()
        ))
        .map_err(|_| NodeValidationError::UnsupportedCapability("node_id".into()))?;
        Ok(Self {
            node_id,
            inner: node,
        })
    }
    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }
    pub fn protocol(&self) -> ProxyProtocol {
        self.inner.protocol
    }
    pub fn endpoint(&self) -> &Endpoint {
        &self.inner.endpoint
    }
    pub fn credentials(&self) -> &Credentials {
        &self.inner.credentials
    }
    pub fn source_refs(&self) -> &[SourceRef] {
        &self.inner.source_refs
    }
}

impl From<SourceIdError> for NodeValidationError {
    fn from(_: SourceIdError) -> Self {
        Self::UnsupportedCapability("source_id".into())
    }
}
