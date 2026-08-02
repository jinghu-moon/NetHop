use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use nethop_core::{Candidate, GenerationId, GenerationStore, ManagedConfig, TerminalOutbound};
use nethopd::{
    ActivationDiagnosticCode, CandidateActivator, CandidateChecker, CandidateProcess, CoreLauncher,
    HealthProbe, HealthProbeError, ManagedSafetyAuditor, ProcessError, ProcessIdentity,
    RunnerError, SafetyAuditError, SafetyAuditor, StartupLivenessProbe,
};
use serde_json::json;

fn candidate(id: u64, tag: &str) -> Candidate {
    Candidate::new(
        GenerationId::new(id).unwrap(),
        ManagedConfig::from_outbounds(vec![
            TerminalOutbound::new(
                tag,
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
    store.publish(&candidate(1, "one"), |_| Ok(())).unwrap();
    (directory, store)
}

#[derive(Debug, Clone, Copy)]
struct FakeChecker {
    fail: bool,
}

impl CandidateChecker for FakeChecker {
    fn check(&self, _config_path: &Path) -> Result<(), RunnerError> {
        if self.fail {
            Err(RunnerError::SpawnFailed)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct FakeAuditor {
    fail: bool,
}

impl SafetyAuditor for FakeAuditor {
    fn audit(&self, _candidate: &Candidate, _config_path: &Path) -> Result<(), SafetyAuditError> {
        if self.fail {
            Err(SafetyAuditError::ForbiddenTopLevel)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug)]
struct FakeProcess {
    stopped: Arc<AtomicBool>,
    running: bool,
}

impl CandidateProcess for FakeProcess {
    fn identity(&self) -> ProcessIdentity {
        ProcessIdentity::new(42, Some(7)).unwrap()
    }

    fn is_running(&mut self) -> Result<bool, ProcessError> {
        Ok(self.running)
    }

    fn stop(self) -> Result<(), ProcessError> {
        self.stopped.store(true, Ordering::SeqCst);
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct FakeLauncher {
    fail: bool,
    invalidate_manifest: bool,
    stopped: Arc<AtomicBool>,
}

impl CoreLauncher for FakeLauncher {
    type Process = FakeProcess;

    fn start(&self, config_path: &Path) -> Result<Self::Process, ProcessError> {
        if self.fail {
            return Err(ProcessError::SpawnFailed);
        }
        if self.invalidate_manifest {
            fs::remove_file(config_path.with_file_name("manifest.json")).unwrap();
        }
        Ok(FakeProcess {
            stopped: Arc::clone(&self.stopped),
            running: true,
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct FakeHealth {
    fail: bool,
}

impl HealthProbe<FakeProcess> for FakeHealth {
    fn wait_healthy(&self, _process: &mut FakeProcess) -> Result<(), HealthProbeError> {
        if self.fail {
            Err(HealthProbeError::TimedOut)
        } else {
            Ok(())
        }
    }
}

fn assert_old_generation_is_untouched(store: &GenerationStore) {
    assert_eq!(
        store.current_generation().unwrap(),
        Some(GenerationId::new(1).unwrap())
    );
    assert!(!store.generations_root().join("2").exists());
    assert!(
        !fs::read_dir(store.generations_root())
            .unwrap()
            .any(|entry| {
                entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".candidate-")
            })
    );
}

fn activator<'a>(
    store: &'a GenerationStore,
    checker: &'a FakeChecker,
    launcher: &'a FakeLauncher,
    auditor: &'a FakeAuditor,
    health: &'a FakeHealth,
) -> CandidateActivator<'a, FakeChecker, FakeLauncher, FakeAuditor, FakeHealth> {
    CandidateActivator::new(store, checker, launcher, auditor, health)
}

#[test]
fn successful_activation_commits_only_after_health() {
    let (_directory, store) = store_with_active_generation();
    let stopped = Arc::new(AtomicBool::new(false));
    let checker = FakeChecker { fail: false };
    let launcher = FakeLauncher {
        fail: false,
        invalidate_manifest: false,
        stopped: Arc::clone(&stopped),
    };
    let auditor = FakeAuditor { fail: false };
    let health = FakeHealth { fail: false };

    let active = activator(&store, &checker, &launcher, &auditor, &health)
        .activate(&candidate(2, "two"))
        .unwrap();

    assert_eq!(active.generation(), GenerationId::new(2).unwrap());
    assert_eq!(
        active.previous_generation(),
        Some(GenerationId::new(1).unwrap())
    );
    assert_eq!(
        store.current_generation().unwrap(),
        Some(active.generation())
    );
    assert!(!stopped.load(Ordering::SeqCst));
    active.stop().unwrap();
    assert!(stopped.load(Ordering::SeqCst));
}

#[test]
fn safety_and_check_failures_discard_prepared_candidate() {
    for (auditor_fail, checker_fail, expected) in [
        (true, false, ActivationDiagnosticCode::SafetyRejected),
        (false, true, ActivationDiagnosticCode::CheckFailed),
    ] {
        let (_directory, store) = store_with_active_generation();
        let checker = FakeChecker { fail: checker_fail };
        let launcher = FakeLauncher {
            fail: false,
            invalidate_manifest: false,
            stopped: Arc::new(AtomicBool::new(false)),
        };
        let auditor = FakeAuditor { fail: auditor_fail };
        let health = FakeHealth { fail: false };

        let error = activator(&store, &checker, &launcher, &auditor, &health)
            .activate(&candidate(2, "two"))
            .unwrap_err();
        assert_eq!(error.code(), expected);
        assert!(!error.cleanup_failed());
        assert_old_generation_is_untouched(&store);
    }
}

#[test]
fn spawn_failure_discards_sealed_generation_without_stopping_old_state() {
    let (_directory, store) = store_with_active_generation();
    let checker = FakeChecker { fail: false };
    let launcher = FakeLauncher {
        fail: true,
        invalidate_manifest: false,
        stopped: Arc::new(AtomicBool::new(false)),
    };
    let auditor = FakeAuditor { fail: false };
    let health = FakeHealth { fail: false };

    let error = activator(&store, &checker, &launcher, &auditor, &health)
        .activate(&candidate(2, "two"))
        .unwrap_err();

    assert_eq!(error.code(), ActivationDiagnosticCode::StartFailed);
    assert_old_generation_is_untouched(&store);
}

#[test]
fn health_failure_stops_candidate_and_keeps_previous_generation() {
    let (_directory, store) = store_with_active_generation();
    let stopped = Arc::new(AtomicBool::new(false));
    let checker = FakeChecker { fail: false };
    let launcher = FakeLauncher {
        fail: false,
        invalidate_manifest: false,
        stopped: Arc::clone(&stopped),
    };
    let auditor = FakeAuditor { fail: false };
    let health = FakeHealth { fail: true };

    let error = activator(&store, &checker, &launcher, &auditor, &health)
        .activate(&candidate(2, "two"))
        .unwrap_err();

    assert_eq!(error.code(), ActivationDiagnosticCode::HealthFailed);
    assert!(stopped.load(Ordering::SeqCst));
    assert_old_generation_is_untouched(&store);
}

#[test]
fn commit_failure_stops_candidate_and_discards_incomplete_generation() {
    let (_directory, store) = store_with_active_generation();
    let stopped = Arc::new(AtomicBool::new(false));
    let checker = FakeChecker { fail: false };
    let launcher = FakeLauncher {
        fail: false,
        invalidate_manifest: true,
        stopped: Arc::clone(&stopped),
    };
    let auditor = FakeAuditor { fail: false };
    let health = FakeHealth { fail: false };

    let error = activator(&store, &checker, &launcher, &auditor, &health)
        .activate(&candidate(2, "two"))
        .unwrap_err();

    assert_eq!(error.code(), ActivationDiagnosticCode::CommitFailed);
    assert!(stopped.load(Ordering::SeqCst));
    assert_old_generation_is_untouched(&store);
}

#[test]
fn managed_auditor_detects_prepared_file_tampering_without_echoing_content() {
    let directory = tempfile::tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    let candidate = candidate(1, "one");
    let prepared = store.prepare_candidate(&candidate).unwrap();
    let auditor = ManagedSafetyAuditor;
    auditor.audit(&candidate, &prepared.config_path()).unwrap();

    fs::write(prepared.config_path(), b"{\"outbounds\":[]}").unwrap();
    let error = auditor
        .audit(&candidate, &prepared.config_path())
        .unwrap_err();
    assert_eq!(error, SafetyAuditError::ConfigMismatch);
    assert!(!error.to_string().contains("fixture-only"));
}

#[test]
fn startup_liveness_probe_rejects_early_exit_and_invalid_limits() {
    let stopped = Arc::new(AtomicBool::new(false));
    let mut process = FakeProcess {
        stopped,
        running: false,
    };
    let probe = StartupLivenessProbe::new(
        Duration::from_millis(30),
        Duration::from_millis(20),
        Duration::from_millis(5),
    )
    .unwrap();
    assert_eq!(
        probe.wait_healthy(&mut process).unwrap_err(),
        HealthProbeError::EarlyExit
    );
    assert_eq!(
        StartupLivenessProbe::new(Duration::ZERO, Duration::ZERO, Duration::ZERO).unwrap_err(),
        HealthProbeError::TimedOut
    );
}
