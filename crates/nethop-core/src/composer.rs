use std::collections::BTreeMap;

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use thiserror::Error;

const MAX_TAG_BYTES: usize = 128;
const MAX_PROTOCOL_BYTES: usize = 32;
const MAX_FIELD_COUNT: usize = 128;
const MAX_FIELD_NAME_BYTES: usize = 64;
const MAX_CONFIG_BYTES: usize = 5 * 1024 * 1024;

const RESERVED_FIELDS: &[&str] = &[
    "inbounds",
    "outbounds",
    "route",
    "dns",
    "experimental",
    "services",
    "log",
    "endpoints",
];

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ComposerError {
    #[error("outbound tag is empty or too long")]
    InvalidTag,
    #[error("outbound protocol is empty or too long")]
    InvalidProtocol,
    #[error("outbound contains reserved field: {0}")]
    ReservedField(String),
    #[error("outbound has too many fields")]
    TooManyFields,
    #[error("outbound field name is too long")]
    FieldNameTooLong,
    #[error("outbound tags must be unique")]
    DuplicateTag,
    #[error("at least one terminal outbound is required")]
    EmptyOutbounds,
    #[error("managed config exceeds the size limit")]
    ConfigTooLarge,
    #[error("managed config serialization failed: {0}")]
    Serialization(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TerminalOutbound {
    tag: String,
    protocol: String,
    fields: BTreeMap<String, Value>,
}

impl TerminalOutbound {
    pub fn new(
        tag: impl Into<String>,
        protocol: impl Into<String>,
        fields: BTreeMap<String, Value>,
    ) -> Result<Self, ComposerError> {
        let tag = tag.into();
        let protocol = protocol.into();
        if tag.is_empty() || tag.len() > MAX_TAG_BYTES {
            return Err(ComposerError::InvalidTag);
        }
        if protocol.is_empty() || protocol.len() > MAX_PROTOCOL_BYTES {
            return Err(ComposerError::InvalidProtocol);
        }
        if fields.len() > MAX_FIELD_COUNT {
            return Err(ComposerError::TooManyFields);
        }
        for key in fields.keys() {
            if key.is_empty() || key.len() > MAX_FIELD_NAME_BYTES {
                return Err(ComposerError::FieldNameTooLong);
            }
            if RESERVED_FIELDS.contains(&key.as_str()) {
                return Err(ComposerError::ReservedField(key.clone()));
            }
        }
        Ok(Self {
            tag,
            protocol,
            fields,
        })
    }

    pub fn tag(&self) -> &str {
        &self.tag
    }

    pub fn protocol(&self) -> &str {
        &self.protocol
    }

    fn to_json(&self) -> Map<String, Value> {
        let mut object: Map<String, Value> = self
            .fields
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        object.insert("tag".into(), Value::String(self.tag.clone()));
        object.insert("type".into(), Value::String(self.protocol.clone()));
        object
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedConfig {
    bytes: Vec<u8>,
    digest: String,
    node_count: usize,
}

impl ManagedConfig {
    pub fn from_outbounds(mut outbounds: Vec<TerminalOutbound>) -> Result<Self, ComposerError> {
        if outbounds.is_empty() {
            return Err(ComposerError::EmptyOutbounds);
        }
        outbounds.sort_by(|left, right| left.tag.cmp(&right.tag));
        if outbounds
            .windows(2)
            .any(|window| window[0].tag == window[1].tag)
        {
            return Err(ComposerError::DuplicateTag);
        }
        let value = serde_json::json!({
            "outbounds": outbounds.iter().map(TerminalOutbound::to_json).collect::<Vec<_>>()
        });
        let bytes = serde_json::to_vec(&value)
            .map_err(|error| ComposerError::Serialization(error.to_string()))?;
        if bytes.len() > MAX_CONFIG_BYTES {
            return Err(ComposerError::ConfigTooLarge);
        }
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let digest = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Ok(Self {
            bytes,
            digest,
            node_count: outbounds.len(),
        })
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn digest_sha256(&self) -> &str {
        &self.digest
    }

    pub const fn node_count(&self) -> usize {
        self.node_count
    }
}
