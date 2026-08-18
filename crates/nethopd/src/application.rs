use std::{
    ffi::{OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

#[cfg(all(unix, feature = "subscription-update"))]
use std::sync::mpsc::Receiver;

use thiserror::Error;

use crate::{
    RestartPolicy, SupervisorError, SystemWorkerBackend, WorkerProcessBackend, WorkerServiceError,
    WorkerSupervisor,
};

#[cfg(all(unix, feature = "subscription-update"))]
use crate::{
    AndroidDataPlaneHealthProbe, ControlServerLimits, CoreProcessLimits, CoreProcessRunner,
    MonotonicClock, RunnerLimits, SingBoxCheckRunner, StartupLivenessProbe, TunRunner,
    TunRunnerLimits, UnixControlServer, WorkerApplication, WorkerRecoveryCoordinator,
    WorkerRuntimeLimits, WorkerServiceDriver, WorkerServiceSignal, run_worker_service,
};
#[cfg(all(unix, feature = "subscription-update"))]
use crate::{
    ApiSecretStore, ClashApiClient, ClashApiLimits, CommitJournalStore, ConfigRuntime, ConfigStore,
    ConfigWatcher, ConfiguredSourceUpdater, FileLogRetention, HttpCoreReleaseBodyFetcher,
    HttpRuleSetBodyFetcher, HttpSourceBodyFetcher, JsonCoreVersionStateStore, ManualSourceStore,
    MutationCoordinator, NodeOverrideStore, NodeSelectionStore, OperationalControl,
    OptionalRuntimeUpdateSource, PersistentCoreVersionSchedule, PersistentRuleSetSchedule,
    PersistentUpdateSchedule, RuleSetLimits, RuleSetProviderManifest, RuleSetStore,
    RuleSetUpdateService, SourceRegistry, SourceStatusStore, SourceUpdateService, StatsStore,
    SystemSourceIdEntropy, UpdateRuntimePolicy, WebUiPayloadStore,
};
#[cfg(all(unix, feature = "subscription-update"))]
use nethop_android::{
    AndroidToolPaths, CapabilityProbe, CommandPrivateDnsFactsSource, CommandProbeBackend,
    CommandUpdateNotifier, CommandWifiFactsSource, NetworkExecutor, NetworkPlanVerifier, PlanSlot,
    ProbeLimits, SystemCommandBackend, SystemCommandLimits, TunHealthVerifier,
    default_tun_interface,
};
#[cfg(all(unix, feature = "subscription-update"))]
use nethop_core::GenerationStore;
#[cfg(all(unix, feature = "subscription-update"))]
use nethop_core::{ClashApi, MANAGED_FETCH_PROXY_ENDPOINT, MANAGED_FETCH_PROXY_USERNAME};
#[cfg(all(unix, feature = "subscription-update"))]
use nethop_subscription::{
    CapabilityMatrix, LocalFetchProxy, PINNED_SING_BOX_VERSION, ParserLimits,
};

const SUPERVISOR_POLL_INTERVAL: Duration = Duration::from_millis(250);
static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonMode {
    Supervise,
    Worker,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonArguments {
    mode: DaemonMode,
    root: PathBuf,
}

impl DaemonArguments {
    pub fn parse<I, S>(arguments: I) -> Result<Self, ApplicationError>
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        let mut arguments = arguments.into_iter().map(Into::into);
        let mode = match arguments.next().as_deref() {
            Some(value) if value == OsStr::new("--supervise") => DaemonMode::Supervise,
            Some(value) if value == OsStr::new("--worker") => DaemonMode::Worker,
            _ => return Err(ApplicationError::Usage),
        };
        if arguments.next().as_deref() != Some(OsStr::new("--root")) {
            return Err(ApplicationError::Usage);
        }
        let root = arguments.next().ok_or(ApplicationError::Usage)?;
        if arguments.next().is_some() {
            return Err(ApplicationError::Usage);
        }
        Ok(Self {
            mode,
            root: PathBuf::from(root),
        })
    }

    pub const fn mode(&self) -> DaemonMode {
        self.mode
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeRoot {
    root: PathBuf,
    run: PathBuf,
}

impl RuntimeRoot {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, ApplicationError> {
        let root = checked_directory(root.into())?;
        let run = checked_directory(root.join("run"))?;
        Ok(Self { root, run })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn run(&self) -> &Path {
        &self.run
    }

    pub fn supervisor_pid_path(&self) -> PathBuf {
        self.run.join("supervisor.pid")
    }

    pub fn worker_arguments(&self) -> Vec<OsString> {
        vec![
            OsString::from("--worker"),
            OsString::from("--root"),
            self.root.as_os_str().to_owned(),
        ]
    }
}

fn checked_directory(path: PathBuf) -> Result<PathBuf, ApplicationError> {
    if !path.is_absolute() {
        return Err(ApplicationError::InvalidRuntimeRoot);
    }
    let metadata = fs::symlink_metadata(&path).map_err(|_| ApplicationError::InvalidRuntimeRoot)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(ApplicationError::InvalidRuntimeRoot);
    }
    let canonical = path
        .canonicalize()
        .map_err(|_| ApplicationError::InvalidRuntimeRoot)?;
    if canonical != path {
        return Err(ApplicationError::InvalidRuntimeRoot);
    }
    Ok(path)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupervisorLoopSignal {
    Wake,
    Stop,
}

pub trait SupervisorLoopDriver {
    fn now(&self) -> Duration;
    fn wait(&mut self, timeout: Duration) -> SupervisorLoopSignal;
}

pub fn run_supervisor_loop<B, D>(
    supervisor: &mut WorkerSupervisor<B>,
    driver: &mut D,
) -> Result<(), ApplicationError>
where
    B: WorkerProcessBackend,
    D: SupervisorLoopDriver,
{
    loop {
        let now = driver.now();
        if let Err(error) = supervisor.tick(now) {
            let _ = supervisor.stop();
            return Err(error.into());
        }
        let timeout = supervisor
            .next_action()
            .map_or(SUPERVISOR_POLL_INTERVAL, |deadline| {
                deadline.saturating_sub(now).min(SUPERVISOR_POLL_INTERVAL)
            });
        if driver.wait(timeout) == SupervisorLoopSignal::Stop {
            supervisor.stop()?;
            return Ok(());
        }
    }
}

#[derive(Debug)]
pub struct SystemSupervisorDriver {
    started: Instant,
}

impl SystemSupervisorDriver {
    pub fn install() -> Result<Self, ApplicationError> {
        STOP_REQUESTED.store(false, Ordering::Release);
        install_signal_handlers()?;
        Ok(Self {
            started: Instant::now(),
        })
    }
}

impl SupervisorLoopDriver for SystemSupervisorDriver {
    fn now(&self) -> Duration {
        self.started.elapsed()
    }

    fn wait(&mut self, timeout: Duration) -> SupervisorLoopSignal {
        if STOP_REQUESTED.load(Ordering::Acquire) {
            return SupervisorLoopSignal::Stop;
        }
        thread::sleep(timeout);
        if STOP_REQUESTED.load(Ordering::Acquire) {
            SupervisorLoopSignal::Stop
        } else {
            SupervisorLoopSignal::Wake
        }
    }
}

pub fn run_system_supervisor(runtime: &RuntimeRoot) -> Result<(), ApplicationError> {
    ensure_root()?;
    let _pid = PidFile::acquire(runtime.supervisor_pid_path())?;
    let executable = std::env::current_exe().map_err(|_| ApplicationError::InvalidExecutable)?;
    let backend = SystemWorkerBackend::new(executable, runtime.worker_arguments())?;
    let mut supervisor = WorkerSupervisor::new(backend, RestartPolicy::default());
    let mut driver = SystemSupervisorDriver::install()?;
    run_supervisor_loop(&mut supervisor, &mut driver)
}

#[cfg(all(unix, feature = "subscription-update"))]
#[derive(Debug)]
struct SystemWorkerServiceDriver {
    wake_receiver: Receiver<()>,
}

#[cfg(all(unix, feature = "subscription-update"))]
impl SystemWorkerServiceDriver {
    fn install(wake_receiver: Receiver<()>) -> Result<Self, ApplicationError> {
        STOP_REQUESTED.store(false, Ordering::Release);
        install_signal_handlers()?;
        Ok(Self { wake_receiver })
    }
}

#[cfg(all(unix, feature = "subscription-update"))]
impl WorkerServiceDriver for SystemWorkerServiceDriver {
    fn wait(&mut self, timeout: Duration) -> WorkerServiceSignal {
        if STOP_REQUESTED.load(Ordering::Acquire) {
            return WorkerServiceSignal::Stop;
        }
        let _ = self.wake_receiver.recv_timeout(timeout);
        if STOP_REQUESTED.load(Ordering::Acquire) {
            WorkerServiceSignal::Stop
        } else {
            WorkerServiceSignal::Wake
        }
    }
}

#[cfg(all(unix, feature = "subscription-update"))]
pub fn run_system_worker(runtime: &RuntimeRoot) -> Result<(), ApplicationError> {
    report_worker_stage("begin");
    ensure_root()?;
    let store = GenerationStore::new(runtime.root())
        .map_err(|_| ApplicationError::WorkerInitializationFailed)?;
    let config_path = runtime.root().join("config/nethop.toml");
    let journal = CommitJournalStore::new(runtime.root())
        .map_err(|_| ApplicationError::WorkerInitializationFailed)?;
    let current_generation = store
        .current_generation()
        .map_err(|_| ApplicationError::WorkerInitializationFailed)?
        .map(nethop_core::GenerationId::get);
    journal
        .recover(&config_path, current_generation)
        .map_err(|_| ApplicationError::WorkerInitializationFailed)?;
    let config_store =
        ConfigStore::new(config_path).map_err(|_| ApplicationError::WorkerInitializationFailed)?;
    let config_snapshot = config_store
        .load()
        .map_err(|_| ApplicationError::WorkerInitializationFailed)?;
    let config = config_snapshot.effective().clone();
    report_worker_stage("config_loaded");
    #[cfg(feature = "subscription-update")]
    let source_registry = SourceRegistry::new(runtime.root().join("state/source-registry.v1.json"))
        .map_err(|_| ApplicationError::WorkerInitializationFailed)?;
    #[cfg(feature = "subscription-update")]
    let source_config = source_registry
        .reconcile(&config_snapshot, &mut SystemSourceIdEntropy)
        .map_err(|_| ApplicationError::WorkerInitializationFailed)?;
    #[cfg(feature = "subscription-update")]
    let restore_current = current_generation_matches(&store, source_config.source_config_digest());
    #[cfg(feature = "subscription-update")]
    report_worker_stage("sources_reconciled");
    #[cfg(feature = "subscription-update")]
    let config_runtime = ConfigRuntime::new(
        config_store,
        source_registry,
        config_snapshot,
        &source_config,
    )
    .with_module_entry("/data/adb/modules/nethop/config/nethop.toml")
    .map_err(|_| ApplicationError::WorkerInitializationFailed)?;
    report_worker_stage("config_runtime_ready");
    let package_backend = CommandProbeBackend::new(
        AndroidToolPaths::from_system()
            .map_err(|_| ApplicationError::WorkerInitializationFailed)?,
        ProbeLimits::default(),
    );
    report_worker_stage("app_resolver_deferred");
    let capture = config_runtime
        .capture_policy()
        .map_err(|_| ApplicationError::WorkerInitializationFailed)?;
    let inbound_port = capture
        .inbound_port()
        .ok_or(ApplicationError::WorkerInitializationFailed)?;
    let executable = std::env::current_exe().map_err(|_| ApplicationError::InvalidExecutable)?;
    let binary = executable
        .parent()
        .ok_or(ApplicationError::InvalidExecutable)?
        .join("sing-box");
    let checker =
        SingBoxCheckRunner::new(&binary, store.generations_root(), RunnerLimits::default())
            .map_err(|_| ApplicationError::WorkerInitializationFailed)?;
    let launcher = CoreProcessRunner::new(
        &binary,
        store.generations_root(),
        CoreProcessLimits::default(),
    )
    .map_err(|_| ApplicationError::WorkerInitializationFailed)?;
    report_worker_stage("core_boundaries_ready");
    let mut core_health = StartupLivenessProbe::new(
        Duration::from_secs(u64::from(config.advanced().health_timeout_seconds())),
        Duration::from_millis(200),
        Duration::from_millis(20),
    )
    .map_err(|_| ApplicationError::WorkerInitializationFailed)?;
    let tun_runner_limits = TunRunnerLimits::new(
        Duration::from_secs(u64::from(config.advanced().health_timeout_seconds())),
        Duration::from_secs(u64::from(config.advanced().health_timeout_seconds())),
        Duration::from_millis(50),
    )
    .map_err(|_| ApplicationError::WorkerInitializationFailed)?;
    let capability_source = CapabilityProbe::new(
        CommandProbeBackend::new(
            AndroidToolPaths::from_system()
                .map_err(|_| ApplicationError::WorkerInitializationFailed)?,
            ProbeLimits::default(),
        ),
        config.allocations().to_vec(),
        inbound_port,
    )
    .map_err(|_| ApplicationError::WorkerInitializationFailed)?;
    let network = NetworkExecutor::new(
        SystemCommandBackend::from_system(SystemCommandLimits::default())
            .map_err(|_| ApplicationError::WorkerInitializationFailed)?,
    );
    let data_plane_health = AndroidDataPlaneHealthProbe::new(
        NetworkPlanVerifier::new(
            CommandProbeBackend::new(
                AndroidToolPaths::from_system()
                    .map_err(|_| ApplicationError::WorkerInitializationFailed)?,
                ProbeLimits::default(),
            ),
            inbound_port,
        )
        .map_err(|_| ApplicationError::WorkerInitializationFailed)?,
        TunRunner::new(
            TunHealthVerifier::new(
                CommandProbeBackend::new(
                    AndroidToolPaths::from_system()
                        .map_err(|_| ApplicationError::WorkerInitializationFailed)?,
                    ProbeLimits::default(),
                ),
                default_tun_interface(),
            )
            .map_err(|_| ApplicationError::WorkerInitializationFailed)?,
            tun_runner_limits,
        ),
    );
    let verifier = AndroidDataPlaneHealthProbe::new(
        NetworkPlanVerifier::new(
            CommandProbeBackend::new(
                AndroidToolPaths::from_system()
                    .map_err(|_| ApplicationError::WorkerInitializationFailed)?,
                ProbeLimits::default(),
            ),
            inbound_port,
        )
        .map_err(|_| ApplicationError::WorkerInitializationFailed)?,
        TunRunner::new(
            TunHealthVerifier::new(
                CommandProbeBackend::new(
                    AndroidToolPaths::from_system()
                        .map_err(|_| ApplicationError::WorkerInitializationFailed)?,
                    ProbeLimits::default(),
                ),
                default_tun_interface(),
            )
            .map_err(|_| ApplicationError::WorkerInitializationFailed)?,
            tun_runner_limits,
        ),
    );
    let private_dns_source = CommandPrivateDnsFactsSource::new(CommandProbeBackend::new(
        AndroidToolPaths::from_system()
            .map_err(|_| ApplicationError::WorkerInitializationFailed)?,
        ProbeLimits::default(),
    ));
    report_worker_stage("android_boundaries_ready");
    #[cfg(feature = "subscription-update")]
    let wifi_facts = CommandWifiFactsSource::new(CommandProbeBackend::new(
        AndroidToolPaths::from_system()
            .map_err(|_| ApplicationError::WorkerInitializationFailed)?,
        ProbeLimits::default(),
    ));
    #[cfg(feature = "subscription-update")]
    let core_version_source = crate::CoreVersionChecker::new(
        HttpCoreReleaseBodyFetcher::default(),
        crate::CoreVersion::parse(PINNED_SING_BOX_VERSION)
            .map_err(|_| ApplicationError::WorkerInitializationFailed)?,
    );
    #[cfg(feature = "subscription-update")]
    let core_update_notifier = CommandUpdateNotifier::new(CommandProbeBackend::new(
        AndroidToolPaths::from_system()
            .map_err(|_| ApplicationError::WorkerInitializationFailed)?,
        ProbeLimits::default(),
    ));
    #[cfg(feature = "subscription-update")]
    let core_version_state =
        JsonCoreVersionStateStore::new(runtime.root().join("state/runtime.json"))
            .map_err(|_| ApplicationError::WorkerInitializationFailed)?;
    let recovery = WorkerRecoveryCoordinator::new(
        &store,
        &checker,
        &launcher,
        &mut core_health,
        capability_source,
        data_plane_health,
    );
    report_worker_stage("recovery_ready");
    #[cfg(feature = "subscription-update")]
    let (worker_wake, wake_receiver) = std::sync::mpsc::channel();
    #[cfg(feature = "subscription-update")]
    let watcher = {
        let mut paths = vec![runtime.root().join("config")];
        let module_config = PathBuf::from("/data/adb/modules/nethop/config");
        if module_config.is_dir() && !paths.contains(&module_config) {
            paths.push(module_config);
        }
        ConfigWatcher::start_with_wake(&paths, worker_wake.clone())
            .map_err(|_| ApplicationError::WorkerInitializationFailed)?
    };
    #[cfg(feature = "subscription-update")]
    let watcher_dirty = watcher.dirty();
    #[cfg(feature = "subscription-update")]
    let watcher_healthy = watcher.healthy();
    #[cfg(feature = "subscription-update")]
    report_worker_stage("watcher_ready");
    #[cfg(feature = "subscription-update")]
    let mut application = {
        let secret = ApiSecretStore::new(runtime.root().join("state/api.secret"))
            .and_then(|store| store.load_or_create())
            .map_err(|_| ApplicationError::WorkerInitializationFailed)?;
        let secret_value = secret.expose_for_composer().to_owned();
        let operational_control = OperationalControl::new(
            ClashApiClient::new(
                "127.0.0.1:9090"
                    .parse()
                    .map_err(|_| ApplicationError::WorkerInitializationFailed)?,
                secret_value.clone(),
                ClashApiLimits::default(),
            )
            .map_err(|_| ApplicationError::WorkerInitializationFailed)?,
            NodeSelectionStore::new(runtime.root().join("state/selection.v2.json"))
                .map_err(|_| ApplicationError::WorkerInitializationFailed)?,
            runtime.root().join("state/diagnostics-latest.json"),
        )
        .and_then(|control| control.with_generation_root(runtime.root().join("generations")))
        .map_err(|_| ApplicationError::WorkerInitializationFailed)?;
        report_worker_stage("operational_control_ready");
        let clash_api = ClashApi::new("127.0.0.1:9090", secret_value.clone())
            .map_err(|_| ApplicationError::WorkerInitializationFailed)?;
        let limits = ParserLimits::default();
        let matrix = CapabilityMatrix::default();
        let local_fetch_proxy = LocalFetchProxy::new(
            MANAGED_FETCH_PROXY_ENDPOINT
                .parse()
                .map_err(|_| ApplicationError::WorkerInitializationFailed)?,
            MANAGED_FETCH_PROXY_USERNAME,
            secret_value.clone(),
        )
        .map_err(|_| ApplicationError::WorkerInitializationFailed)?;
        let fetcher = HttpSourceBodyFetcher::new(limits, matrix.clone())
            .with_cache_root(runtime.root().join("subscriptions/cache"))
            .map_err(|_| ApplicationError::WorkerInitializationFailed)?
            .with_local_proxy(local_fetch_proxy);
        let node_override_store =
            NodeOverrideStore::new(runtime.root().join("subscriptions/node-overrides.json"))
                .map_err(|_| ApplicationError::WorkerInitializationFailed)?;
        let node_overrides = node_override_store
            .load()
            .map_err(|_| ApplicationError::WorkerInitializationFailed)?;
        let service = SourceUpdateService::new(
            &store,
            fetcher,
            &checker,
            limits,
            matrix,
            UpdateRuntimePolicy::new(
                capture.clone(),
                clash_api,
                config.managed_tun_stack(),
                config
                    .managed_options()
                    .map_err(|_| ApplicationError::WorkerInitializationFailed)?,
            ),
        )
        .with_manual_source_store(
            ManualSourceStore::new(runtime.root().join("subscriptions/manual-source.body"))
                .map_err(|_| ApplicationError::WorkerInitializationFailed)?,
        )
        .with_node_override_store(node_override_store, node_overrides);
        let updater = ConfiguredSourceUpdater::new(service, source_config);
        report_worker_stage("source_updater_ready");
        let database_path = runtime.root().join("state/nethop.db");
        let schedule = PersistentUpdateSchedule::load(
            StatsStore::open(&database_path)
                .map_err(|_| ApplicationError::WorkerInitializationFailed)?,
        )
        .map_err(|_| ApplicationError::WorkerInitializationFailed)?;
        let core_version_schedule = PersistentCoreVersionSchedule::load(
            StatsStore::open(&database_path)
                .map_err(|_| ApplicationError::WorkerInitializationFailed)?,
        )
        .map_err(|_| ApplicationError::WorkerInitializationFailed)?;
        report_worker_stage("base_schedules_ready");
        let rule_set_root = runtime.root().join("rulesets");
        let rule_set_store = RuleSetStore::open(&rule_set_root, RuleSetLimits::default())
            .map_err(|_| ApplicationError::WorkerInitializationFailed)?;
        report_worker_stage("ruleset_store_ready");
        let rule_set_checker =
            SingBoxCheckRunner::new(&binary, &rule_set_root, RunnerLimits::default())
                .map_err(|_| ApplicationError::WorkerInitializationFailed)?;
        report_worker_stage("ruleset_checker_ready");
        let rule_set_updater = RuleSetUpdateService::new(
            rule_set_store,
            HttpRuleSetBodyFetcher::default()
                .with_cache_root(runtime.root().join("state/ruleset-cache"))
                .map_err(|_| ApplicationError::WorkerInitializationFailed)?,
            rule_set_checker,
            RuleSetProviderManifest::bundled()
                .map_err(|_| ApplicationError::WorkerInitializationFailed)?
                .clone(),
        );
        let rule_set_schedule = PersistentRuleSetSchedule::load(
            StatsStore::open(&database_path)
                .map_err(|_| ApplicationError::WorkerInitializationFailed)?,
        )
        .map_err(|_| ApplicationError::WorkerInitializationFailed)?;
        report_worker_stage("ruleset_runtime_ready");
        let source_status = SourceStatusStore::open(&database_path)
            .map_err(|_| ApplicationError::WorkerInitializationFailed)?;
        let webui_payload_store =
            WebUiPayloadStore::open(runtime.root().join("state/webui-payloads"))
                .map_err(|_| ApplicationError::WorkerInitializationFailed)?;
        WorkerApplication::new(
            recovery,
            network,
            verifier,
            MonotonicClock::start(),
            capture.clone(),
            PlanSlot::A,
            WorkerRuntimeLimits::new(
                Duration::from_millis(250),
                Duration::from_secs(u64::from(config.advanced().reconcile_interval_seconds())),
                Duration::from_secs(5 * 60),
            )
            .map_err(|_| ApplicationError::WorkerInitializationFailed)?,
        )
        .with_updater(OptionalRuntimeUpdateSource::new(Some(updater)))
        .with_webui_payload_store(webui_payload_store)
        .with_source_status_store(source_status)
        .with_operational_control(operational_control)
        .with_private_dns_source(private_dns_source)
        .with_wifi_facts_source(wifi_facts)
        .with_core_version_source(core_version_source)
        .with_core_update_notifier(core_update_notifier)
        .with_core_version_state(core_version_state)
        .with_core_version_schedule(core_version_schedule)
        .with_rule_set_update_source(rule_set_updater)
        .with_rule_set_schedule(rule_set_schedule)
        .with_subscription_transactions(journal, MutationCoordinator::default())
        .with_package_backend(package_backend)
        .with_configuration(config_runtime, restore_current)
        .with_update_schedule(schedule)
        .with_log_retention(
            FileLogRetention::new(runtime.root().join("logs"))
                .map_err(|_| ApplicationError::WorkerInitializationFailed)?,
        )
        .with_event_log_directory(runtime.root().join("logs"))
        .map_err(|_| ApplicationError::WorkerInitializationFailed)?
        .with_config_wake(watcher_dirty, watcher_healthy)
        .with_worker_wake(worker_wake)
    };
    report_worker_stage("application_ready");
    #[cfg(not(feature = "subscription-update"))]
    let mut application = WorkerApplication::new(
        recovery,
        network,
        verifier,
        MonotonicClock::start(),
        capture,
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    )
    .with_private_dns_source(private_dns_source);
    let server = UnixControlServer::bind(
        runtime.run().join("nethopd.sock"),
        ControlServerLimits::default(),
    )
    .map_err(|_| ApplicationError::WorkerInitializationFailed)?;
    report_worker_stage("control_socket_ready");
    #[cfg(feature = "subscription-update")]
    let mut driver = SystemWorkerServiceDriver::install(wake_receiver)?;
    #[cfg(not(feature = "subscription-update"))]
    let mut driver = SystemWorkerServiceDriver::install(std::sync::mpsc::channel().1)?;
    #[cfg(feature = "subscription-update")]
    let _watcher = watcher;
    run_worker_service(&server, &mut application, &mut driver).map_err(ApplicationError::Worker)
}

#[cfg(all(unix, feature = "subscription-update"))]
fn report_worker_stage(stage: &'static str) {
    eprintln!("nethopd worker init stage: {stage}");
}

#[cfg(feature = "subscription-update")]
#[cfg_attr(not(unix), allow(dead_code))]
fn current_generation_matches(
    store: &nethop_core::GenerationStore,
    source_config_digest: &str,
) -> bool {
    store
        .current_manifest()
        .ok()
        .flatten()
        .and_then(|manifest| manifest.source_config_digest)
        .is_some_and(|digest| digest == source_config_digest)
}

#[cfg(all(unix, not(feature = "subscription-update")))]
pub fn run_system_worker(_runtime: &RuntimeRoot) -> Result<(), ApplicationError> {
    Err(ApplicationError::WorkerInitializationFailed)
}

#[cfg(not(unix))]
pub fn run_system_worker(_runtime: &RuntimeRoot) -> Result<(), ApplicationError> {
    Err(ApplicationError::UnsupportedPlatform)
}

#[derive(Debug)]
struct PidFile {
    path: PathBuf,
}

impl PidFile {
    fn acquire(path: PathBuf) -> Result<Self, ApplicationError> {
        if path.file_name().is_none() {
            return Err(ApplicationError::PidFileFailed);
        }
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                remove_stale_pid_file(&path)?;
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .map_err(|_| ApplicationError::AlreadyRunning)?
            }
            Err(_) => return Err(ApplicationError::PidFileFailed),
        };
        set_private_file(&file).map_err(|_| ApplicationError::PidFileFailed)?;
        let start_time = process_start_time_ticks(std::process::id()).unwrap_or(0);
        if writeln!(file, "{} {start_time}", std::process::id()).is_err()
            || file.sync_all().is_err()
        {
            drop(file);
            let _ = fs::remove_file(&path);
            return Err(ApplicationError::PidFileFailed);
        }
        Ok(Self { path })
    }
}

fn remove_stale_pid_file(path: &Path) -> Result<(), ApplicationError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| ApplicationError::AlreadyRunning)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(ApplicationError::AlreadyRunning);
    }
    let contents = fs::read_to_string(path).map_err(|_| ApplicationError::AlreadyRunning)?;
    let mut fields = contents.split_whitespace();
    let pid = fields
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or(ApplicationError::AlreadyRunning)?;
    let expected = fields
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or(ApplicationError::AlreadyRunning)?;
    if fields.next().is_some()
        || process_start_time_ticks(pid).is_some_and(|actual| actual == expected)
    {
        return Err(ApplicationError::AlreadyRunning);
    }
    fs::remove_file(path).map_err(|_| ApplicationError::AlreadyRunning)
}

