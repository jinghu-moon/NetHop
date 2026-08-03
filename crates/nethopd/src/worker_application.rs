use std::time::{Duration, Instant};

use nethop_android::{NetworkHealthVerifier, PlanSlot};
use nethop_core::{CapturePolicy, GenerationId, RuntimeState};
use nethop_protocol::{ControlRequest, ControlResponse};

use crate::{
    ActiveRuntime, CandidateProcess, CapabilitySource, ControlCommand, ControlRequestHandler,
    ControlSnapshot, CurrentGenerationActivator, DataPlaneHealthProbe, HealthProbe,
    NetworkController, RestartBudget, RestartDecision, RuntimeTick, WorkerControlHandler,
    WorkerRecoveryError, WorkerRuntime, WorkerRuntimeLimits, WorkerServiceError,
    WorkerServiceTasks,
};
use crate::{CandidateChecker, CoreLauncher};

const IDLE_WAKEUP: Duration = Duration::from_secs(1);

pub trait WorkerClock {
    fn now(&self) -> Duration;
}

pub type ApplicationRecovery<P, R> = Result<Option<ActiveRuntime<P, R>>, WorkerRecoveryError>;

#[derive(Debug)]
pub struct MonotonicClock {
    started: Instant,
}

impl MonotonicClock {
    pub fn start() -> Self {
        Self {
            started: Instant::now(),
        }
    }
}

impl WorkerClock for MonotonicClock {
    fn now(&self) -> Duration {
        self.started.elapsed()
    }
}

pub trait RuntimeRecoverySource<N>
where
    N: NetworkController,
{
    type Process: CandidateProcess;

    fn recover(
        &mut self,
        network: &mut N,
        policy: &CapturePolicy,
        slot: PlanSlot,
    ) -> ApplicationRecovery<Self::Process, N::Receipt>;

    fn probe(&mut self) -> bool;
}

pub struct WorkerRecoveryCoordinator<'a, C, L, H, S, D> {
    store: &'a nethop_core::GenerationStore,
    checker: &'a C,
    launcher: &'a L,
    core_health: &'a H,
    capability_source: S,
    data_plane_health: D,
}

impl<'a, C, L, H, S, D> WorkerRecoveryCoordinator<'a, C, L, H, S, D> {
    pub const fn new(
        store: &'a nethop_core::GenerationStore,
        checker: &'a C,
        launcher: &'a L,
        core_health: &'a H,
        capability_source: S,
        data_plane_health: D,
    ) -> Self {
        Self {
            store,
            checker,
            launcher,
            core_health,
            capability_source,
            data_plane_health,
        }
    }
}

impl<C, L, H, S, D, N> RuntimeRecoverySource<N> for WorkerRecoveryCoordinator<'_, C, L, H, S, D>
where
    C: CandidateChecker,
    L: CoreLauncher,
    H: HealthProbe<L::Process>,
    S: CapabilitySource,
    D: DataPlaneHealthProbe<L::Process>,
    N: NetworkController,
{
    type Process = L::Process;

    fn recover(
        &mut self,
        network: &mut N,
        policy: &CapturePolicy,
        slot: PlanSlot,
    ) -> ApplicationRecovery<Self::Process, N::Receipt> {
        CurrentGenerationActivator::new(
            self.store,
            self.checker,
            self.launcher,
            self.core_health,
            &mut self.capability_source,
            network,
            &mut self.data_plane_health,
        )
        .recover(policy, slot)
    }

    fn probe(&mut self) -> bool {
        self.capability_source.probe().is_ok()
    }
}

pub struct WorkerApplication<S, N, V, C>
where
    N: NetworkController,
    S: RuntimeRecoverySource<N>,
    S::Process: CandidateProcess,
{
    control: WorkerControlHandler,
    recovery: S,
    runtime: Option<WorkerRuntime<S::Process, N::Receipt>>,
    network: N,
    verifier: V,
    clock: C,
    policy: CapturePolicy,
    slot: PlanSlot,
    limits: WorkerRuntimeLimits,
    restart_budget: RestartBudget,
    restart_at: Option<Duration>,
    start_pending: bool,
}

