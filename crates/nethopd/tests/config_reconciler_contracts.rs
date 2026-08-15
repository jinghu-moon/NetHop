#![cfg(feature = "subscription-update")]

use std::{fs, path::Path};

use nethop_android::{AppCatalog, PackageSnapshot};
use nethop_protocol::{ApplicationPolicyMode, ApplicationTarget, ConfigMutation};
use nethopd::{
    ConfigChange, ConfigRuntime, ConfigStore, SourceIdEntropy, SourceRegistry, SourceRegistryError,
};
use serde_json::json;
use tempfile::tempdir;

struct FixedEntropy(u8);

impl SourceIdEntropy for FixedEntropy {
    fn fill(&mut self, output: &mut [u8; 16]) -> Result<(), SourceRegistryError> {
        output.fill(self.0);
        self.0 = self.0.saturating_add(1);
        Ok(())
    }
}

fn write(path: &Path, name: &str, url: &str, enabled: bool) {
    let text = format!(
        "schema_version = 3\n[service]\nenabled = {enabled}\n[subscriptions]\n[[subscriptions.sources]]\nname = \"{name}\"\nurl = \"{url}\"\n"
    );
    fs::write(path, text).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
}

#[test]
fn reload_keeps_source_digest_stable_for_rename_but_persists_service_switch() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    let registry_path = directory.path().join("source-registry.v1.json");
    write(&config_path, "Primary", "https://one.example/sub", true);
    let store = ConfigStore::new(&config_path).unwrap();
    let registry = SourceRegistry::new(&registry_path).unwrap();
    let snapshot = store.load().unwrap();
    let sources = registry.reconcile(&snapshot, &mut FixedEntropy(1)).unwrap();
    let mut runtime = ConfigRuntime::new(store, registry, snapshot, &sources);

    write(&config_path, "Renamed", "https://one.example/sub", true);
    let change = runtime.reload().unwrap();
    assert!(matches!(
        change,
        ConfigChange::Changed {
            service_changed: false,
            sources_changed: false,
            ..
        }
    ));

    let change = runtime.set_service_enabled(false).unwrap();
    assert!(matches!(
        change,
        ConfigChange::Changed {
            service_changed: true,
            sources_changed: false,
            enabled: false,
            ..
        }
    ));
    assert!(!runtime.current().effective().service_enabled());
}

