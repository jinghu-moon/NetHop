use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};
#[cfg(feature = "subscription-update")]
use std::{collections::BTreeMap, path::Path};

use nethop_android::{
    AllocationCapability, CapabilityError, CapabilityReport, CapabilityStatus, ExecutionError,
    FamilyCapability, IpFamily, NetfilterBackend, NetworkHealthError, NetworkHealthVerifier,
    NetworkPlan, PlanSlot, PrivateDnsError, PrivateDnsFactsSource, PrivateDnsMode,
    PrivateDnsStatus, ResourceCandidate,
};
use nethop_android::{UpdateNotificationOutcome, UpdateNotificationSink};
#[cfg(feature = "subscription-update")]
use nethop_android::{WifiFactsSource, WifiNetworkFacts, WifiSceneError};
#[cfg(feature = "subscription-update")]
use nethop_core::{Candidate, GenerationId, GenerationStore, ManagedConfig, TerminalOutbound};
use nethop_core::{CaptureMode, CapturePolicy, RuntimeState};
#[cfg(feature = "subscription-update")]
use nethop_protocol::ControlParams;
use nethop_protocol::{ControlMethod, ControlRequest, RequestId};
use nethopd::{
    ActiveRuntime, CandidateProcess, ControlRequestHandler, NetworkController, ProcessError,
    ProcessIdentity, RuntimeRecoverySource, WorkerApplication, WorkerClock, WorkerRecoveryError,
    WorkerRuntimeLimits, WorkerServiceTasks,
};
#[cfg(feature = "subscription-update")]
use nethopd::{
    CandidateChecker, CapabilitySource, CommitJournalStore, ConfigRuntime, ConfigStore,
    CoreLauncher, CurrentGenerationActivator, DataPlaneHealthError, DataPlaneHealthProbe,
    HealthProbe, HealthProbeError, InMemoryScheduleStore, MutationCoordinator,
    PersistentCoreVersionSchedule, RuleSetUpdateError, RuleSetUpdatePreparation, RunnerError,
    RuntimeAttachmentView, RuntimeCoreVersionSchedule, RuntimePolicyError, RuntimeRuleSetSchedule,
    RuntimeRuleSetUpdateSource, RuntimeUpdateError, RuntimeUpdateSource, SchedulerError,
    SourceIdEntropy, SourceRegistry, SourceRegistryError, SourceStatusStore, UpdateStatus,
};
use nethopd::{CoreReleaseBodyFetcher, CoreVersion, CoreVersionCheckError, CoreVersionChecker};
#[cfg(feature = "subscription-update")]
use serde_json::json;
#[cfg(feature = "subscription-update")]
use tempfile::tempdir;

#[derive(Clone)]
struct TestClock(Rc<Cell<Duration>>);

impl WorkerClock for TestClock {
    fn now(&self) -> Duration {
        self.0.get()
    }
}

struct TestProcess;

struct FixedCoreRelease;

impl CoreReleaseBodyFetcher for FixedCoreRelease {
    fn fetch_release_body(&mut self) -> Result<Vec<u8>, CoreVersionCheckError> {
        Ok(br#"{"tag_name":"v1.13.16","draft":false,"prerelease":false}"#.to_vec())
    }
}

#[cfg(feature = "subscription-update")]
struct CountingCoreRelease(Rc<Cell<u32>>);

#[cfg(feature = "subscription-update")]
impl CoreReleaseBodyFetcher for CountingCoreRelease {
    fn fetch_release_body(&mut self) -> Result<Vec<u8>, CoreVersionCheckError> {
        self.0.set(self.0.get() + 1);
        Ok(br#"{"tag_name":"v1.13.16","draft":false,"prerelease":false}"#.to_vec())
    }
}

struct RecordingCoreUpdateNotifier(Rc<Cell<u32>>);

impl UpdateNotificationSink for RecordingCoreUpdateNotifier {
    fn notify_core_update(&mut self) -> UpdateNotificationOutcome {
        self.0.set(self.0.get() + 1);
        UpdateNotificationOutcome::Posted
    }
}

struct FixedPrivateDnsFacts(PrivateDnsMode);

impl PrivateDnsFactsSource for FixedPrivateDnsFacts {
    fn current(&mut self) -> Result<PrivateDnsStatus, PrivateDnsError> {
        Ok(PrivateDnsStatus::from_mode(self.0))
    }
}

#[cfg(feature = "subscription-update")]
struct FailingCoreSchedule {
    fail_on_take: bool,
    attempts: Rc<Cell<u32>>,
}

#[cfg(feature = "subscription-update")]
struct TestRuleSetSchedule {
    due: bool,
    results: Rc<RefCell<Vec<bool>>>,
}

#[cfg(feature = "subscription-update")]
impl RuntimeRuleSetSchedule for TestRuleSetSchedule {
    fn next_wakeup_in(&self) -> Option<Duration> {
        self.due.then_some(Duration::ZERO)
    }

    fn take_due(&mut self) -> Result<bool, SchedulerError> {
        Ok(std::mem::take(&mut self.due))
    }

    fn record_result(&mut self, succeeded: bool) -> Result<(), SchedulerError> {
        self.results.borrow_mut().push(succeeded);
        Ok(())
    }
}

#[cfg(feature = "subscription-update")]
struct TestRuleSetUpdater {
    events: Rc<RefCell<Vec<&'static str>>>,
    unchanged: bool,
    fail_commit: bool,
}

#[cfg(feature = "subscription-update")]
impl RuntimeRuleSetUpdateSource for TestRuleSetUpdater {
    fn prepare_update(&mut self) -> Result<RuleSetUpdatePreparation, RuleSetUpdateError> {
        self.events.borrow_mut().push("prepare_ruleset");
        Ok(if self.unchanged {
            RuleSetUpdatePreparation::Unchanged
        } else {
            RuleSetUpdatePreparation::Prepared
        })
    }

    fn publish_update(&mut self) -> Result<(), RuleSetUpdateError> {
        self.events.borrow_mut().push("publish_ruleset");
        Ok(())
    }

    fn commit_update(&mut self) -> Result<(), RuleSetUpdateError> {
        self.events.borrow_mut().push("commit_ruleset");
        if self.fail_commit {
            Err(RuleSetUpdateError::Admission)
        } else {
            Ok(())
        }
    }

    fn rollback_update(&mut self) -> Result<(), RuleSetUpdateError> {
        self.events.borrow_mut().push("rollback_ruleset");
        Ok(())
    }
}

#[cfg(feature = "subscription-update")]
impl RuntimeCoreVersionSchedule for FailingCoreSchedule {
    fn next_wakeup_in(&self) -> Option<Duration> {
        Some(Duration::ZERO)
    }

    fn take_due(&mut self) -> Result<bool, SchedulerError> {
        self.attempts.set(self.attempts.get() + 1);
        if self.fail_on_take {
            Err(SchedulerError::PersistenceFailed)
        } else {
            Ok(true)
        }
    }

    fn record_result(&mut self, _succeeded: bool) -> Result<(), SchedulerError> {
        Err(SchedulerError::PersistenceFailed)
    }
}

impl CandidateProcess for TestProcess {
    fn identity(&self) -> ProcessIdentity {
        ProcessIdentity::new(1, Some(1)).unwrap()
    }

    fn is_running(&mut self) -> Result<bool, ProcessError> {
        Ok(true)
    }

    fn stop(self) -> Result<(), ProcessError> {
        Ok(())
    }
}

#[derive(Default)]
struct TestNetwork;

impl NetworkController for TestNetwork {
    type Receipt = ();

    fn apply(&mut self, _plan: &NetworkPlan) -> Result<Self::Receipt, ExecutionError> {
        Ok(())
    }

    fn rollback(
        &mut self,
        _plan: &NetworkPlan,
        _receipt: &mut Self::Receipt,
    ) -> Result<(), ExecutionError> {
        Ok(())
    }
}

#[cfg(feature = "subscription-update")]
struct DelayedRuleSetSchedule {
    calls: u32,
    due_on_call: u32,
    results: Rc<RefCell<Vec<bool>>>,
}

#[cfg(feature = "subscription-update")]
impl RuntimeRuleSetSchedule for DelayedRuleSetSchedule {
    fn next_wakeup_in(&self) -> Option<Duration> {
        Some(Duration::ZERO)
    }

    fn take_due(&mut self) -> Result<bool, SchedulerError> {
        self.calls += 1;
        Ok(self.calls == self.due_on_call)
    }

    fn record_result(&mut self, succeeded: bool) -> Result<(), SchedulerError> {
        self.results.borrow_mut().push(succeeded);
        Ok(())
    }
}

#[cfg(feature = "subscription-update")]
struct RuleSetProcess {
    events: Rc<RefCell<Vec<&'static str>>>,
}

#[cfg(feature = "subscription-update")]
impl CandidateProcess for RuleSetProcess {
    fn identity(&self) -> ProcessIdentity {
        ProcessIdentity::new(7, Some(7)).unwrap()
    }

