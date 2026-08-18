use std::{
    fs,
    path::Path,
    thread,
    time::{Duration, Instant},
};

use nethop_core::{Candidate, GenerationId, GenerationStore, SealedGeneration};
use serde_json::Value;
use thiserror::Error;

use crate::{
    CoreProcessRunner, ProcessError, ProcessIdentity, RunnerError, RunningCore, SingBoxCheckRunner,
};

const MAX_STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const MIN_POLL_INTERVAL: Duration = Duration::from_millis(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationDiagnosticCode {
    PrepareFailed,
    SafetyRejected,
    CheckFailed,
    SealFailed,
    StartFailed,
    HealthFailed,
    CommitFailed,
}

impl ActivationDiagnosticCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PrepareFailed => "candidate_prepare_failed",
            Self::SafetyRejected => "candidate_safety_rejected",
            Self::CheckFailed => "candidate_check_failed",
            Self::SealFailed => "candidate_seal_failed",
            Self::StartFailed => "candidate_start_failed",
            Self::HealthFailed => "candidate_health_failed",
            Self::CommitFailed => "candidate_commit_failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("candidate activation failed: {code}")]
pub struct ActivationError {
    code: &'static str,
    diagnostic_code: ActivationDiagnosticCode,
    cleanup_failed: bool,
}

impl ActivationError {
    fn new(code: ActivationDiagnosticCode, cleanup_failed: bool) -> Self {
        Self {
            code: code.as_str(),
            diagnostic_code: code,
            cleanup_failed,
        }
    }

    pub const fn code(&self) -> ActivationDiagnosticCode {
        self.diagnostic_code
    }

    pub const fn cleanup_failed(&self) -> bool {
        self.cleanup_failed
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SafetyAuditError {
    #[error("candidate bytes differ from the prepared file")]
    ConfigMismatch,
    #[error("candidate config is not valid managed JSON")]
    InvalidJson,
    #[error("candidate config contains non-managed top-level semantics")]
    ForbiddenTopLevel,
    #[error("candidate has no terminal outbounds")]
    EmptyOutbounds,
    #[error("candidate contains an invalid terminal outbound")]
    InvalidOutbound,
}

pub trait SafetyAuditor {
    fn audit(&self, candidate: &Candidate, config_path: &Path) -> Result<(), SafetyAuditError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ManagedSafetyAuditor;

impl SafetyAuditor for ManagedSafetyAuditor {
    fn audit(&self, candidate: &Candidate, config_path: &Path) -> Result<(), SafetyAuditError> {
        let bytes = fs::read(config_path).map_err(|_| SafetyAuditError::ConfigMismatch)?;
        if bytes != candidate.config().bytes() {
            return Err(SafetyAuditError::ConfigMismatch);
        }
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|_| SafetyAuditError::InvalidJson)?;
        let object = value.as_object().ok_or(SafetyAuditError::InvalidJson)?;
        if object.len() != 1 || !object.contains_key("outbounds") {
            return Err(SafetyAuditError::ForbiddenTopLevel);
        }
        let outbounds = object["outbounds"]
            .as_array()
            .ok_or(SafetyAuditError::InvalidJson)?;
        if outbounds.is_empty() {
            return Err(SafetyAuditError::EmptyOutbounds);
        }
        if outbounds.iter().any(|outbound| {
            outbound.as_object().is_none_or(|item| {
                !is_nonempty_string(item.get("tag")) || !is_nonempty_string(item.get("type"))
            })
        }) {
            return Err(SafetyAuditError::InvalidOutbound);
        }
        Ok(())
    }
}

fn is_nonempty_string(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|value| !value.is_empty())
}

pub trait CandidateChecker {
    fn check(&self, config_path: &Path) -> Result<(), RunnerError>;
}

impl CandidateChecker for SingBoxCheckRunner {
    fn check(&self, config_path: &Path) -> Result<(), RunnerError> {
        self.check_candidate(config_path).map(|_| ())
    }
}

pub trait CandidateProcess: Sized {
    fn identity(&self) -> ProcessIdentity;
    fn is_running(&mut self) -> Result<bool, ProcessError>;
    fn supports_reload(&self) -> bool {
        false
    }
    fn stage_reload(&mut self, _config_path: &Path) -> Result<(), ProcessError> {
        Err(ProcessError::ReloadUnsupported)
    }
    fn commit_reload(&mut self) -> Result<(), ProcessError> {
        Err(ProcessError::ReloadUnsupported)
    }
    fn rollback_reload(&mut self) -> Result<(), ProcessError> {
        Err(ProcessError::ReloadUnsupported)
    }
    fn stop(self) -> Result<(), ProcessError>;
}

impl CandidateProcess for RunningCore {
    fn identity(&self) -> ProcessIdentity {
        self.identity()
    }

    fn is_running(&mut self) -> Result<bool, ProcessError> {
        self.try_exit().map(|status| status.is_none())
    }

    fn supports_reload(&self) -> bool {
        self.supports_reload()
    }

    fn stage_reload(&mut self, config_path: &Path) -> Result<(), ProcessError> {
        self.stage_reload(config_path)
    }

    fn commit_reload(&mut self) -> Result<(), ProcessError> {
        self.commit_reload()
    }

    fn rollback_reload(&mut self) -> Result<(), ProcessError> {
        self.rollback_reload()
    }

    fn stop(self) -> Result<(), ProcessError> {
        self.stop().map(|_| ())
    }
}

pub trait CoreLauncher {
    type Process: CandidateProcess;

    fn start(&self, config_path: &Path) -> Result<Self::Process, ProcessError>;
}

impl CoreLauncher for CoreProcessRunner {
    type Process = RunningCore;

    fn start(&self, config_path: &Path) -> Result<Self::Process, ProcessError> {
        self.start(config_path)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum HealthProbeError {
    #[error("candidate core exited before becoming healthy")]
    EarlyExit,
    #[error("candidate core health probe timed out")]
    TimedOut,
    #[error("candidate core health state could not be observed")]
    ObserveFailed,
}

pub trait HealthProbe<P: CandidateProcess> {
    fn wait_healthy(&self, process: &mut P) -> Result<(), HealthProbeError>;

    fn replace_timeout(&mut self, _timeout: Duration) -> Result<(), HealthProbeError> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartupLivenessProbe {
    timeout: Duration,
    stable_window: Duration,
    poll_interval: Duration,
}

impl StartupLivenessProbe {
    pub fn new(
        timeout: Duration,
        stable_window: Duration,
        poll_interval: Duration,
    ) -> Result<Self, HealthProbeError> {
        if timeout.is_zero()
            || timeout > MAX_STARTUP_TIMEOUT
            || stable_window.is_zero()
            || stable_window > timeout
            || poll_interval < MIN_POLL_INTERVAL
            || poll_interval > stable_window
        {
            return Err(HealthProbeError::TimedOut);
        }
        Ok(Self {
            timeout,
            stable_window,
            poll_interval,
        })
    }
}

impl Default for StartupLivenessProbe {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(3),
            stable_window: Duration::from_millis(200),
            poll_interval: Duration::from_millis(20),
        }
    }
}

impl<P: CandidateProcess> HealthProbe<P> for StartupLivenessProbe {
    fn wait_healthy(&self, process: &mut P) -> Result<(), HealthProbeError> {
        let started = Instant::now();
        while started.elapsed() < self.stable_window {
            if started.elapsed() >= self.timeout {
                return Err(HealthProbeError::TimedOut);
            }
            match process.is_running() {
                Ok(false) => return Err(HealthProbeError::EarlyExit),
                Ok(true) => thread::sleep(self.poll_interval),
                Err(_) => return Err(HealthProbeError::ObserveFailed),
            }
        }
        Ok(())
    }

    fn replace_timeout(&mut self, timeout: Duration) -> Result<(), HealthProbeError> {
        if timeout.is_zero() || timeout > MAX_STARTUP_TIMEOUT || timeout < self.stable_window {
            return Err(HealthProbeError::TimedOut);
        }
        self.timeout = timeout;
        Ok(())
    }
}

#[derive(Debug)]
pub struct ActiveGeneration<P: CandidateProcess> {
    generation: SealedGeneration,
    previous_generation: Option<GenerationId>,
    process: P,
}

#[derive(Debug)]
pub(crate) struct StagedGeneration<P: CandidateProcess> {
    generation: SealedGeneration,
    previous_generation: Option<GenerationId>,
    process: P,
}

impl<P: CandidateProcess> StagedGeneration<P> {
    pub(crate) const fn generation(&self) -> GenerationId {
        self.generation.generation()
    }

    pub(crate) fn process_mut(&mut self) -> &mut P {
        &mut self.process
    }
}

impl<P: CandidateProcess> ActiveGeneration<P> {
    pub(crate) fn recovered(generation: SealedGeneration, process: P) -> Self {
        Self {
            generation,
            previous_generation: None,
            process,
        }
    }

    pub const fn generation(&self) -> GenerationId {
        self.generation.generation()
    }

    pub const fn previous_generation(&self) -> Option<GenerationId> {
        self.previous_generation
    }

    pub fn identity(&self) -> ProcessIdentity {
        self.process.identity()
    }

    pub fn process_mut(&mut self) -> &mut P {
        &mut self.process
    }

    pub(crate) fn replace_generation(&mut self, generation: SealedGeneration) {
        self.previous_generation = Some(self.generation.generation());
        self.generation = generation;
    }

    pub fn stop(self) -> Result<(), ProcessError> {
        self.process.stop()
    }
}

#[derive(Debug)]
pub struct CandidateActivator<'a, C, L, A, H> {
    store: &'a GenerationStore,
    checker: &'a C,
    launcher: &'a L,
    auditor: &'a A,
    health_probe: &'a H,
}

impl<'a, C, L, A, H> CandidateActivator<'a, C, L, A, H> {
    pub const fn new(
        store: &'a GenerationStore,
        checker: &'a C,
        launcher: &'a L,
        auditor: &'a A,
        health_probe: &'a H,
    ) -> Self {
        Self {
            store,
            checker,
            launcher,
            auditor,
            health_probe,
        }
    }
}

impl<C, L, A, H> CandidateActivator<'_, C, L, A, H>
where
    C: CandidateChecker,
    L: CoreLauncher,
    A: SafetyAuditor,
    H: HealthProbe<L::Process>,
{
    pub(crate) fn stage(
        &self,
        candidate: &Candidate,
    ) -> Result<StagedGeneration<L::Process>, ActivationError> {
        let previous_generation = self
            .store
            .current_generation()
            .map_err(|_| ActivationError::new(ActivationDiagnosticCode::PrepareFailed, false))?;
        let prepared = self
            .store
            .prepare_candidate(candidate)
            .map_err(|_| ActivationError::new(ActivationDiagnosticCode::PrepareFailed, false))?;

        if self
            .auditor
            .audit(candidate, &prepared.config_path())
            .is_err()
        {
            let cleanup_failed = self.store.discard_prepared(prepared).is_err();
            return Err(ActivationError::new(
                ActivationDiagnosticCode::SafetyRejected,
                cleanup_failed,
            ));
        }
        if self.checker.check(&prepared.config_path()).is_err() {
            let cleanup_failed = self.store.discard_prepared(prepared).is_err();
            return Err(ActivationError::new(
                ActivationDiagnosticCode::CheckFailed,
                cleanup_failed,
            ));
        }
        let sealed = match self.store.seal_candidate(&prepared) {
            Ok(sealed) => sealed,
            Err(_) => {
                let cleanup_failed = self.store.discard_prepared(prepared).is_err();
                return Err(ActivationError::new(
                    ActivationDiagnosticCode::SealFailed,
                    cleanup_failed,
                ));
            }
        };
        let mut process = match self.launcher.start(&sealed.config_path()) {
            Ok(process) => process,
            Err(_) => {
                let cleanup_failed = self.store.discard_sealed(sealed).is_err();
                return Err(ActivationError::new(
                    ActivationDiagnosticCode::StartFailed,
                    cleanup_failed,
                ));
            }
        };
        if self.health_probe.wait_healthy(&mut process).is_err() {
            let stop_failed = process.stop().is_err();
            let discard_failed = self.store.discard_sealed(sealed).is_err();
            return Err(ActivationError::new(
                ActivationDiagnosticCode::HealthFailed,
                stop_failed || discard_failed,
            ));
        }
        Ok(StagedGeneration {
            generation: sealed,
            previous_generation,
            process,
        })
    }

    pub(crate) fn commit_staged(
        &self,
        staged: StagedGeneration<L::Process>,
    ) -> Result<ActiveGeneration<L::Process>, StagedGeneration<L::Process>> {
        if self.store.commit_generation(&staged.generation).is_err() {
            return Err(staged);
        }
        Ok(ActiveGeneration {
            generation: staged.generation,
            previous_generation: staged.previous_generation,
            process: staged.process,
        })
    }

    pub(crate) fn abort_staged(&self, staged: StagedGeneration<L::Process>) -> bool {
        let stop_failed = staged.process.stop().is_err();
        let discard_failed = self.store.discard_sealed(staged.generation).is_err();
        stop_failed || discard_failed
    }

    pub fn activate(
        &self,
        candidate: &Candidate,
    ) -> Result<ActiveGeneration<L::Process>, ActivationError> {
        let staged = self.stage(candidate)?;
        match self.commit_staged(staged) {
            Ok(active) => Ok(active),
            Err(staged) => {
                let cleanup_failed = self.abort_staged(staged);
                Err(ActivationError::new(
                    ActivationDiagnosticCode::CommitFailed,
                    cleanup_failed,
                ))
            }
        }
    }
}
