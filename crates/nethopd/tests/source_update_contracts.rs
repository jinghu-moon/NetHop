#![cfg(feature = "subscription-update")]

use std::{cell::Cell, collections::BTreeMap, fs, rc::Rc};

use nethop_core::{
    CaptureMode, CapturePolicy, ClashApi, GenerationId, GenerationNodeRegistry, GenerationStore,
    ManagedOptions, TunStack,
};
use nethop_subscription::{CapabilityMatrix, FormatHint, ParserLimits};
use nethopd::{
    CandidateChecker, ConfigStore, ConfiguredSourceUpdater, ManualSourceStore, NodeOverride,
    NodeOverrideSet, NodeOverrideStore, RunnerError, RuntimeUpdateSource, SourceBody,
    SourceBodyFetcher, SourceBodyOrigin, SourceConfig, SourceDefinition, SourceIdEntropy,
    SourceRegistry, SourceRegistryError, SourceUpdateError, SourceUpdateService, StableNodeId,
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
    fn fetch(&mut self, source: &SourceDefinition) -> Result<SourceBody, SourceUpdateError> {
        if self.fail {
            return Err(SourceUpdateError::Fetch);
        }
        if self.failed_source.as_deref() == Some(source.id().as_str()) {
            return Err(SourceUpdateError::Fetch);
        }
        self.bodies
            .get(source.id().as_str())
            .cloned()
            .map(|bytes| SourceBody::new(bytes, SourceBodyOrigin::Fresh))
            .ok_or(SourceUpdateError::Fetch)
    }

    fn cached(&mut self, source: &SourceDefinition) -> Result<SourceBody, SourceUpdateError> {
        self.bodies
            .get(source.id().as_str())
            .cloned()
            .map(|bytes| SourceBody::new(bytes, SourceBodyOrigin::LastKnownGood))
            .ok_or(SourceUpdateError::Cache)
    }
}

struct FakeChecker {
    calls: Rc<Cell<usize>>,
    reject: bool,
}

struct CountingFetcher {
    bodies: BTreeMap<String, Vec<u8>>,
    fresh_calls: Rc<Cell<usize>>,
    cached_calls: Rc<Cell<usize>>,
}

impl SourceBodyFetcher for CountingFetcher {
    fn fetch(&mut self, source: &SourceDefinition) -> Result<SourceBody, SourceUpdateError> {
        self.fresh_calls.set(self.fresh_calls.get() + 1);
        self.bodies
            .get(source.id().as_str())
            .cloned()
            .map(|bytes| SourceBody::new(bytes, SourceBodyOrigin::Fresh))
            .ok_or(SourceUpdateError::Fetch)
    }

