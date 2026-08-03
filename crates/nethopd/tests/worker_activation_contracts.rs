use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    path::Path,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use nethop_android::{
    AllocationCapability, CapabilityError, CapabilityReport, CapabilityStatus, ExecutionError,
    FamilyCapability, IpFamily, NetfilterBackend, NetworkAction, NetworkChange, NetworkEvent,
    NetworkHealthError, NetworkHealthVerifier, NetworkPlan, PlanSlot, ResourceCandidate,
};
use nethop_core::{
    Candidate, CaptureMode, CapturePolicy, GenerationId, GenerationStore, ManagedConfig,
    RuntimeState, TerminalOutbound,
};
use nethopd::{
    ActiveRuntime, CandidateActivator, CandidateChecker, CandidateProcess, CapabilitySource,
    CoreLauncher, CurrentGenerationActivator, DataPlaneHealthError, DataPlaneHealthProbe,
    EventReconcileGate, HealthProbe, HealthProbeError, NetworkController,
    NetworkDataPlaneHealthProbe, ProcessError, ProcessIdentity, RestartBudget, RestartDecision,
    RunnerError, RuntimeFailureCode, RuntimeTick, SafetyAuditError, SafetyAuditor,
    WorkerActivationDiagnosticCode, WorkerActivator, WorkerLoopDriver, WorkerLoopSignal,
    WorkerRecoveryError, WorkerRunExit, WorkerRuntime, WorkerRuntimeLimits,
};
use serde_json::json;

const PORT: u16 = 7893;

fn candidate(id: u64) -> Candidate {
    Candidate::new(
        GenerationId::new(id).unwrap(),
        ManagedConfig::from_outbounds(vec![
            TerminalOutbound::new(
                "node",
                "trojan",
                BTreeMap::from([
                    ("server".into(), json!("example.com")),
                    ("server_port".into(), json!(443)),
                    ("password".into(), json!("fixture-only")),
                ]),
            )
            .unwrap(),
        ])
        .unwrap(),
    )
}

fn store_with_active_generation() -> (tempfile::TempDir, GenerationStore) {
    let directory = tempfile::tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    store.publish(&candidate(1), |_| Ok(())).unwrap();
    (directory, store)
}