    fn is_running(&mut self) -> Result<bool, ProcessError> {
        Ok(true)
    }

    fn stop(self) -> Result<(), ProcessError> {
        self.events.borrow_mut().push("core_stop");
        Ok(())
    }
}

#[cfg(feature = "subscription-update")]
struct RuleSetLauncher {
    starts: Rc<Cell<u32>>,
    events: Rc<RefCell<Vec<&'static str>>>,
    fail_on_start: Option<u32>,
}

#[cfg(feature = "subscription-update")]
impl CoreLauncher for RuleSetLauncher {
    type Process = RuleSetProcess;

    fn start(&self, _config_path: &Path) -> Result<Self::Process, ProcessError> {
        let attempt = self.starts.get() + 1;
        self.starts.set(attempt);
        self.events.borrow_mut().push("core_start");
        if self.fail_on_start == Some(attempt) {
            Err(ProcessError::SpawnFailed)
        } else {
            Ok(RuleSetProcess {
                events: Rc::clone(&self.events),
            })
        }
    }
}

#[cfg(feature = "subscription-update")]
struct RuleSetChecker;

#[cfg(feature = "subscription-update")]
impl CandidateChecker for RuleSetChecker {
    fn check(&self, _config_path: &Path) -> Result<(), RunnerError> {
        Ok(())
    }
}

#[cfg(feature = "subscription-update")]
struct RuleSetCoreHealth;

#[cfg(feature = "subscription-update")]
impl HealthProbe<RuleSetProcess> for RuleSetCoreHealth {
    fn wait_healthy(&self, _process: &mut RuleSetProcess) -> Result<(), HealthProbeError> {
        Ok(())
    }
}

#[cfg(feature = "subscription-update")]
struct RuleSetCapability;

#[cfg(feature = "subscription-update")]
impl CapabilitySource for RuleSetCapability {
    fn probe(&mut self) -> Result<CapabilityReport, CapabilityError> {
        Ok(test_capability_report())
    }
}

#[cfg(feature = "subscription-update")]
struct RuleSetDataPlaneHealth;

#[cfg(feature = "subscription-update")]
impl DataPlaneHealthProbe<RuleSetProcess> for RuleSetDataPlaneHealth {
    fn wait_healthy(
        &mut self,
        _process: &mut RuleSetProcess,
        _attachment: RuntimeAttachmentView<'_>,
        _capabilities: &CapabilityReport,
    ) -> Result<(), DataPlaneHealthError> {
        Ok(())
    }
}

#[cfg(feature = "subscription-update")]
struct RuleSetRecovery {
    _directory: tempfile::TempDir,
    store: GenerationStore,
    checker: RuleSetChecker,
    launcher: RuleSetLauncher,
    health: RuleSetCoreHealth,
    capability: RuleSetCapability,
    data_health: RuleSetDataPlaneHealth,
}

#[cfg(feature = "subscription-update")]
impl RuleSetRecovery {
    fn new(
        events: Rc<RefCell<Vec<&'static str>>>,
        starts: Rc<Cell<u32>>,
        fail_on_start: Option<u32>,
    ) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let store = GenerationStore::new(directory.path()).unwrap();
        let outbound = TerminalOutbound::new(
            "fixture",
            "trojan",
            BTreeMap::from([
                ("server".to_owned(), json!("example.com")),
                ("server_port".to_owned(), json!(443)),
                ("password".to_owned(), json!("fixture-only")),
            ]),
        )
        .unwrap();
        let candidate = Candidate::new(
            GenerationId::new(1).unwrap(),
            ManagedConfig::from_outbounds(vec![outbound]).unwrap(),
        );
        store.publish(&candidate, |_| Ok(())).unwrap();
        Self {
            _directory: directory,
            store,
            checker: RuleSetChecker,
            launcher: RuleSetLauncher {
                starts,
                events,
                fail_on_start,
            },
            health: RuleSetCoreHealth,
            capability: RuleSetCapability,
            data_health: RuleSetDataPlaneHealth,
        }
    }
}

#[cfg(feature = "subscription-update")]
impl RuntimeRecoverySource<TestNetwork> for RuleSetRecovery {
    type Process = RuleSetProcess;

    fn recover(
        &mut self,
        network: &mut TestNetwork,
        policy: &CapturePolicy,
        slot: PlanSlot,
    ) -> Result<Option<ActiveRuntime<Self::Process, ()>>, WorkerRecoveryError> {
        CurrentGenerationActivator::new(
            &self.store,
            &self.checker,
            &self.launcher,
            &self.health,
            &mut self.capability,
            network,
            &mut self.data_health,
        )
        .recover(policy, slot)
    }

    fn recover_generation(
        &mut self,
        network: &mut TestNetwork,
        policy: &CapturePolicy,
        slot: PlanSlot,
        generation: GenerationId,
    ) -> Result<Option<ActiveRuntime<Self::Process, ()>>, WorkerRecoveryError> {
        CurrentGenerationActivator::new(
            &self.store,
            &self.checker,
            &self.launcher,
            &self.health,
            &mut self.capability,
            network,
            &mut self.data_health,
        )
        .recover_generation(generation, policy, slot)
    }

    fn probe(&mut self) -> Result<CapabilityReport, CapabilityError> {
        self.capability.probe()
    }
}

#[derive(Default)]
struct TestRecovery {
    attempts: Rc<RefCell<Vec<&'static str>>>,
    fail: bool,
    fail_generation: bool,
}

#[cfg(feature = "subscription-update")]
struct TestUpdater {
    events: Rc<RefCell<Vec<&'static str>>>,
    fail_prepare: bool,
    needed: bool,
    generation: nethop_core::GenerationId,
    current: bool,
}

#[cfg(feature = "subscription-update")]
impl RuntimeUpdateSource for TestUpdater {
    type Prepared = ();

    fn is_needed(&self) -> bool {
        self.needed
    }

    fn request_cached_rebuild(&mut self) -> Result<(), RuntimeUpdateError> {
        self.events.borrow_mut().push("cached_rebuild");
        Ok(())
    }

    fn prepare(&mut self) -> Result<Self::Prepared, RuntimeUpdateError> {
        self.events.borrow_mut().push("prepare");
        if self.fail_prepare {
            Err(RuntimeUpdateError::Prepare)
        } else {
            Ok(())
        }
    }

    fn generation(&self, _prepared: &Self::Prepared) -> nethop_core::GenerationId {
        self.generation
    }

    fn is_current(&self, _prepared: &Self::Prepared) -> bool {
        self.current
    }

    fn commit(
        &mut self,
        _prepared: Self::Prepared,
    ) -> Result<nethop_core::GenerationId, RuntimeUpdateError> {
        self.events.borrow_mut().push("commit");
        nethop_core::GenerationId::new(2).map_err(|_| RuntimeUpdateError::Commit)
    }

    fn discard(&mut self, _prepared: Self::Prepared) -> Result<(), RuntimeUpdateError> {
        self.events.borrow_mut().push("discard");
        Ok(())
    }
}

#[cfg(feature = "subscription-update")]
struct SelectingUpdater {
    selected: Rc<RefCell<Vec<Option<String>>>>,
}

#[cfg(feature = "subscription-update")]
struct ImportUpdater {
    events: Rc<RefCell<Vec<&'static str>>>,
    pending_import: bool,
    generation: GenerationId,
}

#[cfg(feature = "subscription-update")]
struct FixedWifiFacts;

#[cfg(feature = "subscription-update")]
impl WifiFactsSource for FixedWifiFacts {
    fn current(&mut self) -> Result<WifiNetworkFacts, WifiSceneError> {
        WifiNetworkFacts::new(
            Some("Trusted Home".into()),
            Some("aa:bb:cc:dd:ee:ff".into()),
        )
    }
}

#[cfg(feature = "subscription-update")]
impl RuntimeUpdateSource for SelectingUpdater {
    type Prepared = ();

    fn request_source_update(&mut self, source_id: Option<&str>) -> Result<(), RuntimeUpdateError> {
        self.selected
            .borrow_mut()
            .push(source_id.map(str::to_owned));
        Ok(())
    }

    fn prepare(&mut self) -> Result<Self::Prepared, RuntimeUpdateError> {
        Ok(())
    }

    fn generation(&self, _prepared: &Self::Prepared) -> nethop_core::GenerationId {
        nethop_core::GenerationId::new(2).unwrap()
    }

    fn commit(
        &mut self,
        _prepared: Self::Prepared,
    ) -> Result<nethop_core::GenerationId, RuntimeUpdateError> {
        Ok(nethop_core::GenerationId::new(2).unwrap())
    }

