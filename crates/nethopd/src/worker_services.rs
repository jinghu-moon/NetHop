use std::{collections::VecDeque, time::Duration};

use nethop_android::{NetlinkDebouncer, NetlinkError, NetworkEvent};
#[cfg(feature = "subscription-update")]
use nethop_core::{
    Candidate, CapturePolicy, ClashApi, GenerationNodeRecord, GenerationNodeRegistry,
    ManagedConfig, ManagedOptions, ManagedProfile, TunStack,
};
use nethop_core::{GenerationId, RuntimeState};
use nethop_protocol::{
    ControlError, ControlMethod, ControlRequest, ControlResponse, ErrorCode, ErrorDomain,
};
#[cfg(feature = "subscription-update")]
use nethop_subscription::{
    SourceId, StableConversion, TerminalOutboundAdapterError, adapt_terminal_outbound,
    infer_display_territory,
};
use serde_json::json;
use thiserror::Error;

#[cfg(feature = "subscription-update")]
use crate::{
    CandidatePoolError, CandidatePoolNode, NodeAttribution, SelectionModelError, StableNodeId,
    SubscriptionMode, build_candidate_pools,
};
use crate::{
    CandidateProcess, ControlRequestHandler, CounterDeltaTracker, CounterTransport, StatsError,
    StatsStore, StatsStoreError, WorkerRuntime, WorkerRuntimeError,
};

#[cfg(feature = "subscription-update")]
#[derive(Debug, Clone)]
pub struct CandidateBuildProfile {
    capture: CapturePolicy,
    clash_api: ClashApi,
    tun_stack: TunStack,
    options: ManagedOptions,
}

#[cfg(feature = "subscription-update")]
impl CandidateBuildProfile {
    pub const fn new(
        capture: CapturePolicy,
        clash_api: ClashApi,
        tun_stack: TunStack,
        options: ManagedOptions,
    ) -> Self {
        Self {
            capture,
            clash_api,
            tun_stack,
            options,
        }
    }

    pub(crate) fn replace(
        &mut self,
        capture: CapturePolicy,
        tun_stack: TunStack,
        options: ManagedOptions,
    ) {
        self.capture = capture;
        self.tun_stack = tun_stack;
        self.options = options;
    }
}

