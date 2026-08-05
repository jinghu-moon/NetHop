#![doc = "Controlled daemon process boundaries for NetHop."]

pub mod activation;
pub mod api_secret;
pub mod application;
#[cfg(feature = "subscription-update")]
pub mod auto_update;
mod config_model;
#[cfg(feature = "subscription-update")]
pub mod config_reconciler;
#[cfg(feature = "subscription-update")]
pub mod config_watch;
pub mod events;
pub mod log_retention;
pub mod process;
pub mod runner;
pub mod scheduler;
#[cfg(feature = "subscription-update")]
pub mod source_config;
#[cfg(feature = "subscription-update")]
pub mod source_update;
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
pub use api_secret::{
    ApiSecret, ApiSecretError, ApiSecretStore, SecretEntropy, SystemSecretEntropy,
};
pub use application::{
    ApplicationError, DaemonArguments, DaemonMode, RuntimeRoot, SupervisorLoopDriver,
    SupervisorLoopSignal, SystemSupervisorDriver, run_supervisor_loop, run_system_supervisor,
    run_system_worker,
};
#[cfg(feature = "subscription-update")]
pub use auto_update::{PersistentUpdateSchedule, RuntimeUpdateSchedule, UnavailableUpdateSchedule};
pub use config_model::{
    AdvancedSettings, ApplicationMode, ApplicationSettings, ApplyImpact, CanonicalCidr,
    CaptureIntent, ChangeKind, ChangePlan, DnsMode, EffectiveConfig, Ipv6Mode, LogLevel,
    LoggingSettings, NetworkSettings, OutboundMode, ProxySettings, RoutingSettings, SelectorMode,
    SourceFormatHint, SourceName, SubscriptionSettings, TunStackIntent, UrltestSettings,
    UserSource,
};
#[cfg(feature = "subscription-update")]
pub use config_reconciler::{
    ConfigChange, ConfigMutationOutcome, ConfigPreview, ConfigReloadState, ConfigRuntime,
    ConfigRuntimeCheckpoint, ConfigRuntimeError,
};
#[cfg(feature = "subscription-update")]
pub use config_watch::{ConfigWatchError, ConfigWatcher};
pub use events::{EventError, EventHub, EventSubscription};
pub use log_retention::{
    FileLogRetention, LogRetentionError, RuntimeLogRetention, UnavailableLogRetention,
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
#[cfg(feature = "subscription-update")]
pub use source_config::{
    SourceConfig, SourceDefinition, SourceIdEntropy, SourceRegistry, SourceRegistryError,
    SystemSourceIdEntropy,
};
#[cfg(feature = "subscription-update")]
pub use source_update::{
    ConfiguredSourceUpdater, HttpSourceBodyFetcher, PreparedSourceUpdate, SourceBodyFetcher,
    SourceUpdateError, SourceUpdateReport, SourceUpdateService, UpdateRuntimePolicy,
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
    ApplicationRecovery, MonotonicClock, OptionalRuntimeUpdateSource, RuntimePolicyError,
    RuntimeRecoverySource, RuntimeUpdateError, RuntimeUpdateSource, UnavailableRuntimeUpdateSource,
    WorkerApplication, WorkerClock, WorkerRecoveryCoordinator,
};
pub use worker_config::{ConfigError, ConfigSnapshot, ConfigStore};
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
    StatsCollector, StatsCollectorError, UpdateStatus, WorkerControlHandler, build_candidate,
};
