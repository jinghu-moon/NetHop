use std::{cell::RefCell, collections::BTreeMap, path::Path, rc::Rc};

use nethop_android::{
    AllocationCapability, CapabilityError, CapabilityReport, CapabilityStatus, ExecutionError,
    FamilyCapability, IpFamily, NetfilterBackend, NetworkPlan, PlanSlot, ResourceCandidate,
};
use nethop_core::{
    Candidate, CaptureMode, CapturePolicy, GenerationId, GenerationStore, ManagedConfig,
    RuntimeState, TerminalOutbound,
};
use nethopd::{
    CandidateActivator, CandidateChecker, CandidateProcess, CapabilitySource, CoreLauncher,
    DataPlaneHealthError, DataPlaneHealthProbe, HealthProbe, HealthProbeError, NetworkController,
    ProcessError, ProcessIdentity, RunnerError, SafetyAuditError, SafetyAuditor,
    WorkerActivationDiagnosticCode, WorkerActivator,
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
        &self,
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
            Err(DataPlaneHealthError::Unhealthy)
        } else {
            Ok(())
        }
    }
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
    let data_health = FakeDataPlaneHealth {
        store: &store,
        events: Rc::clone(&events),
        fail: false,
    };

    let active = {
        let mut worker = WorkerActivator::new(core, &mut capabilities, &mut network, &data_health);
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
    let data_health = FakeDataPlaneHealth {
        store: &store,
        events: Rc::clone(&events),
        fail: false,
    };

    let mut worker = WorkerActivator::new(core, &mut capabilities, &mut network, &data_health);
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
    let data_health = FakeDataPlaneHealth {
        store: &store,
        events: Rc::clone(&events),
        fail: true,
    };

    let mut worker = WorkerActivator::new(core, &mut capabilities, &mut network, &data_health);
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
    let data_health = FakeDataPlaneHealth {
        store: &store,
        events: Rc::clone(&events),
        fail: false,
    };

    let mut worker = WorkerActivator::new(core, &mut capabilities, &mut network, &data_health);
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
        let data_health = FakeDataPlaneHealth {
            store: &store,
            events: Rc::clone(&events),
            fail: false,
        };

        let mut worker = WorkerActivator::new(core, &mut capabilities, &mut network, &data_health);
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
    let data_health = FakeDataPlaneHealth {
        store: &store,
        events: Rc::clone(&events),
        fail: true,
    };

    let mut worker = WorkerActivator::new(core, &mut capabilities, &mut network, &data_health);
    let error = worker
        .activate(&candidate(2), &policy(), PlanSlot::A)
        .unwrap_err();
    assert!(error.cleanup_failed());
    assert_candidate_removed(&store);
    assert_eq!(events.borrow().last(), Some(&"core_stop"));
}
