use std::{collections::VecDeque, time::Duration};

use nethop_android::{NetlinkDebouncer, NetlinkError, NetworkEvent};
use nethop_core::{
    Candidate, CapturePolicy, ClashApi, GenerationId, ManagedConfig, ManagedProfile, RuntimeState,
    TunStack,
};
use nethop_protocol::{
    ControlError, ControlMethod, ControlRequest, ControlResponse, ErrorCode, ErrorDomain,
};
use nethop_subscription::{
    StableConversion, TerminalOutboundAdapterError, adapt_terminal_outbounds,
};
use serde_json::json;
use thiserror::Error;

use crate::{
    CandidateProcess, ControlRequestHandler, CounterDeltaTracker, CounterTransport, StatsError,
    StatsStore, StatsStoreError, WorkerRuntime, WorkerRuntimeError,
};

#[derive(Debug, Error)]
pub enum BuildCandidateError {
    #[error("stable conversion contains no usable terminal outbounds")]
    EmptyConversion,
    #[error("parser outbound adapter rejected the conversion")]
    Adapter(#[from] TerminalOutboundAdapterError),
    #[error("managed configuration composer rejected the conversion")]
    Composer(#[from] nethop_core::ComposerError),
}

pub fn build_candidate(
    generation: GenerationId,
    conversion: &StableConversion,
    capture: CapturePolicy,
    clash_api: ClashApi,
    tun_stack: TunStack,
) -> Result<Candidate, BuildCandidateError> {
    if conversion.nodes.is_empty() || !conversion.report.summary.source_success {
        return Err(BuildCandidateError::EmptyConversion);
    }
    let outbounds = adapt_terminal_outbounds(&conversion.nodes)?;
    let profile = ManagedProfile::new(capture, outbounds, clash_api)?.with_tun_stack(tun_stack);
    let config = ManagedConfig::from_profile(profile)?;
    Ok(Candidate::new(generation, config))
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlCommand {
    Start,
    Stop,
    Probe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlSnapshot {
    pub state: RuntimeState,
    pub generation: Option<GenerationId>,
}

#[derive(Debug)]
pub struct WorkerControlHandler {
    snapshot: ControlSnapshot,
    pending: VecDeque<ControlCommand>,
}

impl WorkerControlHandler {
    pub fn new(snapshot: ControlSnapshot) -> Self {
        Self {
            snapshot,
            pending: VecDeque::new(),
        }
    }

    pub fn update_snapshot(&mut self, snapshot: ControlSnapshot) {
        self.snapshot = snapshot;
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
                json!({"state": state_wire(self.snapshot.state)}),
            ),
            ControlMethod::ServiceStart => {
                self.pending.push_back(ControlCommand::Start);
                ControlResponse::success(request_id, generation, json!({"accepted":true}))
            }
            ControlMethod::ServiceStop => {
                self.pending.push_back(ControlCommand::Stop);
                ControlResponse::success(request_id, generation, json!({"accepted":true}))
            }
            ControlMethod::CapabilityProbe => {
                self.pending.push_back(ControlCommand::Probe);
                ControlResponse::success(request_id, generation, json!({"accepted":true}))
            }
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
