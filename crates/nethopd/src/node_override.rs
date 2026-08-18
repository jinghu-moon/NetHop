use std::{collections::BTreeMap, fmt, fs, path::PathBuf};

use nethop_core::TerminalOutbound;
use nethop_subscription::adapt_terminal_outbound_value;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::{StableNodeId, worker_config::atomic_write};

const OVERRIDE_SCHEMA: &str = "nethop-node-overrides-v1";
const MAX_FILE_BYTES: u64 = 1024 * 1024;
const MAX_OVERRIDE_BYTES: usize = 64 * 1024;
const MAX_OVERRIDES: usize = 512;
const MAX_DISPLAY_NAME_BYTES: usize = 128;
const TERMINAL_PROTOCOLS: &[&str] = &[
    "anytls",
    "http",
    "hysteria2",
    "shadowsocks",
    "socks",
    "trojan",
    "tuic",
    "vless",
    "vmess",
];

#[derive(Clone, PartialEq)]
pub struct NodeOverride {
    node_id: StableNodeId,
    display_name: String,
    outbound: Value,
}

impl NodeOverride {
    pub fn new(
        node_id: StableNodeId,
        display_name: impl Into<String>,
        mut outbound: Value,
    ) -> Result<Self, NodeOverrideError> {
        let display_name = display_name.into();
        validate_display_name(&display_name)?;
        let object = outbound
            .as_object_mut()
            .ok_or(NodeOverrideError::InvalidOutbound)?;
        if let Some(tag) = object.remove("tag")
            && tag.as_str() != Some(node_id.as_str())
        {
            return Err(NodeOverrideError::InvalidOutbound);
        }
        let protocol = object
            .get("type")
            .and_then(Value::as_str)
            .ok_or(NodeOverrideError::InvalidOutbound)?;
        if !TERMINAL_PROTOCOLS.contains(&protocol) || object.contains_key("detour") {
            return Err(NodeOverrideError::InvalidOutbound);
        }
        validate_endpoint(object)?;
        if serde_json::to_vec(&outbound)
            .map_err(|_| NodeOverrideError::InvalidOutbound)?
            .len()
            > MAX_OVERRIDE_BYTES
        {
            return Err(NodeOverrideError::InvalidOutbound);
        }
        let override_value = Self {
            node_id,
            display_name,
            outbound,
        };
        override_value.terminal_outbound()?;
        Ok(override_value)
    }

