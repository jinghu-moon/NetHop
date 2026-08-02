use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::protocol::{Credentials, ProxyProtocol, TransportKind, UnvalidatedNode};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityEvidence {
    pub id: String,
    pub check_fixture: String,
    pub connectivity_fixture: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CapabilityQuery {
    pub protocol: ProxyProtocol,
    pub transport: TransportKind,
    pub tls: bool,
    pub reality: bool,
    pub utls: bool,
    pub udp: bool,
    pub flow: Option<String>,
    pub plugin: Option<String>,
}

impl CapabilityQuery {
    pub fn from_node(node: &UnvalidatedNode) -> Self {
        let (reality, utls) = node.tls.reality.as_ref().map_or((false, false), |reality| {
            (true, reality.fingerprint.is_some())
        });
        let flow = match &node.protocol_options {
            crate::protocol::ProtocolOptions::Vless { flow } => {
                flow.as_ref().map(|value| value.as_str().to_owned())
            }
            _ => None,
        };
        let plugin = match &node.credentials {
            Credentials::Shadowsocks { plugin, .. } => {
                plugin.as_ref().map(|value| value.name.as_str().to_owned())
            }
            _ => None,
        };
        Self {
            protocol: node.protocol,
            transport: node.transport.kind(),
            tls: node.tls.enabled,
            reality,
            utls,
            udp: node.capabilities.udp,
            flow,
            plugin,
        }
    }
}
impl fmt::Display for CapabilityQuery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{:?}:tls={}:reality={}:utls={}:udp={}",
            self.protocol, self.transport, self.tls, self.reality, self.utls, self.udp
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityEntry {
    pub query: CapabilityQuery,
    pub supported: bool,
    pub evidence: Option<CapabilityEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityMatrix {
    pub schema_version: u32,
    pub sing_box_version: String,
    pub build_tags: Vec<String>,
    pub entries: Vec<CapabilityEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CapabilityError {
    #[error("supported capability requires evidence")]
    MissingEvidence,
    #[error("sing-box version must be pinned")]
    UnpinnedVersion,
    #[error("capability evidence id is empty")]
    EmptyEvidenceId,
}

impl CapabilityMatrix {
    pub fn new(
        schema_version: u32,
        sing_box_version: impl Into<String>,
        build_tags: Vec<String>,
        entries: Vec<CapabilityEntry>,
    ) -> Result<Self, CapabilityError> {
        let sing_box_version = sing_box_version.into();
        if sing_box_version != "1.13.15" {
            return Err(CapabilityError::UnpinnedVersion);
        }
        for entry in &entries {
            if entry.supported {
                let evidence = entry
                    .evidence
                    .as_ref()
                    .ok_or(CapabilityError::MissingEvidence)?;
                if evidence.id.is_empty() {
                    return Err(CapabilityError::EmptyEvidenceId);
                }
            }
        }
        Ok(Self {
            schema_version,
            sing_box_version,
            build_tags,
            entries,
        })
    }
    pub fn supports(&self, query: &CapabilityQuery) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.supported && entry.query == *query && entry.evidence.is_some())
    }
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }
}

impl Default for CapabilityMatrix {
    fn default() -> Self {
        let baseline = [
            (
                ProxyProtocol::Vless,
                TransportKind::Tcp,
                false,
                false,
                false,
                false,
            ),
            (
                ProxyProtocol::Vless,
                TransportKind::Tcp,
                true,
                false,
                false,
                false,
            ),
            (
                ProxyProtocol::Vless,
                TransportKind::WebSocket,
                true,
                false,
                false,
                false,
            ),
            (
                ProxyProtocol::Vless,
                TransportKind::Http,
                true,
                false,
                false,
                false,
            ),
            (
                ProxyProtocol::Vless,
                TransportKind::HttpUpgrade,
                true,
                false,
                false,
                false,
            ),
            (
                ProxyProtocol::Vless,
                TransportKind::Grpc,
                true,
                false,
                false,
                false,
            ),
            (
                ProxyProtocol::Vless,
                TransportKind::Tcp,
                true,
                true,
                true,
                false,
            ),
            (
                ProxyProtocol::Vmess,
                TransportKind::Tcp,
                false,
                false,
                false,
                false,
            ),
            (
                ProxyProtocol::Vmess,
                TransportKind::Tcp,
                true,
                false,
                false,
                false,
            ),
            (
                ProxyProtocol::Vmess,
                TransportKind::WebSocket,
                true,
                false,
                false,
                false,
            ),
            (
                ProxyProtocol::Vmess,
                TransportKind::Http,
                true,
                false,
                false,
                false,
            ),
            (
                ProxyProtocol::Vmess,
                TransportKind::HttpUpgrade,
                true,
                false,
                false,
                false,
            ),
            (
                ProxyProtocol::Vmess,
                TransportKind::Grpc,
                true,
                false,
                false,
                false,
            ),
            (
                ProxyProtocol::Shadowsocks,
                TransportKind::Tcp,
                false,
                false,
                false,
                false,
            ),
            (
                ProxyProtocol::Trojan,
                TransportKind::Tcp,
                true,
                false,
                false,
                false,
            ),
            (
                ProxyProtocol::Trojan,
                TransportKind::WebSocket,
                true,
                false,
                false,
                false,
            ),
            (
                ProxyProtocol::Trojan,
                TransportKind::Grpc,
                true,
                false,
                false,
                false,
            ),
            (
                ProxyProtocol::Hysteria2,
                TransportKind::Quic,
                true,
                false,
                false,
                true,
            ),
            (
                ProxyProtocol::Tuic,
                TransportKind::Quic,
                true,
                false,
                false,
                true,
            ),
            (
                ProxyProtocol::AnyTls,
                TransportKind::Tcp,
                true,
                false,
                false,
                false,
            ),
        ];
        let entries = baseline
            .into_iter()
            .map(
                |(protocol, transport, tls, reality, utls, udp)| CapabilityEntry {
                    query: CapabilityQuery {
                        protocol,
                        transport,
                        tls,
                        reality,
                        utls,
                        udp,
                        flow: None,
                        plugin: None,
                    },
                    supported: true,
                    evidence: Some(CapabilityEvidence {
                        id: "singbox-1.13.15-baseline".into(),
                        check_fixture: "baseline-outbound".into(),
                        connectivity_fixture: None,
                    }),
                },
            )
            .collect();
        Self::new(1, "1.13.15", vec!["default".into()], entries)
            .expect("baseline capability matrix must be valid")
    }
}