#[cfg(unix)]
#[test]
fn editor_replaced_module_symlink_is_validated_imported_and_restored() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let directory = tempdir().unwrap();
    let persistent = directory.path().join("persistent/nethop.toml");
    let module_entry = directory.path().join("module/nethop.toml");
    fs::create_dir_all(persistent.parent().unwrap()).unwrap();
    fs::create_dir_all(module_entry.parent().unwrap()).unwrap();
    write(&persistent, "Primary", "https://one.example/sub", true);
    symlink(&persistent, &module_entry).unwrap();

    let store = ConfigStore::new(&persistent).unwrap();
    let registry = SourceRegistry::new(directory.path().join("source-registry.v1.json")).unwrap();
    let snapshot = store.load().unwrap();
    let sources = registry.reconcile(&snapshot, &mut FixedEntropy(1)).unwrap();
    let mut runtime = ConfigRuntime::new(store, registry, snapshot, &sources)
        .with_module_entry(&module_entry)
        .unwrap();

    fs::remove_file(&module_entry).unwrap();
    write(&module_entry, "Primary", "https://two.example/sub", true);
    fs::set_permissions(&module_entry, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(matches!(
        runtime.reload().unwrap(),
        ConfigChange::Changed {
            sources_changed: true,
            ..
        }
    ));
    assert!(
        fs::symlink_metadata(&module_entry)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(
        fs::read_to_string(&persistent)
            .unwrap()
            .contains("two.example")
    );
}

#[cfg(unix)]
#[test]
fn rejected_editor_replacement_does_not_overwrite_persistent_config() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let directory = tempdir().unwrap();
    let persistent = directory.path().join("nethop.toml");
    let module_directory = directory.path().join("module");
    fs::create_dir(&module_directory).unwrap();
    let module_entry = module_directory.join("nethop.toml");
    let original = "schema_version = 3\n[service]\nenabled = false\n[subscriptions]\n[[subscriptions.sources]]\nname = \"Primary\"\nurl = \"\"\n";
    fs::write(&persistent, original).unwrap();
    fs::set_permissions(&persistent, fs::Permissions::from_mode(0o600)).unwrap();
    symlink(&persistent, &module_entry).unwrap();

    let store = ConfigStore::new(&persistent).unwrap();
    let snapshot = store.load().unwrap();
    let registry = SourceRegistry::new(directory.path().join("source-registry.v1.json")).unwrap();
    let sources = registry.reconcile(&snapshot, &mut FixedEntropy(1)).unwrap();
    let mut runtime = ConfigRuntime::new(store, registry, snapshot, &sources)
        .with_module_entry(&module_entry)
        .unwrap();

    fs::remove_file(&module_entry).unwrap();
    fs::write(
        &module_entry,
        "schema_version = 3\n[service]\nenabled = true\n[subscriptions]\n[[subscriptions.sources]]\nname = \"Primary\"\nurl = \"\"\n[applications]\nmode = \"whitelist\"\ntargets = [{ kind = \"package\", package = \"com.example.missing\" }]\n",
    )
    .unwrap();
    fs::set_permissions(&module_entry, fs::Permissions::from_mode(0o600)).unwrap();

    assert!(runtime.reload().is_err());
    assert_eq!(fs::read_to_string(&persistent).unwrap(), original);
    assert!(fs::symlink_metadata(&module_entry).unwrap().is_file());
}

#[test]
fn typed_source_mutations_keep_private_identity_and_obey_cas() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    let registry_path = directory.path().join("source-registry.v1.json");
    write(&config_path, "Primary", "https://one.example/sub", true);
    let store = ConfigStore::new(&config_path).unwrap();
    let registry = SourceRegistry::new(&registry_path).unwrap();
    let snapshot = store.load().unwrap();
    let sources = registry.reconcile(&snapshot, &mut FixedEntropy(1)).unwrap();
    let mut runtime = ConfigRuntime::new(store, registry, snapshot, &sources);
    let first_id = runtime.redacted_document()["subscriptions"]["sources"][0]["source_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let stale = runtime.current().digest().to_owned();
    let add = runtime
        .mutate_with_entropy(
            &stale,
            &ConfigMutation::AddSource {
                name: "Backup".into(),
                url: "https://two.example/sub".into(),
            },
            &mut FixedEntropy(2),
        )
        .unwrap();
    let added_id = add.source_id().unwrap().to_owned();
    assert!(added_id.starts_with("src_"));
    assert!(!fs::read_to_string(&config_path).unwrap().contains("src_"));

    let before_stale_attempt = fs::read(&config_path).unwrap();
    assert!(
        runtime
            .mutate_with_entropy(
                &stale,
                &ConfigMutation::SetServiceEnabled { enabled: false },
                &mut FixedEntropy(3),
            )
            .is_err()
    );
    assert_eq!(fs::read(&config_path).unwrap(), before_stale_attempt);

    let current = runtime.current().digest().to_owned();
    runtime
        .mutate_with_entropy(
            &current,
            &ConfigMutation::UpdateSource {
                source_id: first_id.clone(),
                name: Some("Renamed".into()),
                url: Some("https://rotated.example/sub".into()),
                enabled: None,
            },
            &mut FixedEntropy(4),
        )
        .unwrap();
    let document = runtime.redacted_document();
    assert_eq!(
        document["subscriptions"]["sources"][0]["source_id"],
        first_id
    );
    assert_eq!(
        document["subscriptions"]["sources"][1]["source_id"],
        added_id
    );
    assert_eq!(document["subscriptions"]["sources"][0]["enabled"], true);
    assert_eq!(document["subscriptions"]["sources"][1]["enabled"], false);

    let current = runtime.current().digest().to_owned();
    runtime.subscription_select(&current, &added_id).unwrap();
    let selected = runtime.redacted_document();
    assert_eq!(selected["subscriptions"]["sources"][0]["enabled"], false);
    assert_eq!(selected["subscriptions"]["sources"][1]["enabled"], true);
    let persisted = fs::read_to_string(&config_path).unwrap();
    let persisted: toml::Value = toml::from_str(&persisted).unwrap();
    assert_eq!(
        persisted["subscriptions"]["sources"][0]["enabled"].as_bool(),
        Some(false)
    );
    assert_eq!(
        persisted["subscriptions"]["sources"][1]["enabled"].as_bool(),
        Some(true)
    );

    let single_digest = runtime.current().digest().to_owned();
    runtime
        .subscription_mode_set(&single_digest, nethopd::SubscriptionMode::Merge, None)
        .unwrap();
    assert_eq!(runtime.source_config().active_sources().count(), 1);
    let merge_digest = runtime.current().digest().to_owned();
    runtime
        .subscription_set_enabled(&merge_digest, &first_id, true)
        .unwrap();
    assert_eq!(runtime.source_config().active_sources().count(), 2);
    let both_digest = runtime.current().digest().to_owned();
    assert!(
        runtime
            .subscription_set_enabled(&merge_digest, &added_id, false)
            .is_err()
    );
    assert_eq!(runtime.current().digest(), both_digest);
    runtime
        .subscription_set_enabled(&both_digest, &added_id, false)
        .unwrap();
    let last_digest = runtime.current().digest().to_owned();
    assert!(
        runtime
            .subscription_set_enabled(&last_digest, &first_id, false)
            .is_err()
    );
    assert_eq!(runtime.source_config().active_sources().count(), 1);
}