#[cfg(feature = "subscription-update")]
#[derive(Debug, Error)]
pub enum BuildCandidateError {
    #[error("stable conversion contains no usable terminal outbounds")]
    EmptyConversion,
    #[error("parser outbound adapter rejected the conversion")]
    Adapter(#[from] TerminalOutboundAdapterError),
    #[error("managed configuration composer rejected the conversion")]
    Composer(#[from] nethop_core::ComposerError),
    #[error("candidate pool rejected the conversion")]
    Pool(#[from] CandidatePoolError),
    #[error("candidate node identity is invalid")]
    NodeIdentity(#[from] SelectionModelError),
    #[error("candidate source attribution is invalid")]
    Attribution(#[from] crate::SourceRegistryError),
    #[error("generation node registry rejected the conversion")]
    Registry(#[from] nethop_core::CoreError),
    #[error("node override was rejected")]
    Override(#[from] crate::NodeOverrideError),
}

#[cfg(feature = "subscription-update")]
pub fn build_candidate(
    generation: GenerationId,
    conversion: &StableConversion,
    profile: CandidateBuildProfile,
    subscription_mode: SubscriptionMode,
    active_source_ids: &[SourceId],
) -> Result<Candidate, BuildCandidateError> {
    build_candidate_with_overrides(
        generation,
        conversion,
        profile,
        subscription_mode,
        active_source_ids,
        &crate::NodeOverrideSet::default(),
    )
}

#[cfg(feature = "subscription-update")]
pub fn build_candidate_with_overrides(
    generation: GenerationId,
    conversion: &StableConversion,
    profile: CandidateBuildProfile,
    subscription_mode: SubscriptionMode,
    active_source_ids: &[SourceId],
    overrides: &crate::NodeOverrideSet,
) -> Result<Candidate, BuildCandidateError> {
    if conversion.nodes.is_empty() || !conversion.report.summary.source_success {
        return Err(BuildCandidateError::EmptyConversion);
    }
    let pool_nodes = conversion
        .nodes
        .iter()
        .map(|node| {
            Ok(CandidatePoolNode::new(
                StableNodeId::new(node.node_id.as_str())?,
                NodeAttribution::new(
                    node.source_refs
                        .iter()
                        .map(|source| source.source_id.clone()),
                )?,
            ))
        })
        .collect::<Result<Vec<_>, BuildCandidateError>>()?;
    let pools = build_candidate_pools(
        subscription_mode,
        active_source_ids,
        &pool_nodes,
        profile.options.urltest_max_candidates(),
    )?;
    let auto_tags = pools
        .auto()
        .iter()
        .map(|node_id| node_id.as_str().to_owned())
        .collect::<Vec<_>>();
    let outbounds = conversion
        .nodes
        .iter()
        .map(|node| {
            let node_id = StableNodeId::new(node.node_id.as_str())?;
            overrides.get(&node_id).map_or_else(
                || adapt_terminal_outbound(node).map_err(BuildCandidateError::from),
                |value| value.terminal_outbound().map_err(BuildCandidateError::from),
            )
        })
        .collect::<Result<Vec<_>, BuildCandidateError>>()?;
    let managed = ManagedProfile::new(
        profile.capture,
        outbounds,
        auto_tags.clone(),
        profile.clash_api,
    )?
    .with_tun_stack(profile.tun_stack)
    .with_options(profile.options);
    let config = ManagedConfig::from_profile(managed)?;
    let auto_ids = pools
        .auto()
        .iter()
        .map(StableNodeId::as_str)
        .collect::<std::collections::HashSet<_>>();
    let records = conversion
        .nodes
        .iter()
        .map(|node| {
            let node_id = StableNodeId::new(node.node_id.as_str())?;
            let node_override = overrides.get(&node_id);
            let display_name = node_override.map_or_else(
                || node.node.display_name().as_str(),
                |value| value.display_name(),
            );
            let protocol = node_override
                .map_or_else(|| node.node.protocol().as_str(), |value| value.protocol());
            let display_territory_code = node_override
                .map_or(node.display_territory_code, |value| {
                    infer_display_territory([value.display_name()])
                });
            GenerationNodeRecord::new(
                node.node_id.as_str(),
                node.node_id.as_str(),
                bounded_display_name(display_name),
                protocol,
                NodeAttribution::new(
                    node.source_refs
                        .iter()
                        .map(|source| source.source_id.clone()),
                )?
                .source_ids()
                .iter()
                .map(|source_id| source_id.as_str().to_owned())
                .collect(),
                auto_ids.contains(node.node_id.as_str()),
            )
            .map(|record| record.with_display_territory_code(display_territory_code))
            .map_err(BuildCandidateError::from)
        })
        .collect::<Result<Vec<_>, BuildCandidateError>>()?;
    Ok(Candidate::new(generation, config)
        .with_node_registry(GenerationNodeRegistry::with_auto_pool(records, auto_tags)?)?)
}

#[cfg(feature = "subscription-update")]
fn bounded_display_name(value: &str) -> String {
    const MAX_BYTES: usize = 128;
    if value.len() <= MAX_BYTES {
        return value.to_owned();
    }
    let mut end = MAX_BYTES;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

#[derive(Debug, Clone, Default)]
pub struct EventReconcileGate {
    debounce: NetlinkDebouncer,
    ready: VecDeque<u32>,
}

impl EventReconcileGate {
    pub fn observe(&mut self, now: Duration, event: NetworkEvent) -> Result<(), NetlinkError> {
        if let Some(batch) = self.debounce.observe(now, event)? {
            self.ready.push_back(batch.event_count());
        }
        Ok(())
    }

    pub fn take_ready(&mut self, now: Duration) -> Result<Option<u32>, NetlinkError> {
        if let Some(count) = self.ready.pop_front() {
            return Ok(Some(count));
        }
        Ok(self
            .debounce
            .take_ready(now)?
            .map(|batch| batch.event_count()))
    }

    pub fn deadline(&self) -> Option<Duration> {
        self.debounce.deadline()
    }

    pub fn request_ready<P, R>(
        &mut self,
        now: Duration,
        runtime: &mut WorkerRuntime<P, R>,
    ) -> Result<Option<u32>, EventReconcileError>
    where
        P: CandidateProcess,
    {
        let Some(event_count) = self.take_ready(now)? else {
            return Ok(None);
        };
        runtime.request_reconcile(now)?;
        Ok(Some(event_count))
    }
}

#[derive(Debug, Error)]
pub enum EventReconcileError {
    #[error("network event debounce failed")]
    Netlink(#[from] NetlinkError),
    #[error("network event could not schedule runtime reconcile")]
    Runtime(#[from] WorkerRuntimeError),
}

#[derive(Debug, Error)]
pub enum StatsCollectorError {
    #[error("counter transport or delta validation failed")]
    Stats(#[from] StatsError),
    #[error("stats delta could not be committed")]
    Store(#[from] StatsStoreError),
}

pub struct StatsCollector<T> {
    transport: T,
    tracker: CounterDeltaTracker,
}

impl<T> StatsCollector<T>
where
    T: CounterTransport,
{
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            tracker: CounterDeltaTracker::default(),
        }
    }

    pub fn collect(
        &mut self,
        bucket_start: i64,
        store: &mut StatsStore,
    ) -> Result<bool, StatsCollectorError> {
        let batch = self.transport.read_counters()?;
        let delta = self.tracker.apply(batch)?;
        let baseline_only = delta.baseline_only();
        store.apply_delta(bucket_start, &delta)?;
        Ok(!baseline_only)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlCommand {
    Start,
    Stop,
    Probe,
    Update,
    RebuildGeneration,
    UpdateSource(String),
    RuleSetUpdate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlSnapshot {
    pub state: RuntimeState,
    pub generation: Option<GenerationId>,
    pub last_update: UpdateStatus,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UpdateStatus {
    #[default]
    Never,
    Succeeded,
    Failed,
}

impl UpdateStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug)]
pub struct WorkerControlHandler {
    snapshot: ControlSnapshot,
    pending: VecDeque<ControlCommand>,
    update_available: bool,
}

impl WorkerControlHandler {
    pub fn new(snapshot: ControlSnapshot) -> Self {
        Self {
            snapshot,
            pending: VecDeque::new(),
            update_available: false,
        }
    }

    pub fn with_update_available(mut self) -> Self {
        self.update_available = true;
        self
    }

    pub fn with_update_available_if(self, available: bool) -> Self {
        if available {
            self.with_update_available()
        } else {
            self
        }
    }

    pub fn set_update_available(&mut self, available: bool) {
        self.update_available = available;
    }

    pub fn queue_command(&mut self, command: ControlCommand) {
        match command {
            ControlCommand::Stop => {
                self.pending
                    .retain(|pending| matches!(pending, ControlCommand::Probe));
                self.pending.push_front(ControlCommand::Stop);
            }
            _ => self.pending.push_back(command),
        }
    }

    pub fn update_snapshot(&mut self, snapshot: ControlSnapshot) {
        self.snapshot = snapshot;
    }

    pub const fn snapshot(&self) -> ControlSnapshot {
        self.snapshot
    }

    pub fn take_command(&mut self) -> Option<ControlCommand> {
        self.pending.pop_front()
    }
}

impl ControlRequestHandler for WorkerControlHandler {
    fn handle(&mut self, request: ControlRequest) -> ControlResponse {
        let request_id = request.request_id().clone();
        let generation = self.snapshot.generation.map(GenerationId::get);
        match request.method() {
            ControlMethod::StatusGet => ControlResponse::success(
                request_id,
                generation,
                json!({
                    "state": state_wire(self.snapshot.state),
                    "last_update": self.snapshot.last_update.as_str(),
                }),
            ),
            ControlMethod::ServiceStart => {
                self.queue_command(ControlCommand::Start);
                ControlResponse::success(request_id, generation, json!({"accepted":true}))
            }
            ControlMethod::ServiceStop => {
                self.queue_command(ControlCommand::Stop);
                ControlResponse::success(request_id, generation, json!({"accepted":true}))
            }
            ControlMethod::CapabilityProbe => {
                self.queue_command(ControlCommand::Probe);
                ControlResponse::success(request_id, generation, json!({"accepted":true}))
            }
            ControlMethod::SubscriptionUpdate if self.update_available => {
                if let Some(source_id) = request.params().source_id() {
                    self.queue_command(ControlCommand::UpdateSource(source_id.to_owned()));
                } else {
                    self.queue_command(ControlCommand::Update);
                }
                ControlResponse::success(request_id, generation, json!({"accepted":true}))
            }
            ControlMethod::SubscriptionUpdate => ControlResponse::failure(
                request_id,
                generation,
                unavailable_control_error(ErrorDomain::Subscription, "UPDATE-UNAVAILABLE"),
            ),
            ControlMethod::ConfigReload => ControlResponse::failure(
                request_id,
                generation,
                unavailable_control_error(ErrorDomain::Config, "RELOAD-UNAVAILABLE"),
            ),
            ControlMethod::ProtocolHello
            | ControlMethod::ConfigGet
            | ControlMethod::ConfigExport
            | ControlMethod::CoreVersionCheck
            | ControlMethod::RuleSetStatus
            | ControlMethod::RuleSetUpdate
            | ControlMethod::ConfigValidate
            | ControlMethod::ConfigApply
            | ControlMethod::ConfigSchema
            | ControlMethod::CapabilityGet
            | ControlMethod::ConfigMutate
            | ControlMethod::SubscriptionImportPreview
            | ControlMethod::SubscriptionImportApply
            | ControlMethod::SubscriptionModeGet
            | ControlMethod::SubscriptionModeSet
            | ControlMethod::SubscriptionSelect
            | ControlMethod::SubscriptionSetEnabled
            | ControlMethod::EventsSubscribe
            | ControlMethod::WebUiPayloadCreate
            | ControlMethod::WebUiPayloadAppend
            | ControlMethod::WebUiPayloadCommit
            | ControlMethod::WebUiPayloadRemove => ControlResponse::failure(
                request_id,
                generation,
                unavailable_control_error(ErrorDomain::Config, "MANAGER-UNAVAILABLE"),
            ),
            ControlMethod::NodeList
            | ControlMethod::NodeTest
            | ControlMethod::NodeTestAll
            | ControlMethod::NodeTestOperationGet
            | ControlMethod::NodeSelectionGet
            | ControlMethod::NodeSelectAuto
            | ControlMethod::NodeSelectManual
            | ControlMethod::NodeExport
            | ControlMethod::NodeOverrideGet
            | ControlMethod::NodeOverrideApply
            | ControlMethod::NodeOverrideRemove
            | ControlMethod::ConnectionsGet
            | ControlMethod::ConnectionClose
            | ControlMethod::ConnectionsCloseAll
            | ControlMethod::LogsGet
            | ControlMethod::LogsClear
            | ControlMethod::DiagnosticsBundle
            | ControlMethod::TopologyGet
            | ControlMethod::TrafficGet
            | ControlMethod::MetricsGet => ControlResponse::failure(
                request_id,
                generation,
                unavailable_control_error(ErrorDomain::Core, "CONTROL-UNAVAILABLE"),
            ),
        }
    }
}

fn state_wire(state: RuntimeState) -> &'static str {
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

pub fn unavailable_control_error(domain: ErrorDomain, detail: &str) -> ControlError {
    ControlError::new(
        ErrorCode::new(domain, detail).expect("static control error detail is valid"),
        "requested service is unavailable",
    )
    .expect("static control message is valid")
}

pub fn unavailable_control_error_with_details(
    domain: ErrorDomain,
    detail: &str,
    details: serde_json::Value,
) -> ControlError {
    ControlError::with_details(
        ErrorCode::new(domain, detail).expect("static control error detail is valid"),
        "requested service is unavailable",
        details,
    )
    .expect("static control message is valid")
}
