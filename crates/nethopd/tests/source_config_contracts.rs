#![cfg(feature = "subscription-update")]

use std::{collections::VecDeque, fs};

use nethop_subscription::{
    CapabilityMatrix, FormatHint, ParserLimits, SourceId, SourceInput, convert_stable_sources,
};
use nethopd::{
    ConfigStore, NodeAttribution, SourceIdEntropy, SourceRegistry, SourceRegistryError,
    SourceUpdateParticipation, SubscriptionMode,
};
use tempfile::tempdir;

#[derive(Debug)]
struct FakeEntropy {
    values: VecDeque<[u8; 16]>,
    fail: bool,
}

impl FakeEntropy {
    fn new(values: impl IntoIterator<Item = u8>) -> Self {
        Self {
            values: values.into_iter().map(|value| [value; 16]).collect(),
            fail: false,
        }
    }
}

impl SourceIdEntropy for FakeEntropy {
    fn fill(&mut self, output: &mut [u8; 16]) -> Result<(), SourceRegistryError> {
        if self.fail {
            return Err(SourceRegistryError::EntropyUnavailable);
        }
        *output = self
            .values
            .pop_front()
            .ok_or(SourceRegistryError::EntropyUnavailable)?;
        Ok(())
    }
}

fn config(primary_name: &str, primary_url: &str, backup: Option<(&str, &str)>) -> String {
    let mode = if backup.is_some() { "merge" } else { "single" };
    let mut value = format!(
        "schema_version = 3\n[service]\nenabled = true\n[subscriptions]\nmode = \"{mode}\"\n\n[[subscriptions.sources]]\nname = \"{primary_name}\"\nurl = \"{primary_url}\"\n"
    );
    if let Some((name, url)) = backup {
        value.push_str(&format!(
            "\n[[subscriptions.sources]]\nname = \"{name}\"\nurl = \"{url}\"\n"
        ));
    }
    value
}

fn write_private(path: &std::path::Path, contents: &str) {
    fs::write(path, contents).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
}

fn single_with_configured_backup() -> &'static str {
    r#"schema_version = 3
[service]
enabled = true
[subscriptions]
mode = "single"
[[subscriptions.sources]]
name = "Primary"
enabled = true
url = "https://one.example/sub"
[[subscriptions.sources]]
name = "Backup"
enabled = false
url = "https://two.example/sub"
"#
}

#[test]
fn daemon_assigns_private_random_ids_without_writing_them_to_toml() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    let registry_path = directory.path().join("source-registry.v1.json");
    write_private(
        &config_path,
        &config(
            "Primary",
            "https://one.example/sub",
            Some(("Backup", "https://two.example/sub")),
        ),
    );
    let snapshot = ConfigStore::new(&config_path).unwrap().load().unwrap();
    let registry = SourceRegistry::new(&registry_path).unwrap();
    let sources = registry
        .reconcile(&snapshot, &mut FakeEntropy::new([1, 2]))
        .unwrap();

    assert_eq!(sources.sources().len(), 2);
    assert_eq!(
        sources.sources()[0].id().as_str(),
        "src_01010101010101010101010101010101"
    );
    assert_eq!(
        sources.sources()[1].id().as_str(),
        "src_02020202020202020202020202020202"
    );
    assert!(!fs::read_to_string(&config_path).unwrap().contains("id ="));
    let registry_text = fs::read_to_string(&registry_path).unwrap();
    assert!(!registry_text.contains("https://"));
    assert!(!registry_text.contains("Primary"));
}

#[test]
fn rename_reorder_and_url_rotation_preserve_identity_deterministically() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    let registry = SourceRegistry::new(directory.path().join("source-registry.v1.json")).unwrap();
    write_private(
        &config_path,
        &config(
            "Primary",
            "https://one.example/sub",
            Some(("Backup", "https://two.example/sub")),
        ),
    );
    let first = registry
        .reconcile(
            &ConfigStore::new(&config_path).unwrap().load().unwrap(),
            &mut FakeEntropy::new([1, 2]),
        )
        .unwrap();
    let primary = first.sources()[0].id().clone();
    let backup = first.sources()[1].id().clone();

    write_private(
        &config_path,
        &config(
            "Backup Renamed",
            "https://two.example/sub",
            Some(("Primary", "https://one.example/rotated-token")),
        ),
    );
    let second = registry
        .reconcile(
            &ConfigStore::new(&config_path).unwrap().load().unwrap(),
            &mut FakeEntropy::new([]),
        )
        .unwrap();
    assert_eq!(second.sources()[0].id(), &backup);
    assert_eq!(second.sources()[1].id(), &primary);
}

#[test]
fn changing_name_and_url_together_allocates_a_new_identity() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    let registry = SourceRegistry::new(directory.path().join("source-registry.v1.json")).unwrap();
    write_private(
        &config_path,
        &config("Primary", "https://one.example/sub", None),
    );
    let first = registry
        .reconcile(
            &ConfigStore::new(&config_path).unwrap().load().unwrap(),
            &mut FakeEntropy::new([1]),
        )
        .unwrap();
    write_private(
        &config_path,
        &config("Replacement", "https://replacement.example/sub", None),
    );
    let second = registry
        .reconcile(
            &ConfigStore::new(&config_path).unwrap().load().unwrap(),
            &mut FakeEntropy::new([2]),
        )
        .unwrap();
    assert_ne!(first.sources()[0].id(), second.sources()[0].id());
}