impl Drop for PidFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(unix)]
fn set_private_file(file: &File) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;

    file.set_permissions(fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_private_file(_file: &File) -> Result<(), std::io::Error> {
    Ok(())
}

#[cfg(unix)]
fn ensure_root() -> Result<(), ApplicationError> {
    // SAFETY: geteuid has no preconditions and does not dereference pointers.
    (unsafe { libc::geteuid() } == 0)
        .then_some(())
        .ok_or(ApplicationError::RootRequired)
}

#[cfg(any(target_os = "android", target_os = "linux"))]
fn process_start_time_ticks(pid: u32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat.rsplit_once(") ")?
        .1
        .split_whitespace()
        .nth(19)?
        .parse()
        .ok()
}

#[cfg(not(any(target_os = "android", target_os = "linux")))]
const fn process_start_time_ticks(_pid: u32) -> Option<u64> {
    None
}

#[cfg(not(unix))]
fn ensure_root() -> Result<(), ApplicationError> {
    Err(ApplicationError::UnsupportedPlatform)
}

#[cfg(unix)]
extern "C" fn request_stop(_signal: libc::c_int) {
    STOP_REQUESTED.store(true, Ordering::Release);
}

#[cfg(unix)]
fn install_signal_handlers() -> Result<(), ApplicationError> {
    // SAFETY: signal installs a process-global handler with the C ABI. The
    // handler performs only an atomic store, which is signal-safe.
    unsafe {
        let handler = request_stop as *const () as libc::sighandler_t;
        if libc::signal(libc::SIGTERM, handler) == libc::SIG_ERR
            || libc::signal(libc::SIGINT, handler) == libc::SIG_ERR
        {
            return Err(ApplicationError::SignalHandlerFailed);
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn install_signal_handlers() -> Result<(), ApplicationError> {
    Err(ApplicationError::UnsupportedPlatform)
}

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error("usage: nethopd <--supervise|--worker> --root <absolute-path>")]
    Usage,
    #[error("runtime root and run directory must be absolute real directories")]
    InvalidRuntimeRoot,
    #[error("nethopd must run as root")]
    RootRequired,
    #[error("nethopd is unsupported on this platform")]
    UnsupportedPlatform,
    #[error("nethopd executable path is invalid")]
    InvalidExecutable,
    #[error("nethopd instance is already running")]
    AlreadyRunning,
    #[error("nethopd PID file could not be published")]
    PidFileFailed,
    #[error("nethopd signal handlers could not be installed")]
    SignalHandlerFailed,
    #[error("nethopd worker could not be initialized")]
    WorkerInitializationFailed,
    #[error("nethopd worker failed")]
    Worker(#[from] WorkerServiceError),
    #[error("worker supervisor failed")]
    Supervisor(#[from] SupervisorError),
}

#[cfg(all(test, feature = "subscription-update"))]
mod tests {
    use super::current_generation_matches;
    use nethop_core::GenerationStore;

    #[test]
    fn invalid_derived_generation_is_rebuilt_instead_of_aborting_startup() {
        let directory = tempfile::tempdir().unwrap();
        let store = GenerationStore::new(directory.path()).unwrap();
        let generations = directory.path().join("generations");
        std::fs::create_dir(generations.join("1")).unwrap();
        std::fs::write(generations.join("current"), b"1\n").unwrap();

        assert!(!current_generation_matches(&store, &"a".repeat(64)));
    }

    #[test]
    fn missing_generation_requires_a_fresh_subscription_build() {
        let directory = tempfile::tempdir().unwrap();
        let store = GenerationStore::new(directory.path()).unwrap();

        assert!(!current_generation_matches(&store, &"a".repeat(64)));
    }
}