#[test]
fn add_source_replaces_the_enabled_empty_single_mode_placeholder() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    let registry_path = directory.path().join("source-registry.v1.json");
    write(&config_path, "Primary", "", true);
    let store = ConfigStore::new(&config_path).unwrap();
    let registry = SourceRegistry::new(&registry_path).unwrap();
    let snapshot = store.load().unwrap();
    let sources = registry.reconcile(&snapshot, &mut FixedEntropy(1)).unwrap();
    let mut runtime = ConfigRuntime::new(store, registry, snapshot, &sources);
    let digest = runtime.current().digest().to_owned();

    let outcome = runtime
        .mutate_with_entropy(
            &digest,
            &ConfigMutation::AddSource {
                name: "Live".into(),
                url: "https://live.example/sub".into(),
            },
            &mut FixedEntropy(2),
        )
        .unwrap();

    let added_id = outcome.source_id().expect("daemon assigns the source ID");
    let document = runtime.redacted_document();
    assert_eq!(document["subscriptions"]["sources"][0]["enabled"], false);
    assert_eq!(document["subscriptions"]["sources"][1]["enabled"], true);
    assert_eq!(
        document["subscriptions"]["sources"][1]["source_id"],
        added_id
    );
    assert_eq!(runtime.source_config().active_sources().count(), 1);
}

