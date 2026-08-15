use nethop_android::{
    ApplyReceipt, CapabilityError, CapabilityProbe, CapabilityReport, ExecutionError,
    NetworkCommandBackend, NetworkExecutor, NetworkHealthError, NetworkHealthVerifier, NetworkPlan,
    NetworkPlanError, NetworkPlanner, PlanSlot, ProbeBackend, TunFallbackPlanner,
    default_tun_interface,
};
use nethop_core::{
    Candidate, CaptureMode, CapturePolicy, GenerationId, GenerationStore, RuntimeState,
    SealedGeneration,
};
use thiserror::Error;

use crate::{
    ActivationDiagnosticCode, ActiveGeneration, CandidateActivator, CandidateChecker,
    CandidateProcess, CoreLauncher, HealthProbe, ProcessIdentity, SafetyAuditor, TunRuntime,
};

pub trait CapabilitySource {
    fn probe(&mut self) -> Result<CapabilityReport, CapabilityError>;

    fn replace_policy(
        &mut self,
        _candidates: Vec<nethop_android::ResourceCandidate>,
        _inbound_port: u16,
    ) -> Result<(), CapabilityError> {
        Ok(())
    }
}

impl<B: ProbeBackend> CapabilitySource for CapabilityProbe<B> {
    fn probe(&mut self) -> Result<CapabilityReport, CapabilityError> {
        CapabilityProbe::probe(self)
    }

