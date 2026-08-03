use nethop_android::{
    ApplyReceipt, CapabilityError, CapabilityProbe, CapabilityReport, ExecutionError,
    NetworkCommandBackend, NetworkExecutor, NetworkHealthVerifier, NetworkPlan, NetworkPlanError,
    NetworkPlanner, PlanSlot, ProbeBackend,
};
use nethop_core::{Candidate, CapturePolicy, GenerationId, RuntimeState};
use thiserror::Error;

use crate::{
    ActivationDiagnosticCode, ActiveGeneration, CandidateActivator, CandidateChecker,
    CandidateProcess, CoreLauncher, HealthProbe, SafetyAuditor,
};

pub trait CapabilitySource {
    fn probe(&mut self) -> Result<CapabilityReport, CapabilityError>;
}

impl<B: ProbeBackend> CapabilitySource for CapabilityProbe<B> {
    fn probe(&mut self) -> Result<CapabilityReport, CapabilityError> {
        CapabilityProbe::probe(self)
    }
}

pub trait NetworkController {
    type Receipt;

    fn apply(&mut self, plan: &NetworkPlan) -> Result<Self::Receipt, ExecutionError>;

    fn rollback(
        &mut self,
        plan: &NetworkPlan,
        receipt: &mut Self::Receipt,
    ) -> Result<(), ExecutionError>;
}

impl<B: NetworkCommandBackend> NetworkController for NetworkExecutor<B> {
    type Receipt = ApplyReceipt;

    fn apply(&mut self, plan: &NetworkPlan) -> Result<Self::Receipt, ExecutionError> {
        NetworkExecutor::apply(self, plan)
    }

    fn rollback(
        &mut self,
        plan: &NetworkPlan,
        receipt: &mut Self::Receipt,
    ) -> Result<(), ExecutionError> {
        NetworkExecutor::rollback(self, plan, receipt)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum DataPlaneHealthError {
    #[error("candidate core exited during data-plane verification")]
    CoreExited,
    #[error("candidate core state could not be observed during data-plane verification")]
    CoreObserveFailed,
    #[error("candidate network plan is unhealthy")]
    NetworkUnhealthy,
}

impl DataPlaneHealthError {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CoreExited => "data_plane_core_exited",
            Self::CoreObserveFailed => "data_plane_core_observe_failed",
            Self::NetworkUnhealthy => "data_plane_network_unhealthy",
        }
    }
}

pub trait DataPlaneHealthProbe<P: CandidateProcess> {
    fn wait_healthy(
        &mut self,
        process: &mut P,
        plan: &NetworkPlan,
        capabilities: &CapabilityReport,
    ) -> Result<(), DataPlaneHealthError>;
}

#[derive(Debug)]
pub struct NetworkDataPlaneHealthProbe<V> {
    verifier: V,
}

impl<V> NetworkDataPlaneHealthProbe<V> {
    pub const fn new(verifier: V) -> Self {
        Self { verifier }
    }

    pub fn into_verifier(self) -> V {
        self.verifier
    }
}