#[test]
fn subscription_candidate_preview_is_pure_and_cas_guarded() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    let registry_path = directory.path().join("source-registry.v1.json");
    fs::write(
        &config_path,
        "schema_version = 3\n[service]\nenabled = true\n[subscriptions]\nmode = \"single\"\n[[subscriptions.sources]]\nname = \"Primary\"\nenabled = true\nurl = \"https://one.example/sub\"\n[[subscriptions.sources]]\nname = \"Backup\"\nenabled = false\nurl = \"https://two.example/sub\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let store = ConfigStore::new(&config_path).unwrap();
    let registry = SourceRegistry::new(&registry_path).unwrap();
    let snapshot = store.load().unwrap();
    let sources = registry.reconcile(&snapshot, &mut FixedEntropy(1)).unwrap();
    let runtime = ConfigRuntime::new(store, registry, snapshot, &sources);
    let digest = runtime.current().digest().to_owned();
    let backup_id = runtime.source_config().sources()[1]
        .id()
        .as_str()
        .to_owned();
    let config_before = fs::read(&config_path).unwrap();
    let registry_before = fs::read(&registry_path).unwrap();

    let preview = runtime
        .preview_subscription_select(&digest, &backup_id)
        .unwrap();
    assert_ne!(preview.candidate_digest(), digest);
    let candidate = std::str::from_utf8(preview.bytes()).unwrap();
    assert!(candidate.contains("name = \"Backup\"\nenabled = true"));
    assert_eq!(fs::read(&config_path).unwrap(), config_before);
    assert_eq!(fs::read(&registry_path).unwrap(), registry_before);

    let error = runtime
        .preview_subscription_select(&"0".repeat(64), &backup_id)
        .unwrap_err();
    assert!(error.is_conflict());
    assert_eq!(fs::read(&config_path).unwrap(), config_before);
    assert_eq!(fs::read(&registry_path).unwrap(), registry_before);
}

#[test]
fn schema_v3_scalar_mutation_rejects_removed_selector_mode_without_writing() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    let registry_path = directory.path().join("source-registry.v1.json");
    write(&config_path, "Primary", "https://one.example/sub", true);
    let store = ConfigStore::new(&config_path).unwrap();
    let registry = SourceRegistry::new(&registry_path).unwrap();
    let snapshot = store.load().unwrap();
    let sources = registry.reconcile(&snapshot, &mut FixedEntropy(1)).unwrap();
    let mut runtime = ConfigRuntime::new(store, registry, snapshot, &sources);
    let before = fs::read(&config_path).unwrap();
    let digest = runtime.current().digest().to_owned();

    let error = runtime
        .mutate_with_entropy(
            &digest,
            &ConfigMutation::SetScalarField {
                field_id: "proxy.selector_mode".into(),
                value: json!("manual"),
            },
            &mut FixedEntropy(2),
        )
        .unwrap_err();

    assert_eq!(error.diagnostic().1, "UNKNOWN-FIELD");
    assert_eq!(fs::read(config_path).unwrap(), before);
}

#[test]
fn application_policy_mutation_updates_mode_and_targets_in_one_transaction() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    let registry_path = directory.path().join("source-registry.v1.json");
    write(&config_path, "Primary", "https://one.example/sub", true);
    let store = ConfigStore::new(&config_path).unwrap();
    let registry = SourceRegistry::new(&registry_path).unwrap();
    let snapshot = store.load().unwrap();
    let sources = registry.reconcile(&snapshot, &mut FixedEntropy(1)).unwrap();
    let mut runtime = ConfigRuntime::new(store, registry, snapshot, &sources);

    let digest = runtime.current().digest().to_owned();
    runtime
        .mutate_with_entropy(
            &digest,
            &ConfigMutation::SetApplicationPolicy {
                mode: ApplicationPolicyMode::Whitelist,
                targets: vec![ApplicationTarget::Uid { uid: 10123 }],
            },
            &mut FixedEntropy(2),
        )
        .unwrap();

    let document = runtime.redacted_document();
    assert_eq!(document["applications"]["mode"], "whitelist");
    assert_eq!(
        document["applications"]["targets"],
        json!([{ "kind": "uid", "uid": 10123 }])
    );
    let persisted = fs::read_to_string(&config_path).unwrap();
    assert!(persisted.contains("mode = \"whitelist\""));
    assert!(persisted.contains("uid = 10123"));
}