#[test]
fn configured_and_active_sources_have_one_mode_aware_entry_point() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nethop.toml");
    write_private(&path, single_with_configured_backup());
    let snapshot = ConfigStore::new(&path).unwrap().load().unwrap();
    let config = SourceRegistry::new(directory.path().join("source-registry.v1.json"))
        .unwrap()
        .reconcile(&snapshot, &mut FakeEntropy::new([1, 2]))
        .unwrap();

    assert_eq!(config.mode(), SubscriptionMode::Single);
    assert_eq!(config.configured_sources().count(), 2);
    assert_eq!(config.active_sources().count(), 1);
    assert_eq!(
        config.active_sources().next().unwrap().name().as_str(),
        "Primary"
    );
    assert_eq!(
        config
            .update_participation(config.sources()[0].id())
            .unwrap(),
        SourceUpdateParticipation::ActiveGeneration
    );
    assert_eq!(
        config
            .update_participation(config.sources()[1].id())
            .unwrap(),
        SourceUpdateParticipation::InactiveCacheOnly
    );

    let active = config.active_set_snapshot();
    assert_eq!(active.mode(), SubscriptionMode::Single);
    assert_eq!(
        active.active_source_ids(),
        [config.sources()[0].id().clone()]
    );
    assert_eq!(active.config_digest(), snapshot.digest());
    let encoded = serde_json::to_string(&active).unwrap();
    assert!(!encoded.contains("https://"));
    assert!(!encoded.contains("one.example"));
    assert_eq!(active.sources().len(), 2);
    assert!(active.sources()[0].active());
    assert!(!active.sources()[1].active());
}

#[test]
fn node_attribution_is_ordered_deduplicated_and_bounded_to_sources() {
    let first = SourceId::new("src_11111111111111111111111111111111").unwrap();
    let second = SourceId::new("src_22222222222222222222222222222222").unwrap();
    let attribution = NodeAttribution::new([first.clone(), second.clone(), first]).unwrap();
    assert_eq!(
        attribution.source_ids(),
        [
            SourceId::new("src_11111111111111111111111111111111").unwrap(),
            second,
        ]
    );
    let too_many = (0..17)
        .map(|number| SourceId::new(format!("src_{number:032x}")).unwrap())
        .collect::<Vec<_>>();
    assert!(NodeAttribution::new(too_many).is_err());
    assert!(NodeAttribution::new(Vec::<SourceId>::new()).is_err());
}

#[test]
fn cross_source_dedupe_preserves_all_sources_without_changing_node_identity() {
    let one = SourceId::new("src_11111111111111111111111111111111").unwrap();
    let two = SourceId::new("src_22222222222222222222222222222222").unwrap();
    let bytes = b"trojan://secret@example.com:443#node".to_vec();
    let convert = |source_ids: [SourceId; 2]| {
        convert_stable_sources(
            source_ids
                .into_iter()
                .map(|source_id| SourceInput {
                    source_id,
                    format_hint: FormatHint::UriList,
                    bytes: bytes.clone(),
                })
                .collect(),
            &ParserLimits::default(),
            &CapabilityMatrix::default(),
        )
    };
    let forward = convert([one.clone(), two.clone()]);
    let reverse = convert([two, one]);
    assert_eq!(forward.nodes.len(), 1);
    assert_eq!(forward.nodes[0].source_refs.len(), 2);
    assert_eq!(
        forward.nodes[0].node_id.as_str(),
        reverse.nodes[0].node_id.as_str()
    );
}

#[test]
fn entropy_fails_closed_but_corrupt_registry_is_replaced_with_new_identity() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    let registry_path = directory.path().join("source-registry.v1.json");
    write_private(
        &config_path,
        &config("Primary", "https://one.example/sub", None),
    );
    let snapshot = ConfigStore::new(&config_path).unwrap().load().unwrap();
    let registry = SourceRegistry::new(&registry_path).unwrap();
    let mut entropy = FakeEntropy::new([]);
    entropy.fail = true;
    assert_eq!(
        registry.reconcile(&snapshot, &mut entropy).unwrap_err(),
        SourceRegistryError::EntropyUnavailable
    );

    write_private(&registry_path, "not-json");
    let rebuilt = registry
        .reconcile(&snapshot, &mut FakeEntropy::new([1]))
        .unwrap();
    assert_eq!(
        rebuilt.sources()[0].id().as_str(),
        "src_01010101010101010101010101010101"
    );
    assert!(
        fs::read_to_string(registry_path)
            .unwrap()
            .contains("nethop-source-registry-v1")
    );
}

#[test]
fn pending_binding_is_recovered_when_toml_digest_matches_after_a_crash() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    let registry_path = directory.path().join("source-registry.v1.json");
    let registry = SourceRegistry::new(&registry_path).unwrap();
    write_private(
        &config_path,
        &config("Primary", "https://one.example/sub", None),
    );
    let first_snapshot = ConfigStore::new(&config_path).unwrap().load().unwrap();
    let first = registry
        .reconcile(&first_snapshot, &mut FakeEntropy::new([1]))
        .unwrap();
    let source_id = first.sources()[0].id().clone();

    write_private(
        &config_path,
        &config("Primary", "https://one.example/rotated", None),
    );
    let second_snapshot = ConfigStore::new(&config_path).unwrap().load().unwrap();
    let prepared = registry
        .prepare(&second_snapshot, &mut FakeEntropy::new([]))
        .unwrap();
    let staged = fs::read_to_string(&registry_path).unwrap();
    assert!(staged.contains("\"pending\":{"));
    assert!(staged.contains(first_snapshot.digest()));
    drop(prepared);

    let recovered = registry
        .reconcile(&second_snapshot, &mut FakeEntropy::new([]))
        .unwrap();
    assert_eq!(recovered.sources()[0].id(), &source_id);
    let activated = fs::read_to_string(&registry_path).unwrap();
    assert!(activated.contains(second_snapshot.digest()));
    assert!(activated.contains("\"pending\":null"));
}