    fn replace_policy(
        &mut self,
        candidates: Vec<nethop_android::ResourceCandidate>,
        inbound_port: u16,
    ) -> Result<(), CapabilityError> {
        CapabilityProbe::replace_policy(self, candidates, inbound_port)
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
    NetworkUnhealthy { cause: NetworkHealthError },
    #[error("candidate TUN interface is unhealthy")]
    TunUnhealthy,
    #[error("candidate TUN interface remained after core shutdown")]
    TunCleanupFailed,
}

impl DataPlaneHealthError {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CoreExited => "data_plane_core_exited",
            Self::CoreObserveFailed => "data_plane_core_observe_failed",
            Self::NetworkUnhealthy { .. } => "data_plane_network_unhealthy",
            Self::TunUnhealthy => "data_plane_tun_unhealthy",
            Self::TunCleanupFailed => "data_plane_tun_cleanup_failed",
        }
    }

    pub const fn cause_code(self) -> &'static str {
        match self {
            Self::NetworkUnhealthy { cause } => cause.code().as_str(),
            _ => self.as_str(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum RuntimeAttachmentView<'a> {
    Tproxy(&'a NetworkPlan),
    Tun { interface: &'a str },
}

impl RuntimeAttachmentView<'_> {
    pub const fn mode(self) -> CaptureMode {
        match self {
            Self::Tproxy(_) => CaptureMode::Tproxy,
            Self::Tun { .. } => CaptureMode::Tun,
        }
    }
}

pub trait DataPlaneHealthProbe<P: CandidateProcess> {
    fn wait_healthy(
        &mut self,
        process: &mut P,
        attachment: RuntimeAttachmentView<'_>,
        capabilities: &CapabilityReport,
    ) -> Result<(), DataPlaneHealthError>;

    fn wait_stopped(
        &mut self,
        _attachment: RuntimeAttachmentView<'_>,
    ) -> Result<(), DataPlaneHealthError> {
        Ok(())
    }

    fn replace_inbound_port(&mut self, _inbound_port: u16) -> Result<(), DataPlaneHealthError> {
        Ok(())
    }

    fn replace_health_timeout(
        &mut self,
        _health_timeout: std::time::Duration,
    ) -> Result<(), DataPlaneHealthError> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct AndroidDataPlaneHealthProbe<N, T> {
    network: N,
    tun: T,
}

impl<N, T> AndroidDataPlaneHealthProbe<N, T> {
    pub const fn new(network: N, tun: T) -> Self {
        Self { network, tun }
    }

    pub fn into_parts(self) -> (N, T) {
        (self.network, self.tun)
    }
}

impl<P, N, T> DataPlaneHealthProbe<P> for AndroidDataPlaneHealthProbe<N, T>
where
    P: CandidateProcess,
    N: NetworkHealthVerifier,
    T: TunRuntime,
{
    fn wait_healthy(
        &mut self,
        process: &mut P,
        attachment: RuntimeAttachmentView<'_>,
        _capabilities: &CapabilityReport,
    ) -> Result<(), DataPlaneHealthError> {
        match attachment {
            RuntimeAttachmentView::Tproxy(plan) => {
                match process.is_running() {
                    Ok(true) => {}
                    Ok(false) => return Err(DataPlaneHealthError::CoreExited),
                    Err(_) => return Err(DataPlaneHealthError::CoreObserveFailed),
                }
                self.network
                    .verify(plan)
                    .map_err(|cause| DataPlaneHealthError::NetworkUnhealthy { cause })
            }
            RuntimeAttachmentView::Tun { .. } => self
                .tun
                .wait_healthy(process)
                .map_err(|_| DataPlaneHealthError::TunUnhealthy),
        }
    }

    fn wait_stopped(
        &mut self,
        attachment: RuntimeAttachmentView<'_>,
    ) -> Result<(), DataPlaneHealthError> {
        match attachment {
            RuntimeAttachmentView::Tproxy(_) => Ok(()),
            RuntimeAttachmentView::Tun { .. } => self
                .tun
                .wait_stopped()
                .map_err(|_| DataPlaneHealthError::TunCleanupFailed),
        }
    }

    fn replace_inbound_port(&mut self, inbound_port: u16) -> Result<(), DataPlaneHealthError> {
        self.network
            .replace_inbound_port(inbound_port)
            .map_err(|cause| DataPlaneHealthError::NetworkUnhealthy { cause })
    }

    fn replace_health_timeout(
        &mut self,
        health_timeout: std::time::Duration,
    ) -> Result<(), DataPlaneHealthError> {
        self.tun
            .replace_timeout(health_timeout)
            .map_err(|_| DataPlaneHealthError::TunUnhealthy)
    }
}

pub trait RuntimeHealthVerifier {
    fn verify(&mut self, attachment: RuntimeAttachmentView<'_>)
    -> Result<(), DataPlaneHealthError>;

    fn wait_stopped(
        &mut self,
        attachment: RuntimeAttachmentView<'_>,
    ) -> Result<(), DataPlaneHealthError>;

    fn replace_inbound_port(&mut self, _inbound_port: u16) -> Result<(), DataPlaneHealthError> {
        Ok(())
    }

    fn replace_health_timeout(
        &mut self,
        _health_timeout: std::time::Duration,
    ) -> Result<(), DataPlaneHealthError> {
        Ok(())
    }
}

impl<V: NetworkHealthVerifier> RuntimeHealthVerifier for V {
    fn verify(
        &mut self,
        attachment: RuntimeAttachmentView<'_>,
    ) -> Result<(), DataPlaneHealthError> {
        match attachment {
            RuntimeAttachmentView::Tproxy(plan) => NetworkHealthVerifier::verify(self, plan)
                .map_err(|cause| DataPlaneHealthError::NetworkUnhealthy { cause }),
            RuntimeAttachmentView::Tun { .. } => Err(DataPlaneHealthError::TunUnhealthy),
        }
    }

    fn wait_stopped(
        &mut self,
        _attachment: RuntimeAttachmentView<'_>,
    ) -> Result<(), DataPlaneHealthError> {
        Ok(())
    }

    fn replace_inbound_port(&mut self, inbound_port: u16) -> Result<(), DataPlaneHealthError> {
        NetworkHealthVerifier::replace_inbound_port(self, inbound_port)
            .map_err(|cause| DataPlaneHealthError::NetworkUnhealthy { cause })
    }
}

#[derive(Debug)]
pub struct TproxyDataPlaneHealthProbe<V> {
    verifier: V,
}

impl<V> TproxyDataPlaneHealthProbe<V> {
    pub const fn new(verifier: V) -> Self {
        Self { verifier }
    }
}

impl<P, V> DataPlaneHealthProbe<P> for TproxyDataPlaneHealthProbe<V>
where
    P: CandidateProcess,
    V: NetworkHealthVerifier,
{
    fn wait_healthy(
        &mut self,
        process: &mut P,
        attachment: RuntimeAttachmentView<'_>,
        _capabilities: &CapabilityReport,
    ) -> Result<(), DataPlaneHealthError> {
        match process.is_running() {
            Ok(true) => {}
            Ok(false) => return Err(DataPlaneHealthError::CoreExited),
            Err(_) => return Err(DataPlaneHealthError::CoreObserveFailed),
        }
        let RuntimeAttachmentView::Tproxy(plan) = attachment else {
            return Err(DataPlaneHealthError::TunUnhealthy);
        };
        self.verifier
            .verify(plan)
            .map_err(|cause| DataPlaneHealthError::NetworkUnhealthy { cause })
    }

    fn replace_inbound_port(&mut self, inbound_port: u16) -> Result<(), DataPlaneHealthError> {
        self.verifier
            .replace_inbound_port(inbound_port)
            .map_err(|cause| DataPlaneHealthError::NetworkUnhealthy { cause })
    }
}

impl<N, T> RuntimeHealthVerifier for AndroidDataPlaneHealthProbe<N, T>
where
    N: NetworkHealthVerifier,
    T: TunRuntime,
{
    fn verify(
        &mut self,
        attachment: RuntimeAttachmentView<'_>,
    ) -> Result<(), DataPlaneHealthError> {
        match attachment {
            RuntimeAttachmentView::Tproxy(plan) => self
                .network
                .verify(plan)
                .map_err(|cause| DataPlaneHealthError::NetworkUnhealthy { cause }),
            RuntimeAttachmentView::Tun { .. } => self
                .tun
                .verify_active()
                .map_err(|_| DataPlaneHealthError::TunUnhealthy),
        }
    }

    fn wait_stopped(
        &mut self,
        attachment: RuntimeAttachmentView<'_>,
    ) -> Result<(), DataPlaneHealthError> {
        match attachment {
            RuntimeAttachmentView::Tproxy(_) => Ok(()),
            RuntimeAttachmentView::Tun { .. } => self
                .tun
                .wait_stopped()
                .map_err(|_| DataPlaneHealthError::TunCleanupFailed),
        }
    }

    fn replace_inbound_port(&mut self, inbound_port: u16) -> Result<(), DataPlaneHealthError> {
        self.network
            .replace_inbound_port(inbound_port)
            .map_err(|cause| DataPlaneHealthError::NetworkUnhealthy { cause })
    }

    fn replace_health_timeout(
        &mut self,
        health_timeout: std::time::Duration,
    ) -> Result<(), DataPlaneHealthError> {
        self.tun
            .replace_timeout(health_timeout)
            .map_err(|_| DataPlaneHealthError::TunUnhealthy)
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
    attachment: RuntimeAttachment<R>,
    capabilities: CapabilityReport,
}

#[derive(Debug)]
pub enum RuntimeAttachment<R> {
    Tproxy { plan: NetworkPlan, receipt: R },
    Tun,
}

enum PreparedAttachment {
    Tproxy(NetworkPlan),
    Tun,
}

fn prepare_attachment(
    generation: GenerationId,
    policy: &CapturePolicy,
    slot: PlanSlot,
    capabilities: &CapabilityReport,
) -> Result<PreparedAttachment, &'static str> {
    match policy.mode() {
        CaptureMode::Tproxy => NetworkPlanner
            .build_tproxy(generation, slot, policy, capabilities)
            .map(PreparedAttachment::Tproxy)
            .map_err(|error| error.code().as_str()),
        CaptureMode::Tun => TunFallbackPlanner
            .build(capabilities)
            .map(|_| PreparedAttachment::Tun)
            .map_err(|error| error.as_str()),
        CaptureMode::Direct => Err(NetworkPlanError::UnsupportedCaptureMode.code().as_str()),
    }
}

impl<R> RuntimeAttachment<R> {
    pub const fn view(&self) -> RuntimeAttachmentView<'_> {
        match self {
            Self::Tproxy { plan, .. } => RuntimeAttachmentView::Tproxy(plan),
            Self::Tun => RuntimeAttachmentView::Tun {
                interface: default_tun_interface(),
            },
        }
    }

    pub const fn state(&self) -> RuntimeState {
        match self {
            Self::Tproxy { .. } => RuntimeState::RunningTproxy,
            Self::Tun => RuntimeState::RunningTun,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub(crate) enum RuntimeReconcileError {
    #[error("active network plan could not be withdrawn for reconcile")]
    WithdrawFailed,
    #[error("active network plan could not be reapplied for reconcile")]
    ReapplyFailed,
    #[error("TUN attachment cannot be rebuilt with netfilter operations")]
    UnsupportedAttachment,
}

impl<P: CandidateProcess, R> ActiveRuntime<P, R> {
    pub const fn generation(&self) -> GenerationId {
        self.active.generation()
    }

    pub const fn attachment(&self) -> RuntimeAttachmentView<'_> {
        self.attachment.view()
    }

    pub const fn state(&self) -> RuntimeState {
        self.attachment.state()
    }

    pub const fn capabilities(&self) -> &CapabilityReport {
        &self.capabilities
    }

    pub fn process_identity(&self) -> ProcessIdentity {
        self.active.identity()
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
        match &mut self.attachment {
            RuntimeAttachment::Tproxy { plan, receipt } => {
                network
                    .rollback(plan, receipt)
                    .map_err(|_| RuntimeReconcileError::WithdrawFailed)?;
                *receipt = network
                    .apply(plan)
                    .map_err(|_| RuntimeReconcileError::ReapplyFailed)?;
                Ok(())
            }
            RuntimeAttachment::Tun => Err(RuntimeReconcileError::UnsupportedAttachment),
        }
    }

    pub fn stop<N, V>(mut self, network: &mut N, verifier: &mut V) -> Result<(), RuntimeStopError>
    where
        N: NetworkController<Receipt = R>,
        V: RuntimeHealthVerifier,
    {
        let network_failed = match &mut self.attachment {
            RuntimeAttachment::Tproxy { plan, receipt } => network.rollback(plan, receipt).is_err(),
            RuntimeAttachment::Tun => false,
        };
        let core_failed = self.active.stop().is_err();
        let data_plane_failed = verifier.wait_stopped(self.attachment.view()).is_err();
        if network_failed || core_failed || data_plane_failed {
            Err(RuntimeStopError {
                network_failed,
                core_failed,
                data_plane_failed,
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
    data_plane_failed: bool,
}

impl RuntimeStopError {
    pub const fn network_failed(&self) -> bool {
        self.network_failed
    }

    pub const fn core_failed(&self) -> bool {
        self.core_failed
    }

    pub const fn data_plane_failed(&self) -> bool {
        self.data_plane_failed
    }
}

pub struct WorkerActivator<'a, C, L, A, H, S, N, D> {
    core: CandidateActivator<'a, C, L, A, H>,
    capability_source: &'a mut S,
    network: &'a mut N,
    data_plane_health: &'a mut D,
    state: RuntimeState,
}

pub struct CurrentGenerationActivator<'a, C, L, H, S, N, D> {
    store: &'a GenerationStore,
    checker: &'a C,
    launcher: &'a L,
    core_health: &'a H,
    capability_source: &'a mut S,
    network: &'a mut N,
    data_plane_health: &'a mut D,
}

pub type WorkerRecovery<P, R> = Result<Option<ActiveRuntime<P, R>>, WorkerRecoveryError>;

impl<'a, C, L, H, S, N, D> CurrentGenerationActivator<'a, C, L, H, S, N, D> {
    pub const fn new(
        store: &'a GenerationStore,
        checker: &'a C,
        launcher: &'a L,
        core_health: &'a H,
        capability_source: &'a mut S,
        network: &'a mut N,
        data_plane_health: &'a mut D,
    ) -> Self {
        Self {
            store,
            checker,
            launcher,
            core_health,
            capability_source,
            network,
            data_plane_health,
        }
    }
}

impl<C, L, H, S, N, D> CurrentGenerationActivator<'_, C, L, H, S, N, D>
where
    C: CandidateChecker,
    L: CoreLauncher,
    H: HealthProbe<L::Process>,
    S: CapabilitySource,
    N: NetworkController,
    D: DataPlaneHealthProbe<L::Process>,
{
    pub fn recover(
        &mut self,
        policy: &CapturePolicy,
        slot: PlanSlot,
    ) -> WorkerRecovery<L::Process, N::Receipt> {
        let Some(generation) = self
            .store
            .current_sealed_generation()
            .map_err(|_| WorkerRecoveryError::InvalidCurrentGeneration)?
        else {
            return Ok(None);
        };
        self.recover_sealed(generation, policy, slot).map(Some)
    }

    pub fn recover_generation(
        &mut self,
        generation: GenerationId,
        policy: &CapturePolicy,
        slot: PlanSlot,
    ) -> WorkerRecovery<L::Process, N::Receipt> {
        let generation = self
            .store
            .sealed_generation(generation)
            .map_err(|_| WorkerRecoveryError::InvalidCurrentGeneration)?;
        self.recover_sealed(generation, policy, slot).map(Some)
    }

    fn recover_sealed(
        &mut self,
        generation: SealedGeneration,
        policy: &CapturePolicy,
        slot: PlanSlot,
    ) -> Result<ActiveRuntime<L::Process, N::Receipt>, WorkerRecoveryError> {
        self.checker
            .check(&generation.config_path())
            .map_err(|_| WorkerRecoveryError::CoreCheckFailed)?;
        let capabilities = self
            .capability_source
            .probe()
            .map_err(|_| WorkerRecoveryError::CapabilityProbeFailed)?;
        let prepared = prepare_attachment(generation.generation(), policy, slot, &capabilities)
            .map_err(|_| WorkerRecoveryError::NetworkPlanRejected)?;
        let mut process = self
            .launcher
            .start(&generation.config_path())
            .map_err(|_| WorkerRecoveryError::CoreStartFailed)?;
        if self.core_health.wait_healthy(&mut process).is_err() {
            let cleanup_failed = process.stop().is_err();
            return Err(WorkerRecoveryError::CoreHealthFailed { cleanup_failed });
        }
        let mut attachment = match prepared {
            PreparedAttachment::Tproxy(plan) => match self.network.apply(&plan) {
                Ok(receipt) => RuntimeAttachment::Tproxy { plan, receipt },
                Err(error) => {
                    let cleanup_failed = process.stop().is_err()
                        || matches!(error, ExecutionError::ApplyRollbackFailed { .. });
                    return Err(WorkerRecoveryError::NetworkApplyFailed {
                        error,
                        cleanup_failed,
                    });
                }
            },
            PreparedAttachment::Tun => RuntimeAttachment::Tun,
        };
        if let Err(error) =
            self.data_plane_health
                .wait_healthy(&mut process, attachment.view(), &capabilities)
        {
            let network_failed = match &mut attachment {
                RuntimeAttachment::Tproxy { plan, receipt } => {
                    self.network.rollback(plan, receipt).is_err()
                }
                RuntimeAttachment::Tun => false,
            };
            let core_failed = process.stop().is_err();
            let data_plane_failed = self
                .data_plane_health
                .wait_stopped(attachment.view())
                .is_err();
            let cleanup_failed = network_failed || core_failed || data_plane_failed;
            return Err(WorkerRecoveryError::DataPlaneHealthFailed {
                error,
                cleanup_failed,
            });
        }
        Ok(ActiveRuntime {
            active: ActiveGeneration::recovered(generation, process),
            attachment,
            capabilities,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum WorkerRecoveryError {
    #[error("current generation is missing, incomplete, or invalid")]
    InvalidCurrentGeneration,
    #[error("Android capability probe failed during recovery")]
    CapabilityProbeFailed,
    #[error("current generation failed sing-box check")]
    CoreCheckFailed,
    #[error("current generation network plan was rejected")]
    NetworkPlanRejected,
    #[error("current generation core could not be started")]
    CoreStartFailed,
    #[error("current generation core failed startup health")]
    CoreHealthFailed { cleanup_failed: bool },
    #[error("current generation network plan could not be applied")]
    NetworkApplyFailed {
        error: ExecutionError,
        cleanup_failed: bool,
    },
    #[error("current generation data plane failed health verification")]
    DataPlaneHealthFailed {
        error: DataPlaneHealthError,
        cleanup_failed: bool,
    },
}

impl WorkerRecoveryError {
    pub const fn diagnostic_code(self) -> &'static str {
        match self {
            Self::InvalidCurrentGeneration => "worker_invalid_current_generation",
            Self::CapabilityProbeFailed => "worker_capability_probe_failed",
            Self::CoreCheckFailed => "worker_core_check_failed",
            Self::NetworkPlanRejected => "worker_network_plan_rejected",
            Self::CoreStartFailed => "worker_core_start_failed",
            Self::CoreHealthFailed { .. } => "worker_core_health_failed",
            Self::NetworkApplyFailed { .. } => "worker_network_apply_failed",
            Self::DataPlaneHealthFailed { .. } => "worker_data_plane_health_failed",
        }
    }

    pub const fn cause_code(self) -> Option<&'static str> {
        match self {
            Self::NetworkApplyFailed { error, .. } => Some(error.code().as_str()),
            Self::DataPlaneHealthFailed { error, .. } => Some(error.cause_code()),
            _ => None,
        }
    }

    pub const fn apply_step(self) -> Option<usize> {
        match self {
            Self::NetworkApplyFailed {
                error: ExecutionError::ApplyFailed { step },
                ..
            } => Some(step),
            Self::NetworkApplyFailed {
                error: ExecutionError::ApplyRollbackFailed { apply_step, .. },
                ..
            } => Some(apply_step),
            _ => None,
        }
    }

    pub const fn rollback_step(self) -> Option<usize> {
        match self {
            Self::NetworkApplyFailed {
                error: ExecutionError::ApplyRollbackFailed { rollback_step, .. },
                ..
            } => Some(rollback_step),
            _ => None,
        }
    }

    pub const fn cleanup_failed(self) -> bool {
        match self {
            Self::CoreHealthFailed { cleanup_failed }
            | Self::NetworkApplyFailed { cleanup_failed, .. }
            | Self::DataPlaneHealthFailed { cleanup_failed, .. } => cleanup_failed,
            _ => false,
        }
    }
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
        let prepared = match prepare_attachment(candidate.generation(), policy, slot, &capabilities)
        {
            Ok(prepared) => prepared,
            Err(cause_code) => {
                return Err(self.fail_open(
                    WorkerActivationDiagnosticCode::NetworkPlanRejected,
                    Some(cause_code),
                    false,
                ));
            }
        };
        self.transition_to(match policy.mode() {
            CaptureMode::Tun => RuntimeState::StartingTun,
            _ => RuntimeState::StartingCore,
        })?;
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
        let mut attachment = match prepared {
            PreparedAttachment::Tproxy(plan) => {
                debug_assert_eq!(staged.generation(), plan.generation());
                match self.network.apply(&plan) {
                    Ok(receipt) => RuntimeAttachment::Tproxy { plan, receipt },
                    Err(error) => {
                        let core_cleanup_failed = self.core.abort_staged(staged);
                        return Err(self.fail_open(
                            WorkerActivationDiagnosticCode::NetworkApplyFailed,
                            Some(error.code().as_str()),
                            core_cleanup_failed
                                || matches!(error, ExecutionError::ApplyRollbackFailed { .. }),
                        ));
                    }
                }
            }
            PreparedAttachment::Tun => RuntimeAttachment::Tun,
        };
        if let Err(error) = self.data_plane_health.wait_healthy(
            staged.process_mut(),
            attachment.view(),
            &capabilities,
        ) {
            let network_cleanup_failed = match &mut attachment {
                RuntimeAttachment::Tproxy { plan, receipt } => {
                    self.network.rollback(plan, receipt).is_err()
                }
                RuntimeAttachment::Tun => false,
            };
            let core_cleanup_failed = self.core.abort_staged(staged);
            let data_plane_cleanup_failed = self
                .data_plane_health
                .wait_stopped(attachment.view())
                .is_err();
            return Err(self.fail_open(
                WorkerActivationDiagnosticCode::DataPlaneHealthFailed,
                Some(error.as_str()),
                network_cleanup_failed || core_cleanup_failed || data_plane_cleanup_failed,
            ));
        }
        let active = match self.core.commit_staged(staged) {
            Ok(active) => active,
            Err(staged) => {
                let network_cleanup_failed = match &mut attachment {
                    RuntimeAttachment::Tproxy { plan, receipt } => {
                        self.network.rollback(plan, receipt).is_err()
                    }
                    RuntimeAttachment::Tun => false,
                };
                let core_cleanup_failed = self.core.abort_staged(staged);
                let data_plane_cleanup_failed = self
                    .data_plane_health
                    .wait_stopped(attachment.view())
                    .is_err();
                return Err(self.fail_open(
                    WorkerActivationDiagnosticCode::CommitFailed,
                    Some(ActivationDiagnosticCode::CommitFailed.as_str()),
                    network_cleanup_failed || core_cleanup_failed || data_plane_cleanup_failed,
                ));
            }
        };
        self.state = attachment.state();
        Ok(ActiveRuntime {
            active,
            attachment,
            capabilities,
        })
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