    fn discard(&mut self, _prepared: Self::Prepared) -> Result<(), RuntimeUpdateError> {
        Ok(())
    }
}

#[cfg(feature = "subscription-update")]
impl RuntimeUpdateSource for ImportUpdater {
    type Prepared = ();

    fn request_source_update(
        &mut self,
        _source_id: Option<&str>,
    ) -> Result<(), RuntimeUpdateError> {
        self.events.borrow_mut().push("unexpected_source_update");
        Err(RuntimeUpdateError::Prepare)
    }

    fn request_import(
        &mut self,
        _bytes: Vec<u8>,
        _format_hint: nethop_subscription::FormatHint,
        _candidate_digest: String,
    ) -> Result<(), RuntimeUpdateError> {
        if self.pending_import {
            return Err(RuntimeUpdateError::Prepare);
        }
        self.pending_import = true;
        self.events.borrow_mut().push("request_import");
        Ok(())
    }

    fn prepare(&mut self) -> Result<Self::Prepared, RuntimeUpdateError> {
        if !std::mem::take(&mut self.pending_import) {
            return Err(RuntimeUpdateError::Prepare);
        }
        self.events.borrow_mut().push("prepare_import");
        Ok(())
    }

    fn generation(&self, _prepared: &Self::Prepared) -> GenerationId {
        self.generation
    }

    fn commit(&mut self, _prepared: Self::Prepared) -> Result<GenerationId, RuntimeUpdateError> {
        self.events.borrow_mut().push("commit_import");
        Ok(self.generation)
    }

    fn discard(&mut self, _prepared: Self::Prepared) -> Result<(), RuntimeUpdateError> {
        self.events.borrow_mut().push("discard_import");
        Ok(())
    }
}

impl RuntimeRecoverySource<TestNetwork> for TestRecovery {
    type Process = TestProcess;

    fn recover(
        &mut self,
        _network: &mut TestNetwork,
        _policy: &CapturePolicy,
        _slot: PlanSlot,
    ) -> Result<Option<ActiveRuntime<Self::Process, ()>>, WorkerRecoveryError> {
        self.attempts.borrow_mut().push("recover");
        if self.fail {
            Err(WorkerRecoveryError::CapabilityProbeFailed)
        } else {
            Ok(None)
        }
    }

    fn probe(&mut self) -> Result<CapabilityReport, CapabilityError> {
        self.attempts.borrow_mut().push("probe");
        Ok(test_capability_report())
    }

    fn recover_generation(
        &mut self,
        _network: &mut TestNetwork,
        _policy: &CapturePolicy,
        _slot: PlanSlot,
        _generation: nethop_core::GenerationId,
    ) -> Result<Option<ActiveRuntime<Self::Process, ()>>, WorkerRecoveryError> {
        self.attempts.borrow_mut().push("recover_generation");
        if self.fail_generation {
            Err(WorkerRecoveryError::CoreHealthFailed {
                cleanup_failed: false,
            })
        } else {
            Ok(None)
        }
    }
}

#[cfg(feature = "subscription-update")]
struct PolicyRecordingRecovery {
    inner: TestRecovery,
    updates: Rc<RefCell<Vec<(u16, u64, usize)>>>,
}

#[cfg(feature = "subscription-update")]
impl RuntimeRecoverySource<TestNetwork> for PolicyRecordingRecovery {
    type Process = TestProcess;

    fn recover(
        &mut self,
        network: &mut TestNetwork,
        policy: &CapturePolicy,
        slot: PlanSlot,
    ) -> Result<Option<ActiveRuntime<Self::Process, ()>>, WorkerRecoveryError> {
        self.inner.recover(network, policy, slot)
    }

    fn recover_generation(
        &mut self,
        network: &mut TestNetwork,
        policy: &CapturePolicy,
        slot: PlanSlot,
        generation: nethop_core::GenerationId,
    ) -> Result<Option<ActiveRuntime<Self::Process, ()>>, WorkerRecoveryError> {
        self.inner
            .recover_generation(network, policy, slot, generation)
    }

    fn probe(&mut self) -> Result<CapabilityReport, CapabilityError> {
        self.inner.probe()
    }

    fn replace_runtime_policy(
        &mut self,
        candidates: Vec<ResourceCandidate>,
        inbound_port: u16,
        health_timeout: Duration,
    ) -> Result<(), RuntimePolicyError> {
        self.updates
            .borrow_mut()
            .push((inbound_port, health_timeout.as_secs(), candidates.len()));
        Ok(())
    }
}

#[cfg(feature = "subscription-update")]
#[derive(Default)]
struct PolicyRecordingVerifier(Rc<RefCell<Vec<u16>>>);

#[cfg(feature = "subscription-update")]
impl NetworkHealthVerifier for PolicyRecordingVerifier {
    fn verify(&mut self, _plan: &NetworkPlan) -> Result<(), NetworkHealthError> {
        Ok(())
    }

