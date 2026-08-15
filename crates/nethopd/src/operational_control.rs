use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, Instant},
};

use nethop_core::{CapturePolicy, GenerationId, GenerationNodeRegistry, RuntimeState};
use nethop_protocol::{BenchmarkControlTiming, ControlMethod, ControlParams, ErrorDomain};
use serde::Serialize;
use serde_json::{Value, json};
use thiserror::Error;

use crate::{
    AutoSelectionDecision, BenchmarkCandidate, BenchmarkEndpoint, BenchmarkError, BenchmarkReport,
    BenchmarkTrigger, ClashApiClient, ClashApiError, ClashGroupSnapshot, NodeListSnapshot,
    NodeSelectionIntent, NodeSelectionStore, ProcessIdentity, SelectionModelError, StableNodeId,
    choose_auto_target, collect_outbound_route, collect_process_metrics, join_node_snapshot,
    resolve_active_terminal,
};

static TEMP_SEQUENCE: AtomicU32 = AtomicU32::new(0);

fn duration_us(value: Duration) -> u64 {
    u64::try_from(value.as_micros()).unwrap_or(u64::MAX)
}

fn measure_result<T, E>(slot: &mut u64, operation: impl FnOnce() -> Result<T, E>) -> Result<T, E> {
    let started = Instant::now();
    let result = operation();
    *slot = duration_us(started.elapsed());
    result
}

fn measure_result_accumulated<T, E>(
    slot: &mut u64,
    operation: impl FnOnce() -> Result<T, E>,
) -> Result<T, E> {
    let started = Instant::now();
    let result = operation();
    *slot = slot.saturating_add(duration_us(started.elapsed()));
    result
}

pub(crate) enum FastSelectionContext {
    Auto { current_node_id: Option<String> },
    Manual,
}

pub(crate) enum FastSelectionApply {
    Switched(Value),
    Kept,
    NotApplicable,
}

pub(crate) struct FastSelectionInput<'a> {
    pub outcomes: &'a [nethop_protocol::NodeProbeOutcome],
    pub ordered_node_ids: &'a [String],
    pub current_node_id: Option<&'a str>,
    pub tolerance_ms: u32,
    pub deadline: Instant,
}

#[derive(Debug)]
pub(crate) struct BenchmarkPlan {
    pub endpoint: BenchmarkEndpoint,
    pub candidates: Vec<BenchmarkCandidate>,
    pub ordered_node_ids: Vec<String>,
    pub trigger: BenchmarkTrigger,
    pub generation: u64,
}

pub struct OperationalControl {
    api: ClashApiClient,
    selection_store: NodeSelectionStore,
    diagnostics_path: PathBuf,
    generation_root: Option<PathBuf>,
}

impl OperationalControl {
    pub(crate) fn selection_is_auto(&self) -> bool {
        self.selection_store
            .load()
            .is_ok_and(|(intent, _)| matches!(intent, NodeSelectionIntent::Auto))
    }

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
            ControlMethod::NodeTestAll | ControlMethod::NodeTestOperationGet => {
                Err(OperationalControlError::UnsupportedMethod)
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
                        "active_terminal": selection.as_ref().map(|snapshot| snapshot.selection().active_terminal()),
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
                "active_terminal": selection.as_ref().map(|snapshot| snapshot.selection().active_terminal()),
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
        let (intent, _) = match self.selection_store.load() {
            Ok(selection) => selection,
            Err(SelectionModelError::UnsupportedSnapshot | SelectionModelError::InvalidStore) => {
                self.selection_store.reset_auto(unix_seconds())?;
                return Ok(ReplayResult::FellBackToAuto);
            }
            Err(error) => return Err(error.into()),
        };
        match intent {
            NodeSelectionIntent::Auto => Ok(ReplayResult::Restored),
            NodeSelectionIntent::Manual { node_id } => {
                let registry = self.current_registry()?;
                let Some(record) = registry.by_stable_id(node_id.as_str()) else {
                    self.selection_store.reset_auto(unix_seconds())?;
                    return Ok(ReplayResult::FellBackToAuto);
                };
                match self.api.select_manual_tag(record.internal_tag()) {
                    Ok(()) => Ok(ReplayResult::Restored),
                    Err(ClashApiError::UnknownTarget) => {
                        self.selection_store.reset_auto(unix_seconds())?;
                        Ok(ReplayResult::FellBackToAuto)
                    }
                    Err(error) => Err(error.into()),
                }
            }
        }
    }