    pub const fn node_id(&self) -> &StableNodeId {
        &self.node_id
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub fn protocol(&self) -> &str {
        self.outbound["type"]
            .as_str()
            .expect("validated override protocol")
    }

    pub fn outbound(&self) -> &Value {
        &self.outbound
    }

    pub fn terminal_outbound(&self) -> Result<TerminalOutbound, NodeOverrideError> {
        let mut outbound = self.outbound.clone();
        outbound
            .as_object_mut()
            .expect("validated override object")
            .insert(
                "tag".into(),
                Value::String(self.node_id.as_str().to_owned()),
            );
        adapt_terminal_outbound_value(outbound).map_err(|_| NodeOverrideError::InvalidOutbound)
    }
}

impl fmt::Debug for NodeOverride {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NodeOverride")
            .field("node_id", &self.node_id)
            .field("display_name", &self.display_name)
            .field("outbound", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct NodeOverrideSet {
    entries: BTreeMap<StableNodeId, NodeOverride>,
}

impl NodeOverrideSet {
    pub fn get(&self, node_id: &StableNodeId) -> Option<&NodeOverride> {
        self.entries.get(node_id)
    }

    pub fn upsert(&mut self, value: NodeOverride) -> Result<(), NodeOverrideError> {
        if !self.entries.contains_key(value.node_id()) && self.entries.len() >= MAX_OVERRIDES {
            return Err(NodeOverrideError::LimitExceeded);
        }
        self.entries.insert(value.node_id().clone(), value);
        Ok(())
    }

    pub fn remove(&mut self, node_id: &StableNodeId) -> bool {
        self.entries.remove(node_id).is_some()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn validate(&self) -> Result<(), NodeOverrideError> {
        if self.entries.len() > MAX_OVERRIDES
            || self
                .entries
                .iter()
                .any(|(key, value)| key != value.node_id() || value.terminal_outbound().is_err())
        {
            return Err(NodeOverrideError::InvalidFile);
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct NodeOverrideStore {
    path: PathBuf,
}

impl NodeOverrideStore {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, NodeOverrideError> {
        let path = path.into();
        if !path.is_absolute() || path.file_name().is_none() {
            return Err(NodeOverrideError::InvalidPath);
        }
        let parent = path.parent().ok_or(NodeOverrideError::InvalidPath)?;
        let metadata = fs::symlink_metadata(parent).map_err(|_| NodeOverrideError::InvalidPath)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() || !private_parent(&metadata) {
            return Err(NodeOverrideError::InvalidPath);
        }
        Ok(Self { path })
    }

    pub fn load(&self) -> Result<NodeOverrideSet, NodeOverrideError> {
        let metadata = match fs::symlink_metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(NodeOverrideSet::default());
            }
            Err(_) => return Err(NodeOverrideError::Read),
        };
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() == 0
            || metadata.len() > MAX_FILE_BYTES
            || !private_file(&metadata)
        {
            return Err(NodeOverrideError::InvalidFile);
        }
        let document: StoredDocument =
            serde_json::from_slice(&fs::read(&self.path).map_err(|_| NodeOverrideError::Read)?)
                .map_err(|_| NodeOverrideError::InvalidFile)?;
        if document.schema != OVERRIDE_SCHEMA {
            return Err(NodeOverrideError::InvalidFile);
        }
        let mut result = NodeOverrideSet::default();
        for entry in document.entries {
            let node_id =
                StableNodeId::new(entry.node_id).map_err(|_| NodeOverrideError::InvalidFile)?;
            let value = NodeOverride::new(node_id, entry.display_name, entry.outbound)
                .map_err(|_| NodeOverrideError::InvalidFile)?;
            if result.entries.contains_key(value.node_id()) {
                return Err(NodeOverrideError::InvalidFile);
            }
            result
                .upsert(value)
                .map_err(|_| NodeOverrideError::InvalidFile)?;
        }
        result.validate()?;
        Ok(result)
    }

    pub fn replace(&self, overrides: &NodeOverrideSet) -> Result<(), NodeOverrideError> {
        overrides.validate()?;
        let document = StoredDocument {
            schema: OVERRIDE_SCHEMA.to_owned(),
            entries: overrides
                .entries
                .values()
                .map(|value| StoredOverride {
                    node_id: value.node_id().as_str().to_owned(),
                    display_name: value.display_name().to_owned(),
                    outbound: value.outbound().clone(),
                })
                .collect(),
        };
        let bytes = serde_json::to_vec(&document).map_err(|_| NodeOverrideError::Write)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(NodeOverrideError::LimitExceeded);
        }
        atomic_write(&self.path, &bytes).map_err(|_| NodeOverrideError::Write)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredDocument {
    schema: String,
    entries: Vec<StoredOverride>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredOverride {
    node_id: String,
    display_name: String,
    outbound: Value,
}

fn validate_display_name(value: &str) -> Result<(), NodeOverrideError> {
    if value.is_empty()
        || value.len() > MAX_DISPLAY_NAME_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(NodeOverrideError::InvalidDisplayName);
    }
    Ok(())
}

fn validate_endpoint(object: &serde_json::Map<String, Value>) -> Result<(), NodeOverrideError> {
    let server = object
        .get("server")
        .and_then(Value::as_str)
        .filter(|value| {
            !value.is_empty() && value.len() <= 253 && !value.chars().any(char::is_control)
        })
        .ok_or(NodeOverrideError::InvalidOutbound)?;
    let _ = server;
    let single_port = object
        .get("server_port")
        .and_then(Value::as_u64)
        .is_some_and(|port| (1..=65_535).contains(&port));
    let port_hopping = object
        .get("server_ports")
        .and_then(Value::as_array)
        .is_some_and(|ports| !ports.is_empty() && ports.len() <= 64);
    if !single_port && !port_hopping {
        return Err(NodeOverrideError::InvalidOutbound);
    }
    Ok(())
}

#[cfg(unix)]
fn private_file(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o077 == 0
}

#[cfg(unix)]
fn private_parent(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o077 == 0
}

#[cfg(not(unix))]
fn private_file(_metadata: &fs::Metadata) -> bool {
    true
}

#[cfg(not(unix))]
fn private_parent(_metadata: &fs::Metadata) -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum NodeOverrideError {
    #[error("node override path is invalid")]
    InvalidPath,
    #[error("node override file is invalid")]
    InvalidFile,
    #[error("node override display name is invalid")]
    InvalidDisplayName,
    #[error("node override outbound is invalid")]
    InvalidOutbound,
    #[error("node override limit was exceeded")]
    LimitExceeded,
    #[error("node override could not be read")]
    Read,
    #[error("node override could not be written")]
    Write,
}