    fn replace_inbound_port(&mut self, inbound_port: u16) -> Result<(), NetworkHealthError> {
        self.0.borrow_mut().push(inbound_port);
        Ok(())
    }
}

fn test_capability_report() -> CapabilityReport {
    fn family(family: IpFamily) -> FamilyCapability {
        FamilyCapability::new(
            family,
            CapabilityStatus::Supported,
            CapabilityStatus::Supported,
            CapabilityStatus::Supported,
            CapabilityStatus::Supported,
            CapabilityStatus::Supported,
            CapabilityStatus::Supported,
            CapabilityStatus::Supported,
            CapabilityStatus::Supported,
            CapabilityStatus::Supported,
            CapabilityStatus::Supported,
        )
    }
    let candidate = ResourceCandidate::new(0x4e49_0100, u32::MAX, 100, 12_000).unwrap();
    CapabilityReport::new(
        CapabilityStatus::Supported,
        "arm64-v8a",
        CapabilityStatus::Supported,
        true,
        NetfilterBackend::NftWrapper,
        family(IpFamily::Ipv4),
        family(IpFamily::Ipv6),
        CapabilityStatus::Supported,
        CapabilityStatus::Supported,
        7893,
        CapabilityStatus::Supported,
        vec![AllocationCapability::new(
            candidate,
            CapabilityStatus::Supported,
        )],
    )
    .unwrap()
    .with_interfaces(vec!["wlan0".into()])
    .unwrap()
}

#[derive(Default)]
struct TestVerifier;

impl NetworkHealthVerifier for TestVerifier {
    fn verify(&mut self, _plan: &NetworkPlan) -> Result<(), NetworkHealthError> {
        Ok(())
    }
}

fn policy() -> CapturePolicy {
    CapturePolicy::new(
        CaptureMode::Tproxy,
        true,
        Some(7893),
        Some(0x20_000),
        Vec::new(),
        vec![0],
    )
    .unwrap()
}

fn request(id: &str, method: ControlMethod) -> ControlRequest {
    ControlRequest::new(RequestId::new(id).unwrap(), method)
}

#[cfg(feature = "subscription-update")]
fn request_with_params(
    id: &str,
    method: ControlMethod,
    wait: bool,
    if_needed: bool,
) -> ControlRequest {
    request(id, method)
        .with_params(ControlParams::new(wait, if_needed))
        .unwrap()
}

#[test]
#[cfg(feature = "subscription-update")]
fn manual_import_apply_executes_the_armed_import_without_requesting_a_source_refresh() {
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    fs::write(
        &config_path,
        "schema_version = 3\n[service]\nenabled = true\n[subscriptions]\n[[subscriptions.sources]]\nname = \"Primary\"\nurl = \"\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
    let store = ConfigStore::new(&config_path).unwrap();
    let snapshot = store.load().unwrap();
    let digest = snapshot.digest().to_owned();
    let registry = SourceRegistry::new(directory.path().join("source-registry.v1.json")).unwrap();
    let sources = registry.reconcile(&snapshot, &mut FixedEntropy(1)).unwrap();
    let config_runtime = ConfigRuntime::new(store, registry, snapshot, &sources);
    let runtime_events = Rc::new(RefCell::new(Vec::new()));
    let update_events = Rc::new(RefCell::new(Vec::new()));
    let generation = GenerationId::new(1).unwrap();
    let recovery = RuleSetRecovery::new(Rc::clone(&runtime_events), Rc::new(Cell::new(0)), None);
    let updater = ImportUpdater {
        events: Rc::clone(&update_events),
        pending_import: false,
        generation,
    };
    let mut application = WorkerApplication::new(
        recovery,
        TestNetwork,
        TestVerifier,
        TestClock(Rc::new(Cell::new(Duration::ZERO))),
        policy(),
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    )
    .with_updater(updater)
    .with_configuration(config_runtime, false);
    let request = request("manual-import", ControlMethod::SubscriptionImportApply)
        .with_params(ControlParams::import_document(
            digest,
            Some("a".repeat(64)),
            json!({"content":"vmess://fixture", "format_hint":"uri_list"}),
        ))
        .unwrap();

    let response = application.handle(request);

    assert!(response.ok(), "{response:?}");
    assert_eq!(response.generation(), Some(generation.get()));
    assert_eq!(response.result().unwrap()["completed"], true);
    assert_eq!(
        update_events.borrow().as_slice(),
        ["request_import", "prepare_import", "commit_import"]
    );
    assert_eq!(runtime_events.borrow().as_slice(), ["core_start"]);
}

#[test]
fn missing_current_generation_stays_available_in_fail_open_direct() {
    let clock = TestClock(Rc::new(Cell::new(Duration::ZERO)));
    let attempts = Rc::new(RefCell::new(Vec::new()));
    let recovery = TestRecovery {
        attempts: Rc::clone(&attempts),
        fail: false,
        fail_generation: false,
    };
    let mut application = WorkerApplication::new(
        recovery,
        TestNetwork,
        TestVerifier,
        clock,
        policy(),
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    )
    .with_private_dns_source(FixedPrivateDnsFacts(PrivateDnsMode::Strict));

    assert_eq!(application.next_wakeup_in(), Duration::ZERO);
    application.run_ready().unwrap();
    assert_eq!(application.snapshot().state, RuntimeState::FailOpenDirect);
    assert_eq!(attempts.borrow().as_slice(), ["recover"]);
    assert_eq!(application.next_wakeup_in(), Duration::from_secs(1));

    let status = application.handle(request("status", ControlMethod::StatusGet));
    let status = status.result().unwrap();
    assert_eq!(status["schema_version"], 1);
    assert_eq!(status["state"], "fail_open_direct");
    assert_eq!(status["runtime"]["process_health"], "stopped");
    assert_eq!(status["capture"]["active"], false);
    assert_eq!(status["capture"]["dns_guard"], "inactive");
    assert_eq!(status["dns_split"]["mode"], "strict");
    assert_eq!(status["dns_split"]["dns_split"], "degraded_private_dns");
    assert_eq!(status["operational"]["core_api"], "unavailable");
}

#[test]
fn core_version_check_is_read_only_and_notifies_once_per_latest_version() {
    let notifications = Rc::new(Cell::new(0));
    let mut application = WorkerApplication::new(
        TestRecovery::default(),
        TestNetwork,
        TestVerifier,
        TestClock(Rc::new(Cell::new(Duration::ZERO))),
        policy(),
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    )
    .with_core_version_source(CoreVersionChecker::new(
        FixedCoreRelease,
        CoreVersion::parse("1.13.15").unwrap(),
    ))
    .with_core_update_notifier(RecordingCoreUpdateNotifier(Rc::clone(&notifications)));
    application.run_ready().unwrap();
    let state_before = application.snapshot();

    let first = application.handle(request(
        "core-version-first",
        ControlMethod::CoreVersionCheck,
    ));
    assert!(first.ok());
    assert_eq!(first.result().unwrap()["status"]["current"], "1.13.15");
    assert_eq!(first.result().unwrap()["status"]["latest"], "1.13.16");
    assert_eq!(first.result().unwrap()["notification"], "posted");

    let second = application.handle(request(
        "core-version-second",
        ControlMethod::CoreVersionCheck,
    ));
    assert_eq!(second.result().unwrap()["notification"], "already_notified");
    assert_eq!(notifications.get(), 1);
    assert_eq!(application.snapshot(), state_before);

    let status = application.handle(request("status-after-version", ControlMethod::StatusGet));
    assert_eq!(status.result().unwrap()["core_update"]["latest"], "1.13.16");
    assert_eq!(
        status.result().unwrap()["core_update"]["availability"],
        "available"
    );
}

#[cfg(feature = "subscription-update")]
#[test]
fn due_core_version_check_runs_once_and_reschedules_without_touching_proxy_state() {
    let checks = Rc::new(Cell::new(0));
    let notifications = Rc::new(Cell::new(0));
    let schedule = PersistentCoreVersionSchedule::load(InMemoryScheduleStore::default()).unwrap();
    let mut application = WorkerApplication::new(
        TestRecovery::default(),
        TestNetwork,
        TestVerifier,
        TestClock(Rc::new(Cell::new(Duration::ZERO))),
        policy(),
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    )
    .with_core_version_source(CoreVersionChecker::new(
        CountingCoreRelease(Rc::clone(&checks)),
        CoreVersion::parse("1.13.15").unwrap(),
    ))
    .with_core_update_notifier(RecordingCoreUpdateNotifier(Rc::clone(&notifications)))
    .with_core_version_schedule(schedule);

    application.run_ready().unwrap();
    let state_after_first_check = application.snapshot();
    application.run_ready().unwrap();

    assert_eq!(checks.get(), 1);
    assert_eq!(notifications.get(), 1);
    assert_eq!(application.snapshot(), state_after_first_check);
    let status = application.handle(request("status-auto", ControlMethod::StatusGet));
    assert_eq!(
        status.result().unwrap()["core_update"]["availability"],
        "available"
    );
}

#[cfg(feature = "subscription-update")]
#[test]
fn core_version_schedule_failures_are_best_effort_and_never_stop_the_worker() {
    for fail_on_take in [true, false] {
        let attempts = Rc::new(Cell::new(0));
        let mut application = WorkerApplication::new(
            TestRecovery::default(),
            TestNetwork,
            TestVerifier,
            TestClock(Rc::new(Cell::new(Duration::ZERO))),
            policy(),
            PlanSlot::A,
            WorkerRuntimeLimits::default(),
        )
        .with_core_version_source(CoreVersionChecker::new(
            FixedCoreRelease,
            CoreVersion::parse("1.13.15").unwrap(),
        ))
        .with_core_version_schedule(FailingCoreSchedule {
            fail_on_take,
            attempts: Rc::clone(&attempts),
        });

        application.run_ready().unwrap();
        application.run_ready().unwrap();
        assert_eq!(attempts.get(), 1);
        assert_eq!(application.next_wakeup_in(), Duration::from_secs(60 * 60));
        assert_eq!(
            application.snapshot().state,
            nethop_core::RuntimeState::FailOpenDirect
        );
    }
}

#[cfg(feature = "subscription-update")]
#[test]
fn due_rule_set_update_commits_without_restart_when_proxy_is_inactive() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let schedule_results = Rc::new(RefCell::new(Vec::new()));
    let mut application = WorkerApplication::new(
        TestRecovery::default(),
        TestNetwork,
        TestVerifier,
        TestClock(Rc::new(Cell::new(Duration::ZERO))),
        policy(),
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    )
    .with_rule_set_update_source(TestRuleSetUpdater {
        events: Rc::clone(&events),
        unchanged: false,
        fail_commit: false,
    })
    .with_rule_set_schedule(TestRuleSetSchedule {
        due: true,
        results: Rc::clone(&schedule_results),
    });

    application.run_ready().unwrap();

    assert_eq!(
        events.borrow().as_slice(),
        ["prepare_ruleset", "publish_ruleset", "commit_ruleset"]
    );
    assert_eq!(schedule_results.borrow().as_slice(), [true]);
    assert_eq!(application.snapshot().state, RuntimeState::FailOpenDirect);
}

#[cfg(feature = "subscription-update")]
#[test]
fn failed_inactive_rule_set_commit_rolls_back_and_reschedules_failure() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let schedule_results = Rc::new(RefCell::new(Vec::new()));
    let mut application = WorkerApplication::new(
        TestRecovery::default(),
        TestNetwork,
        TestVerifier,
        TestClock(Rc::new(Cell::new(Duration::ZERO))),
        policy(),
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    )
    .with_rule_set_update_source(TestRuleSetUpdater {
        events: Rc::clone(&events),
        unchanged: false,
        fail_commit: true,
    })
    .with_rule_set_schedule(TestRuleSetSchedule {
        due: true,
        results: Rc::clone(&schedule_results),
    });

    application.run_ready().unwrap();

    assert_eq!(
        events.borrow().as_slice(),
        [
            "prepare_ruleset",
            "publish_ruleset",
            "commit_ruleset",
            "rollback_ruleset"
        ]
    );
    assert_eq!(schedule_results.borrow().as_slice(), [false]);
}