fn family(family: IpFamily, tproxy: CapabilityStatus) -> FamilyCapability {
    FamilyCapability::new(
        family,
        tproxy,
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

fn report(ipv4_tproxy: CapabilityStatus) -> CapabilityReport {
    let allocation = ResourceCandidate::new(0x100, 0xff00, 100, 10_000).unwrap();
    CapabilityReport::new(
        CapabilityStatus::Supported,
        "arm64-v8a",
        CapabilityStatus::Supported,
        true,
        NetfilterBackend::Legacy,
        family(IpFamily::Ipv4, ipv4_tproxy),
        family(IpFamily::Ipv6, CapabilityStatus::Supported),
        CapabilityStatus::Supported,
        CapabilityStatus::Supported,
        PORT,
        CapabilityStatus::Supported,
        vec![AllocationCapability::new(
            allocation,
            CapabilityStatus::Supported,
        )],
    )
    .unwrap()
}

fn supported_report() -> CapabilityReport {
    report(CapabilityStatus::Supported)
}

fn policy() -> CapturePolicy {
    CapturePolicy::new(
        CaptureMode::Tproxy,
        true,
        Some(PORT),
        Some(0x200),
        vec![10_001],
        vec![],
    )
    .unwrap()
}

#[derive(Debug, Clone, Copy)]
struct FakeChecker;

impl CandidateChecker for FakeChecker {
    fn check(&self, _config_path: &Path) -> Result<(), RunnerError> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct FakeAuditor;

impl SafetyAuditor for FakeAuditor {
    fn audit(&self, _candidate: &Candidate, _config_path: &Path) -> Result<(), SafetyAuditError> {
        Ok(())
    }
}

#[derive(Debug)]
struct FakeProcess {
    events: Rc<RefCell<Vec<&'static str>>>,
}

impl CandidateProcess for FakeProcess {
    fn identity(&self) -> ProcessIdentity {
        ProcessIdentity::new(42, Some(7)).unwrap()
    }

    fn is_running(&mut self) -> Result<bool, ProcessError> {
        Ok(true)
    }

    fn stop(self) -> Result<(), ProcessError> {
        self.events.borrow_mut().push("core_stop");
        Ok(())
    }
}

#[derive(Debug)]
struct ExitedProcess;

impl CandidateProcess for ExitedProcess {
    fn identity(&self) -> ProcessIdentity {
        ProcessIdentity::new(43, Some(8)).unwrap()
    }

    fn is_running(&mut self) -> Result<bool, ProcessError> {
        Ok(false)
    }

    fn stop(self) -> Result<(), ProcessError> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct FakeLauncher {
    events: Rc<RefCell<Vec<&'static str>>>,
    invalidate_manifest: bool,
}

impl CoreLauncher for FakeLauncher {
    type Process = FakeProcess;

    fn start(&self, config_path: &Path) -> Result<Self::Process, ProcessError> {
        self.events.borrow_mut().push("core_start");
        if self.invalidate_manifest {
            std::fs::remove_file(config_path.with_file_name("manifest.json")).unwrap();
        }
        Ok(FakeProcess {
            events: Rc::clone(&self.events),
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct FakeCoreHealth;

impl HealthProbe<FakeProcess> for FakeCoreHealth {
    fn wait_healthy(&self, _process: &mut FakeProcess) -> Result<(), HealthProbeError> {
        Ok(())
    }
}

#[derive(Debug)]
struct FakeCapabilitySource {
    report: Option<CapabilityReport>,
}

impl CapabilitySource for FakeCapabilitySource {
    fn probe(&mut self) -> Result<CapabilityReport, CapabilityError> {
        self.report
            .take()
            .ok_or(CapabilityError::CommandSpawnFailed)
    }
}

#[derive(Debug)]
struct FakeNetworkExecutor {
    events: Rc<RefCell<Vec<&'static str>>>,
    fail_apply: bool,
    fail_rollback: bool,
}

impl NetworkController for FakeNetworkExecutor {
    type Receipt = usize;

    fn apply(&mut self, plan: &NetworkPlan) -> Result<Self::Receipt, ExecutionError> {
        self.events.borrow_mut().push("network_apply");
        if self.fail_apply {
            Err(ExecutionError::ApplyFailed { step: 0 })
        } else {
            Ok(plan.operation_kinds().len())
        }
    }

    fn rollback(
        &mut self,
        _plan: &NetworkPlan,
        receipt: &mut Self::Receipt,
    ) -> Result<(), ExecutionError> {
        self.events.borrow_mut().push("network_rollback");
        if self.fail_rollback {
            return Err(ExecutionError::RollbackFailed { step: 0 });
        }
        *receipt = 0;
        Ok(())
    }
}

#[derive(Debug)]
struct FakeDataPlaneHealth<'a> {
    store: &'a GenerationStore,
    events: Rc<RefCell<Vec<&'static str>>>,
    fail: bool,
}

impl DataPlaneHealthProbe<FakeProcess> for FakeDataPlaneHealth<'_> {
    fn wait_healthy(
        &mut self,
        _process: &mut FakeProcess,
        _plan: &NetworkPlan,
        _capabilities: &CapabilityReport,
    ) -> Result<(), DataPlaneHealthError> {
        assert_eq!(
            self.store.current_generation().unwrap(),
            Some(GenerationId::new(1).unwrap())
        );
        self.events.borrow_mut().push("data_health");
        if self.fail {
            Err(DataPlaneHealthError::NetworkUnhealthy)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug)]
struct FakeNetworkHealthVerifier {
    fail: bool,
}

impl NetworkHealthVerifier for FakeNetworkHealthVerifier {
    fn verify(&mut self, _plan: &NetworkPlan) -> Result<(), NetworkHealthError> {
        if self.fail {
            Err(NetworkHealthError::OwnerMarkerMissing)
        } else {
            Ok(())
        }
    }
}

fn full_network_plan() -> NetworkPlan {
    nethop_android::NetworkPlanner
        .build_tproxy(
            GenerationId::new(2).unwrap(),
            PlanSlot::A,
            &policy(),
            &supported_report(),
        )
        .unwrap()
}

#[test]
fn production_data_plane_adapter_requires_a_live_core_and_verified_network_plan() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut process = FakeProcess { events };
    let mut probe = NetworkDataPlaneHealthProbe::new(FakeNetworkHealthVerifier { fail: false });
    probe
        .wait_healthy(&mut process, &full_network_plan(), &supported_report())
        .unwrap();

    let mut probe = NetworkDataPlaneHealthProbe::new(FakeNetworkHealthVerifier { fail: true });
    assert_eq!(
        probe
            .wait_healthy(&mut process, &full_network_plan(), &supported_report())
            .unwrap_err(),
        DataPlaneHealthError::NetworkUnhealthy
    );

    let mut process = ExitedProcess;
    let mut probe = NetworkDataPlaneHealthProbe::new(FakeNetworkHealthVerifier { fail: false });
    assert_eq!(
        probe
            .wait_healthy(&mut process, &full_network_plan(), &supported_report())
            .unwrap_err(),
        DataPlaneHealthError::CoreExited
    );
}

#[test]
fn recovery_without_current_generation_is_an_idle_direct_state() {
    let directory = tempfile::tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    let events = Rc::new(RefCell::new(Vec::new()));
    let launcher = FakeLauncher {
        events: Rc::clone(&events),
        invalidate_manifest: false,
    };
    let core_health = FakeCoreHealth;
    let mut capabilities = FakeCapabilitySource { report: None };
    let mut network = FakeNetworkExecutor {
        events: Rc::clone(&events),
        fail_apply: false,
        fail_rollback: false,
    };
    let mut data_health = FakeDataPlaneHealth {
        store: &store,
        events: Rc::clone(&events),
        fail: false,
    };

    let recovered = CurrentGenerationActivator::new(
        &store,
        &launcher,
        &core_health,
        &mut capabilities,
        &mut network,
        &mut data_health,
    )
    .recover(&policy(), PlanSlot::A)
    .unwrap();
    assert!(recovered.is_none());
    assert!(events.borrow().is_empty());
}

#[test]
fn recovery_activates_verified_current_without_creating_a_generation() {
    let (_directory, store) = store_with_active_generation();
    let events = Rc::new(RefCell::new(Vec::new()));
    let launcher = FakeLauncher {
        events: Rc::clone(&events),
        invalidate_manifest: false,
    };
    let core_health = FakeCoreHealth;
    let mut capabilities = FakeCapabilitySource {
        report: Some(supported_report()),
    };
    let mut network = FakeNetworkExecutor {
        events: Rc::clone(&events),
        fail_apply: false,
        fail_rollback: false,
    };
    let mut data_health = FakeDataPlaneHealth {
        store: &store,
        events: Rc::clone(&events),
        fail: false,
    };

    let active = CurrentGenerationActivator::new(
        &store,
        &launcher,
        &core_health,
        &mut capabilities,
        &mut network,
        &mut data_health,
    )
    .recover(&policy(), PlanSlot::A)
    .unwrap()
    .unwrap();
    assert_eq!(active.generation(), GenerationId::new(1).unwrap());
    assert_eq!(
        events.borrow().as_slice(),
        ["core_start", "network_apply", "data_health"]
    );
    assert_eq!(
        store.current_generation().unwrap(),
        Some(active.generation())
    );
    assert_eq!(
        std::fs::read_dir(store.generations_root()).unwrap().count(),
        1
    );
    active.stop(&mut network).unwrap();
}

#[test]
fn recovery_data_plane_failure_rolls_back_before_stopping_core() {
    let (_directory, store) = store_with_active_generation();
    let events = Rc::new(RefCell::new(Vec::new()));
    let launcher = FakeLauncher {
        events: Rc::clone(&events),
        invalidate_manifest: false,
    };
    let core_health = FakeCoreHealth;
    let mut capabilities = FakeCapabilitySource {
        report: Some(supported_report()),
    };
    let mut network = FakeNetworkExecutor {
        events: Rc::clone(&events),
        fail_apply: false,
        fail_rollback: false,
    };
    let mut data_health = FakeDataPlaneHealth {
        store: &store,
        events: Rc::clone(&events),
        fail: true,
    };

    let error = CurrentGenerationActivator::new(
        &store,
        &launcher,
        &core_health,
        &mut capabilities,
        &mut network,
        &mut data_health,
    )
    .recover(&policy(), PlanSlot::A)
    .unwrap_err();
    assert_eq!(
        error,
        WorkerRecoveryError::DataPlaneHealthFailed {
            cleanup_failed: false
        }
    );
    assert_eq!(
        events.borrow().as_slice(),
        [
            "core_start",
            "network_apply",
            "data_health",
            "network_rollback",
            "core_stop"
        ]
    );
    assert_eq!(
        store.current_generation().unwrap(),
        Some(GenerationId::new(1).unwrap())
    );
}

fn assert_candidate_removed(store: &GenerationStore) {
    assert_eq!(
        store.current_generation().unwrap(),
        Some(GenerationId::new(1).unwrap())
    );
    assert!(!store.generations_root().join("2").exists());
}

#[test]
fn worker_commits_only_after_network_and_data_plane_health() {
    let (_directory, store) = store_with_active_generation();
    let events = Rc::new(RefCell::new(Vec::new()));
    let checker = FakeChecker;
    let launcher = FakeLauncher {
        events: Rc::clone(&events),
        invalidate_manifest: false,
    };
    let auditor = FakeAuditor;
    let core_health = FakeCoreHealth;
    let core = CandidateActivator::new(&store, &checker, &launcher, &auditor, &core_health);
    let mut capabilities = FakeCapabilitySource {
        report: Some(report(CapabilityStatus::Supported)),
    };
    let mut network = FakeNetworkExecutor {
        events: Rc::clone(&events),
        fail_apply: false,
        fail_rollback: false,
    };
    let mut data_health = FakeDataPlaneHealth {
        store: &store,
        events: Rc::clone(&events),
        fail: false,
    };

    let active = {
        let mut worker =
            WorkerActivator::new(core, &mut capabilities, &mut network, &mut data_health);
        let active = worker
            .activate(&candidate(2), &policy(), PlanSlot::A)
            .unwrap();
        assert_eq!(worker.state(), RuntimeState::RunningTproxy);
        active
    };
    assert_eq!(
        store.current_generation().unwrap(),
        Some(GenerationId::new(2).unwrap())
    );
    assert_eq!(
        events.borrow().as_slice(),
        ["core_start", "network_apply", "data_health"]
    );

    active.stop(&mut network).unwrap();
    assert_eq!(
        events.borrow().as_slice(),
        [
            "core_start",
            "network_apply",
            "data_health",
            "network_rollback",
            "core_stop"
        ]
    );
}

#[test]
fn network_apply_failure_stops_core_and_keeps_previous_generation() {
    let (_directory, store) = store_with_active_generation();
    let events = Rc::new(RefCell::new(Vec::new()));
    let checker = FakeChecker;
    let launcher = FakeLauncher {
        events: Rc::clone(&events),
        invalidate_manifest: false,
    };
    let auditor = FakeAuditor;
    let core_health = FakeCoreHealth;
    let core = CandidateActivator::new(&store, &checker, &launcher, &auditor, &core_health);
    let mut capabilities = FakeCapabilitySource {
        report: Some(report(CapabilityStatus::Supported)),
    };
    let mut network = FakeNetworkExecutor {
        events: Rc::clone(&events),
        fail_apply: true,
        fail_rollback: false,
    };
    let mut data_health = FakeDataPlaneHealth {
        store: &store,
        events: Rc::clone(&events),
        fail: false,
    };

    let mut worker = WorkerActivator::new(core, &mut capabilities, &mut network, &mut data_health);
    let error = worker
        .activate(&candidate(2), &policy(), PlanSlot::A)
        .unwrap_err();
    assert_eq!(
        error.code(),
        WorkerActivationDiagnosticCode::NetworkApplyFailed
    );
    assert_eq!(worker.state(), RuntimeState::FailOpenDirect);
    assert_candidate_removed(&store);
    assert_eq!(
        events.borrow().as_slice(),
        ["core_start", "network_apply", "core_stop"]
    );
}

#[test]
fn data_plane_failure_rolls_back_network_before_stopping_core() {
    let (_directory, store) = store_with_active_generation();
    let events = Rc::new(RefCell::new(Vec::new()));
    let checker = FakeChecker;
    let launcher = FakeLauncher {
        events: Rc::clone(&events),
        invalidate_manifest: false,
    };
    let auditor = FakeAuditor;
    let core_health = FakeCoreHealth;
    let core = CandidateActivator::new(&store, &checker, &launcher, &auditor, &core_health);
    let mut capabilities = FakeCapabilitySource {
        report: Some(report(CapabilityStatus::Supported)),
    };
    let mut network = FakeNetworkExecutor {
        events: Rc::clone(&events),
        fail_apply: false,
        fail_rollback: false,
    };
    let mut data_health = FakeDataPlaneHealth {
        store: &store,
        events: Rc::clone(&events),
        fail: true,
    };

    let mut worker = WorkerActivator::new(core, &mut capabilities, &mut network, &mut data_health);
    let error = worker
        .activate(&candidate(2), &policy(), PlanSlot::A)
        .unwrap_err();
    assert_eq!(
        error.code(),
        WorkerActivationDiagnosticCode::DataPlaneHealthFailed
    );
    assert_candidate_removed(&store);
    assert_eq!(
        events.borrow().as_slice(),
        [
            "core_start",
            "network_apply",
            "data_health",
            "network_rollback",
            "core_stop"
        ]
    );
}

#[test]
fn commit_failure_rolls_back_network_and_discards_candidate() {
    let (_directory, store) = store_with_active_generation();
    let events = Rc::new(RefCell::new(Vec::new()));
    let checker = FakeChecker;
    let launcher = FakeLauncher {
        events: Rc::clone(&events),
        invalidate_manifest: true,
    };
    let auditor = FakeAuditor;
    let core_health = FakeCoreHealth;
    let core = CandidateActivator::new(&store, &checker, &launcher, &auditor, &core_health);
    let mut capabilities = FakeCapabilitySource {
        report: Some(report(CapabilityStatus::Supported)),
    };
    let mut network = FakeNetworkExecutor {
        events: Rc::clone(&events),
        fail_apply: false,
        fail_rollback: false,
    };
    let mut data_health = FakeDataPlaneHealth {
        store: &store,
        events: Rc::clone(&events),
        fail: false,
    };

    let mut worker = WorkerActivator::new(core, &mut capabilities, &mut network, &mut data_health);
    let error = worker
        .activate(&candidate(2), &policy(), PlanSlot::A)
        .unwrap_err();
    assert_eq!(error.code(), WorkerActivationDiagnosticCode::CommitFailed);
    assert_candidate_removed(&store);
    assert_eq!(
        events.borrow().as_slice(),
        [
            "core_start",
            "network_apply",
            "data_health",
            "network_rollback",
            "core_stop"
        ]
    );
}

#[test]
fn capability_and_plan_failures_do_not_start_core_or_touch_network() {
    for capability_report in [None, Some(report(CapabilityStatus::Unsupported))] {
        let (_directory, store) = store_with_active_generation();
        let events = Rc::new(RefCell::new(Vec::new()));
        let checker = FakeChecker;
        let launcher = FakeLauncher {
            events: Rc::clone(&events),
            invalidate_manifest: false,
        };
        let auditor = FakeAuditor;
        let core_health = FakeCoreHealth;
        let core = CandidateActivator::new(&store, &checker, &launcher, &auditor, &core_health);
        let mut capabilities = FakeCapabilitySource {
            report: capability_report,
        };
        let mut network = FakeNetworkExecutor {
            events: Rc::clone(&events),
            fail_apply: false,
            fail_rollback: false,
        };
        let mut data_health = FakeDataPlaneHealth {
            store: &store,
            events: Rc::clone(&events),
            fail: false,
        };

        let mut worker =
            WorkerActivator::new(core, &mut capabilities, &mut network, &mut data_health);
        let error = worker
            .activate(&candidate(2), &policy(), PlanSlot::A)
            .unwrap_err();
        assert!(matches!(
            error.code(),
            WorkerActivationDiagnosticCode::CapabilityProbeFailed
                | WorkerActivationDiagnosticCode::NetworkPlanRejected
        ));
        assert_eq!(worker.state(), RuntimeState::FailOpenDirect);
        assert_candidate_removed(&store);
        assert!(events.borrow().is_empty());
    }
}

#[test]
fn rollback_failure_is_reported_but_does_not_skip_core_stop() {
    let (_directory, store) = store_with_active_generation();
    let events = Rc::new(RefCell::new(Vec::new()));
    let checker = FakeChecker;
    let launcher = FakeLauncher {
        events: Rc::clone(&events),
        invalidate_manifest: false,
    };
    let auditor = FakeAuditor;
    let core_health = FakeCoreHealth;
    let core = CandidateActivator::new(&store, &checker, &launcher, &auditor, &core_health);
    let mut capabilities = FakeCapabilitySource {
        report: Some(report(CapabilityStatus::Supported)),
    };
    let mut network = FakeNetworkExecutor {
        events: Rc::clone(&events),
        fail_apply: false,
        fail_rollback: true,
    };
    let mut data_health = FakeDataPlaneHealth {
        store: &store,
        events: Rc::clone(&events),
        fail: true,
    };

    let mut worker = WorkerActivator::new(core, &mut capabilities, &mut network, &mut data_health);
    let error = worker
        .activate(&candidate(2), &policy(), PlanSlot::A)
        .unwrap_err();
    assert!(error.cleanup_failed());
    assert_candidate_removed(&store);
    assert_eq!(events.borrow().last(), Some(&"core_stop"));
}

#[derive(Debug)]
struct MonitorProcess {
    running: Arc<AtomicBool>,
    observe_error: Arc<AtomicBool>,
    events: Rc<RefCell<Vec<&'static str>>>,
}

impl CandidateProcess for MonitorProcess {
    fn identity(&self) -> ProcessIdentity {
        ProcessIdentity::new(50, Some(9)).unwrap()
    }

    fn is_running(&mut self) -> Result<bool, ProcessError> {
        if self.observe_error.load(Ordering::SeqCst) {
            return Err(ProcessError::ObserveFailed);
        }
        Ok(self.running.load(Ordering::SeqCst))
    }

    fn stop(self) -> Result<(), ProcessError> {
        self.events.borrow_mut().push("core_stop");
        Ok(())
    }
}

#[derive(Debug)]
struct MonitorLauncher {
    running: Arc<AtomicBool>,
    observe_error: Arc<AtomicBool>,
    events: Rc<RefCell<Vec<&'static str>>>,
}

impl CoreLauncher for MonitorLauncher {
    type Process = MonitorProcess;

    fn start(&self, _config_path: &Path) -> Result<Self::Process, ProcessError> {
        self.events.borrow_mut().push("core_start");
        Ok(MonitorProcess {
            running: Arc::clone(&self.running),
            observe_error: Arc::clone(&self.observe_error),
            events: Rc::clone(&self.events),
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct AlwaysCoreHealthy;

impl HealthProbe<MonitorProcess> for AlwaysCoreHealthy {
    fn wait_healthy(&self, _process: &mut MonitorProcess) -> Result<(), HealthProbeError> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct AlwaysDataPlaneHealthy;

impl DataPlaneHealthProbe<MonitorProcess> for AlwaysDataPlaneHealthy {
    fn wait_healthy(
        &mut self,
        _process: &mut MonitorProcess,
        _plan: &NetworkPlan,
        _capabilities: &CapabilityReport,
    ) -> Result<(), DataPlaneHealthError> {
        Ok(())
    }
}

#[derive(Debug)]
struct SequenceVerifier {
    outcomes: VecDeque<Result<(), NetworkHealthError>>,
    calls: usize,
}

impl SequenceVerifier {
    fn healthy() -> Self {
        Self {
            outcomes: VecDeque::new(),
            calls: 0,
        }
    }

    fn drifting_then_healthy() -> Self {
        Self {
            outcomes: VecDeque::from([Err(NetworkHealthError::OwnerMarkerMissing), Ok(())]),
            calls: 0,
        }
    }
}

impl NetworkHealthVerifier for SequenceVerifier {
    fn verify(&mut self, _plan: &NetworkPlan) -> Result<(), NetworkHealthError> {
        self.calls += 1;
        self.outcomes.pop_front().unwrap_or(Ok(()))
    }
}

fn monitored_runtime(
    store: &GenerationStore,
    running: &Arc<AtomicBool>,
    observe_error: &Arc<AtomicBool>,
    events: &Rc<RefCell<Vec<&'static str>>>,
    network: &mut FakeNetworkExecutor,
) -> ActiveRuntime<MonitorProcess, usize> {
    let checker = FakeChecker;
    let launcher = MonitorLauncher {
        running: Arc::clone(running),
        observe_error: Arc::clone(observe_error),
        events: Rc::clone(events),
    };
    let auditor = FakeAuditor;
    let core_health = AlwaysCoreHealthy;
    let core = CandidateActivator::new(store, &checker, &launcher, &auditor, &core_health);
    let mut capabilities = FakeCapabilitySource {
        report: Some(supported_report()),
    };
    let mut data_health = AlwaysDataPlaneHealthy;
    WorkerActivator::new(core, &mut capabilities, network, &mut data_health)
        .activate(&candidate(2), &policy(), PlanSlot::A)
        .unwrap()
}

fn runtime_limits() -> WorkerRuntimeLimits {
    WorkerRuntimeLimits::new(
        Duration::from_millis(250),
        Duration::from_secs(60),
        Duration::from_secs(300),
    )
    .unwrap()
}

#[test]
fn runtime_core_exit_rolls_back_capture_and_requests_bounded_restart() {
    let (_directory, store) = store_with_active_generation();
    let events = Rc::new(RefCell::new(Vec::new()));
    let running = Arc::new(AtomicBool::new(true));
    let observe_error = Arc::new(AtomicBool::new(false));
    let mut network = FakeNetworkExecutor {
        events: Rc::clone(&events),
        fail_apply: false,
        fail_rollback: false,
    };
    let active = monitored_runtime(&store, &running, &observe_error, &events, &mut network);
    events.borrow_mut().clear();
    running.store(false, Ordering::SeqCst);
    let mut runtime = WorkerRuntime::new(active, Duration::ZERO, runtime_limits());
    let mut verifier = SequenceVerifier::healthy();
    let mut budget = RestartBudget::new(Duration::from_secs(300)).unwrap();

    assert_eq!(
        runtime
            .tick(Duration::ZERO, &mut network, &mut verifier, &mut budget)
            .unwrap(),
        RuntimeTick::RestartScheduled {
            after: Duration::from_secs(1),
            failure: RuntimeFailureCode::CoreExited,
            cleanup_failed: false,
        }
    );
    assert_eq!(
        events.borrow().as_slice(),
        ["network_rollback", "core_stop"]
    );
}

#[test]
fn runtime_core_observation_failure_fails_open_before_requesting_restart() {
    let (_directory, store) = store_with_active_generation();
    let events = Rc::new(RefCell::new(Vec::new()));
    let running = Arc::new(AtomicBool::new(true));
    let observe_error = Arc::new(AtomicBool::new(false));
    let mut network = FakeNetworkExecutor {
        events: Rc::clone(&events),
        fail_apply: false,
        fail_rollback: false,
    };
    let active = monitored_runtime(&store, &running, &observe_error, &events, &mut network);
    events.borrow_mut().clear();
    observe_error.store(true, Ordering::SeqCst);
    let mut runtime = WorkerRuntime::new(active, Duration::ZERO, runtime_limits());
    let mut verifier = SequenceVerifier::healthy();
    let mut budget = RestartBudget::new(Duration::from_secs(300)).unwrap();

    assert_eq!(
        runtime
            .tick(Duration::ZERO, &mut network, &mut verifier, &mut budget)
            .unwrap(),
        RuntimeTick::RestartScheduled {
            after: Duration::from_secs(1),
            failure: RuntimeFailureCode::CoreObserveFailed,
            cleanup_failed: false,
        }
    );
    assert_eq!(runtime.state(), RuntimeState::Backoff);
    assert!(!runtime.has_active_runtime());
    assert_eq!(
        events.borrow().as_slice(),
        ["network_rollback", "core_stop"]
    );
}

#[test]
fn runtime_reconcile_is_low_frequency_and_repairs_owned_plan_without_core_restart() {
    let (_directory, store) = store_with_active_generation();
    let events = Rc::new(RefCell::new(Vec::new()));
    let running = Arc::new(AtomicBool::new(true));
    let observe_error = Arc::new(AtomicBool::new(false));
    let mut network = FakeNetworkExecutor {
        events: Rc::clone(&events),
        fail_apply: false,
        fail_rollback: false,
    };
    let active = monitored_runtime(&store, &running, &observe_error, &events, &mut network);
    events.borrow_mut().clear();
    let mut runtime = WorkerRuntime::new(active, Duration::ZERO, runtime_limits());
    let mut verifier = SequenceVerifier::drifting_then_healthy();
    let mut budget = RestartBudget::new(Duration::from_secs(300)).unwrap();

    assert_eq!(
        runtime
            .tick(
                Duration::from_secs(59),
                &mut network,
                &mut verifier,
                &mut budget,
            )
            .unwrap(),
        RuntimeTick::Healthy
    );
    assert_eq!(verifier.calls, 0);
    assert_eq!(
        runtime
            .tick(
                Duration::from_secs(60),
                &mut network,
                &mut verifier,
                &mut budget,
            )
            .unwrap(),
        RuntimeTick::Repaired
    );
    assert_eq!(verifier.calls, 2);
    assert_eq!(
        events.borrow().as_slice(),
        ["network_rollback", "network_apply"]
    );
    assert!(running.load(Ordering::SeqCst));
}

#[test]
fn debounced_network_event_advances_existing_runtime_reconcile() {
    let (_directory, store) = store_with_active_generation();
    let events = Rc::new(RefCell::new(Vec::new()));
    let running = Arc::new(AtomicBool::new(true));
    let observe_error = Arc::new(AtomicBool::new(false));
    let mut network = FakeNetworkExecutor {
        events: Rc::clone(&events),
        fail_apply: false,
        fail_rollback: false,
    };
    let active = monitored_runtime(&store, &running, &observe_error, &events, &mut network);
    events.borrow_mut().clear();
    let mut runtime = WorkerRuntime::new(active, Duration::ZERO, runtime_limits());
    let mut gate = EventReconcileGate::default();
    let mut verifier = SequenceVerifier::healthy();
    let mut budget = RestartBudget::new(Duration::from_secs(300)).unwrap();

    gate.observe(
        Duration::ZERO,
        NetworkEvent::new(NetworkAction::Upsert, NetworkChange::Link),
    )
    .unwrap();
    assert_eq!(
        gate.request_ready(Duration::from_millis(249), &mut runtime)
            .unwrap(),
        None
    );
    assert_eq!(
        gate.request_ready(Duration::from_millis(250), &mut runtime)
            .unwrap(),
        Some(1)
    );
    assert_eq!(
        runtime
            .tick(
                Duration::from_millis(250),
                &mut network,
                &mut verifier,
                &mut budget,
            )
            .unwrap(),
        RuntimeTick::Reconciled
    );
    assert_eq!(verifier.calls, 1);
    assert!(events.borrow().is_empty());
    assert!(running.load(Ordering::SeqCst));
}

#[test]
fn runtime_reconcile_failure_withdraws_capture_and_stops_core() {
    let (_directory, store) = store_with_active_generation();
    let events = Rc::new(RefCell::new(Vec::new()));
    let running = Arc::new(AtomicBool::new(true));
    let observe_error = Arc::new(AtomicBool::new(false));
    let mut network = FakeNetworkExecutor {
        events: Rc::clone(&events),
        fail_apply: false,
        fail_rollback: false,
    };
    let active = monitored_runtime(&store, &running, &observe_error, &events, &mut network);
    events.borrow_mut().clear();
    network.fail_apply = true;
    let mut runtime = WorkerRuntime::new(active, Duration::ZERO, runtime_limits());
    let mut verifier = SequenceVerifier::drifting_then_healthy();
    let mut budget = RestartBudget::new(Duration::from_secs(300)).unwrap();

    assert_eq!(
        runtime
            .tick(
                Duration::from_secs(60),
                &mut network,
                &mut verifier,
                &mut budget,
            )
            .unwrap(),
        RuntimeTick::RestartScheduled {
            after: Duration::from_secs(1),
            failure: RuntimeFailureCode::DriftRepairFailed,
            cleanup_failed: false,
        }
    );
    assert_eq!(
        events.borrow().as_slice(),
        [
            "network_rollback",
            "network_apply",
            "network_rollback",
            "core_stop"
        ]
    );
}

#[test]
fn runtime_persistent_drift_fails_open_after_one_bounded_repair() {
    let (_directory, store) = store_with_active_generation();
    let events = Rc::new(RefCell::new(Vec::new()));
    let running = Arc::new(AtomicBool::new(true));
    let observe_error = Arc::new(AtomicBool::new(false));
    let mut network = FakeNetworkExecutor {
        events: Rc::clone(&events),
        fail_apply: false,
        fail_rollback: false,
    };
    let active = monitored_runtime(&store, &running, &observe_error, &events, &mut network);
    events.borrow_mut().clear();
    let mut runtime = WorkerRuntime::new(active, Duration::ZERO, runtime_limits());
    let mut verifier = SequenceVerifier {
        outcomes: VecDeque::from([
            Err(NetworkHealthError::OwnerMarkerMissing),
            Err(NetworkHealthError::OwnerMarkerMissing),
        ]),
        calls: 0,
    };
    let mut budget = RestartBudget::new(Duration::from_secs(300)).unwrap();

    assert_eq!(
        runtime
            .tick(
                Duration::from_secs(60),
                &mut network,
                &mut verifier,
                &mut budget,
            )
            .unwrap(),
        RuntimeTick::RestartScheduled {
            after: Duration::from_secs(1),
            failure: RuntimeFailureCode::DriftPersisted,
            cleanup_failed: false,
        }
    );
    assert_eq!(verifier.calls, 2);
    assert_eq!(
        events.borrow().as_slice(),
        [
            "network_rollback",
            "network_apply",
            "network_rollback",
            "core_stop"
        ]
    );
}

#[test]
fn restart_budget_uses_one_two_four_seconds_then_opens_circuit() {
    let mut budget = RestartBudget::new(Duration::from_secs(300)).unwrap();
    assert_eq!(
        budget.register_failure(Duration::ZERO),
        RestartDecision::RetryAfter(Duration::from_secs(1))
    );
    assert_eq!(
        budget.register_failure(Duration::from_secs(1)),
        RestartDecision::RetryAfter(Duration::from_secs(2))
    );
    assert_eq!(
        budget.register_failure(Duration::from_secs(2)),
        RestartDecision::RetryAfter(Duration::from_secs(4))
    );
    assert_eq!(
        budget.register_failure(Duration::from_secs(3)),
        RestartDecision::CircuitOpen
    );
    assert_eq!(
        budget.register_failure(Duration::from_secs(304)),
        RestartDecision::RetryAfter(Duration::from_secs(1))
    );
}

#[derive(Debug)]
struct StopDriver {
    now: Duration,
}

impl WorkerLoopDriver for StopDriver {
    fn now(&self) -> Duration {
        self.now
    }

    fn wait(&mut self, timeout: Duration) -> WorkerLoopSignal {
        self.now += timeout;
        WorkerLoopSignal::Stop
    }
}

#[test]
fn worker_run_loop_honors_stop_after_a_healthy_tick() {
    let (_directory, store) = store_with_active_generation();
    let events = Rc::new(RefCell::new(Vec::new()));
    let running = Arc::new(AtomicBool::new(true));
    let observe_error = Arc::new(AtomicBool::new(false));
    let mut network = FakeNetworkExecutor {
        events: Rc::clone(&events),
        fail_apply: false,
        fail_rollback: false,
    };
    let active = monitored_runtime(&store, &running, &observe_error, &events, &mut network);
    events.borrow_mut().clear();
    let mut runtime = WorkerRuntime::new(active, Duration::ZERO, runtime_limits());
    let mut verifier = SequenceVerifier::healthy();
    let mut budget = RestartBudget::new(Duration::from_secs(300)).unwrap();
    let mut driver = StopDriver {
        now: Duration::ZERO,
    };

    assert_eq!(
        runtime.run(&mut driver, &mut network, &mut verifier, &mut budget),
        WorkerRunExit::Stopped {
            cleanup_failed: false
        }
    );
    assert_eq!(
        events.borrow().as_slice(),
        ["network_rollback", "core_stop"]
    );

    runtime.stop(&mut network).unwrap();
    assert_eq!(
        events.borrow().as_slice(),
        ["network_rollback", "core_stop"]
    );
}
