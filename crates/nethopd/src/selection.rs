use std::{
    collections::{BTreeMap, HashSet},
    fmt, fs,
    path::{Path, PathBuf},
};

use nethop_core::GenerationNodeRegistry;
use nethop_subscription::SourceId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const SELECTION_SNAPSHOT_VERSION: u8 = 1;
const MAX_NODE_NAME_BYTES: usize = 128;
const MAX_PROTOCOL_BYTES: usize = 32;
const MAX_NODE_SOURCES: usize = 16;
const MAX_SELECTION_STORE_BYTES: u64 = 512;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SelectionModelError {
    #[error("stable node ID is invalid")]
    InvalidNodeId,
    #[error("selection snapshot version is unsupported")]
    UnsupportedSnapshot,
    #[error("node list item is outside the bounded model")]
    InvalidNode,
    #[error("selection store is invalid")]
    InvalidStore,
    #[error("selection state could not be written")]
    StoreWrite,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct StableNodeId(String);

impl StableNodeId {
    pub fn new(value: impl Into<String>) -> Result<Self, SelectionModelError> {
        let value = value.into();
        if value.len() != 21
            || !value.starts_with("nh1s-")
            || !value[5..]
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(SelectionModelError::InvalidNodeId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for StableNodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for StableNodeId {
    type Error = SelectionModelError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<StableNodeId> for String {
    fn from(value: StableNodeId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum NodeSelectionIntent {
    Auto,
    Manual { node_id: StableNodeId },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedSelection {
    version: u8,
    intent: NodeSelectionIntent,
    changed_at: u64,
}

pub struct NodeSelectionStore {
    path: PathBuf,
}

impl NodeSelectionStore {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, SelectionModelError> {
        let path = path.into();
        let parent = path.parent().ok_or(SelectionModelError::InvalidStore)?;
        let parent_valid = fs::symlink_metadata(parent)
            .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink());
        if !path.is_absolute() || path.file_name().is_none() || !parent_valid {
            return Err(SelectionModelError::InvalidStore);
        }
        Ok(Self { path })
    }

    pub fn load(&self) -> Result<(NodeSelectionIntent, u64), SelectionModelError> {
        let metadata = match fs::symlink_metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok((NodeSelectionIntent::Auto, 0));
            }
            Err(_) => return Err(SelectionModelError::InvalidStore),
        };
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() == 0
            || metadata.len() > MAX_SELECTION_STORE_BYTES
            || !private_file(&metadata)
        {
            return Err(SelectionModelError::InvalidStore);
        }
        let persisted: PersistedSelection = serde_json::from_slice(
            &fs::read(&self.path).map_err(|_| SelectionModelError::InvalidStore)?,
        )
        .map_err(|_| SelectionModelError::InvalidStore)?;
        if persisted.version != SELECTION_SNAPSHOT_VERSION {
            return Err(SelectionModelError::UnsupportedSnapshot);
        }
        Ok((persisted.intent, persisted.changed_at))
    }

    pub fn save(
        &self,
        intent: &NodeSelectionIntent,
        changed_at: u64,
    ) -> Result<(), SelectionModelError> {
        let bytes = serde_json::to_vec(&PersistedSelection {
            version: SELECTION_SNAPSHOT_VERSION,
            intent: intent.clone(),
            changed_at,
        })
        .map_err(|_| SelectionModelError::StoreWrite)?;
        crate::worker_config::atomic_write(&self.path, &bytes)
            .map_err(|_| SelectionModelError::StoreWrite)
    }

    pub fn reset_auto(&self, changed_at: u64) -> Result<(), SelectionModelError> {
        self.save(&NodeSelectionIntent::Auto, changed_at)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(unix)]
fn private_file(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o077 == 0
}

#[cfg(not(unix))]
fn private_file(_metadata: &fs::Metadata) -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupState {
    tag: String,
    now: Option<String>,
    all: Vec<String>,
}

impl GroupState {
    pub fn new(
        tag: impl Into<String>,
        now: Option<String>,
        all: Vec<String>,
    ) -> Result<Self, SelectionModelError> {
        let tag = tag.into();
        if !valid_internal_tag(&tag)
            || now
                .as_deref()
                .is_some_and(|value| !valid_internal_tag(value))
            || all.len() > 2_000
            || all.iter().any(|value| !valid_internal_tag(value))
        {
            return Err(SelectionModelError::InvalidNode);
        }
        Ok(Self { tag, now, all })
    }

    pub fn tag(&self) -> &str {
        &self.tag
    }

    pub fn now(&self) -> Option<&str> {
        self.now.as_deref()
    }

    pub fn all(&self) -> &[String] {
        &self.all
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveTerminal {
    Node(StableNodeId),
    Direct,
    Block,
    Unresolved(SelectionDiagnosticCode),
}

impl ActiveTerminal {
    pub const fn degraded_reason(&self) -> Option<SelectionDiagnosticCode> {
        match self {
            Self::Unresolved(code) => Some(*code),
            Self::Node(_) | Self::Direct | Self::Block => None,
        }
    }
}

pub fn resolve_active_terminal(
    root: &str,
    groups: &BTreeMap<String, GroupState>,
    registry: &GenerationNodeRegistry,
) -> ActiveTerminal {
    let Some(current) = groups.get(root).and_then(GroupState::now) else {
        return ActiveTerminal::Unresolved(SelectionDiagnosticCode::ActiveNodeUnresolved);
    };
    if current == "direct" {
        return ActiveTerminal::Direct;
    }
    if current == "block" {
        return ActiveTerminal::Block;
    }
    registry.by_internal_tag(current).map_or(
        ActiveTerminal::Unresolved(SelectionDiagnosticCode::ActiveNodeUnresolved),
        |record| {
            StableNodeId::new(record.stable_node_id()).map_or(
                ActiveTerminal::Unresolved(SelectionDiagnosticCode::ActiveNodeUnresolved),
                ActiveTerminal::Node,
            )
        },
    )
}

fn valid_internal_tag(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeSelectionSnapshot {
    version: u8,
    intent: NodeSelectionIntent,
    active_node_id: Option<StableNodeId>,
    changed_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    degraded_reason: Option<SelectionDiagnosticCode>,
}

impl NodeSelectionSnapshot {
    pub fn new(
        intent: NodeSelectionIntent,
        active_node_id: Option<StableNodeId>,
        changed_at: u64,
    ) -> Self {
        Self {
            version: SELECTION_SNAPSHOT_VERSION,
            intent,
            active_node_id,
            changed_at,
            degraded_reason: None,
        }
    }

    pub const fn version(&self) -> u8 {
        self.version
    }

    pub const fn intent(&self) -> &NodeSelectionIntent {
        &self.intent
    }

    pub const fn active_node_id(&self) -> Option<&StableNodeId> {
        self.active_node_id.as_ref()
    }

    pub const fn changed_at(&self) -> u64 {
        self.changed_at
    }

    pub const fn degraded_reason(&self) -> Option<SelectionDiagnosticCode> {
        self.degraded_reason
    }

    pub fn with_degraded_reason(mut self, reason: Option<SelectionDiagnosticCode>) -> Self {
        self.degraded_reason = reason;
        self
    }

    pub fn validate(&self) -> Result<(), SelectionModelError> {
        if self.version != SELECTION_SNAPSHOT_VERSION {
            return Err(SelectionModelError::UnsupportedSnapshot);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeListItem {
    id: StableNodeId,
    name: String,
    protocol: String,
    source_ids: Vec<SourceId>,
    latency_ms: Option<u32>,
    alive: Option<bool>,
    is_requested: bool,
    is_active: bool,
}

impl NodeListItem {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: StableNodeId,
        name: impl Into<String>,
        protocol: impl Into<String>,
        source_ids: Vec<SourceId>,
        latency_ms: Option<u32>,
        alive: Option<bool>,
        is_requested: bool,
        is_active: bool,
    ) -> Result<Self, SelectionModelError> {
        let name = name.into();
        let protocol = protocol.into();
        let unique = source_ids.iter().collect::<HashSet<_>>();
        if name.is_empty()
            || name.len() > MAX_NODE_NAME_BYTES
            || name.chars().any(char::is_control)
            || protocol.is_empty()
            || protocol.len() > MAX_PROTOCOL_BYTES
            || !protocol
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
            || source_ids.is_empty()
            || source_ids.len() > MAX_NODE_SOURCES
            || unique.len() != source_ids.len()
        {
            return Err(SelectionModelError::InvalidNode);
        }
        Ok(Self {
            id,
            name,
            protocol,
            source_ids,
            latency_ms,
            alive,
            is_requested,
            is_active,
        })
    }

    pub const fn id(&self) -> &StableNodeId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn protocol(&self) -> &str {
        &self.protocol
    }

    pub fn source_ids(&self) -> &[SourceId] {
        &self.source_ids
    }

    pub const fn is_requested(&self) -> bool {
        self.is_requested
    }

    pub const fn is_active(&self) -> bool {
        self.is_active
    }

    pub(crate) fn set_observation(&mut self, latency_ms: Option<u32>, alive: Option<bool>) {
        self.latency_ms = latency_ms;
        self.alive = alive;
    }
}

pub fn join_node_snapshot(
    registry: &GenerationNodeRegistry,
    intent: NodeSelectionIntent,
    active: ActiveTerminal,
    changed_at: u64,
) -> Result<NodeListSnapshot, SelectionModelError> {
    let requested = match &intent {
        NodeSelectionIntent::Auto => None,
        NodeSelectionIntent::Manual { node_id } => Some(node_id),
    };
    let degraded_reason = active.degraded_reason();
    let active_node_id = match active {
        ActiveTerminal::Node(node_id) => Some(node_id),
        ActiveTerminal::Direct | ActiveTerminal::Block | ActiveTerminal::Unresolved(_) => None,
    };
    let mut nodes = Vec::with_capacity(registry.records().len());
    for record in registry.records() {
        let id = StableNodeId::new(record.stable_node_id())?;
        let source_ids = record
            .source_ids()
            .iter()
            .map(|value| SourceId::new(value.clone()).map_err(|_| SelectionModelError::InvalidNode))
            .collect::<Result<Vec<_>, _>>()?;
        nodes.push(NodeListItem::new(
            id.clone(),
            record.display_name(),
            record.protocol(),
            source_ids,
            None,
            None,
            requested == Some(&id),
            active_node_id.as_ref() == Some(&id),
        )?);
    }
    Ok(NodeListSnapshot::new(
        nodes,
        NodeSelectionSnapshot::new(intent, active_node_id, changed_at)
            .with_degraded_reason(degraded_reason),
    ))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeListSnapshot {
    nodes: Vec<NodeListItem>,
    selection: NodeSelectionSnapshot,
}

impl NodeListSnapshot {
    pub fn new(nodes: Vec<NodeListItem>, selection: NodeSelectionSnapshot) -> Self {
        Self { nodes, selection }
    }

    pub fn nodes(&self) -> &[NodeListItem] {
        &self.nodes
    }

    pub const fn selection(&self) -> &NodeSelectionSnapshot {
        &self.selection
    }

    pub(crate) fn nodes_mut(&mut self) -> &mut [NodeListItem] {
        &mut self.nodes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING-KEBAB-CASE")]
pub enum SelectionDiagnosticCode {
    #[serde(rename = "NH-SUB-MODE-MISMATCH")]
    SubscriptionModeMismatch,
    #[serde(rename = "NH-SUB-SINGLE-NOT-UNIQUE")]
    SingleSourceNotUnique,
    #[serde(rename = "NH-SUB-NO-ACTIVE-SOURCE")]
    NoActiveSource,
    #[serde(rename = "NH-SUB-LAST-ACTIVE")]
    LastActiveSource,
    #[serde(rename = "NH-SUB-TARGET-NOT-READY")]
    TargetNotReady,
    #[serde(rename = "NH-SUB-MODE-TARGET-REQUIRED")]
    ModeTargetRequired,
    #[serde(rename = "NH-NODE-SELECTION-STALE")]
    NodeSelectionStale,
    #[serde(rename = "NH-NODE-ACTIVE-UNRESOLVED")]
    ActiveNodeUnresolved,
    #[serde(rename = "NH-NODE-TEST-PARTIAL")]
    NodeTestPartial,
}