impl<S, N, V, C> WorkerApplication<S, N, V, C>
where
    N: NetworkController,
    S: RuntimeRecoverySource<N>,
    S::Process: CandidateProcess,
    V: NetworkHealthVerifier,
    C: WorkerClock,
{
    pub fn new(
        recovery: S,
        network: N,
        verifier: V,
        clock: C,
        policy: CapturePolicy,
        slot: PlanSlot,
        limits: WorkerRuntimeLimits,
    ) -> Self {
        let restart_budget = RestartBudget::new(limits.failure_window())
            .expect("validated worker limits contain a valid failure window");
        Self {
            control: WorkerControlHandler::new(ControlSnapshot {
                state: RuntimeState::Init,
                generation: None,
            }),
            recovery,
            runtime: None,
            network,
            verifier,
            clock,
            policy,
            slot,
            limits,
            restart_budget,
            restart_at: None,
            start_pending: true,
        }
    }

    pub fn snapshot(&self) -> ControlSnapshot {
        let state = self
            .runtime
            .as_ref()
            .map_or(self.control_snapshot().state, WorkerRuntime::state);
        let generation = self.runtime.as_ref().and_then(WorkerRuntime::generation);
        ControlSnapshot { state, generation }
    }

    fn control_snapshot(&self) -> ControlSnapshot {
        self.control.snapshot()
    }

    fn publish_snapshot(&mut self, state: RuntimeState, generation: Option<GenerationId>) {
        self.control
            .update_snapshot(ControlSnapshot { state, generation });
    }

    fn start(&mut self, now: Duration) {
        if self.runtime.is_some() {
            return;
        }
        self.restart_at = None;
        match self
            .recovery
            .recover(&mut self.network, &self.policy, self.slot)
        {
            Ok(Some(active)) => {
                let generation = active.generation();
                self.restart_budget.clear();
                self.runtime = Some(WorkerRuntime::new(active, now, self.limits));
                self.publish_snapshot(RuntimeState::RunningTproxy, Some(generation));
            }
            Ok(None) => {
                self.publish_snapshot(RuntimeState::FailOpenDirect, None);
            }
            Err(_) => match self.restart_budget.register_failure(now) {
                RestartDecision::RetryAfter(after) => {
                    self.restart_at = Some(now.saturating_add(after));
                    self.publish_snapshot(RuntimeState::Backoff, None);
                }
                RestartDecision::CircuitOpen => {
                    self.publish_snapshot(RuntimeState::CircuitOpen, None);
                }
            },
        }
    }

    fn stop(&mut self) -> Result<(), WorkerServiceError> {
        self.restart_at = None;
        self.start_pending = false;
        let cleanup_failed = self
            .runtime
            .as_mut()
            .is_some_and(|runtime| runtime.stop(&mut self.network).is_err());
        self.runtime = None;
        self.publish_snapshot(RuntimeState::FailOpenDirect, None);
        if cleanup_failed {
            Err(WorkerServiceError::ShutdownFailed)
        } else {
            Ok(())
        }
    }

    fn handle_commands(&mut self, now: Duration) -> Result<(), WorkerServiceError> {
        while let Some(command) = self.control.take_command() {
            match command {
                ControlCommand::Start => {
                    self.restart_budget.clear();
                    self.start_pending = true;
                }
                ControlCommand::Stop => self.stop()?,
                ControlCommand::Probe => {
                    let _ = self.recovery.probe();
                }
            }
        }
        if self.start_pending {
            self.start_pending = false;
            self.start(now);
        }
        Ok(())
    }

    fn tick_runtime(&mut self, now: Duration) -> Result<(), WorkerServiceError> {
        let Some(runtime) = self.runtime.as_mut() else {
            if self.restart_at.is_some_and(|deadline| now >= deadline) {
                self.start(now);
            }
            return Ok(());
        };
        let tick = runtime
            .tick(
                now,
                &mut self.network,
                &mut self.verifier,
                &mut self.restart_budget,
            )
            .map_err(|_| WorkerServiceError::TaskFailed)?;
        match tick {
            RuntimeTick::RestartScheduled { after, .. } => {
                self.runtime = None;
                self.restart_at = Some(now.saturating_add(after));
                self.publish_snapshot(RuntimeState::Backoff, None);
            }
            RuntimeTick::CircuitOpen { .. } => {
                self.runtime = None;
                self.restart_at = None;
                self.publish_snapshot(RuntimeState::CircuitOpen, None);
            }
            _ => {
                let snapshot = self.snapshot();
                self.control.update_snapshot(snapshot);
            }
        }
        Ok(())
    }
}

impl<S, N, V, C> ControlRequestHandler for WorkerApplication<S, N, V, C>
where
    N: NetworkController,
    S: RuntimeRecoverySource<N>,
    S::Process: CandidateProcess,
    V: NetworkHealthVerifier,
    C: WorkerClock,
{
    fn handle(&mut self, request: ControlRequest) -> ControlResponse {
        self.control.handle(request)
    }
}

impl<S, N, V, C> WorkerServiceTasks for WorkerApplication<S, N, V, C>
where
    N: NetworkController,
    S: RuntimeRecoverySource<N>,
    S::Process: CandidateProcess,
    V: NetworkHealthVerifier,
    C: WorkerClock,
{
    fn next_wakeup_in(&self) -> Duration {
        let now = self.clock.now();
        if self.start_pending {
            return Duration::ZERO;
        }
        let runtime = self
            .runtime
            .as_ref()
            .map(|runtime| runtime.next_wakeup_in(now));
        let restart = self.restart_at.map(|deadline| deadline.saturating_sub(now));
        runtime
            .into_iter()
            .chain(restart)
            .min()
            .unwrap_or(IDLE_WAKEUP)
    }

    fn run_ready(&mut self) -> Result<(), WorkerServiceError> {
        let now = self.clock.now();
        self.handle_commands(now)?;
        self.tick_runtime(now)
    }

    fn shutdown(&mut self) -> Result<(), WorkerServiceError> {
        self.stop()
    }
}
