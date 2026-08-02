use std::{collections::HashSet, fmt, sync::OnceLock};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::protocol::{Credentials, ProxyProtocol, TransportKind, UnvalidatedNode};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityEvidence {
    pub id: String,
    pub check_fixture: String,
    pub connectivity_fixture: Option<String>,
    pub source_paths: Vec<String>,
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
    pub sing_box_tag: String,
    pub sing_box_commit: String,
    pub go_version: String,
    pub build_tags: Vec<String>,
    pub entries: Vec<CapabilityEntry>,
    mapping_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CapabilityError {
    #[error("supported capability requires evidence")]
    MissingEvidence,
    #[error("sing-box version must be pinned")]
    UnpinnedVersion,
    #[error("capability evidence id is empty")]
    EmptyEvidenceId,
    #[error("mapping manifest is invalid")]
    InvalidManifest,
    #[error("mapping manifest identity does not match sing-box 1.13.15")]
    ManifestIdentityMismatch,
    #[error("mapping manifest contains a duplicate or incomplete protocol entry")]
    InvalidProtocolSet,
}

const SING_BOX_VERSION: &str = "1.13.15";
const SING_BOX_TAG: &str = "v1.13.15";
const SING_BOX_COMMIT: &str = "3708fa18766cda1f11b77f6ed9c7bd61688f17df";
const SING_BOX_GO_VERSION: &str = "1.24.7";
const MAPPING_MANIFEST_JSON: &str = include_str!("../manifests/sing-box-1.13.15-mapping.json");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MappingManifest {
    schema_version: u32,
    sing_box_version: String,
    sing_box_tag: String,
    sing_box_commit: String,
    go_version: String,
    build_tags: Vec<String>,
    check_fixture: String,
    protocols: Vec<ProtocolMapping>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProtocolMapping {
    protocol: ProxyProtocol,
    mapped_fields: Vec<String>,
    source_paths: Vec<String>,
    capabilities: Vec<CapabilityShape>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilityShape {
    transport: TransportKind,
    tls: bool,
    reality: bool,
    utls: bool,
    udp: bool,
    flow: Option<String>,
    plugin: Option<String>,
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
            sing_box_tag: SING_BOX_TAG.into(),
            sing_box_commit: SING_BOX_COMMIT.into(),
            go_version: SING_BOX_GO_VERSION.into(),
            build_tags,
            entries,
            mapping_digest: String::new(),
        })
    }

    pub fn from_manifest_json(input: &str) -> Result<Self, CapabilityError> {
        let manifest: MappingManifest =
            serde_json::from_str(input).map_err(|_| CapabilityError::InvalidManifest)?;
        if manifest.schema_version != 1
            || manifest.sing_box_version != SING_BOX_VERSION
            || manifest.sing_box_tag != SING_BOX_TAG
            || manifest.sing_box_commit != SING_BOX_COMMIT
            || manifest.go_version != SING_BOX_GO_VERSION
            || !manifest.build_tags.iter().any(|tag| tag == "with_quic")
            || !manifest.build_tags.iter().any(|tag| tag == "with_utls")
            || manifest.check_fixture.is_empty()
        {
            return Err(CapabilityError::ManifestIdentityMismatch);
        }
        let mut protocols = HashSet::new();
        let mut queries = HashSet::new();
        let mut entries = Vec::new();
        for mapping in manifest.protocols {
            if !protocols.insert(mapping.protocol)
                || mapping.mapped_fields.is_empty()
                || mapping.source_paths.is_empty()
                || mapping.capabilities.is_empty()
                || mapping
                    .source_paths
                    .iter()
                    .any(|path| path.is_empty() || path.starts_with('/') || path.contains(".."))
            {
                return Err(CapabilityError::InvalidProtocolSet);
            }
            for shape in mapping.capabilities {
                let query = CapabilityQuery {
                    protocol: mapping.protocol,
                    transport: shape.transport,
                    tls: shape.tls,
                    reality: shape.reality,
                    utls: shape.utls,
                    udp: shape.udp,
                    flow: shape.flow,
                    plugin: shape.plugin,
                };
                if !queries.insert(query.clone()) {
                    return Err(CapabilityError::InvalidProtocolSet);
                }
                entries.push(CapabilityEntry {
                    query,
                    supported: true,
                    evidence: Some(CapabilityEvidence {
                        id: format!(
                            "sing-box-{}-{}",
                            manifest.sing_box_version,
                            mapping.protocol.as_str()
                        ),
                        check_fixture: manifest.check_fixture.clone(),
                        connectivity_fixture: None,
                        source_paths: mapping.source_paths.clone(),
                    }),
                });
            }
        }
        if protocols.len() != ProxyProtocol::ALL.len()
            || ProxyProtocol::ALL
                .iter()
                .any(|protocol| !protocols.contains(protocol))
        {
            return Err(CapabilityError::InvalidProtocolSet);
        }
        let mapping_digest = hex_digest(input.as_bytes());
        let mut matrix = Self::new(
            manifest.schema_version,
            manifest.sing_box_version,
            manifest.build_tags,
            entries,
        )?;
        matrix.mapping_digest = mapping_digest;
        Ok(matrix)
    }
    pub fn supports(&self, query: &CapabilityQuery) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.supported && entry.query == *query && entry.evidence.is_some())
    }
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }
    pub fn mapping_digest(&self) -> &str {
        &self.mapping_digest
    }
}

impl Default for CapabilityMatrix {
    fn default() -> Self {
        static MATRIX: OnceLock<CapabilityMatrix> = OnceLock::new();
        MATRIX
            .get_or_init(|| {
                Self::from_manifest_json(MAPPING_MANIFEST_JSON)
                    .expect("embedded capability manifest must be valid")
            })
            .clone()
    }
}

fn hex_digest(input: &[u8]) -> String {
    Sha256::digest(input)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