#[cfg(feature = "subscription-update")]
#[test]
fn manual_rule_set_update_uses_the_same_transaction_and_status_contract() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let schedule_results = Rc::new(RefCell::new(Vec::new()));
    let mut application = WorkerApplication::new(
        TestRecovery::default(),
        TestNetwork,
        TestVerifier,
        TestClock(Rc::new(Cell::new(Duration::ZERO))),
        policy(),
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    )
    .with_rule_set_update_source(TestRuleSetUpdater {
        events: Rc::clone(&events),
        unchanged: false,
        fail_commit: false,
    })
    .with_rule_set_schedule(TestRuleSetSchedule {
        due: false,
        results: Rc::clone(&schedule_results),
    });
    application.run_ready().unwrap();

    let accepted = application.handle(request("ruleset-update", ControlMethod::RuleSetUpdate));
    assert_eq!(accepted.result().unwrap()["accepted"], true);
    application.run_ready().unwrap();

    let status = application.handle(request("ruleset-status", ControlMethod::RuleSetStatus));
    let result = status.result().unwrap();
    assert_eq!(result["available"], true);
    assert_eq!(result["state"], "updated_inactive");
    assert!(result["last_attempt_wall_seconds"].is_i64());
    assert!(result["last_success_wall_seconds"].is_i64());
    assert_eq!(result["diagnostic_code"], serde_json::Value::Null);
    assert_eq!(schedule_results.borrow().as_slice(), [true]);
}

#[cfg(feature = "subscription-update")]
#[test]
fn running_proxy_commits_rule_sets_only_after_the_restarted_core_is_healthy() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let starts = Rc::new(Cell::new(0));
    let schedule_results = Rc::new(RefCell::new(Vec::new()));
    let recovery = RuleSetRecovery::new(Rc::clone(&events), Rc::clone(&starts), None);
    let mut application = WorkerApplication::new(
        recovery,
        TestNetwork,
        TestVerifier,
        TestClock(Rc::new(Cell::new(Duration::ZERO))),
        policy(),
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    )
    .with_rule_set_update_source(TestRuleSetUpdater {
        events: Rc::clone(&events),
        unchanged: false,
        fail_commit: false,
    })
    .with_rule_set_schedule(DelayedRuleSetSchedule {
        calls: 0,
        due_on_call: 2,
        results: Rc::clone(&schedule_results),
    });

    application.run_ready().unwrap();
    assert_eq!(application.snapshot().state, RuntimeState::RunningTproxy);
    application.run_ready().unwrap();

    assert_eq!(
        events.borrow().as_slice(),
        [
            "core_start",
            "prepare_ruleset",
            "core_stop",
            "publish_ruleset",
            "core_start",
            "commit_ruleset"
        ]
    );
    assert_eq!(starts.get(), 2);
    assert_eq!(schedule_results.borrow().as_slice(), [true]);
    assert_eq!(application.snapshot().state, RuntimeState::RunningTproxy);
}

#[cfg(feature = "subscription-update")]
#[test]
fn failed_rule_set_restart_restores_the_old_pair_and_restarts_previous_generation() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let starts = Rc::new(Cell::new(0));
    let schedule_results = Rc::new(RefCell::new(Vec::new()));
    let recovery = RuleSetRecovery::new(Rc::clone(&events), Rc::clone(&starts), Some(2));
    let mut application = WorkerApplication::new(
        recovery,
        TestNetwork,
        TestVerifier,
        TestClock(Rc::new(Cell::new(Duration::ZERO))),
        policy(),
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    )
    .with_rule_set_update_source(TestRuleSetUpdater {
        events: Rc::clone(&events),
        unchanged: false,
        fail_commit: false,
    })
    .with_rule_set_schedule(DelayedRuleSetSchedule {
        calls: 0,
        due_on_call: 2,
        results: Rc::clone(&schedule_results),
    });

    application.run_ready().unwrap();
    application.run_ready().unwrap();

    assert_eq!(
        events.borrow().as_slice(),
        [
            "core_start",
            "prepare_ruleset",
            "core_stop",
            "publish_ruleset",
            "core_start",
            "rollback_ruleset",
            "core_start"
        ]
    );
    assert_eq!(starts.get(), 3);
    assert_eq!(schedule_results.borrow().as_slice(), [false]);
    assert_eq!(application.snapshot().state, RuntimeState::RunningTproxy);
}

#[test]
fn typed_start_stop_and_probe_commands_are_consumed_on_the_worker_loop() {
    let clock = TestClock(Rc::new(Cell::new(Duration::ZERO)));
    let attempts = Rc::new(RefCell::new(Vec::new()));
    let recovery = TestRecovery {
        attempts: Rc::clone(&attempts),
        fail: false,
        fail_generation: false,
    };
    let mut application = WorkerApplication::new(
        recovery,
        TestNetwork,
        TestVerifier,
        clock,
        policy(),
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    );
    application.run_ready().unwrap();

    application.handle(request("probe", ControlMethod::CapabilityProbe));
    application.handle(request("start", ControlMethod::ServiceStart));
    application.run_ready().unwrap();
    assert_eq!(
        attempts.borrow().as_slice(),
        ["recover", "probe", "recover"]
    );

    application.handle(request("stop", ControlMethod::ServiceStop));
    application.run_ready().unwrap();
    assert_eq!(application.snapshot().state, RuntimeState::FailOpenDirect);
}

#[test]
fn transient_recovery_failure_uses_bounded_restart_deadline() {
    let now = Rc::new(Cell::new(Duration::ZERO));
    let clock = TestClock(Rc::clone(&now));
    let attempts = Rc::new(RefCell::new(Vec::new()));
    let recovery = TestRecovery {
        attempts: Rc::clone(&attempts),
        fail: true,
        fail_generation: false,
    };
    let mut application = WorkerApplication::new(
        recovery,
        TestNetwork,
        TestVerifier,
        clock,
        policy(),
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    );

    application.run_ready().unwrap();
    assert_eq!(application.snapshot().state, RuntimeState::Backoff);
    assert_eq!(application.next_wakeup_in(), Duration::from_secs(1));

    now.set(Duration::from_secs(1));
    application.run_ready().unwrap();
    assert_eq!(attempts.borrow().as_slice(), ["recover", "recover"]);
    assert_eq!(application.snapshot().state, RuntimeState::Backoff);
    assert_eq!(application.next_wakeup_in(), Duration::from_secs(2));
}

#[test]
#[cfg(feature = "subscription-update")]
fn failed_update_prepare_preserves_the_existing_runtime_state() {
    let clock = TestClock(Rc::new(Cell::new(Duration::ZERO)));
    let recoveries = Rc::new(RefCell::new(Vec::new()));
    let updates = Rc::new(RefCell::new(Vec::new()));
    let recovery = TestRecovery {
        attempts: Rc::clone(&recoveries),
        fail: false,
        fail_generation: false,
    };
    let updater = TestUpdater {
        events: Rc::clone(&updates),
        fail_prepare: true,
        needed: true,
        generation: nethop_core::GenerationId::new(2).unwrap(),
        current: true,
    };
    let mut application = WorkerApplication::new(
        recovery,
        TestNetwork,
        TestVerifier,
        clock,
        policy(),
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    )
    .with_updater(updater);
    application.run_ready().unwrap();
    application.handle(request("update", ControlMethod::SubscriptionUpdate));
    application.run_ready().unwrap();

    assert_eq!(updates.borrow().as_slice(), ["prepare"]);
    assert_eq!(recoveries.borrow().as_slice(), ["recover"]);
    assert_eq!(application.snapshot().state, RuntimeState::FailOpenDirect);
    assert_eq!(application.snapshot().last_update, UpdateStatus::Failed);
}

#[test]
#[cfg(feature = "subscription-update")]
fn failed_candidate_activation_discards_it_and_recovers_the_previous_current() {
    let clock = TestClock(Rc::new(Cell::new(Duration::ZERO)));
    let recoveries = Rc::new(RefCell::new(Vec::new()));
    let updates = Rc::new(RefCell::new(Vec::new()));
    let recovery = TestRecovery {
        attempts: Rc::clone(&recoveries),
        fail: false,
        fail_generation: true,
    };
    let updater = TestUpdater {
        events: Rc::clone(&updates),
        fail_prepare: false,
        needed: true,
        generation: nethop_core::GenerationId::new(2).unwrap(),
        current: true,
    };
    let mut application = WorkerApplication::new(
        recovery,
        TestNetwork,
        TestVerifier,
        clock,
        policy(),
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    )
    .with_updater(updater);
    application.run_ready().unwrap();
    application.handle(request("update", ControlMethod::SubscriptionUpdate));
    application.run_ready().unwrap();

    assert_eq!(updates.borrow().as_slice(), ["prepare", "discard"]);
    assert_eq!(
        recoveries.borrow().as_slice(),
        ["recover", "recover_generation", "recover"]
    );
    assert_eq!(application.snapshot().last_update, UpdateStatus::Failed);
}