    fn cached(&mut self, source: &SourceDefinition) -> Result<SourceBody, SourceUpdateError> {
        self.cached_calls.set(self.cached_calls.get() + 1);
        self.bodies
            .get(source.id().as_str())
            .cloned()
            .map(|bytes| SourceBody::new(bytes, SourceBodyOrigin::LastKnownGood))
            .ok_or(SourceUpdateError::Cache)
    }
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
        br#"schema_version = 3
[service]
enabled = true
[subscriptions]
mode = "merge"
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
fn surfboard_source_reaches_the_existing_nodes_only_candidate_pipeline() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    fs::write(
        &config_path,
        br#"schema_version = 3
[service]
enabled = true
[subscriptions]
[[subscriptions.sources]]
name = "Surfboard"
url = "https://surfboard.example/sub"
request_profile = "surfboard"
format_hint = "surfboard_ini"
"#,
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let snapshot = ConfigStore::new(&config_path).unwrap().load().unwrap();
    let config = SourceRegistry::new(directory.path().join("source-registry.v1.json"))
        .unwrap()
        .reconcile(&snapshot, &mut FixedEntropy(1))
        .unwrap();
    assert_eq!(
        config.sources()[0].expected_format(),
        FormatHint::SurfboardIni
    );

    let body = include_bytes!("../../nethop-subscription/tests/fixtures/surfboard/basic.conf");
    let store = GenerationStore::new(directory.path().join("state")).unwrap();
    let calls = Rc::new(Cell::new(0));
    let checker = FakeChecker {
        calls: Rc::clone(&calls),
        reject: false,
    };
    let mut service = SourceUpdateService::new(
        &store,
        FakeFetcher {
            bodies: BTreeMap::from([(config.sources()[0].id().as_str().to_owned(), body.to_vec())]),
            fail: false,
            failed_source: None,
        },
        &checker,
        ParserLimits::default(),
        CapabilityMatrix::default(),
        runtime(),
    );
    let report = service.update(&config).unwrap();
    assert_eq!(report.accepted, 3);
    assert_eq!(report.node_count, 3);
    assert_eq!(calls.get(), 1);
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
fn topology_rebuild_uses_only_cached_subscription_bodies() {
    let directory = tempdir().unwrap();
    let config = write_sources(&directory.path().join("nethop.toml"));
    let store = GenerationStore::new(directory.path().join("state")).unwrap();
    let fresh_calls = Rc::new(Cell::new(0));
    let cached_calls = Rc::new(Cell::new(0));
    let checker = FakeChecker {
        calls: Rc::new(Cell::new(0)),
        reject: false,
    };
    let service = SourceUpdateService::new(
        &store,
        CountingFetcher {
            bodies: bodies(&config),
            fresh_calls: Rc::clone(&fresh_calls),
            cached_calls: Rc::clone(&cached_calls),
        },
        &checker,
        ParserLimits::default(),
        CapabilityMatrix::default(),
        runtime(),
    );
    let mut updater = ConfiguredSourceUpdater::new(service, config);

    updater.request_cached_rebuild().unwrap();
    let prepared = updater.prepare().unwrap();
    assert_eq!(updater.generation(&prepared), GenerationId::new(1).unwrap());
    assert_eq!(fresh_calls.get(), 0);
    assert_eq!(cached_calls.get(), 2);
    updater.discard(prepared).unwrap();
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

#[test]
fn selected_source_update_uses_only_cached_bodies_for_other_sources() {
    let directory = tempdir().unwrap();
    let config = write_sources(&directory.path().join("nethop.toml"));
    let selected = config.sources()[0].id().clone();
    let unselected = config.sources()[1].id().as_str().to_owned();
    let store = GenerationStore::new(directory.path().join("state")).unwrap();
    let calls = Rc::new(Cell::new(0));
    let checker = FakeChecker {
        calls,
        reject: false,
    };
    let mut service = SourceUpdateService::new(
        &store,
        FakeFetcher {
            bodies: bodies(&config),
            fail: false,
            failed_source: Some(unselected),
        },
        &checker,
        ParserLimits::default(),
        CapabilityMatrix::default(),
        runtime(),
    );

    let report = service.update_source(&config, &selected).unwrap();
    assert_eq!(report.sources.len(), 2);
    assert_eq!(report.sources[0].origin, Some(SourceBodyOrigin::Fresh));
    assert_eq!(
        report.sources[1].origin,
        Some(SourceBodyOrigin::LastKnownGood)
    );
    assert_eq!(report.node_count, 2);
}

#[test]
fn selected_source_update_fails_closed_when_another_source_has_no_cache() {
    let directory = tempdir().unwrap();
    let config = write_sources(&directory.path().join("nethop.toml"));
    let selected = config.sources()[0].id().clone();
    let mut available = bodies(&config);
    available.remove(config.sources()[1].id().as_str());
    let store = GenerationStore::new(directory.path().join("state")).unwrap();
    let checker = FakeChecker {
        calls: Rc::new(Cell::new(0)),
        reject: false,
    };
    let mut service = SourceUpdateService::new(
        &store,
        FakeFetcher {
            bodies: available,
            fail: false,
            failed_source: None,
        },
        &checker,
        ParserLimits::default(),
        CapabilityMatrix::default(),
        runtime(),
    );

    assert!(matches!(
        service.update_source(&config, &selected),
        Err(SourceUpdateError::Cache)
    ));
    assert_eq!(store.current_generation().unwrap(), None);
}

#[test]
fn source_local_filter_runs_before_candidate_composition() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    fs::write(
        &config_path,
        br#"schema_version = 3
[service]
enabled = true
[subscriptions]
[[subscriptions.sources]]
name = "Filtered"
url = "https://one.example/s"
filter = { include_names = ["keep"], protocols = ["vless"] }
"#,
    )
    .unwrap();
    let snapshot = ConfigStore::new(&config_path).unwrap().load().unwrap();
    let config = SourceRegistry::new(config_path.with_file_name("source-registry.v1.json"))
        .unwrap()
        .reconcile(&snapshot, &mut FixedEntropy(1))
        .unwrap();
    let source_id = config.sources()[0].id().as_str().to_owned();
    let bodies = BTreeMap::from([(
        source_id,
        b"trojan://secret@drop.example:443?security=tls#keep-trojan\n\
vless://550e8400-e29b-41d4-a716-446655440000@keep.example:443?security=tls#keep-vless\n\
vless://550e8400-e29b-41d4-a716-446655440001@drop.example:443?security=tls#drop-vless\n"
            .to_vec(),
    )]);
    let store = GenerationStore::new(directory.path().join("state")).unwrap();
    let calls = Rc::new(Cell::new(0));
    let checker = FakeChecker {
        calls: calls.clone(),
        reject: false,
    };
    let mut service = SourceUpdateService::new(
        &store,
        FakeFetcher {
            bodies,
            fail: false,
            failed_source: None,
        },
        &checker,
        ParserLimits::default(),
        CapabilityMatrix::default(),
        runtime(),
    );

    let report = service.update(&config).unwrap();
    assert_eq!(report.node_count, 1);
    assert_eq!(report.accepted, 1);
    let generated = fs::read_to_string(store.generations_root().join("1/config.json")).unwrap();
    assert!(generated.contains("keep.example"));
    assert!(!generated.contains("drop.example"));
}

#[test]
fn local_import_requires_preview_digest_and_does_not_publish_before_commit() {
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
    )
    .with_manual_source_store(
        ManualSourceStore::new(directory.path().join("manual-source.body")).unwrap(),
    );
    let payload = b"trojan://local-secret@local.example:443#local\n";
    let preview = service
        .preview_import(&config, payload, FormatHint::UriList)
        .unwrap();
    assert_eq!(preview.node_count, 3);
    assert_eq!(calls.get(), 0);
    assert!(matches!(
        service.prepare_import(&config, payload, FormatHint::UriList, &"0".repeat(64)),
        Err(SourceUpdateError::CandidateDigestMismatch)
    ));
    let prepared = service
        .prepare_import(
            &config,
            payload,
            FormatHint::UriList,
            &preview.candidate_digest,
        )
        .unwrap();
    assert_eq!(store.current_generation().unwrap(), None);
    let report = service.commit(prepared).unwrap();
    assert_eq!(store.current_generation().unwrap(), Some(report.generation));
    assert_eq!(calls.get(), 1);
    assert!(directory.path().join("manual-source.body").is_file());

    let refreshed = service.update(&config).unwrap();
    assert_eq!(refreshed.node_count, 3);
    let generated = fs::read_to_string(store.generations_root().join("2/config.json")).unwrap();
    assert!(generated.contains("local.example"));
}

#[test]
fn local_import_bootstraps_without_configured_source_cache() {
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
            bodies: BTreeMap::new(),
            fail: false,
            failed_source: None,
        },
        &checker,
        ParserLimits::default(),
        CapabilityMatrix::default(),
        runtime(),
    )
    .with_manual_source_store(
        ManualSourceStore::new(directory.path().join("manual-source.body")).unwrap(),
    );
    let payload = b"trojan://local-secret@local.example:443#local\n";

    let preview = service
        .preview_import(&config, payload, FormatHint::UriList)
        .unwrap();
    assert_eq!(preview.node_count, 1);
    let prepared = service
        .prepare_import(
            &config,
            payload,
            FormatHint::UriList,
            &preview.candidate_digest,
        )
        .unwrap();
    let report = service.commit(prepared).unwrap();

    assert_eq!(report.node_count, 1);
    assert_eq!(store.current_generation().unwrap(), Some(report.generation));
    assert_eq!(calls.get(), 1);
    assert!(directory.path().join("manual-source.body").is_file());
}

