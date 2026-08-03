#![doc = "Controlled daemon process boundaries for NetHop."]

pub mod activation;
pub mod application;
pub mod process;
pub mod runner;
pub mod scheduler;
pub mod stats;
pub mod storage;
pub mod supervisor;
pub mod uds;
pub mod worker_activation;
pub mod worker_application;
pub mod worker_config;
pub mod worker_runtime;
pub mod worker_service;
pub mod worker_services;

pub use activation::{
    ActivationDiagnosticCode, ActivationError, ActiveGeneration, CandidateActivator,
    CandidateChecker, CandidateProcess, CoreLauncher, HealthProbe, HealthProbeError,
    ManagedSafetyAuditor, SafetyAuditError, SafetyAuditor, StartupLivenessProbe,
};
pub use application::{
    ApplicationError, DaemonArguments, DaemonMode, RuntimeRoot, SupervisorLoopDriver,
    SupervisorLoopSignal, SystemSupervisorDriver, run_supervisor_loop, run_system_supervisor,
    run_system_worker,
};
pub use process::{
    CoreProcessLimits, CoreProcessRunner, ProcessDiagnosticCode, ProcessError, ProcessExitReport,
    ProcessIdentity, RunningCore, StopReport,
};
pub use runner::{
    CheckOutputSummary, CheckReport, RunnerDiagnosticCode, RunnerError, RunnerLimits,
    SingBoxCheckRunner,
};
pub use scheduler::{
    InMemoryScheduleStore, ScheduleKey, SchedulePolicy, ScheduleRecord, ScheduleStore,
    SchedulerEngine, SchedulerError,
};
pub use stats::{
    CounterBatch, CounterDelta, CounterDeltaBatch, CounterDeltaTracker, CounterName,
    CounterReading, CounterTransport, StatsError,
};
pub use storage::{StatsBucket, StatsStore, StatsStoreError};
pub use supervisor::{
    RestartPolicy, SupervisorError, SupervisorEvent, SupervisorState, SystemWorkerBackend,
    SystemWorkerProcess, WorkerExit, WorkerProcess, WorkerProcessBackend, WorkerSignal,
    WorkerSupervisor,
};
#[cfg(unix)]
pub use uds::UnixControlServer;
pub use uds::{
    ControlRequestHandler, ControlServerError, ControlServerLimits, PeerCredentials,
    RootPeerAuthorizer,
};
pub use worker_activation::{
    ActiveRuntime, CapabilitySource, CurrentGenerationActivator, DataPlaneHealthError,
    DataPlaneHealthProbe, NetworkController, NetworkDataPlaneHealthProbe, RuntimeStopError,
    WorkerActivationDiagnosticCode, WorkerActivationError, WorkerActivator, WorkerRecovery,
    WorkerRecoveryError,
};
pub use worker_application::{
    ApplicationRecovery, MonotonicClock, RuntimeRecoverySource, WorkerApplication, WorkerClock,
    WorkerRecoveryCoordinator,
};
pub use worker_config::{WorkerConfig, WorkerConfigError};
pub use worker_runtime::{
    RestartBudget, RestartDecision, RuntimeFailureCode, RuntimeTick, SystemLoopDriver,
    WorkerLoopDriver, WorkerLoopSignal, WorkerRunExit, WorkerRuntime, WorkerRuntimeError,
    WorkerRuntimeLimits, WorkerStopHandle,
};
pub use worker_service::{
    WorkerControlService, WorkerServiceDriver, WorkerServiceError, WorkerServiceSignal,
    WorkerServiceTasks, run_worker_service,
};
pub use worker_services::{
    BuildCandidateError, ControlCommand, ControlSnapshot, EventReconcileError, EventReconcileGate,
    StatsCollector, StatsCollectorError, WorkerControlHandler, build_candidate,
};