#[test]
#[cfg(feature = "subscription-update")]
fn update_if_needed_skips_a_matching_generation_without_preparing() {
    let updates = Rc::new(RefCell::new(Vec::new()));
    let updater = TestUpdater {
        events: Rc::clone(&updates),
        fail_prepare: false,
        needed: false,
        generation: nethop_core::GenerationId::new(2).unwrap(),
        current: true,
    };
    let mut application = WorkerApplication::new(
        TestRecovery::default(),
        TestNetwork,
        TestVerifier,
        TestClock(Rc::new(Cell::new(Duration::ZERO))),
        policy(),
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    )
    .with_updater(updater);

    let response = application.handle(request_with_params(
        "if-needed",
        ControlMethod::SubscriptionUpdate,
        true,
        true,
    ));
    assert!(response.ok());
    assert_eq!(response.result().unwrap()["needed"], false);
    assert!(updates.borrow().is_empty());
}

#[test]
#[cfg(feature = "subscription-update")]
fn subscription_update_forwards_the_exact_daemon_source_id() {
    let selected = Rc::new(RefCell::new(Vec::new()));
    let updater = SelectingUpdater {
        selected: Rc::clone(&selected),
    };
    let mut application = WorkerApplication::new(
        TestRecovery::default(),
        TestNetwork,
        TestVerifier,
        TestClock(Rc::new(Cell::new(Duration::ZERO))),
        policy(),
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    )
    .with_updater(updater);
    let source_id = "src_01010101010101010101010101010101";
    let request = request("source-update", ControlMethod::SubscriptionUpdate)
        .with_params(ControlParams::subscription_update(
            false,
            false,
            Some(source_id.to_owned()),
        ))
        .unwrap();

    assert!(application.handle(request).ok());
    application.run_ready().unwrap();
    assert_eq!(selected.borrow().as_slice(), [Some(source_id.to_owned())]);
}

#[test]
#[cfg(feature = "subscription-update")]
fn superseded_prepared_update_is_discarded_before_core_or_network_activation() {
    let updates = Rc::new(RefCell::new(Vec::new()));
    let attempts = Rc::new(RefCell::new(Vec::new()));
    let recovery = TestRecovery {
        attempts: Rc::clone(&attempts),
        fail: false,
        fail_generation: false,
    };
    let updater = TestUpdater {
        events: Rc::clone(&updates),
        fail_prepare: false,
        needed: true,
        generation: nethop_core::GenerationId::new(2).unwrap(),
        current: false,
    };
    let mut application = WorkerApplication::new(
        recovery,
        TestNetwork,
        TestVerifier,
        TestClock(Rc::new(Cell::new(Duration::ZERO))),
        policy(),
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    )
    .with_updater(updater);

    application.handle(request(
        "update-superseded",
        ControlMethod::SubscriptionUpdate,
    ));
    application.run_ready().unwrap();

    assert_eq!(updates.borrow().as_slice(), ["prepare", "discard"]);
    assert!(!attempts.borrow().contains(&"recover_generation"));
    assert_eq!(application.snapshot().state, RuntimeState::FailOpenDirect);
}

#[cfg(feature = "subscription-update")]
struct FixedEntropy(u8);

#[cfg(feature = "subscription-update")]
impl SourceIdEntropy for FixedEntropy {
    fn fill(&mut self, output: &mut [u8; 16]) -> Result<(), SourceRegistryError> {
        output.fill(self.0);
        self.0 = self.0.saturating_add(1);
        Ok(())
    }
}