impl<P, V> DataPlaneHealthProbe<P> for NetworkDataPlaneHealthProbe<V>
where
    P: CandidateProcess,
    V: NetworkHealthVerifier,
{
    fn wait_healthy(
        &mut self,
        process: &mut P,
        plan: &NetworkPlan,
        _capabilities: &CapabilityReport,
    ) -> Result<(), DataPlaneHealthError> {
        match process.is_running() {
            Ok(true) => {}
            Ok(false) => return Err(DataPlaneHealthError::CoreExited),
            Err(_) => return Err(DataPlaneHealthError::CoreObserveFailed),
        }
        self.verifier
            .verify(plan)
            .map_err(|_| DataPlaneHealthError::NetworkUnhealthy)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerActivationDiagnosticCode {
    InvalidState,
    CapabilityProbeFailed,
    NetworkPlanRejected,
    CoreActivationFailed,
    NetworkApplyFailed,
    DataPlaneHealthFailed,
    CommitFailed,
}

impl WorkerActivationDiagnosticCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidState => "worker_activation_invalid_state",
            Self::CapabilityProbeFailed => "worker_capability_probe_failed",
            Self::NetworkPlanRejected => "worker_network_plan_rejected",
            Self::CoreActivationFailed => "worker_core_activation_failed",
            Self::NetworkApplyFailed => "worker_network_apply_failed",
            Self::DataPlaneHealthFailed => "worker_data_plane_health_failed",
            Self::CommitFailed => "worker_generation_commit_failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("worker activation failed: {code}")]
pub struct WorkerActivationError {
    code: &'static str,
    diagnostic_code: WorkerActivationDiagnosticCode,
    cause_code: Option<&'static str>,
    cleanup_failed: bool,
}

impl WorkerActivationError {
    fn new(
        diagnostic_code: WorkerActivationDiagnosticCode,
        cause_code: Option<&'static str>,
        cleanup_failed: bool,
    ) -> Self {
        Self {
            code: diagnostic_code.as_str(),
            diagnostic_code,
            cause_code,
            cleanup_failed,
        }
    }

    pub const fn code(&self) -> WorkerActivationDiagnosticCode {
        self.diagnostic_code
    }

    pub const fn cause_code(&self) -> Option<&'static str> {
        self.cause_code
    }

    pub const fn cleanup_failed(&self) -> bool {
        self.cleanup_failed
    }
}

#[derive(Debug)]
pub struct ActiveRuntime<P: CandidateProcess, R> {
    active: ActiveGeneration<P>,
    plan: NetworkPlan,
    receipt: R,
    capabilities: CapabilityReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub(crate) enum RuntimeReconcileError {
    #[error("active network plan could not be withdrawn for reconcile")]
    WithdrawFailed,
    #[error("active network plan could not be reapplied for reconcile")]
    ReapplyFailed,
}

impl<P: CandidateProcess, R> ActiveRuntime<P, R> {
    pub const fn generation(&self) -> GenerationId {
        self.active.generation()
    }

    pub const fn plan(&self) -> &NetworkPlan {
        &self.plan
    }

    pub const fn capabilities(&self) -> &CapabilityReport {
        &self.capabilities
    }

    pub fn process_mut(&mut self) -> &mut P {
        self.active.process_mut()
    }

    pub(crate) fn rebuild_network<N>(
        &mut self,
        network: &mut N,
    ) -> Result<(), RuntimeReconcileError>
    where
        N: NetworkController<Receipt = R>,
    {
        network
            .rollback(&self.plan, &mut self.receipt)
            .map_err(|_| RuntimeReconcileError::WithdrawFailed)?;
        let receipt = network
            .apply(&self.plan)
            .map_err(|_| RuntimeReconcileError::ReapplyFailed)?;
        self.receipt = receipt;
        Ok(())
    }

    pub fn stop<N>(mut self, network: &mut N) -> Result<(), RuntimeStopError>
    where
        N: NetworkController<Receipt = R>,
    {
        let network_failed = network.rollback(&self.plan, &mut self.receipt).is_err();
        let core_failed = self.active.stop().is_err();
        if network_failed || core_failed {
            Err(RuntimeStopError {
                network_failed,
                core_failed,
            })
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("active runtime stop failed")]
pub struct RuntimeStopError {
    network_failed: bool,
    core_failed: bool,
}

impl RuntimeStopError {
    pub const fn network_failed(&self) -> bool {
        self.network_failed
    }

    pub const fn core_failed(&self) -> bool {
        self.core_failed
    }
}

pub struct WorkerActivator<'a, C, L, A, H, S, N, D> {
    core: CandidateActivator<'a, C, L, A, H>,
    capability_source: &'a mut S,
    network: &'a mut N,
    data_plane_health: &'a mut D,
    state: RuntimeState,
}

impl<'a, C, L, A, H, S, N, D> WorkerActivator<'a, C, L, A, H, S, N, D> {
    pub const fn new(
        core: CandidateActivator<'a, C, L, A, H>,
        capability_source: &'a mut S,
        network: &'a mut N,
        data_plane_health: &'a mut D,
    ) -> Self {
        Self {
            core,
            capability_source,
            network,
            data_plane_health,
            state: RuntimeState::Init,
        }
    }

    pub const fn state(&self) -> RuntimeState {
        self.state
    }
}

impl<C, L, A, H, S, N, D> WorkerActivator<'_, C, L, A, H, S, N, D>
where
    C: CandidateChecker,
    L: CoreLauncher,
    A: SafetyAuditor,
    H: HealthProbe<L::Process>,
    S: CapabilitySource,
    N: NetworkController,
    D: DataPlaneHealthProbe<L::Process>,
{
    pub fn activate(
        &mut self,
        candidate: &Candidate,
        policy: &CapturePolicy,
        slot: PlanSlot,
    ) -> Result<ActiveRuntime<L::Process, N::Receipt>, WorkerActivationError> {
        self.transition_to(RuntimeState::Probing)?;
        let capabilities = match self.capability_source.probe() {
            Ok(report) => report,
            Err(error) => {
                return Err(self.fail_open(
                    WorkerActivationDiagnosticCode::CapabilityProbeFailed,
                    Some(error.code().as_str()),
                    false,
                ));
            }
        };
        let plan = match NetworkPlanner.build_tproxy(
            candidate.generation(),
            slot,
            policy,
            &capabilities,
        ) {
            Ok(plan) => plan,
            Err(error) => {
                return Err(self.plan_failure(error));
            }
        };
        self.transition_to(RuntimeState::StartingCore)?;
        let mut staged = match self.core.stage(candidate) {
            Ok(staged) => staged,
            Err(error) => {
                return Err(self.fail_open(
                    WorkerActivationDiagnosticCode::CoreActivationFailed,
                    Some(error.code().as_str()),
                    error.cleanup_failed(),
                ));
            }
        };
        debug_assert_eq!(staged.generation(), plan.generation());
        let mut receipt = match self.network.apply(&plan) {
            Ok(receipt) => receipt,
            Err(error) => {
                let core_cleanup_failed = self.core.abort_staged(staged);
                return Err(self.fail_open(
                    WorkerActivationDiagnosticCode::NetworkApplyFailed,
                    Some(error.code().as_str()),
                    core_cleanup_failed
                        || matches!(error, ExecutionError::ApplyRollbackFailed { .. }),
                ));
            }
        };
        if let Err(error) =
            self.data_plane_health
                .wait_healthy(staged.process_mut(), &plan, &capabilities)
        {
            let network_cleanup_failed = self.network.rollback(&plan, &mut receipt).is_err();
            let core_cleanup_failed = self.core.abort_staged(staged);
            return Err(self.fail_open(
                WorkerActivationDiagnosticCode::DataPlaneHealthFailed,
                Some(error.as_str()),
                network_cleanup_failed || core_cleanup_failed,
            ));
        }
        let active = match self.core.commit_staged(staged) {
            Ok(active) => active,
            Err(staged) => {
                let network_cleanup_failed = self.network.rollback(&plan, &mut receipt).is_err();
                let core_cleanup_failed = self.core.abort_staged(staged);
                return Err(self.fail_open(
                    WorkerActivationDiagnosticCode::CommitFailed,
                    Some(ActivationDiagnosticCode::CommitFailed.as_str()),
                    network_cleanup_failed || core_cleanup_failed,
                ));
            }
        };
        self.state = RuntimeState::RunningTproxy;
        Ok(ActiveRuntime {
            active,
            plan,
            receipt,
            capabilities,
        })
    }

    fn plan_failure(&mut self, error: NetworkPlanError) -> WorkerActivationError {
        self.fail_open(
            WorkerActivationDiagnosticCode::NetworkPlanRejected,
            Some(error.code().as_str()),
            false,
        )
    }

    fn transition_to(&mut self, next: RuntimeState) -> Result<(), WorkerActivationError> {
        self.state = self.state.transition(next).map_err(|_| {
            WorkerActivationError::new(WorkerActivationDiagnosticCode::InvalidState, None, false)
        })?;
        Ok(())
    }

    fn fail_open(
        &mut self,
        code: WorkerActivationDiagnosticCode,
        cause_code: Option<&'static str>,
        cleanup_failed: bool,
    ) -> WorkerActivationError {
        self.state = self
            .state
            .transition(RuntimeState::FailOpenDirect)
            .unwrap_or(RuntimeState::FailOpenDirect);
        WorkerActivationError::new(code, cause_code, cleanup_failed)
    }
}
