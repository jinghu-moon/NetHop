#![cfg(feature = "subscription-update")]

use std::{cell::Cell, collections::BTreeMap, fs, rc::Rc};

use nethop_core::{
    CaptureMode, CapturePolicy, ClashApi, GenerationId, GenerationStore, ManagedOptions, TunStack,
};
use nethop_subscription::{CapabilityMatrix, ParserLimits};
use nethopd::{
    CandidateChecker, ConfigStore, RunnerError, SourceBodyFetcher, SourceConfig, SourceDefinition,
    SourceIdEntropy, SourceRegistry, SourceRegistryError, SourceUpdateError, SourceUpdateService,
    UpdateRuntimePolicy,
};
use tempfile::tempdir;

struct FakeFetcher {
    bodies: BTreeMap<String, Vec<u8>>,
    fail: bool,
    failed_source: Option<String>,
}

struct FixedEntropy(u8);

impl SourceIdEntropy for FixedEntropy {
    fn fill(&mut self, output: &mut [u8; 16]) -> Result<(), SourceRegistryError> {
        output.fill(self.0);
        self.0 = self.0.saturating_add(1);
        Ok(())
    }
}

impl SourceBodyFetcher for FakeFetcher {
    fn fetch(&mut self, source: &SourceDefinition) -> Result<Vec<u8>, SourceUpdateError> {
        if self.fail {
            return Err(SourceUpdateError::Fetch);
        }
        if self.failed_source.as_deref() == Some(source.id().as_str()) {
            return Err(SourceUpdateError::Fetch);
        }
        self.bodies
            .get(source.id().as_str())
            .cloned()
            .ok_or(SourceUpdateError::Fetch)
    }
}

struct FakeChecker {
    calls: Rc<Cell<usize>>,
    reject: bool,
}

impl CandidateChecker for FakeChecker {
    fn check(&self, config_path: &std::path::Path) -> Result<(), RunnerError> {
        self.calls.set(self.calls.get() + 1);
        assert_eq!(config_path.file_name().unwrap(), "config.json");
        if self.reject {
            Err(RunnerError::InvalidPolicy)
        } else {
            Ok(())
        }
    }
}