    pub(crate) fn benchmark_plan(
        &self,
        trigger: BenchmarkTrigger,
        generation: u64,
    ) -> Result<BenchmarkPlan, OperationalControlError> {
        let registry = self.current_registry()?;
        let ordered_node_ids = registry.auto_pool().to_vec();
        let candidates = ordered_node_ids
            .iter()
            .map(|node_id| {
                let record = registry
                    .by_stable_id(node_id)
                    .ok_or(OperationalControlError::GenerationUnavailable)?;
                BenchmarkCandidate::new(node_id.clone(), record.internal_tag())
                    .map_err(OperationalControlError::Benchmark)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(BenchmarkPlan {
            endpoint: self.api.benchmark_endpoint()?,
            candidates,
            ordered_node_ids,
            trigger,
            generation,
        })
    }

    #[cfg(any(test, feature = "benchmark-evidence"))]
    pub(crate) fn complete_benchmark(
        &mut self,
        report: &BenchmarkReport,
        ordered_node_ids: &[String],
        tolerance_ms: u32,
        deadline: Instant,
    ) -> Result<Value, OperationalControlError> {
        let mut timing = BenchmarkControlTiming::zero();
        self.complete_benchmark_timed(
            report,
            ordered_node_ids,
            tolerance_ms,
            deadline,
            &mut timing,
        )
    }

    #[cfg(any(test, feature = "benchmark-evidence"))]
    pub(crate) fn complete_benchmark_timed(
        &mut self,
        report: &BenchmarkReport,
        ordered_node_ids: &[String],
        tolerance_ms: u32,
        deadline: Instant,
        timing: &mut BenchmarkControlTiming,
    ) -> Result<Value, OperationalControlError> {
        self.complete_benchmark_with_commit_timed(
            report,
            ordered_node_ids,
            tolerance_ms,
            deadline,
            false,
            timing,
        )
    }

    pub(crate) fn complete_benchmark_with_commit_timed(
        &mut self,
        report: &BenchmarkReport,
        ordered_node_ids: &[String],
        tolerance_ms: u32,
        deadline: Instant,
        mutation_committed: bool,
        timing: &mut BenchmarkControlTiming,
    ) -> Result<Value, OperationalControlError> {
        *timing = BenchmarkControlTiming::zero();
        let started = Instant::now();
        let result = self.complete_benchmark_inner(
            report,
            ordered_node_ids,
            tolerance_ms,
            deadline,
            mutation_committed,
            timing,
        );
        timing.total_us = duration_us(started.elapsed());
        result
    }

    fn complete_benchmark_inner(
        &mut self,
        report: &BenchmarkReport,
        ordered_node_ids: &[String],
        tolerance_ms: u32,
        deadline: Instant,
        mutation_committed: bool,
        timing: &mut BenchmarkControlTiming,
    ) -> Result<Value, OperationalControlError> {
        if deadline <= Instant::now() {
            return Err(OperationalControlError::BenchmarkDeadline);
        }
        let (intent, _) =
            measure_result(&mut timing.intent_load_us, || self.selection_store.load())?;
        if matches!(intent, NodeSelectionIntent::Auto) && !mutation_committed {
            let snapshot_started = Instant::now();
            let current = self
                .node_snapshot()
                .ok()
                .and_then(|snapshot| snapshot.selection().active_node_id().cloned());
            timing.current_snapshot_us = duration_us(snapshot_started.elapsed());
            let decision_started = Instant::now();
            let decision = choose_auto_target(
                ordered_node_ids,
                &report.nodes,
                current.as_ref().map(StableNodeId::as_str),
                tolerance_ms,
            );
            timing.decision_us = duration_us(decision_started.elapsed());
            if let AutoSelectionDecision::Switch { node_id } = decision {
                let internal_tag = measure_result(&mut timing.target_resolve_us, || {
                    let registry = self.current_registry()?;
                    registry
                        .by_stable_id(&node_id)
                        .map(|record| record.internal_tag().to_owned())
                        .ok_or(OperationalControlError::UnknownNode)
                })?;
                measure_result(&mut timing.selector_apply_us, || {
                    self.api.select_node_before(&internal_tag, deadline)
                })?;
            }
        }
        let timeout = deadline.saturating_duration_since(Instant::now());
        if timeout.is_zero() {
            return Err(OperationalControlError::BenchmarkDeadline);
        }
        measure_result(&mut timing.final_snapshot_us, || {
            let core = self.api.group_snapshot_with_timeout(timeout)?;
            Ok(serde_json::to_value(
                self.node_snapshot_from_core(&core)?.selection(),
            )?)
        })
    }

    pub(crate) fn fast_selection_context(
        &self,
        timing: &mut BenchmarkControlTiming,
    ) -> Result<FastSelectionContext, OperationalControlError> {
        let started = Instant::now();
        let result = (|| {
            let (intent, _) = measure_result_accumulated(&mut timing.intent_load_us, || {
                self.selection_store.load()
            })?;
            Ok(if matches!(intent, NodeSelectionIntent::Auto) {
                let snapshot = measure_result_accumulated(&mut timing.current_snapshot_us, || {
                    self.node_snapshot()
                })?;
                FastSelectionContext::Auto {
                    current_node_id: snapshot
                        .selection()
                        .active_node_id()
                        .map(StableNodeId::as_str)
                        .map(ToOwned::to_owned),
                }
            } else {
                FastSelectionContext::Manual
            })
        })();
        timing.total_us = timing
            .total_us
            .saturating_add(duration_us(started.elapsed()));
        result
    }

    pub(crate) fn apply_fast_selection(
        &mut self,
        input: FastSelectionInput<'_>,
        mutation_committed: &mut bool,
        timing: &mut BenchmarkControlTiming,
    ) -> Result<FastSelectionApply, OperationalControlError> {
        if input.deadline <= Instant::now() {
            return Err(OperationalControlError::BenchmarkDeadline);
        }
        let started = Instant::now();
        let result = (|| {
            let (intent, _) = measure_result_accumulated(&mut timing.intent_load_us, || {
                self.selection_store.load()
            })?;
            if !matches!(intent, NodeSelectionIntent::Auto) {
                return Ok(FastSelectionApply::NotApplicable);
            }
            let decision_started = Instant::now();
            let decision = choose_auto_target(
                input.ordered_node_ids,
                input.outcomes,
                input.current_node_id,
                input.tolerance_ms,
            );
            timing.decision_us = timing
                .decision_us
                .saturating_add(duration_us(decision_started.elapsed()));
            Ok(match decision {
                AutoSelectionDecision::Keep => FastSelectionApply::Kept,
                AutoSelectionDecision::Switch { node_id } => {
                    let internal_tag =
                        measure_result_accumulated(&mut timing.target_resolve_us, || {
                            let registry = self.current_registry()?;
                            registry
                                .by_stable_id(&node_id)
                                .map(|record| record.internal_tag().to_owned())
                                .ok_or(OperationalControlError::UnknownNode)
                        })?;
                    measure_result_accumulated(&mut timing.selector_apply_us, || {
                        self.api.select_node_before(&internal_tag, input.deadline)
                    })?;
                    *mutation_committed = true;
                    let timeout = input.deadline.saturating_duration_since(Instant::now());
                    if timeout.is_zero() {
                        return Err(OperationalControlError::BenchmarkDeadline);
                    }
                    let selection = measure_result_accumulated(
                        &mut timing.final_snapshot_us,
                        || -> Result<Value, OperationalControlError> {
                            let core = self.api.group_snapshot_with_timeout(timeout)?;
                            Ok(serde_json::to_value(
                                self.node_snapshot_from_core(&core)?.selection(),
                            )?)
                        },
                    )?;
                    FastSelectionApply::Switched(selection)
                }
            })
        })();
        timing.total_us = timing
            .total_us
            .saturating_add(duration_us(started.elapsed()));
        result
    }

    #[cfg(feature = "benchmark-evidence")]
    pub fn complete_benchmark_for_evidence(
        &mut self,
        report: &BenchmarkReport,
        ordered_node_ids: &[String],
        tolerance_ms: u32,
        deadline: Instant,
    ) -> Result<Value, OperationalControlError> {
        self.complete_benchmark(report, ordered_node_ids, tolerance_ms, deadline)
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
    #[error("node benchmark model operation failed")]
    Benchmark(#[from] BenchmarkError),
    #[error("node benchmark operation deadline expired")]
    BenchmarkDeadline,
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
            Self::Benchmark(_) => (ErrorDomain::Node, "BENCHMARK-INVALID"),
            Self::BenchmarkDeadline => (ErrorDomain::Node, "BENCHMARK-DEADLINE"),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::{Ipv4Addr, SocketAddrV4, TcpListener},
        thread,
    };

    use crate::{
        BenchmarkTrigger, ClashApiLimits, NodeProbeOutcome, NodeSelectionIntent, NodeSelectionStore,
    };
    use nethop_core::{GenerationNodeRecord, GenerationNodeRegistry};
    use tempfile::tempdir;

    const TEST_SECRET: &str = "0123456789abcdef0123456789abcdef";
    const FIRST: &str = "nh1s-0000000000000001";
    const SECOND: &str = "nh1s-0000000000000002";

    fn serve(responses: Vec<(u16, String)>) -> (SocketAddrV4, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = match listener.local_addr().unwrap() {
            std::net::SocketAddr::V4(address) => address,
            _ => unreachable!(),
        };
        let handle = thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0_u8; 2048];
                loop {
                    let count = stream.read(&mut buffer).unwrap();
                    if count == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&buffer[..count]);
                    if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..end + 4]);
                        let length = headers
                            .lines()
                            .find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                name.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse::<usize>().ok())
                                    .flatten()
                            })
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                requests.push(String::from_utf8(bytes).unwrap());
                write!(
                    stream,
                    "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
            requests
        });
        (address, handle)
    }

    fn selector_document(now: &str) -> String {
        json!({
            "proxies": {
                "nethop-select": {"type":"Selector","now":now,"all":[FIRST,SECOND]},
                FIRST: {"type":"VLESS"},
                SECOND: {"type":"VLESS"}
            }
        })
        .to_string()
    }

    fn generation_root(root: &Path) -> PathBuf {
        let generations = root.join("generations");
        fs::create_dir(&generations).unwrap();
        fs::create_dir(generations.join("7")).unwrap();
        fs::write(generations.join("current"), "7\n").unwrap();
        let records = [FIRST, SECOND]
            .into_iter()
            .map(|tag| {
                GenerationNodeRecord::new(
                    tag,
                    tag,
                    tag,
                    "vless",
                    vec!["src_0123456789abcdef0123456789abcdef".into()],
                    true,
                )
                .unwrap()
            })
            .collect();
        let registry = GenerationNodeRegistry::new(records).unwrap();
        fs::write(
            generations.join("7/nodes.json"),
            serde_json::to_vec(&registry).unwrap(),
        )
        .unwrap();
        generations
    }

    fn control(
        root: &Path,
        address: SocketAddrV4,
        intent: NodeSelectionIntent,
    ) -> OperationalControl {
        let store = NodeSelectionStore::new(root.join("selection.v1.json")).unwrap();
        store.save(&intent, 1).unwrap();
        let api = ClashApiClient::new(address, TEST_SECRET, ClashApiLimits::default()).unwrap();
        OperationalControl::new(api, store, root.join("diagnostics-latest.json"))
            .unwrap()
            .with_generation_root(generation_root(root))
            .unwrap()
    }

    fn report(trigger: BenchmarkTrigger, first: u32, second: u32) -> BenchmarkReport {
        BenchmarkReport::from_outcomes(
            trigger,
            7,
            1,
            20,
            vec![
                NodeProbeOutcome::success(FIRST, first).unwrap(),
                NodeProbeOutcome::success(SECOND, second).unwrap(),
            ],
        )
        .unwrap()
    }

    #[test]
    fn manual_benchmark_updates_observations_without_selecting_a_node() {
        let directory = tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let (address, server) = serve(vec![(200, selector_document(FIRST))]);
        let mut control = control(
            &root,
            address,
            NodeSelectionIntent::Manual {
                node_id: StableNodeId::new(FIRST).unwrap(),
            },
        );

        let selection = control
            .complete_benchmark(
                &report(BenchmarkTrigger::Manual, 200, 40),
                &[FIRST.to_owned(), SECOND.to_owned()],
                50,
                Instant::now() + Duration::from_secs(1),
            )
            .unwrap();

        assert_eq!(selection["intent"]["mode"], "manual");
        assert_eq!(selection["active_terminal"]["kind"], "node");
        assert_eq!(selection["active_terminal"]["node_id"], FIRST);
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].starts_with("GET /proxies "));
    }

    #[test]
    fn manually_triggered_benchmark_selects_when_intent_is_auto() {
        let directory = tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let before = selector_document(FIRST);
        let (address, server) = serve(vec![
            (200, before.clone()),
            (200, before),
            (204, String::new()),
            (200, selector_document(SECOND)),
        ]);
        let mut control = control(&root, address, NodeSelectionIntent::Auto);

        let selection = control
            .complete_benchmark(
                &report(BenchmarkTrigger::Manual, 200, 40),
                &[FIRST.to_owned(), SECOND.to_owned()],
                50,
                Instant::now() + Duration::from_secs(1),
            )
            .unwrap();

        assert_eq!(selection["intent"]["mode"], "auto");
        assert_eq!(selection["active_terminal"]["kind"], "node");
        assert_eq!(selection["active_terminal"]["node_id"], SECOND);
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 4);
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.starts_with("PUT /proxies/nethop-select "))
                .count(),
            1
        );
    }

    #[test]
    fn automatic_benchmark_honors_tolerance_without_selector_put() {
        let directory = tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let snapshot = selector_document(FIRST);
        let (address, server) = serve(vec![(200, snapshot.clone()), (200, snapshot)]);
        let mut control = control(&root, address, NodeSelectionIntent::Auto);

        let selection = control
            .complete_benchmark(
                &report(BenchmarkTrigger::Periodic, 150, 100),
                &[FIRST.to_owned(), SECOND.to_owned()],
                50,
                Instant::now() + Duration::from_secs(1),
            )
            .unwrap();

        assert_eq!(selection["active_terminal"]["kind"], "node");
        assert_eq!(selection["active_terminal"]["node_id"], FIRST);
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(
            requests
                .iter()
                .all(|request| request.starts_with("GET /proxies "))
        );
    }

    #[test]
    fn automatic_benchmark_switches_once_and_reads_the_final_snapshot() {
        let directory = tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let before = selector_document(FIRST);
        let (address, server) = serve(vec![
            (200, before.clone()),
            (200, before),
            (204, String::new()),
            (200, selector_document(SECOND)),
        ]);
        let mut control = control(&root, address, NodeSelectionIntent::Auto);
        let mut timing = BenchmarkControlTiming::zero();

        let selection = control
            .complete_benchmark_timed(
                &report(BenchmarkTrigger::Periodic, 200, 40),
                &[FIRST.to_owned(), SECOND.to_owned()],
                50,
                Instant::now() + Duration::from_secs(1),
                &mut timing,
            )
            .unwrap();

        assert_eq!(selection["active_terminal"]["kind"], "node");
        assert_eq!(selection["active_terminal"]["node_id"], SECOND);
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 4);
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.starts_with("PUT /proxies/nethop-select "))
                .count(),
            1
        );
        assert!(requests[2].ends_with(&format!(r#"{{"name":"{SECOND}"}}"#)));
        assert!(requests[3].starts_with("GET /proxies "));
        assert!(timing.current_snapshot_us > 0);
        assert!(timing.selector_apply_us > 0);
        assert!(timing.final_snapshot_us > 0);
        assert!(
            timing.intent_load_us
                + timing.current_snapshot_us
                + timing.decision_us
                + timing.target_resolve_us
                + timing.selector_apply_us
                + timing.final_snapshot_us
                <= timing.total_us
        );
        timing.validate().unwrap();
    }

    #[test]
    fn fast_switch_commits_once_while_terminal_only_reads_final_snapshot() {
        let directory = tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let before = selector_document(FIRST);
        let after = selector_document(SECOND);
        let (address, server) = serve(vec![
            (200, before.clone()),
            (200, before),
            (204, String::new()),
            (200, after.clone()),
            (200, after),
        ]);
        let mut control = control(&root, address, NodeSelectionIntent::Auto);
        let mut fast_timing = BenchmarkControlTiming::zero();
        let context = control.fast_selection_context(&mut fast_timing).unwrap();
        let FastSelectionContext::Auto { current_node_id } = context else {
            panic!("auto intent must produce an auto context");
        };
        let mut mutation_committed = false;
        let fast = control
            .apply_fast_selection(
                FastSelectionInput {
                    outcomes: &report(BenchmarkTrigger::Manual, 200, 40).nodes,
                    ordered_node_ids: &[FIRST.to_owned(), SECOND.to_owned()],
                    current_node_id: current_node_id.as_deref(),
                    tolerance_ms: 50,
                    deadline: Instant::now() + Duration::from_secs(1),
                },
                &mut mutation_committed,
                &mut fast_timing,
            )
            .unwrap();
        assert!(matches!(fast, FastSelectionApply::Switched(_)));
        assert!(mutation_committed);

        let mut terminal_timing = BenchmarkControlTiming::zero();
        let selection = control
            .complete_benchmark_with_commit_timed(
                &report(BenchmarkTrigger::Manual, 200, 20),
                &[FIRST.to_owned(), SECOND.to_owned()],
                50,
                Instant::now() + Duration::from_secs(1),
                true,
                &mut terminal_timing,
            )
            .unwrap();
        assert_eq!(selection["active_terminal"]["node_id"], SECOND);
        assert_eq!(terminal_timing.selector_apply_us, 0);

        let requests = server.join().unwrap();
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.starts_with("PUT /proxies/nethop-select "))
                .count(),
            1
        );
        assert_eq!(requests.len(), 5);
    }

    #[test]
    fn failed_fast_control_still_accounts_for_total_time() {
        let directory = tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let before = selector_document(FIRST);
        let (address, server) = serve(vec![(200, before), (500, String::new())]);
        let mut control = control(&root, address, NodeSelectionIntent::Auto);
        let mut timing = BenchmarkControlTiming::zero();
        let context = control.fast_selection_context(&mut timing).unwrap();
        let FastSelectionContext::Auto { current_node_id } = context else {
            panic!("auto intent must produce an auto context");
        };
        let mut mutation_committed = false;

        let Err(error) = control.apply_fast_selection(
            FastSelectionInput {
                outcomes: &report(BenchmarkTrigger::Manual, 200, 40).nodes,
                ordered_node_ids: &[FIRST.to_owned(), SECOND.to_owned()],
                current_node_id: current_node_id.as_deref(),
                tolerance_ms: 50,
                deadline: Instant::now() + Duration::from_secs(1),
            },
            &mut mutation_committed,
            &mut timing,
        ) else {
            panic!("rejected selector request must fail");
        };

        assert!(matches!(error, OperationalControlError::ClashApi(_)));
        assert!(!mutation_committed);
        assert!(timing.selector_apply_us > 0);
        timing.validate().unwrap();
        assert_eq!(server.join().unwrap().len(), 2);
    }

    #[test]
    fn failed_fast_ack_never_allows_a_second_selector_mutation() {
        let directory = tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let before = selector_document(FIRST);
        let after = selector_document(SECOND);
        let (address, server) = serve(vec![
            (200, before.clone()),
            (200, before),
            (204, String::new()),
            (500, String::new()),
            (200, after),
        ]);
        let mut control = control(&root, address, NodeSelectionIntent::Auto);
        let mut fast_timing = BenchmarkControlTiming::zero();
        let context = control.fast_selection_context(&mut fast_timing).unwrap();
        let FastSelectionContext::Auto { current_node_id } = context else {
            panic!("auto intent must produce an auto context");
        };
        let mut mutation_committed = false;

        let result = control.apply_fast_selection(
            FastSelectionInput {
                outcomes: &report(BenchmarkTrigger::Manual, 200, 40).nodes,
                ordered_node_ids: &[FIRST.to_owned(), SECOND.to_owned()],
                current_node_id: current_node_id.as_deref(),
                tolerance_ms: 50,
                deadline: Instant::now() + Duration::from_secs(1),
            },
            &mut mutation_committed,
            &mut fast_timing,
        );
        assert!(result.is_err());
        assert!(mutation_committed);

        let mut terminal_timing = BenchmarkControlTiming::zero();
        let selection = control
            .complete_benchmark_with_commit_timed(
                &report(BenchmarkTrigger::Manual, 200, 20),
                &[FIRST.to_owned(), SECOND.to_owned()],
                50,
                Instant::now() + Duration::from_secs(1),
                mutation_committed,
                &mut terminal_timing,
            )
            .unwrap();
        assert_eq!(selection["active_terminal"]["node_id"], SECOND);

        let requests = server.join().unwrap();
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.starts_with("PUT /proxies/nethop-select "))
                .count(),
            1
        );
        assert_eq!(requests.len(), 5);
    }

    #[test]
    fn expired_benchmark_deadline_performs_no_control_request() {
        let directory = tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let (address, server) = serve(Vec::new());
        let mut control = control(&root, address, NodeSelectionIntent::Auto);

        let error = control
            .complete_benchmark(
                &report(BenchmarkTrigger::Periodic, 40, 80),
                &[FIRST.to_owned(), SECOND.to_owned()],
                50,
                Instant::now(),
            )
            .unwrap_err();

        assert!(matches!(error, OperationalControlError::BenchmarkDeadline));
        assert!(server.join().unwrap().is_empty());
    }
}
