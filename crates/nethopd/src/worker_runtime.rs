use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

#[cfg(feature = "subscription-update")]
use nethop_core::SealedGeneration;
use nethop_core::{GenerationId, RuntimeState};
use thiserror::Error;

use crate::{
    ActiveRuntime, CandidateProcess, NetworkController, RuntimeHealthVerifier, RuntimeStopError,
};

const MAX_INTERVAL: Duration = Duration::from_secs(60 * 60);
const MAX_FAILURE_WINDOW: Duration = Duration::from_secs(24 * 60 * 60);
const RESTART_DELAYS: [Duration; 3] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerRuntimeLimits {
    core_poll_interval: Duration,
    reconcile_interval: Duration,
    failure_window: Duration,
}

impl WorkerRuntimeLimits {
    pub fn new(
        core_poll_interval: Duration,
        reconcile_interval: Duration,
        failure_window: Duration,
    ) -> Result<Self, WorkerRuntimeError> {
        if core_poll_interval.is_zero()
            || core_poll_interval > MAX_INTERVAL
            || reconcile_interval < core_poll_interval
            || reconcile_interval > MAX_INTERVAL
            || failure_window.is_zero()
            || failure_window > MAX_FAILURE_WINDOW
        {
            return Err(WorkerRuntimeError::InvalidLimits);
        }
        Ok(Self {
            core_poll_interval,
            reconcile_interval,
            failure_window,
        })
    }

    pub const fn core_poll_interval(self) -> Duration {
        self.core_poll_interval
    }

    pub const fn reconcile_interval(self) -> Duration {
        self.reconcile_interval
    }

    pub const fn failure_window(self) -> Duration {
        self.failure_window
    }
}

