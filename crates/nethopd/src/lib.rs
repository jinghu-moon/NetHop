#![doc = "Controlled daemon process boundaries for NetHop."]

pub mod activation;
pub mod api_secret;
pub mod application;
#[cfg(feature = "subscription-update")]
pub mod auto_update;
pub mod clash_api;
mod config_model;
#[cfg(feature = "subscription-update")]
pub mod config_reconciler;
#[cfg(feature = "subscription-update")]
pub mod config_watch;
pub mod events;
pub mod log_retention;
#[cfg(feature = "subscription-update")]
pub mod manual_source;
pub mod operational_control;
pub mod process;
pub mod ruleset;
pub mod ruleset_provider;
#[cfg(feature = "subscription-update")]
pub mod ruleset_update;
pub mod runner;
pub mod runtime_metrics;
pub mod scheduler;
#[cfg(feature = "subscription-update")]
pub mod source_config;
#[cfg(feature = "subscription-update")]
pub mod source_update;
pub mod stats;
pub mod storage;
pub mod supervisor;
pub mod tun_runner;
pub mod uds;
pub mod version_check;
pub mod webui_payload;
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
pub use auto_update::{
    CORE_VERSION_SCHEDULE_KEY, PersistentCoreVersionSchedule, PersistentRuleSetSchedule,
    PersistentUpdateSchedule, RULE_SET_SCHEDULE_KEY, RuntimeCoreVersionSchedule,
    RuntimeRuleSetSchedule, RuntimeUpdateSchedule, UnavailableCoreVersionSchedule,
    UnavailableRuleSetSchedule, UnavailableUpdateSchedule,
};
pub use clash_api::{
    ClashApiClient, ClashApiError, ClashApiLimits, ConnectionSummary, DelayResult, NodeSummary,
    TrafficSample, TrafficTotals,
};
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
#[cfg(feature = "subscription-update")]
pub use manual_source::{ManualSource, ManualSourceError, ManualSourceStore};
pub use operational_control::{
    OperationalControl, OperationalControlError, ReplayResult, SelectorStore,
};
pub use process::{
    CoreProcessLimits, CoreProcessRunner, ProcessDiagnosticCode, ProcessError, ProcessExitReport,
    ProcessIdentity, RunningCore, StopReport,
};
pub use ruleset::{
    PreparedRuleSet, PublishedRuleSet, RuleSetError, RuleSetLimits, RuleSetPreparation,
    RuleSetReplaceOutcome, RuleSetStore,
};
pub use ruleset_provider::{
    RuleSetManifestError, RuleSetProvider, RuleSetProviderManifest, RuleSetPurpose,
};
#[cfg(feature = "subscription-update")]
pub use ruleset_update::{
    HttpRuleSetBodyFetcher, RuleSetBodyFetcher, RuleSetDigestSnapshot, RuleSetFetchError,
    RuleSetUpdateError, RuleSetUpdatePreparation, RuleSetUpdateService, RuntimeRuleSetUpdateSource,
    UnavailableRuleSetUpdateSource,
};
pub use runner::{
    CheckOutputSummary, CheckReport, RunnerDiagnosticCode, RunnerError, RunnerLimits,
    SingBoxCheckRunner,
};
pub use runtime_metrics::{
    OutboundRoute, ProcessMetrics, collect_outbound_route, collect_process_metrics,
    parse_default_route_interface, parse_process_stat, parse_statm_rss_bytes,
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
    ConfiguredSourceUpdater, HttpSourceBodyFetcher, ImportPreview, PreparedSourceUpdate,
    SourceBody, SourceBodyFetcher, SourceBodyOrigin, SourceUpdateDetail, SourceUpdateError,
    SourceUpdateReport, SourceUpdateService, UpdateRuntimePolicy,
};
pub use stats::{
    CounterBatch, CounterDelta, CounterDeltaBatch, CounterDeltaTracker, CounterName,
    CounterReading, CounterTransport, StatsError,
};
#[cfg(feature = "subscription-update")]
pub use storage::{SourceHealth, SourceStatus, SourceStatusStore};
pub use storage::{StatsBucket, StatsStore, StatsStoreError, TrafficTotal};
pub use supervisor::{
    RestartPolicy, SupervisorError, SupervisorEvent, SupervisorState, SystemWorkerBackend,
    SystemWorkerProcess, WorkerExit, WorkerProcess, WorkerProcessBackend, WorkerSignal,
    WorkerSupervisor,
};
pub use tun_runner::{TunRunner, TunRunnerError, TunRunnerLimits, TunRuntime};
#[cfg(unix)]
pub use uds::UnixControlServer;
pub use uds::{
    ControlRequestHandler, ControlServerError, ControlServerLimits, PeerCredentials,
    RootPeerAuthorizer,
};
#[cfg(feature = "subscription-update")]
pub use version_check::HttpCoreReleaseBodyFetcher;
pub use version_check::{
    CoreReleaseBodyFetcher, CoreUpdateAvailability, CoreVersion, CoreVersionCheckError,
    CoreVersionChecker, CoreVersionStateSink, CoreVersionStatus, JsonCoreVersionStateStore,
    ReleaseMetadata,
};
pub use webui_payload::{
    MAX_PAYLOAD_BYTES, MAX_PAYLOAD_CHUNK_BYTES, PAYLOAD_TTL, WebUiPayloadError, WebUiPayloadStore,
};
pub use worker_activation::{
    ActiveRuntime, AndroidDataPlaneHealthProbe, CapabilitySource, CurrentGenerationActivator,
    DataPlaneHealthError, DataPlaneHealthProbe, NetworkController, RuntimeAttachment,
    RuntimeAttachmentView, RuntimeHealthVerifier, RuntimeStopError, TproxyDataPlaneHealthProbe,
    WorkerActivationDiagnosticCode, WorkerActivationError, WorkerActivator, WorkerRecovery,
    WorkerRecoveryError,
};
pub use worker_application::{
    ApplicationRecovery, MonotonicClock, OptionalRuntimeUpdateSource, RuntimeCoreVersionSource,
    RuntimePolicyError, RuntimeRecoverySource, RuntimeUpdateError, RuntimeUpdateSource,
    UnavailableRuntimeUpdateSource, WorkerApplication, WorkerClock, WorkerRecoveryCoordinator,
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