#[test]
#[cfg(feature = "subscription-update")]
fn enabling_a_configured_service_without_a_generation_attempts_initial_update() {
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    fs::write(
        &config_path,
        "schema_version = 3\n[service]\nenabled = false\n[subscriptions]\n[[subscriptions.sources]]\nname = \"Primary\"\nurl = \"https://one.example/sub\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
    let store = ConfigStore::new(&config_path).unwrap();
    let snapshot = store.load().unwrap();
    let registry = SourceRegistry::new(directory.path().join("source-registry.v1.json")).unwrap();
    let sources = registry.reconcile(&snapshot, &mut FixedEntropy(1)).unwrap();
    let config_runtime = ConfigRuntime::new(store, registry, snapshot, &sources);
    let updates = Rc::new(RefCell::new(Vec::new()));
    let updater = TestUpdater {
        events: Rc::clone(&updates),
        fail_prepare: true,
        needed: true,
        generation: nethop_core::GenerationId::new(2).unwrap(),
        current: true,
    };
    let mut application = WorkerApplication::new(
        TestRecovery::default(),
        TestNetwork,
        TestVerifier,
        TestClock(Rc::new(Cell::new(Duration::ZERO))),
        policy(),
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    )
    .with_updater(updater)
    .with_configuration(config_runtime, false);

    let response = application.handle(request_with_params(
        "enable",
        ControlMethod::ServiceStart,
        true,
        false,
    ));
    assert!(response.ok());
    assert_eq!(updates.borrow().as_slice(), ["prepare"]);
    assert!(
        fs::read_to_string(config_path)
            .unwrap()
            .contains("enabled = true")
    );
}

#[test]
#[cfg(feature = "subscription-update")]
fn source_change_prepare_failure_does_not_stop_the_active_generation() {
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    fs::write(
        &config_path,
        "schema_version = 3\n[service]\nenabled = true\n[subscriptions]\n[[subscriptions.sources]]\nname = \"Primary\"\nurl = \"https://old.example/sub\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
    let store = ConfigStore::new(&config_path).unwrap();
    let snapshot = store.load().unwrap();
    let digest = snapshot.digest().to_owned();
    let registry = SourceRegistry::new(directory.path().join("source-registry.v1.json")).unwrap();
    let sources = registry.reconcile(&snapshot, &mut FixedEntropy(1)).unwrap();
    let config_runtime = ConfigRuntime::new(store, registry, snapshot, &sources);
    let runtime_events = Rc::new(RefCell::new(Vec::new()));
    let starts = Rc::new(Cell::new(0));
    let update_events = Rc::new(RefCell::new(Vec::new()));
    let recovery = RuleSetRecovery::new(Rc::clone(&runtime_events), starts, None);
    let updater = TestUpdater {
        events: Rc::clone(&update_events),
        fail_prepare: true,
        needed: false,
        generation: GenerationId::new(2).unwrap(),
        current: true,
    };
    let mut application = WorkerApplication::new(
        recovery,
        TestNetwork,
        TestVerifier,
        TestClock(Rc::new(Cell::new(Duration::ZERO))),
        policy(),
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    )
    .with_updater(updater)
    .with_configuration(config_runtime, true);
    application.run_ready().unwrap();
    assert_eq!(
        application.snapshot().generation,
        Some(GenerationId::new(1).unwrap())
    );
    assert_eq!(runtime_events.borrow().as_slice(), ["core_start"]);

    let request = ControlRequest::new(
        RequestId::new("replace-source-url").unwrap(),
        ControlMethod::ConfigApply,
    )
    .with_params(ControlParams::config_document(
        digest,
        json!({
            "schema_version": 3,
            "service": {"enabled": true},
            "subscriptions": {
                "sources": [{"name": "Primary", "url": "https://new.example/sub"}]
            }
        }),
    ))
    .unwrap();
    let response = application.handle(request);

    assert!(response.ok(), "{response:?}");
    assert_eq!(update_events.borrow().as_slice(), ["prepare"]);
    assert_eq!(runtime_events.borrow().as_slice(), ["core_start"]);
    assert_eq!(application.snapshot().state, RuntimeState::RunningTproxy);
    assert_eq!(
        application.snapshot().generation,
        Some(GenerationId::new(1).unwrap())
    );
    assert_eq!(application.snapshot().last_update, UpdateStatus::Failed);
}

#[test]
#[cfg(feature = "subscription-update")]
fn non_source_generation_change_requests_a_cached_rebuild() {
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    fs::write(
        &config_path,
        "schema_version = 3\n[service]\nenabled = true\n[subscriptions]\n[[subscriptions.sources]]\nname = \"Primary\"\nurl = \"https://example.test/sub\"\n[proxy]\noutbound_mode = \"rule\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
    let store = ConfigStore::new(&config_path).unwrap();
    let snapshot = store.load().unwrap();
    let digest = snapshot.digest().to_owned();
    let registry = SourceRegistry::new(directory.path().join("source-registry.v1.json")).unwrap();
    let sources = registry.reconcile(&snapshot, &mut FixedEntropy(1)).unwrap();
    let config_runtime = ConfigRuntime::new(store, registry, snapshot, &sources);
    let update_events = Rc::new(RefCell::new(Vec::new()));
    let updater = TestUpdater {
        events: Rc::clone(&update_events),
        fail_prepare: false,
        needed: false,
        generation: GenerationId::new(2).unwrap(),
        current: true,
    };
    let mut application = WorkerApplication::new(
        TestRecovery::default(),
        TestNetwork,
        TestVerifier,
        TestClock(Rc::new(Cell::new(Duration::ZERO))),
        policy(),
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    )
    .with_updater(updater)
    .with_configuration(config_runtime, false);
    application.run_ready().unwrap();
    update_events.borrow_mut().clear();
    let previous_update_status = application.snapshot().last_update;
    let request = ControlRequest::new(
        RequestId::new("proxy-mode-apply").unwrap(),
        ControlMethod::ConfigApply,
    )
    .with_params(ControlParams::config_document(
        digest,
        json!({
            "schema_version": 3,
            "service": {"enabled": true},
            "subscriptions": {
                "sources": [{"name": "Primary", "url": "https://example.test/sub"}]
            },
            "proxy": {"outbound_mode": "global"}
        }),
    ))
    .unwrap();

    let response = application.handle(request);

    assert!(response.ok(), "{response:?}");
    assert_eq!(
        update_events.borrow().as_slice(),
        ["cached_rebuild", "prepare", "discard"]
    );
    assert_eq!(application.snapshot().last_update, previous_update_status);
}

#[test]
#[cfg(feature = "subscription-update")]
fn subscription_select_uses_the_real_journaled_worker_transaction() {
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    let directory = tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let config_path = root.join("nethop.toml");
    fs::write(
        &config_path,
        "schema_version = 3\n[service]\nenabled = false\n[subscriptions]\nmode = \"single\"\n[[subscriptions.sources]]\nname = \"Primary\"\nenabled = true\nurl = \"https://one.example/sub\"\n[[subscriptions.sources]]\nname = \"Backup\"\nenabled = false\nurl = \"https://two.example/sub\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
    let store = ConfigStore::new(&config_path).unwrap();
    let snapshot = store.load().unwrap();
    let digest = snapshot.digest().to_owned();
    let registry = SourceRegistry::new(root.join("source-registry.v1.json")).unwrap();
    let sources = registry.reconcile(&snapshot, &mut FixedEntropy(1)).unwrap();
    let backup_id = sources.sources()[1].id().as_str().to_owned();
    let config_runtime = ConfigRuntime::new(store, registry, snapshot, &sources);
    let journal = CommitJournalStore::new(&root).unwrap();
    let journal_path = journal.path();
    let mut application = WorkerApplication::new(
        TestRecovery::default(),
        TestNetwork,
        TestVerifier,
        TestClock(Rc::new(Cell::new(Duration::ZERO))),
        policy(),
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    )
    .with_configuration(config_runtime, false)
    .with_subscription_transactions(journal, MutationCoordinator::default());

    let request = ControlRequest::new(
        RequestId::new("select-backup").unwrap(),
        ControlMethod::SubscriptionSelect,
    )
    .with_params(ControlParams::subscription_select(
        digest,
        backup_id.clone(),
    ))
    .unwrap();
    let response = application.handle(request);

    assert!(response.ok(), "{response:?}");
    assert!(!journal_path.exists());
    let canonical = fs::read_to_string(config_path).unwrap();
    assert!(canonical.contains("name = \"Primary\"\nenabled = false"));
    assert!(canonical.contains("name = \"Backup\"\nenabled = true"));
    assert_eq!(
        response.result().unwrap()["active_set"]["active_source_ids"],
        json!([backup_id])
    );
}

#[test]
#[cfg(feature = "subscription-update")]
fn stale_manager_apply_returns_current_observed_digest_without_sensitive_values() {
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    fs::write(
        &config_path,
        "schema_version = 3\n[service]\nenabled = false\n[subscriptions]\n[[subscriptions.sources]]\nname = \"Primary\"\nurl = \"https://secret.example/account-token\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
    let store = ConfigStore::new(&config_path).unwrap();
    let snapshot = store.load().unwrap();
    let observed = snapshot.digest().to_owned();
    let registry = SourceRegistry::new(directory.path().join("source-registry.v1.json")).unwrap();
    let sources = registry.reconcile(&snapshot, &mut FixedEntropy(1)).unwrap();
    let config_runtime = ConfigRuntime::new(store, registry, snapshot, &sources);
    let mut application = WorkerApplication::new(
        TestRecovery::default(),
        TestNetwork,
        TestVerifier,
        TestClock(Rc::new(Cell::new(Duration::ZERO))),
        policy(),
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    )
    .with_configuration(config_runtime, false);

    let request = ControlRequest::new(RequestId::new("stale-apply").unwrap(), ControlMethod::ConfigApply)
        .with_params(ControlParams::config_document(
            "0".repeat(64),
            json!({
                "schema_version": 3,
                "service": {"enabled": true},
                "subscriptions": {"sources": [{"name": "Primary", "url": "https://new-secret.example/token"}]}
            }),
        ))
        .unwrap();
    let response = application.handle(request);

    assert!(!response.ok());
    let error = response.error().unwrap();
    assert_eq!(error.code().as_str(), "NH-CONFIG-CONFLICT");
    assert_eq!(error.details().unwrap()["observed_config_digest"], observed);
    assert_eq!(
        error.details().unwrap()["changed_field_ids"],
        json!(["service.enabled", "subscriptions.sources"])
    );
    assert_eq!(error.details().unwrap()["requires_reload"], true);
    let encoded = serde_json::to_string(&response).unwrap();
    assert!(!encoded.contains("account-token"));
    assert!(!encoded.contains("new-secret"));
}

#[test]
#[cfg(feature = "subscription-update")]
fn manager_read_contract_is_versioned_redacted_and_schema_driven() {
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    fs::write(
        &config_path,
        "schema_version = 3\n[service]\nenabled = false\n[subscriptions]\n[[subscriptions.sources]]\nname = \"Primary\"\nurl = \"https://secret.example/account-token\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
    let store = ConfigStore::new(&config_path).unwrap();
    let snapshot = store.load().unwrap();
    let registry = SourceRegistry::new(directory.path().join("source-registry.v1.json")).unwrap();
    let sources = registry.reconcile(&snapshot, &mut FixedEntropy(1)).unwrap();
    let config_runtime = ConfigRuntime::new(store, registry, snapshot, &sources);
    let source_status = SourceStatusStore::open(directory.path().join("nethop.db")).unwrap();
    let mut application = WorkerApplication::new(
        TestRecovery::default(),
        TestNetwork,
        TestVerifier,
        TestClock(Rc::new(Cell::new(Duration::from_secs(2)))),
        policy(),
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    )
    .with_configuration(config_runtime, false)
    .with_source_status_store(source_status);

    let hello = request("hello", ControlMethod::ProtocolHello)
        .with_params(ControlParams::hello("manager-alpha".into(), 5, 5))
        .unwrap();
    let hello = application.handle(hello);
    assert!(hello.ok());
    assert_eq!(hello.result().unwrap()["compatible"], true);
    assert_eq!(hello.result().unwrap()["daemon_protocol_min"], 5);
    assert!(
        hello.result().unwrap()["supported_features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == "multi_source")
    );
    assert!(
        hello.result().unwrap()["supported_features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == "node_benchmark_fast_selection_v1")
    );

    let incompatible = request("hello-new", ControlMethod::ProtocolHello)
        .with_params(ControlParams::hello("manager-old".into(), 1, 1))
        .unwrap();
    assert_eq!(
        application.handle(incompatible).result().unwrap()["compatible"],
        false
    );

    let config = application.handle(request("get", ControlMethod::ConfigGet));
    let config = config.result().unwrap();
    assert_eq!(config["watcher_health"], "not_configured");
    assert_eq!(
        config["document"]["subscriptions"]["sources"][0]["url"],
        json!(null)
    );
    assert_eq!(
        config["document"]["subscriptions"]["sources"][0]["url_configured"],
        true
    );
    let source_id = config["document"]["subscriptions"]["sources"][0]["source_id"]
        .as_str()
        .unwrap();
    assert!(source_id.starts_with("src_"));
    assert_eq!(config["source_status"][0]["source_id"], source_id);
    assert_eq!(config["source_status"][0]["health"], "never");
    assert!(
        !serde_json::to_string(config)
            .unwrap()
            .contains("account-token")
    );

    let schema = application.handle(request("schema", ControlMethod::ConfigSchema));
    let fields = schema.result().unwrap()["fields"].as_array().unwrap();
    assert!(
        !fields
            .iter()
            .any(|field| field["field_id"] == "subscriptions.sources[].source_id")
    );
    assert!(
        !fields
            .iter()
            .any(|field| field["field_id"] == "proxy.selector_mode")
    );
    for field_id in [
        "routing.force_proxy_domains",
        "routing.bypass_domains",
        "routing.block_domains",
    ] {
        let field = fields
            .iter()
            .find(|field| field["field_id"] == field_id)
            .unwrap_or_else(|| panic!("missing domain routing schema field: {field_id}"));
        assert_eq!(field["value_type"], "domain_suffix_array");
        assert_eq!(field["max_items"], 512);
        assert_eq!(field["apply_impact"], "generation_activation");
    }
    let tun_stack = fields
        .iter()
        .find(|field| field["field_id"] == "network.tun_stack")
        .expect("missing TUN stack schema field");
    assert_eq!(tun_stack["default"], "gvisor");
    assert_eq!(tun_stack["enum_values"], json!(["system", "gvisor"]));
    assert_eq!(tun_stack["apply_impact"], "generation_activation");
    assert_eq!(tun_stack["capability_key"], "capture.tun");
    for field in fields {
        for key in [
            "field_id",
            "path",
            "value_type",
            "default",
            "title_key",
            "description_key",
            "group",
            "order",
            "advanced",
            "experimental",
            "deprecated",
            "sensitive",
            "read_only",
            "write_only",
            "apply_impact",
            "risk_level",
            "capability_key",
            "stage",
        ] {
            assert!(field.get(key).is_some(), "missing schema metadata: {key}");
        }
    }

    let capabilities = application.handle(request("caps", ControlMethod::CapabilityGet));
    let capabilities = capabilities.result().unwrap();
    assert!(capabilities["report_digest"].as_str().is_some());
    let items = capabilities["items"].as_array().unwrap();
    assert!(
        items
            .iter()
            .any(|item| { item["key"] == "network.interfaces" && item["status"] == "supported" })
    );
    assert!(items.iter().all(|item| {
        item["reason_code"].is_string()
            && item["requirements"].is_object()
            && item["evidence"].is_object()
            && item["apply_effect"].is_string()
    }));

    let exported = application.handle(request("export", ControlMethod::ConfigExport));
    let exported = exported.result().unwrap();
    assert_eq!(exported["format"], "nethop-config-backup-v1");
    assert_eq!(exported["config_digest"], config["active_config_digest"]);
    assert_eq!(
        exported["document"]["subscriptions"]["sources"][0]["url"],
        "https://secret.example/account-token"
    );
    assert!(
        exported["document"]["subscriptions"]["sources"][0]
            .get("source_id")
            .is_none()
    );
    assert!(
        serde_json::to_string(exported)
            .unwrap()
            .contains("account-token")
    );
}

#[test]
#[cfg(feature = "subscription-update")]
fn dry_run_prepares_and_discards_a_checked_candidate_without_capture() {
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    fs::write(
        &config_path,
        "schema_version = 3\n[service]\nenabled = true\n[subscriptions]\n[[subscriptions.sources]]\nname = \"Primary\"\nurl = \"https://one.example/sub\"\n[advanced]\ndry_run = true\n",
    )
    .unwrap();
    #[cfg(unix)]
    fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
    let store = ConfigStore::new(&config_path).unwrap();
    let snapshot = store.load().unwrap();
    let registry = SourceRegistry::new(directory.path().join("source-registry.v1.json")).unwrap();
    let sources = registry.reconcile(&snapshot, &mut FixedEntropy(1)).unwrap();
    let config_runtime = ConfigRuntime::new(store, registry, snapshot, &sources);
    let updates = Rc::new(RefCell::new(Vec::new()));
    let updater = TestUpdater {
        events: Rc::clone(&updates),
        fail_prepare: false,
        needed: true,
        generation: nethop_core::GenerationId::new(2).unwrap(),
        current: true,
    };
    let mut application = WorkerApplication::new(
        TestRecovery::default(),
        TestNetwork,
        TestVerifier,
        TestClock(Rc::new(Cell::new(Duration::ZERO))),
        policy(),
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    )
    .with_updater(updater)
    .with_configuration(config_runtime, false);

    application.run_ready().unwrap();
    assert_eq!(updates.borrow().as_slice(), ["prepare", "discard"]);
    assert_eq!(application.snapshot().state, RuntimeState::FailOpenDirect);
    assert_eq!(application.snapshot().generation, None);
    assert_eq!(application.snapshot().last_update, UpdateStatus::Succeeded);
}

#[test]
#[cfg(feature = "subscription-update")]
fn wifi_scene_can_transiently_disable_but_never_override_the_persistent_master_switch() {
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    fs::write(
        &config_path,
        "schema_version = 3\n[service]\nenabled = true\n[subscriptions]\n[[subscriptions.sources]]\nname = \"Primary\"\nurl = \"https://one.example/sub\"\n[network.wifi_scenes]\nenabled = true\nprobe_interval_seconds = 30\n[[network.wifi_scenes.rules]]\nid = \"trusted-home\"\nssid = \"Trusted Home\"\nbssid = \"aa:bb:cc:dd:ee:ff\"\naction = \"disable_proxy\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
    let store = ConfigStore::new(&config_path).unwrap();
    let snapshot = store.load().unwrap();
    let registry = SourceRegistry::new(directory.path().join("source-registry.v1.json")).unwrap();
    let sources = registry.reconcile(&snapshot, &mut FixedEntropy(1)).unwrap();
    let config_runtime = ConfigRuntime::new(store, registry, snapshot, &sources);
    let attempts = Rc::new(RefCell::new(Vec::new()));
    let updates = Rc::new(RefCell::new(Vec::new()));
    let recovery = TestRecovery {
        attempts: Rc::clone(&attempts),
        ..TestRecovery::default()
    };
    let updater = TestUpdater {
        events: Rc::clone(&updates),
        fail_prepare: false,
        needed: false,
        generation: nethop_core::GenerationId::new(2).unwrap(),
        current: true,
    };
    let mut application = WorkerApplication::new(
        recovery,
        TestNetwork,
        TestVerifier,
        TestClock(Rc::new(Cell::new(Duration::ZERO))),
        policy(),
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    )
    .with_updater(updater)
    .with_wifi_facts_source(FixedWifiFacts)
    .with_configuration(config_runtime, true);

    application.run_ready().unwrap();
    assert!(attempts.borrow().is_empty());
    assert!(updates.borrow().is_empty());
    assert_eq!(application.snapshot().state, RuntimeState::FailOpenDirect);
    assert!(
        fs::read_to_string(config_path)
            .unwrap()
            .contains("enabled = true")
    );
    let config = application.handle(request("wifi-config", ControlMethod::ConfigGet));
    let encoded = serde_json::to_string(config.result().unwrap()).unwrap();
    assert!(!encoded.contains("Trusted Home"));
    assert!(!encoded.contains("aa:bb:cc"));
    assert!(encoded.contains("ssid_configured"));
}

#[test]
#[cfg(feature = "subscription-update")]
fn advanced_apply_refreshes_runtime_probe_health_and_reconcile_policy() {
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    fs::write(
        &config_path,
        "schema_version = 3\n[service]\nenabled = false\n[subscriptions]\n[[subscriptions.sources]]\nname = \"Primary\"\nurl = \"\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
    let store = ConfigStore::new(&config_path).unwrap();
    let snapshot = store.load().unwrap();
    let digest = snapshot.digest().to_owned();
    let registry = SourceRegistry::new(directory.path().join("source-registry.v1.json")).unwrap();
    let sources = registry.reconcile(&snapshot, &mut FixedEntropy(1)).unwrap();
    let config_runtime = ConfigRuntime::new(store, registry, snapshot, &sources);
    let recovery_updates = Rc::new(RefCell::new(Vec::new()));
    let verifier_updates = Rc::new(RefCell::new(Vec::new()));
    let recovery = PolicyRecordingRecovery {
        inner: TestRecovery::default(),
        updates: Rc::clone(&recovery_updates),
    };
    let verifier = PolicyRecordingVerifier(Rc::clone(&verifier_updates));
    let mut application = WorkerApplication::new(
        recovery,
        TestNetwork,
        verifier,
        TestClock(Rc::new(Cell::new(Duration::ZERO))),
        policy(),
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    )
    .with_configuration(config_runtime, false);
    let request = ControlRequest::new(
        RequestId::new("advanced-apply").unwrap(),
        ControlMethod::ConfigApply,
    )
    .with_params(ControlParams::config_document(
        digest,
        json!({
            "schema_version": 3,
            "service": {"enabled": false},
            "subscriptions": {"sources": [{"name": "Primary", "url": ""}]},
            "advanced": {
                "inbound_port": 7900,
                "health_timeout_seconds": 5,
                "reconcile_interval_seconds": 120
            }
        }),
    ))
    .unwrap();

    let response = application.handle(request);
    assert!(response.ok());
    assert_eq!(recovery_updates.borrow().as_slice(), [(7900, 5, 3)]);
    assert_eq!(verifier_updates.borrow().as_slice(), [7900]);
}
