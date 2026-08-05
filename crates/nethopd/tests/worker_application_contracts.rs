use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

use nethop_android::{
    AllocationCapability, CapabilityError, CapabilityReport, CapabilityStatus, ExecutionError,
    FamilyCapability, IpFamily, NetfilterBackend, NetworkHealthError, NetworkHealthVerifier,
    NetworkPlan, PlanSlot, ResourceCandidate,
};
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
    ConfigRuntime, ConfigStore, RuntimePolicyError, RuntimeUpdateError, RuntimeUpdateSource,
    SourceIdEntropy, SourceRegistry, SourceRegistryError, UpdateStatus,
};
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
    );

    assert_eq!(application.next_wakeup_in(), Duration::ZERO);
    application.run_ready().unwrap();
    assert_eq!(application.snapshot().state, RuntimeState::FailOpenDirect);
    assert_eq!(attempts.borrow().as_slice(), ["recover"]);
    assert_eq!(application.next_wakeup_in(), Duration::from_secs(1));

    let status = application.handle(request("status", ControlMethod::StatusGet));
    assert_eq!(status.result().unwrap()["state"], "fail_open_direct");
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
        "schema_version = 1\n[service]\nenabled = false\n[subscriptions]\n[[subscriptions.sources]]\nname = \"Primary\"\nurl = \"https://one.example/sub\"\n",
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
fn stale_manager_apply_returns_current_observed_digest_without_sensitive_values() {
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    fs::write(
        &config_path,
        "schema_version = 1\n[service]\nenabled = false\n[subscriptions]\n[[subscriptions.sources]]\nname = \"Primary\"\nurl = \"https://secret.example/account-token\"\n",
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
                "schema_version": 1,
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
        "schema_version = 1\n[service]\nenabled = false\n[subscriptions]\n[[subscriptions.sources]]\nname = \"Primary\"\nurl = \"https://secret.example/account-token\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
    let store = ConfigStore::new(&config_path).unwrap();
    let snapshot = store.load().unwrap();
    let registry = SourceRegistry::new(directory.path().join("source-registry.v1.json")).unwrap();
    let sources = registry.reconcile(&snapshot, &mut FixedEntropy(1)).unwrap();
    let config_runtime = ConfigRuntime::new(store, registry, snapshot, &sources);
    let mut application = WorkerApplication::new(
        TestRecovery::default(),
        TestNetwork,
        TestVerifier,
        TestClock(Rc::new(Cell::new(Duration::from_secs(2)))),
        policy(),
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    )
    .with_configuration(config_runtime, false);

    let hello = request("hello", ControlMethod::ProtocolHello)
        .with_params(ControlParams::hello("manager-alpha".into(), 1, 1))
        .unwrap();
    let hello = application.handle(hello);
    assert!(hello.ok());
    assert_eq!(hello.result().unwrap()["compatible"], true);
    assert_eq!(hello.result().unwrap()["daemon_protocol_min"], 1);
    assert!(
        hello.result().unwrap()["supported_features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == "multi_source")
    );

    let incompatible = request("hello-new", ControlMethod::ProtocolHello)
        .with_params(ControlParams::hello("manager-new".into(), 1, 2))
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
        "schema_version = 1\n[service]\nenabled = true\n[subscriptions]\n[[subscriptions.sources]]\nname = \"Primary\"\nurl = \"https://one.example/sub\"\n[advanced]\ndry_run = true\n",
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
fn advanced_apply_refreshes_runtime_probe_health_and_reconcile_policy() {
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    fs::write(
        &config_path,
        "schema_version = 1\n[service]\nenabled = false\n[subscriptions]\n[[subscriptions.sources]]\nname = \"Primary\"\nurl = \"\"\n",
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
            "schema_version": 1,
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