#[test]
fn package_names_are_admitted_to_uids_before_any_config_write() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    write(&config_path, "Primary", "https://one.example/sub", true);
    let store = ConfigStore::new(&config_path).unwrap();
    let registry = SourceRegistry::new(directory.path().join("source-registry.v1.json")).unwrap();
    let snapshot = store.load().unwrap();
    let sources = registry.reconcile(&snapshot, &mut FixedEntropy(1)).unwrap();
    let catalog = AppCatalog::from_snapshots([PackageSnapshot::new(
        0,
        "package:android uid:1000\npackage:com.shared uid:1000\n",
        "package:android uid:1000\npackage:com.shared uid:1000\n",
        "",
    )])
    .unwrap();
    let mut runtime = ConfigRuntime::new(store, registry, snapshot, &sources)
        .with_app_catalog(catalog)
        .unwrap();
    let digest = runtime.current().digest().to_owned();
    let document = json!({
        "schema_version": 3,
        "service": {"enabled": true},
        "subscriptions": {"sources":[{"name":"Primary","url":"https://one.example/sub"}]},
        "applications": {"mode":"whitelist","targets":[
            {"kind":"package","android_user_id":0,"package":"com.shared"},
            {"kind":"uid","uid":10123}
        ]}
    });
    runtime
        .apply_document(&digest, &document)
        .expect("known package is admitted");
    assert_eq!(
        runtime.capture_policy().unwrap().include_uids(),
        [1000, 10123]
    );

    let before = fs::read(&config_path).unwrap();
    let digest = runtime.current().digest().to_owned();
    let unknown = json!({
        "schema_version": 3,
        "service": {"enabled": true},
        "subscriptions": {"sources":[{"name":"Primary","url":"https://one.example/sub"}]},
        "applications": {"mode":"whitelist","targets":[{"kind":"package","android_user_id":0,"package":"com.missing"}]}
    });
    assert!(runtime.apply_document(&digest, &unknown).is_err());
    assert_eq!(fs::read(&config_path).unwrap(), before);
}

#[test]
fn removing_a_node_persists_its_stable_id_in_every_source_filter() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    write(&config_path, "Primary", "https://one.example/sub", true);
    let store = ConfigStore::new(&config_path).unwrap();
    let registry = SourceRegistry::new(directory.path().join("source-registry.v1.json")).unwrap();
    let snapshot = store.load().unwrap();
    let sources = registry.reconcile(&snapshot, &mut FixedEntropy(1)).unwrap();
    let mut runtime = ConfigRuntime::new(store, registry, snapshot, &sources);
    let digest = runtime.current().digest().to_owned();
    runtime
        .mutate_with_entropy(
            &digest,
            &ConfigMutation::RemoveNode {
                node_id: "nh1s-0123456789abcdef".into(),
            },
            &mut FixedEntropy(2),
        )
        .unwrap();
    assert_eq!(
        runtime.current().effective().sources()[0]
            .filter()
            .excluded_node_ids(),
        ["nh1s-0123456789abcdef"]
    );
    assert!(
        fs::read_to_string(config_path)
            .unwrap()
            .contains("excluded_node_ids = [\"nh1s-0123456789abcdef\"]")
    );
}

#[test]
fn complete_apply_repairs_invalid_observed_toml_with_exact_cas() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    write(&config_path, "Primary", "https://one.example/sub", true);
    let store = ConfigStore::new(&config_path).unwrap();
    let registry = SourceRegistry::new(directory.path().join("source-registry.v1.json")).unwrap();
    let snapshot = store.load().unwrap();
    let sources = registry.reconcile(&snapshot, &mut FixedEntropy(1)).unwrap();
    let mut runtime = ConfigRuntime::new(store, registry, snapshot, &sources);

    fs::write(&config_path, "this is not valid toml = [").unwrap();
    let observed = runtime.observed_digest().unwrap();
    assert_ne!(observed, runtime.current().digest());
    let document = json!({
        "schema_version": 3,
        "service": {"enabled": true},
        "subscriptions": {"sources":[{"name":"Primary","url":"https://one.example/sub"}]}
    });
    runtime.apply_document(&observed, &document).unwrap();
    assert!(runtime.disk_matches_current());
    assert_ne!(runtime.current().digest(), observed);
}

