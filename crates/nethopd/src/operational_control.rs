use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};

use nethop_core::{CapturePolicy, GenerationId, GenerationNodeRegistry, RuntimeState};
use nethop_protocol::{ControlMethod, ControlParams, ErrorDomain};
use serde::Serialize;
use serde_json::{Value, json};
use thiserror::Error;

use crate::{
    ClashApiClient, ClashApiError, ClashGroupSnapshot, NodeListSnapshot, NodeSelectionIntent,
    NodeSelectionStore, ProcessIdentity, SelectionModelError, StableNodeId, collect_outbound_route,
    collect_process_metrics, join_node_snapshot, resolve_active_terminal,
};

const AUTO_SELECTOR_TAG: &str = "nethop-auto";
static TEMP_SEQUENCE: AtomicU32 = AtomicU32::new(0);

pub struct OperationalControl {
    api: ClashApiClient,
    selection_store: NodeSelectionStore,
    diagnostics_path: PathBuf,
    generation_root: Option<PathBuf>,
}

impl OperationalControl {
    pub fn new(
        api: ClashApiClient,
        selection_store: NodeSelectionStore,
        diagnostics_path: impl Into<PathBuf>,
    ) -> Result<Self, OperationalControlError> {
        let diagnostics_path = diagnostics_path.into();
        validate_output_path(&diagnostics_path)?;
        Ok(Self {
            api,
            selection_store,
            diagnostics_path,
            generation_root: None,
        })
    }

