use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};

use nethop_core::{CapturePolicy, GenerationId, RuntimeState};
use nethop_protocol::{ControlMethod, ControlParams};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

use crate::{
    ClashApiClient, ClashApiError, ProcessIdentity, collect_outbound_route, collect_process_metrics,
};

const SELECTOR_SCHEMA_VERSION: u8 = 1;
const AUTO_SELECTOR_TAG: &str = "nethop-auto";
const MAX_SELECTOR_STATE_BYTES: usize = 512;
static TEMP_SEQUENCE: AtomicU32 = AtomicU32::new(0);

pub struct OperationalControl {
    api: ClashApiClient,
    selector_store: SelectorStore,
    diagnostics_path: PathBuf,
    generation_root: Option<PathBuf>,
}

impl OperationalControl {
    pub fn new(
        api: ClashApiClient,
        selector_store: SelectorStore,
        diagnostics_path: impl Into<PathBuf>,
    ) -> Result<Self, OperationalControlError> {
        let diagnostics_path = diagnostics_path.into();
        validate_output_path(&diagnostics_path)?;
        Ok(Self {
            api,
            selector_store,
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
            ControlMethod::NodeList => Ok(json!({
                "nodes": self.api.nodes(params.query_value(), params.limit())?,
            })),
            ControlMethod::NodeTest => Ok(json!(
                self.api.test_node(
                    params
                        .target_value()
                        .expect("protocol validated node target"),
                )?
            )),
            ControlMethod::NodeTestAll => Ok(json!({
                "results": self.api.test_all_nodes()?,
            })),
            ControlMethod::NodeSelect => {
                let target = params
                    .target_value()
                    .expect("protocol validated node target");
                self.api.select_node(target)?;
                self.selector_store.save(target)?;
                Ok(json!({"selected":target,"persisted":true}))
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
                let nodes = self.api.nodes(None, Some(128));
                let connections = self.api.connections(None, Some(128));
                let document = json!({
                    "schema_version": 1,
                    "runtime": {
                        "state": runtime_state_wire(state),
                        "generation": generation.map(GenerationId::get),
                    },
                    "capture": capture_document(policy),
                    "clash_api": {
                        "available": nodes.is_ok() && connections.is_ok(),
                        "node_count": nodes.as_ref().map_or(0, Vec::len),
                        "selected": nodes.as_ref().ok().and_then(|nodes| nodes.iter().find(|node| node.selected)).map(|node| &node.tag),
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
        let nodes = self.api.nodes(None, Some(128));
        let connections = self.api.connections(None, Some(128));
        json!({
            "core_api": if nodes.is_ok() && connections.is_ok() { "available" } else { "unavailable" },
            "selector": {
                "selected": nodes.as_ref().ok().and_then(|nodes| nodes.iter().find(|node| node.selected)).map(|node| &node.tag),
                "candidate_count": nodes.as_ref().map_or(0, Vec::len),
            },
            "active_connection_count": connections.as_ref().map_or(0, Vec::len),
        })
    }

    fn export_node(&self, target: &str) -> Result<Value, OperationalControlError> {
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
                outbounds
                    .iter()
                    .find(|outbound| outbound.get("tag").and_then(Value::as_str) == Some(target))
            })
            .cloned()
            .ok_or(OperationalControlError::UnknownNode)?;
        Ok(json!({"generation":generation,"tag":target,"outbound":outbound}))
    }

    pub fn replay_selection(&mut self) -> Result<ReplayResult, OperationalControlError> {
        let Some(selected) = self.selector_store.load()? else {
            return Ok(ReplayResult::NoSelection);
        };
        match self.api.select_node(&selected) {
            Ok(()) => Ok(ReplayResult::Restored),
            Err(ClashApiError::UnknownTarget) => {
                self.api.select_node(AUTO_SELECTOR_TAG)?;
                self.selector_store.save(AUTO_SELECTOR_TAG)?;
                Ok(ReplayResult::FellBackToAuto)
            }
            Err(error) => Err(error.into()),
        }
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
    NoSelection,
    Restored,
    FellBackToAuto,
}

pub struct SelectorStore {
    path: PathBuf,
}

impl SelectorStore {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, OperationalControlError> {
        let path = path.into();
        validate_output_path(&path)?;
        Ok(Self { path })
    }

    pub fn load(&self) -> Result<Option<String>, OperationalControlError> {
        if !self.path.exists() {
            return Ok(None);
        }
        let metadata =
            fs::symlink_metadata(&self.path).map_err(|_| OperationalControlError::SelectorState)?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() as usize > MAX_SELECTOR_STATE_BYTES
        {
            return Err(OperationalControlError::SelectorState);
        }
        let state: SelectorState = serde_json::from_slice(
            &fs::read(&self.path).map_err(|_| OperationalControlError::SelectorState)?,
        )
        .map_err(|_| OperationalControlError::SelectorState)?;
        if state.schema_version != SELECTOR_SCHEMA_VERSION || !valid_tag(&state.selected_tag) {
            return Err(OperationalControlError::SelectorState);
        }
        Ok(Some(state.selected_tag))
    }

    pub fn save(&self, selected_tag: &str) -> Result<(), OperationalControlError> {
        if !valid_tag(selected_tag) {
            return Err(OperationalControlError::SelectorState);
        }
        publish_json(
            &self.path,
            &SelectorState {
                schema_version: SELECTOR_SCHEMA_VERSION,
                selected_tag: selected_tag.to_owned(),
            },
        )
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectorState {
    schema_version: u8,
    selected_tag: String,
}

fn valid_tag(tag: &str) -> bool {
    !tag.is_empty() && tag.len() <= 128 && !tag.chars().any(char::is_control)
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

#[derive(Debug, Error)]
pub enum OperationalControlError {
    #[error("operational control path is invalid")]
    InvalidPath,
    #[error("selector state is invalid")]
    SelectorState,
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
}