#[test]
fn manager_self_write_followed_by_watcher_reload_is_a_digest_noop() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    write(&config_path, "Primary", "https://one.example/sub", false);
    let store = ConfigStore::new(&config_path).unwrap();
    let registry = SourceRegistry::new(directory.path().join("source-registry.v1.json")).unwrap();
    let snapshot = store.load().unwrap();
    let sources = registry.reconcile(&snapshot, &mut FixedEntropy(1)).unwrap();
    let mut runtime = ConfigRuntime::new(store, registry, snapshot, &sources);
    let original_digest = runtime.current().digest().to_owned();
    let document = json!({
        "schema_version": 3,
        "service": {"enabled": true},
        "subscriptions": {
            "sources": [{"name": "Primary", "url": "https://one.example/sub"}]
        }
    });

    assert!(matches!(
        runtime.apply_document(&original_digest, &document).unwrap(),
        ConfigChange::Changed {
            service_changed: true,
            sources_changed: false,
            ..
        }
    ));
    let committed_digest = runtime.current().digest().to_owned();
    assert_ne!(committed_digest, original_digest);

    assert_eq!(runtime.reload().unwrap(), ConfigChange::Unchanged);
    assert_eq!(runtime.current().digest(), committed_digest);
    assert_eq!(runtime.candidate_sequence(), 2);
}

#[test]
fn rejected_reload_advances_candidate_state_without_replacing_active_config() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    write(&config_path, "Primary", "https://one.example/sub", true);
    let store = ConfigStore::new(&config_path).unwrap();
    let registry = SourceRegistry::new(directory.path().join("source-registry.v1.json")).unwrap();
    let snapshot = store.load().unwrap();
    let active_digest = snapshot.digest().to_owned();
    let sources = registry.reconcile(&snapshot, &mut FixedEntropy(1)).unwrap();
    let mut runtime = ConfigRuntime::new(store, registry, snapshot, &sources);

    fs::write(&config_path, "invalid = [").unwrap();
    assert!(runtime.reload().is_err());
    assert_eq!(runtime.candidate_sequence(), 1);
    assert_eq!(runtime.last_reload().as_str(), "rejected");
    assert_eq!(runtime.current().digest(), active_digest);

    write(&config_path, "Primary", "https://one.example/sub", true);
    assert!(runtime.reload().is_ok());
    assert_eq!(runtime.candidate_sequence(), 2);
    assert_eq!(runtime.last_reload().as_str(), "accepted");
    assert_eq!(runtime.current().digest(), active_digest);
}

#[test]
fn rollback_restores_exact_observed_bytes_active_snapshot_and_source_ids() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    write(&config_path, "Primary", "https://one.example/sub", true);
    let store = ConfigStore::new(&config_path).unwrap();
    let registry = SourceRegistry::new(directory.path().join("source-registry.v1.json")).unwrap();
    let snapshot = store.load().unwrap();
    let sources = registry.reconcile(&snapshot, &mut FixedEntropy(1)).unwrap();
    let mut runtime = ConfigRuntime::new(store, registry, snapshot, &sources);
    let original_bytes = fs::read(&config_path).unwrap();
    let original_digest = runtime.current().digest().to_owned();
    let original_id = runtime.redacted_document()["subscriptions"]["sources"][0]["source_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let checkpoint = runtime.checkpoint().unwrap();

    runtime
        .mutate_with_entropy(
            &original_digest,
            &ConfigMutation::UpdateSource {
                source_id: original_id.clone(),
                name: Some("Replacement".into()),
                url: Some("https://two.example/sub".into()),
                enabled: None,
            },
            &mut FixedEntropy(2),
        )
        .unwrap();
    let rollback = runtime.rollback(checkpoint).unwrap();

    assert!(matches!(
        rollback,
        ConfigChange::Changed { enabled: true, .. }
    ));
    assert_eq!(fs::read(&config_path).unwrap(), original_bytes);
    assert_eq!(runtime.current().digest(), original_digest);
    assert_eq!(
        runtime.redacted_document()["subscriptions"]["sources"][0]["source_id"],
        original_id
    );
}