#[test]
fn node_override_commit_persists_registry_and_preserves_node_identity() {
    let directory = tempdir().unwrap();
    let config = write_sources(&directory.path().join("nethop.toml"));
    let store = GenerationStore::new(directory.path().join("state")).unwrap();
    let override_path = directory.path().join("node-overrides.json");
    let override_store = NodeOverrideStore::new(override_path.clone()).unwrap();
    let checker = FakeChecker {
        calls: Rc::new(Cell::new(0)),
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
    )
    .with_node_override_store(override_store, NodeOverrideSet::default());
    let initial = service.update(&config).unwrap();
    let initial_registry: GenerationNodeRegistry = serde_json::from_slice(
        &fs::read(
            store
                .generations_root()
                .join(initial.generation.get().to_string())
                .join("nodes.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let original = initial_registry.records().first().unwrap();
    let node_id = StableNodeId::new(original.stable_node_id()).unwrap();
    let original_sources = original.source_ids().to_vec();
    let mut overrides = NodeOverrideSet::default();
    overrides
        .upsert(
            NodeOverride::new(
                node_id.clone(),
                "编辑后的东京节点",
                serde_json::json!({
                    "type": "trojan",
                    "server": "edited.example.com",
                    "server_port": 443,
                    "password": "private-override-secret",
                    "tls": {"enabled": true}
                }),
            )
            .unwrap(),
        )
        .unwrap();

    let prepared = service
        .prepare_node_overrides(&config, overrides.clone())
        .unwrap();
    assert_eq!(
        store.current_generation().unwrap(),
        Some(initial.generation)
    );
    assert!(!override_path.exists());
    let committed = service.commit(prepared).unwrap();

    assert_eq!(committed.generation.get(), initial.generation.get() + 1);
    assert_eq!(service.node_overrides(), &overrides);
    assert_eq!(
        NodeOverrideStore::new(&override_path)
            .unwrap()
            .load()
            .unwrap(),
        overrides
    );
    let generated_root = store
        .generations_root()
        .join(committed.generation.get().to_string());
    let registry: GenerationNodeRegistry =
        serde_json::from_slice(&fs::read(generated_root.join("nodes.json")).unwrap()).unwrap();
    let edited = registry.by_stable_id(node_id.as_str()).unwrap();
    assert_eq!(edited.display_name(), "编辑后的东京节点");
    assert_eq!(edited.source_ids(), original_sources);
    let generated: serde_json::Value =
        serde_json::from_slice(&fs::read(generated_root.join("config.json")).unwrap()).unwrap();
    let outbound = generated["outbounds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["tag"] == node_id.as_str())
        .unwrap();
    assert_eq!(outbound["server"], "edited.example.com");
    assert_eq!(outbound["password"], "private-override-secret");
}

#[test]
fn rejected_node_override_candidate_does_not_write_registry_or_advance_generation() {
    let directory = tempdir().unwrap();
    let config = write_sources(&directory.path().join("nethop.toml"));
    let store = GenerationStore::new(directory.path().join("state")).unwrap();
    let accepting_checker = FakeChecker {
        calls: Rc::new(Cell::new(0)),
        reject: false,
    };
    let initial = SourceUpdateService::new(
        &store,
        FakeFetcher {
            bodies: bodies(&config),
            fail: false,
            failed_source: None,
        },
        &accepting_checker,
        ParserLimits::default(),
        CapabilityMatrix::default(),
        runtime(),
    )
    .update(&config)
    .unwrap();
    let registry: GenerationNodeRegistry = serde_json::from_slice(
        &fs::read(
            store
                .generations_root()
                .join(initial.generation.get().to_string())
                .join("nodes.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let node_id = StableNodeId::new(registry.records()[0].stable_node_id()).unwrap();
    let override_path = directory.path().join("node-overrides.json");
    let rejecting_checker = FakeChecker {
        calls: Rc::new(Cell::new(0)),
        reject: true,
    };
    let mut service = SourceUpdateService::new(
        &store,
        FakeFetcher {
            bodies: bodies(&config),
            fail: false,
            failed_source: None,
        },
        &rejecting_checker,
        ParserLimits::default(),
        CapabilityMatrix::default(),
        runtime(),
    )
    .with_node_override_store(
        NodeOverrideStore::new(&override_path).unwrap(),
        NodeOverrideSet::default(),
    );
    let mut overrides = NodeOverrideSet::default();
    overrides
        .upsert(
            NodeOverride::new(
                node_id,
                "不会提交的节点",
                serde_json::json!({
                    "type": "trojan",
                    "server": "rejected.example.com",
                    "server_port": 443,
                    "password": "never-persisted"
                }),
            )
            .unwrap(),
        )
        .unwrap();

    assert!(service.prepare_node_overrides(&config, overrides).is_err());
    assert_eq!(
        store.current_generation().unwrap(),
        Some(initial.generation)
    );
    assert!(!override_path.exists());
    assert!(service.node_overrides().is_empty());
}