    pub fn with_generation_root(
        mut self,
        root: impl Into<PathBuf>,
    ) -> Result<Self, OperationalControlError> {
        let root = root.into();
        let metadata =
            fs::symlink_metadata(&root).map_err(|_| OperationalControlError::InvalidPath)?;
        if !root.is_absolute() || !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(OperationalControlError::InvalidPath);
        }
        self.generation_root = Some(root);
        Ok(self)
    }

    pub fn handle(
        &mut self,
        method: ControlMethod,
        params: &ControlParams,
        state: RuntimeState,
        generation: Option<GenerationId>,
        policy: &CapturePolicy,
    ) -> Result<Value, OperationalControlError> {
        match method {
            ControlMethod::NodeList => Ok(serde_json::to_value(self.node_snapshot()?)?),
            ControlMethod::NodeTest => {
                let node_id = StableNodeId::new(
                    params
                        .target_value()
                        .expect("protocol validated node target"),
                )?;
                let registry = self.current_registry()?;
                let record = registry
                    .by_stable_id(node_id.as_str())
                    .ok_or(OperationalControlError::UnknownNode)?;
                let result = self.api.test_node(record.internal_tag())?;
                Ok(json!({"id":node_id,"latency_ms":result.delay_ms}))
            }
            ControlMethod::NodeTestAll => {
                let registry = self.current_registry()?;
                let results = self
                    .api
                    .test_all_nodes()?
                    .into_iter()
                    .filter_map(|result| {
                        registry.by_internal_tag(&result.tag).map(|record| {
                            json!({
                                "id": record.stable_node_id(),
                                "latency_ms": result.delay_ms,
                            })
                        })
                    })
                    .collect::<Vec<_>>();
                let selection = self.node_snapshot()?.selection().clone();
                Ok(json!({"results":results,"selection":selection}))
            }
            ControlMethod::NodeSelectManual => {
                let node_id = StableNodeId::new(
                    params
                        .target_value()
                        .expect("protocol validated node target"),
                )?;
                let registry = self.current_registry()?;
                let record = registry
                    .by_stable_id(node_id.as_str())
                    .ok_or(OperationalControlError::UnknownNode)?;
                let snapshot = self.api.group_snapshot()?;
                let selectable = snapshot.groups().get("nethop-select").is_some_and(|group| {
                    group.all().iter().any(|tag| tag == record.internal_tag())
                });
                if !selectable {
                    return Err(OperationalControlError::UnknownNode);
                }
                self.api.select_manual_tag(record.internal_tag())?;
                self.selection_store
                    .save(&NodeSelectionIntent::Manual { node_id }, unix_seconds())?;
                Ok(serde_json::to_value(self.node_snapshot()?.selection())?)
            }
            ControlMethod::NodeSelectAuto => {
                self.api.select_auto()?;
                self.selection_store
                    .save(&NodeSelectionIntent::Auto, unix_seconds())?;
                Ok(serde_json::to_value(self.node_snapshot()?.selection())?)
            }
            ControlMethod::NodeSelectionGet => {
                Ok(serde_json::to_value(self.node_snapshot()?.selection())?)
            }
            ControlMethod::NodeExport => {
                let target = params
                    .target_value()
                    .expect("protocol validated node target");
                self.export_node(target)
            }
            ControlMethod::ConnectionsGet => Ok(json!({
                "connections": self.api.connections(params.query_value(), params.limit())?,
            })),
            ControlMethod::ConnectionClose => {
                let target = params
                    .target_value()
                    .expect("protocol validated connection target");
                self.api.close_connection(target)?;
                Ok(json!({"closed":true,"id":target}))
            }
            ControlMethod::ConnectionsCloseAll => {
                self.api.close_all_connections()?;
                Ok(json!({"closed_all":true}))
            }
            ControlMethod::TrafficGet => Ok(json!({
                "sample": self.api.traffic_sample()?,
                "interval_seconds": 1,
            })),
            ControlMethod::TopologyGet => Ok(json!({
                "runtime_state": runtime_state_wire(state),
                "generation": generation.map(GenerationId::get),
                "capture": capture_document(policy),
                "operational": self.status_document(),
            })),
            ControlMethod::DiagnosticsBundle => {
                let core = self.api.group_snapshot();
                let connections = self.api.connections(None, Some(128));
                let selection = core
                    .as_ref()
                    .ok()
                    .and_then(|core| self.node_snapshot_from_core(core).ok());
                let document = json!({
                    "schema_version": 1,
                    "runtime": {
                        "state": runtime_state_wire(state),
                        "generation": generation.map(GenerationId::get),
                    },
                    "capture": capture_document(policy),
                    "clash_api": {
                        "available": core.is_ok() && connections.is_ok(),
                        "node_count": selection.as_ref().map_or(0, |snapshot| snapshot.nodes().len()),
                        "active_node_id": selection.as_ref().and_then(|snapshot| snapshot.selection().active_node_id()),
                        "degraded_reason": selection.as_ref().and_then(|snapshot| snapshot.selection().degraded_reason()),
                        "active_connection_count": connections.as_ref().map_or(0, Vec::len),
                    },
                });
                publish_json(&self.diagnostics_path, &document)?;
                Ok(json!({
                    "bundle": document,
                    "path": self.diagnostics_path,
                }))
            }
            _ => Err(OperationalControlError::UnsupportedMethod),
        }
    }

    pub fn status_document(&mut self) -> Value {
        let core = self.api.group_snapshot();
        let connections = self.api.connections(None, Some(128));
        let selection = core
            .as_ref()
            .ok()
            .and_then(|core| self.node_snapshot_from_core(core).ok());
        json!({
            "core_api": if core.is_ok() && connections.is_ok() { "available" } else { "unavailable" },
            "selector": {
                "intent": selection.as_ref().map(|snapshot| snapshot.selection().intent()),
                "active_node_id": selection.as_ref().and_then(|snapshot| snapshot.selection().active_node_id()),
                "degraded_reason": selection.as_ref().and_then(|snapshot| snapshot.selection().degraded_reason()),
                "candidate_count": selection.as_ref().map_or(0, |snapshot| snapshot.nodes().len()),
            },
            "active_connection_count": connections.as_ref().map_or(0, Vec::len),
        })
    }

    fn export_node(&self, target: &str) -> Result<Value, OperationalControlError> {
        let internal_tag = self
            .current_registry()?
            .by_stable_id(target)
            .map(|record| record.internal_tag().to_owned())
            .ok_or(OperationalControlError::UnknownNode)?;
        let root = self
            .generation_root
            .as_ref()
            .ok_or(OperationalControlError::GenerationUnavailable)?;
        let current = root.join("current");
        let metadata = fs::symlink_metadata(&current)
            .map_err(|_| OperationalControlError::GenerationUnavailable)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 32 {
            return Err(OperationalControlError::GenerationUnavailable);
        }
        let generation = fs::read_to_string(&current)
            .map_err(|_| OperationalControlError::GenerationUnavailable)?
            .trim()
            .parse::<u64>()
            .map_err(|_| OperationalControlError::GenerationUnavailable)?;
        let directory = root.join(generation.to_string());
        let directory_meta = fs::symlink_metadata(&directory)
            .map_err(|_| OperationalControlError::GenerationUnavailable)?;
        if !directory_meta.is_dir() || directory_meta.file_type().is_symlink() {
            return Err(OperationalControlError::GenerationUnavailable);
        }
        let config = directory.join("config.json");
        let config_meta = fs::symlink_metadata(&config)
            .map_err(|_| OperationalControlError::GenerationUnavailable)?;
        if !config_meta.is_file()
            || config_meta.file_type().is_symlink()
            || config_meta.len() > 5 * 1024 * 1024
        {
            return Err(OperationalControlError::GenerationUnavailable);
        }
        let document: Value = serde_json::from_slice(
            &fs::read(config).map_err(|_| OperationalControlError::GenerationUnavailable)?,
        )
        .map_err(|_| OperationalControlError::GenerationUnavailable)?;
        let outbound = document
            .get("outbounds")
            .and_then(Value::as_array)
            .and_then(|outbounds| {
                outbounds.iter().find(|outbound| {
                    outbound.get("tag").and_then(Value::as_str) == Some(internal_tag.as_str())
                })
            })
            .cloned()
            .ok_or(OperationalControlError::UnknownNode)?;
        Ok(json!({"generation":generation,"node_id":target,"outbound":outbound}))
    }

    pub fn replay_selection(&mut self) -> Result<ReplayResult, OperationalControlError> {
        let (intent, _) = self.selection_store.load()?;
        match intent {
            NodeSelectionIntent::Auto => {
                self.api.select_node(AUTO_SELECTOR_TAG)?;
                Ok(ReplayResult::Restored)
            }
            NodeSelectionIntent::Manual { node_id } => {
                let registry = self.current_registry()?;
                let Some(record) = registry.by_stable_id(node_id.as_str()) else {
                    self.api.select_auto()?;
                    self.selection_store.reset_auto(unix_seconds())?;
                    return Ok(ReplayResult::FellBackToAuto);
                };
                match self.api.select_manual_tag(record.internal_tag()) {
                    Ok(()) => Ok(ReplayResult::Restored),
                    Err(ClashApiError::UnknownTarget) => {
                        self.api.select_auto()?;
                        self.selection_store.reset_auto(unix_seconds())?;
                        Ok(ReplayResult::FellBackToAuto)
                    }
                    Err(error) => Err(error.into()),
                }
            }
        }
    }

    fn node_snapshot(&self) -> Result<NodeListSnapshot, OperationalControlError> {
        let core = self.api.group_snapshot()?;
        self.node_snapshot_from_core(&core)
    }

    fn node_snapshot_from_core(
        &self,
        core: &ClashGroupSnapshot,
    ) -> Result<NodeListSnapshot, OperationalControlError> {
        let registry = self.current_registry()?;
        let (intent, changed_at) = self.selection_store.load()?;
        let active = resolve_active_terminal("nethop-select", core.groups(), &registry);
        let mut snapshot = join_node_snapshot(&registry, intent, active, changed_at)?;
        for node in snapshot.nodes_mut() {
            if let Some(record) = registry.by_stable_id(node.id().as_str())
                && let Some(observation) = core.terminal(record.internal_tag())
            {
                node.set_observation(observation.latency_ms(), observation.alive());
            }
        }
        Ok(snapshot)
    }

    fn current_registry(&self) -> Result<GenerationNodeRegistry, OperationalControlError> {
        let root = self
            .generation_root
            .as_ref()
            .ok_or(OperationalControlError::GenerationUnavailable)?;
        let current = controlled_current_generation(root)?;
        let path = root.join(current.to_string()).join("nodes.json");
        let metadata = fs::symlink_metadata(&path)
            .map_err(|_| OperationalControlError::GenerationUnavailable)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 1024 * 1024
        {
            return Err(OperationalControlError::GenerationUnavailable);
        }
        serde_json::from_slice(
            &fs::read(path).map_err(|_| OperationalControlError::GenerationUnavailable)?,
        )
        .map_err(|_| OperationalControlError::GenerationUnavailable)
    }

    pub fn metrics_document(
        &self,
        process: Option<ProcessIdentity>,
        daemon_uptime: Duration,
        state: RuntimeState,
        generation: Option<GenerationId>,
    ) -> Value {
        let totals = self.api.traffic_totals().ok();
        json!({
            "schema_version": 1,
            "runtime_state": runtime_state_wire(state),
            "generation": generation.map(GenerationId::get),
            "uptime_seconds": daemon_uptime.as_secs(),
            "core": process.map(collect_process_metrics),
            "traffic": {
                "upload_bytes": totals.map(|value| value.upload),
                "download_bytes": totals.map(|value| value.download),
            },
            "outbound": collect_outbound_route(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayResult {
    Restored,
    FellBackToAuto,
}

fn validate_output_path(path: &Path) -> Result<(), OperationalControlError> {
    let parent = path.parent().ok_or(OperationalControlError::InvalidPath)?;
    if !path.is_absolute() || path.file_name().is_none() {
        return Err(OperationalControlError::InvalidPath);
    }
    let metadata =
        fs::symlink_metadata(parent).map_err(|_| OperationalControlError::InvalidPath)?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || parent
            .canonicalize()
            .map_err(|_| OperationalControlError::InvalidPath)?
            != parent
    {
        return Err(OperationalControlError::InvalidPath);
    }
    Ok(())
}

fn publish_json(path: &Path, value: &impl Serialize) -> Result<(), OperationalControlError> {
    let bytes = serde_json::to_vec(value).map_err(|_| OperationalControlError::Write)?;
    let parent = path.parent().ok_or(OperationalControlError::InvalidPath)?;
    let temporary = parent.join(format!(
        ".nethop-operation-{}-{}.tmp",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| OperationalControlError::Write)?;
        set_private_file(&file)?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| OperationalControlError::Write)?;
        drop(file);
        #[cfg(windows)]
        if path.exists() {
            fs::remove_file(path).map_err(|_| OperationalControlError::Write)?;
        }
        fs::rename(&temporary, path).map_err(|_| OperationalControlError::Write)?;
        sync_parent(parent)
    })();
    let _ = fs::remove_file(temporary);
    result
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> Result<(), OperationalControlError> {
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| OperationalControlError::Write)
}

#[cfg(not(unix))]
fn sync_parent(_parent: &Path) -> Result<(), OperationalControlError> {
    Ok(())
}

#[cfg(unix)]
fn set_private_file(file: &File) -> Result<(), OperationalControlError> {
    use std::os::unix::fs::PermissionsExt;

    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|_| OperationalControlError::Write)
}

#[cfg(not(unix))]
fn set_private_file(_file: &File) -> Result<(), OperationalControlError> {
    Ok(())
}

pub(crate) fn capture_document(policy: &CapturePolicy) -> Value {
    json!({
        "mode": format!("{:?}", policy.mode()).to_lowercase(),
        "tcp": policy.proxy_tcp(),
        "udp": policy.proxy_udp(),
        "ipv6_guard": policy.ipv6_guard(),
        "inbound_port": policy.inbound_port(),
        "include_uid_count": policy.include_uids().len(),
        "exclude_uid_count": policy.exclude_uids().len(),
    })
}

fn runtime_state_wire(state: RuntimeState) -> &'static str {
    match state {
        RuntimeState::Init => "init",
        RuntimeState::Probing => "probing",
        RuntimeState::StartingCore => "starting_core",
        RuntimeState::RunningTproxy => "running_tproxy",
        RuntimeState::StartingTun => "starting_tun",
        RuntimeState::RunningTun => "running_tun",
        RuntimeState::Degraded => "degraded",
        RuntimeState::FailOpenDirect => "fail_open_direct",
        RuntimeState::Backoff => "backoff",
        RuntimeState::CircuitOpen => "circuit_open",
        RuntimeState::Stopping => "stopping",
    }
}