impl Default for WorkerRuntimeLimits {
    fn default() -> Self {
        Self {
            core_poll_interval: Duration::from_millis(250),
            reconcile_interval: Duration::from_secs(60),
            failure_window: Duration::from_secs(5 * 60),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum WorkerRuntimeError {
    #[error("worker runtime limits are invalid")]
    InvalidLimits,
    #[error("worker runtime received non-monotonic time")]
    NonMonotonicTime,
    #[error("worker runtime has no active generation")]
    NotRunning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartDecision {
    RetryAfter(Duration),
    CircuitOpen,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestartBudget {
    failure_window: Duration,
    failures: VecDeque<Duration>,
}

impl RestartBudget {
    pub fn new(failure_window: Duration) -> Result<Self, WorkerRuntimeError> {
        if failure_window.is_zero() || failure_window > MAX_FAILURE_WINDOW {
            return Err(WorkerRuntimeError::InvalidLimits);
        }
        Ok(Self {
            failure_window,
            failures: VecDeque::with_capacity(RESTART_DELAYS.len()),
        })
    }

    pub fn register_failure(&mut self, now: Duration) -> RestartDecision {
        while self
            .failures
            .front()
            .is_some_and(|failure| now.saturating_sub(*failure) >= self.failure_window)
        {
            self.failures.pop_front();
        }
        if self.failures.len() == RESTART_DELAYS.len() {
            return RestartDecision::CircuitOpen;
        }
        let delay = RESTART_DELAYS[self.failures.len()];
        self.failures.push_back(now);
        RestartDecision::RetryAfter(delay)
    }

    pub fn clear(&mut self) {
        self.failures.clear();
    }
}

impl Default for RestartBudget {
    fn default() -> Self {
        Self {
            failure_window: WorkerRuntimeLimits::default().failure_window,
            failures: VecDeque::with_capacity(RESTART_DELAYS.len()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeFailureCode {
    CoreExited,
    CoreObserveFailed,
    DriftRepairFailed,
    DriftPersisted,
    TunUnhealthy,
}

impl RuntimeFailureCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CoreExited => "worker_runtime_core_exited",
            Self::CoreObserveFailed => "worker_runtime_core_observe_failed",
            Self::DriftRepairFailed => "worker_runtime_drift_repair_failed",
            Self::DriftPersisted => "worker_runtime_drift_persisted",
            Self::TunUnhealthy => "worker_runtime_tun_unhealthy",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeTick {
    Idle,
    Healthy,
    Reconciled,
    Repaired,
    RestartScheduled {
        after: Duration,
        failure: RuntimeFailureCode,
        cleanup_failed: bool,
    },
    CircuitOpen {
        failure: RuntimeFailureCode,
        cleanup_failed: bool,
    },
}

pub struct WorkerRuntime<P: CandidateProcess, R> {
    active: Option<ActiveRuntime<P, R>>,
    state: RuntimeState,
    limits: WorkerRuntimeLimits,
    last_tick: Duration,
    next_core_poll: Duration,
    next_reconcile: Duration,
}

impl<P: CandidateProcess, R> WorkerRuntime<P, R> {
    pub fn new(
        active: ActiveRuntime<P, R>,
        started_at: Duration,
        limits: WorkerRuntimeLimits,
    ) -> Self {
        let state = active.state();
        Self {
            active: Some(active),
            state,
            limits,
            last_tick: started_at,
            next_core_poll: started_at,
            next_reconcile: started_at.saturating_add(limits.reconcile_interval),
        }
    }

    pub const fn state(&self) -> RuntimeState {
        self.state
    }

    pub const fn has_active_runtime(&self) -> bool {
        self.active.is_some()
    }

    pub fn generation(&self) -> Option<GenerationId> {
        self.active.as_ref().map(ActiveRuntime::generation)
    }

    pub fn process_identity(&self) -> Option<crate::ProcessIdentity> {
        self.active.as_ref().map(ActiveRuntime::process_identity)
    }

    #[cfg(feature = "subscription-update")]
    pub(crate) fn active_mut(&mut self) -> Option<&mut ActiveRuntime<P, R>> {
        self.active.as_mut()
    }

    #[cfg(feature = "subscription-update")]
    pub(crate) fn commit_reload(
        &mut self,
        generation: SealedGeneration,
    ) -> Result<(), crate::ProcessError> {
        self.active
            .as_mut()
            .ok_or(crate::ProcessError::ReloadFailed)?
            .commit_reload(generation)
    }

    pub fn next_wakeup_in(&self, now: Duration) -> Duration {
        self.next_core_poll
            .min(self.next_reconcile)
            .saturating_sub(now)
    }

    pub fn request_reconcile(&mut self, now: Duration) -> Result<(), WorkerRuntimeError> {
        if now < self.last_tick {
            return Err(WorkerRuntimeError::NonMonotonicTime);
        }
        if self.active.is_none() {
            return Err(WorkerRuntimeError::NotRunning);
        }
        self.next_reconcile = self.next_reconcile.min(now);
        Ok(())
    }

    pub fn replace_limits(&mut self, limits: WorkerRuntimeLimits) {
        self.limits = limits;
        self.next_core_poll = self.last_tick.saturating_add(limits.core_poll_interval);
        self.next_reconcile = self.last_tick.saturating_add(limits.reconcile_interval);
    }

    pub fn tick<N, V>(
        &mut self,
        now: Duration,
        network: &mut N,
        verifier: &mut V,
        budget: &mut RestartBudget,
    ) -> Result<RuntimeTick, WorkerRuntimeError>
    where
        N: NetworkController<Receipt = R>,
        V: RuntimeHealthVerifier,
    {
        if now < self.last_tick {
            return Err(WorkerRuntimeError::NonMonotonicTime);
        }
        if self.active.is_none() {
            return Err(WorkerRuntimeError::NotRunning);
        }
        self.last_tick = now;
        if now < self.next_core_poll && now < self.next_reconcile {
            return Ok(RuntimeTick::Idle);
        }

        if now >= self.next_core_poll {
            let running = self
                .active
                .as_mut()
                .ok_or(WorkerRuntimeError::NotRunning)?
                .process_mut()
                .is_running();
            self.next_core_poll = now.saturating_add(self.limits.core_poll_interval);
            match running {
                Ok(true) => {}
                Ok(false) => {
                    return Ok(self.fail_runtime(
                        now,
                        RuntimeFailureCode::CoreExited,
                        network,
                        verifier,
                        budget,
                    ));
                }
                Err(_) => {
                    return Ok(self.fail_runtime(
                        now,
                        RuntimeFailureCode::CoreObserveFailed,
                        network,
                        verifier,
                        budget,
                    ));
                }
            }
        }

        if now < self.next_reconcile {
            return Ok(RuntimeTick::Healthy);
        }
        self.next_reconcile = now.saturating_add(self.limits.reconcile_interval);
        let attachment = self
            .active
            .as_ref()
            .ok_or(WorkerRuntimeError::NotRunning)?
            .attachment();
        let healthy = verifier.verify(attachment).is_ok();
        if healthy {
            return Ok(RuntimeTick::Reconciled);
        }

        if attachment.mode() == nethop_core::CaptureMode::Tun {
            return Ok(self.fail_runtime(
                now,
                RuntimeFailureCode::TunUnhealthy,
                network,
                verifier,
                budget,
            ));
        }

        let repaired = self
            .active
            .as_mut()
            .ok_or(WorkerRuntimeError::NotRunning)?
            .rebuild_network(network)
            .is_ok();
        if !repaired {
            return Ok(self.fail_runtime(
                now,
                RuntimeFailureCode::DriftRepairFailed,
                network,
                verifier,
                budget,
            ));
        }
        let verified = verifier
            .verify(
                self.active
                    .as_ref()
                    .ok_or(WorkerRuntimeError::NotRunning)?
                    .attachment(),
            )
            .is_ok();
        if !verified {
            return Ok(self.fail_runtime(
                now,
                RuntimeFailureCode::DriftPersisted,
                network,
                verifier,
                budget,
            ));
        }
        Ok(RuntimeTick::Repaired)
    }

    pub fn run<D, N, V>(
        &mut self,
        driver: &mut D,
        network: &mut N,
        verifier: &mut V,
        budget: &mut RestartBudget,
    ) -> WorkerRunExit
    where
        D: WorkerLoopDriver,
        N: NetworkController<Receipt = R>,
        V: RuntimeHealthVerifier,
    {
        loop {
            let now = driver.now();
            match self.tick(now, network, verifier, budget) {
                Ok(RuntimeTick::RestartScheduled {
                    after,
                    failure,
                    cleanup_failed,
                }) => {
                    return WorkerRunExit::RestartScheduled {
                        after,
                        failure,
                        cleanup_failed,
                    };
                }
                Ok(RuntimeTick::CircuitOpen {
                    failure,
                    cleanup_failed,
                }) => {
                    return WorkerRunExit::CircuitOpen {
                        failure,
                        cleanup_failed,
                    };
                }
                Ok(_) => {}
                Err(error) => {
                    let cleanup_failed = self.stop(network, verifier).is_err();
                    return WorkerRunExit::Fatal {
                        error,
                        cleanup_failed,
                    };
                }
            }
            if driver.wait(self.next_wakeup_in(driver.now())) == WorkerLoopSignal::Stop {
                let cleanup_failed = self.stop(network, verifier).is_err();
                return WorkerRunExit::Stopped { cleanup_failed };
            }
        }
    }

    pub fn stop<N, V>(&mut self, network: &mut N, verifier: &mut V) -> Result<(), RuntimeStopError>
    where
        N: NetworkController<Receipt = R>,
        V: RuntimeHealthVerifier,
    {
        self.state = RuntimeState::Stopping;
        let Some(active) = self.active.take() else {
            return Ok(());
        };
        active.stop(network, verifier)
    }

    fn fail_runtime<N, V>(
        &mut self,
        now: Duration,
        failure: RuntimeFailureCode,
        network: &mut N,
        verifier: &mut V,
        budget: &mut RestartBudget,
    ) -> RuntimeTick
    where
        N: NetworkController<Receipt = R>,
        V: RuntimeHealthVerifier,
    {
        self.state = RuntimeState::Degraded;
        let cleanup_failed = self
            .active
            .take()
            .is_some_and(|active| active.stop(network, verifier).is_err());
        self.state = RuntimeState::FailOpenDirect;
        match budget.register_failure(now) {
            RestartDecision::RetryAfter(after) => {
                self.state = RuntimeState::Backoff;
                RuntimeTick::RestartScheduled {
                    after,
                    failure,
                    cleanup_failed,
                }
            }
            RestartDecision::CircuitOpen => {
                self.state = RuntimeState::CircuitOpen;
                RuntimeTick::CircuitOpen {
                    failure,
                    cleanup_failed,
                }
            }
        }
    }
}

pub trait WorkerLoopDriver {
    fn now(&self) -> Duration;
    fn wait(&mut self, timeout: Duration) -> WorkerLoopSignal;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerLoopSignal {
    Wake,
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerRunExit {
    Stopped {
        cleanup_failed: bool,
    },
    RestartScheduled {
        after: Duration,
        failure: RuntimeFailureCode,
        cleanup_failed: bool,
    },
    CircuitOpen {
        failure: RuntimeFailureCode,
        cleanup_failed: bool,
    },
    Fatal {
        error: WorkerRuntimeError,
        cleanup_failed: bool,
    },
}

#[derive(Debug, Clone)]
pub struct WorkerStopHandle {
    requested: Arc<AtomicBool>,
}

impl WorkerStopHandle {
    pub fn request_stop(&self) {
        self.requested.store(true, Ordering::Release);
    }
}

#[derive(Debug)]
pub struct SystemLoopDriver {
    started: Instant,
    stop_requested: Arc<AtomicBool>,
}

impl SystemLoopDriver {
    pub fn new() -> (Self, WorkerStopHandle) {
        let stop_requested = Arc::new(AtomicBool::new(false));
        (
            Self {
                started: Instant::now(),
                stop_requested: Arc::clone(&stop_requested),
            },
            WorkerStopHandle {
                requested: stop_requested,
            },
        )
    }
}

impl WorkerLoopDriver for SystemLoopDriver {
    fn now(&self) -> Duration {
        self.started.elapsed()
    }

    fn wait(&mut self, timeout: Duration) -> WorkerLoopSignal {
        if self.stop_requested.load(Ordering::Acquire) {
            return WorkerLoopSignal::Stop;
        }
        thread::sleep(timeout);
        if self.stop_requested.load(Ordering::Acquire) {
            WorkerLoopSignal::Stop
        } else {
            WorkerLoopSignal::Wake
        }
    }
}
