#![doc = "Controlled daemon process boundaries for NetHop."]

pub mod activation;
pub mod process;
pub mod runner;
pub mod worker_activation;
pub mod worker_runtime;

pub use activation::{
    ActivationDiagnosticCode, ActivationError, ActiveGeneration, CandidateActivator,
    CandidateChecker, CandidateProcess, CoreLauncher, HealthProbe, HealthProbeError,
    ManagedSafetyAuditor, SafetyAuditError, SafetyAuditor, StartupLivenessProbe,
};
pub use process::{
    CoreProcessLimits, CoreProcessRunner, ProcessDiagnosticCode, ProcessError, ProcessExitReport,
    ProcessIdentity, RunningCore, StopReport,
};
pub use runner::{
    CheckOutputSummary, CheckReport, RunnerDiagnosticCode, RunnerError, RunnerLimits,
    SingBoxCheckRunner,
};
pub use worker_activation::{
    ActiveRuntime, CapabilitySource, DataPlaneHealthError, DataPlaneHealthProbe, NetworkController,
    NetworkDataPlaneHealthProbe, RuntimeStopError, WorkerActivationDiagnosticCode,
    WorkerActivationError, WorkerActivator,
};
pub use worker_runtime::{
    RestartBudget, RestartDecision, RuntimeFailureCode, RuntimeTick, SystemLoopDriver,
    WorkerLoopDriver, WorkerLoopSignal, WorkerRunExit, WorkerRuntime, WorkerRuntimeError,
    WorkerRuntimeLimits, WorkerStopHandle,
};