fn controlled_current_generation(root: &Path) -> Result<u64, OperationalControlError> {
    let path = root.join("current");
    let metadata =
        fs::symlink_metadata(&path).map_err(|_| OperationalControlError::GenerationUnavailable)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 32 {
        return Err(OperationalControlError::GenerationUnavailable);
    }
    fs::read_to_string(path)
        .map_err(|_| OperationalControlError::GenerationUnavailable)?
        .trim()
        .parse::<u64>()
        .ok()
        .filter(|generation| *generation > 0)
        .ok_or(OperationalControlError::GenerationUnavailable)
}

fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug, Error)]
pub enum OperationalControlError {
    #[error("operational control path is invalid")]
    InvalidPath,
    #[error("operational state could not be written atomically")]
    Write,
    #[error("operational method is unsupported")]
    UnsupportedMethod,
    #[error("active generation is unavailable for node export")]
    GenerationUnavailable,
    #[error("requested node does not exist in the active generation")]
    UnknownNode,
    #[error("Clash API operation failed")]
    ClashApi(#[from] ClashApiError),
    #[error("node selection model operation failed")]
    Selection(#[from] SelectionModelError),
    #[error("operational response serialization failed")]
    Json(#[from] serde_json::Error),
}

impl OperationalControlError {
    pub const fn control_diagnostic(&self) -> (ErrorDomain, &'static str) {
        match self {
            Self::UnknownNode | Self::ClashApi(ClashApiError::UnknownTarget) => {
                (ErrorDomain::Node, "SELECTION-STALE")
            }
            Self::GenerationUnavailable => (ErrorDomain::Node, "ACTIVE-UNRESOLVED"),
            Self::Selection(SelectionModelError::InvalidNodeId) => {
                (ErrorDomain::Node, "INVALID-ID")
            }
            Self::Selection(_) => (ErrorDomain::Node, "SELECTION-STATE"),
            Self::ClashApi(ClashApiError::Unavailable) => {
                (ErrorDomain::Core, "CONTROL-UNAVAILABLE")
            }
            Self::ClashApi(ClashApiError::Rejected) => (ErrorDomain::Core, "CONTROL-REJECTED"),
            Self::ClashApi(
                ClashApiError::ResponseTooLarge
                | ClashApiError::InvalidResponse
                | ClashApiError::Json(_),
            ) => (ErrorDomain::Core, "CONTROL-INVALID-RESPONSE"),
            Self::ClashApi(
                ClashApiError::InvalidEndpoint
                | ClashApiError::InvalidLimits
                | ClashApiError::InvalidRequest,
            )
            | Self::InvalidPath
            | Self::UnsupportedMethod
            | Self::Json(_) => (ErrorDomain::Core, "CONTROL-INVALID"),
            Self::Write => (ErrorDomain::Core, "CONTROL-WRITE-FAILED"),
        }
    }
}