fn write_sources(path: &std::path::Path) -> SourceConfig {
    fs::write(
        path,
        br#"schema_version = 1
[service]
enabled = true
[subscriptions]
[[subscriptions.sources]]
name = "One"
url = "https://one.example/s"
[[subscriptions.sources]]
name = "Two"
url = "https://two.example/s"
"#,
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let snapshot = ConfigStore::new(path).unwrap().load().unwrap();
    SourceRegistry::new(path.with_file_name("source-registry.v1.json"))
        .unwrap()
        .reconcile(&snapshot, &mut FixedEntropy(1))
        .unwrap()
}

fn runtime() -> UpdateRuntimePolicy {
    UpdateRuntimePolicy::new(
        CapturePolicy::new(
            CaptureMode::Tproxy,
            true,
            Some(7893),
            Some(0x20_000),
            Vec::new(),
            vec![0],
        )
        .unwrap(),
        ClashApi::new("127.0.0.1:9090", "source-update-secret-32-bytes-000").unwrap(),
        TunStack::System,
        ManagedOptions::default(),
    )
}

fn bodies(config: &SourceConfig) -> BTreeMap<String, Vec<u8>> {
    let sources = config.sources();
    BTreeMap::from([
        (
            sources[0].id().as_str().into(),
            b"trojan://secret@example.com:443#one\n".to_vec(),
        ),
        (
            sources[1].id().as_str().into(),
            b"trojan://secret@example.com:443#duplicate\ntrojan://other@two.example:443#two\n"
                .to_vec(),
        ),
    ])
}

#[test]
fn successful_multi_source_update_deduplicates_checks_and_commits_once() {
    let directory = tempdir().unwrap();
    let config = write_sources(&directory.path().join("nethop.toml"));
    let store = GenerationStore::new(directory.path().join("state")).unwrap();
    let calls = Rc::new(Cell::new(0));
    let checker = FakeChecker {
        calls: calls.clone(),
        reject: false,
    };
    let mut service = SourceUpdateService::new(
        &store,
        FakeFetcher {
            bodies: bodies(&config),
            fail: false,
            failed_source: None,
        },
        &checker,
        ParserLimits::default(),
        CapabilityMatrix::default(),
        runtime(),
    );
    let report = service.update(&config).unwrap();
    assert_eq!(report.generation, GenerationId::new(1).unwrap());
    assert_eq!(report.source_count, 2);
    assert_eq!(
        (report.accepted, report.duplicate, report.node_count),
        (2, 1, 2)
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(store.current_generation().unwrap(), Some(report.generation));
}

#[test]
fn prepared_update_does_not_switch_current_before_explicit_commit() {
    let directory = tempdir().unwrap();
    let config = write_sources(&directory.path().join("nethop.toml"));
    let store = GenerationStore::new(directory.path().join("state")).unwrap();
    let calls = Rc::new(Cell::new(0));
    let checker = FakeChecker {
        calls: calls.clone(),
        reject: false,
    };
    let mut service = SourceUpdateService::new(
        &store,
        FakeFetcher {
            bodies: bodies(&config),
            fail: false,
            failed_source: None,
        },
        &checker,
        ParserLimits::default(),
        CapabilityMatrix::default(),
        runtime(),
    );

    let prepared = service.prepare(&config).unwrap();
    assert_eq!(store.current_generation().unwrap(), None);
    assert!(store.generations_root().join("1/config.json").is_file());
    let report = service.commit(prepared).unwrap();
    assert_eq!(store.current_generation().unwrap(), Some(report.generation));
    assert_eq!(calls.get(), 1);
}

#[test]
fn fetch_or_check_failure_keeps_the_previous_generation() {
    for (fail_fetch, reject_check) in [(true, false), (false, true)] {
        let directory = tempdir().unwrap();
        let config = write_sources(&directory.path().join("nethop.toml"));
        let store = GenerationStore::new(directory.path().join("state")).unwrap();
        let calls = Rc::new(Cell::new(0));
        let checker = FakeChecker {
            calls: calls.clone(),
            reject: reject_check,
        };
        let mut service = SourceUpdateService::new(
            &store,
            FakeFetcher {
                bodies: bodies(&config),
                fail: fail_fetch,
                failed_source: None,
            },
            &checker,
            ParserLimits::default(),
            CapabilityMatrix::default(),
            runtime(),
        );
        assert!(service.update(&config).is_err());
        assert_eq!(store.current_generation().unwrap(), None);
        assert_eq!(calls.get(), usize::from(reject_check));
        assert!(
            fs::read_dir(store.generations_root())
                .unwrap()
                .all(|entry| !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".candidate-"))
        );
    }
}

#[test]
fn one_unavailable_source_does_not_discard_other_publishable_sources() {
    let directory = tempdir().unwrap();
    let config = write_sources(&directory.path().join("nethop.toml"));
    let failed_source = config.sources()[1].id().as_str().to_owned();
    let store = GenerationStore::new(directory.path().join("state")).unwrap();
    let calls = Rc::new(Cell::new(0));
    let checker = FakeChecker {
        calls: calls.clone(),
        reject: false,
    };
    let mut service = SourceUpdateService::new(
        &store,
        FakeFetcher {
            bodies: bodies(&config),
            fail: false,
            failed_source: Some(failed_source),
        },
        &checker,
        ParserLimits::default(),
        CapabilityMatrix::default(),
        runtime(),
    );

    let report = service.update(&config).unwrap();
    assert_eq!(report.source_count, 2);
    assert_eq!(report.accepted, 1);
    assert_eq!(report.node_count, 1);
    assert_eq!(calls.get(), 1);
}
