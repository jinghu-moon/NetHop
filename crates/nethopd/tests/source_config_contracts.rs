#![cfg(feature = "subscription-update")]

use std::{collections::VecDeque, fs};

use nethopd::{ConfigStore, SourceIdEntropy, SourceRegistry, SourceRegistryError};
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
    let mut value = format!(
        "schema_version = 1\n[service]\nenabled = true\n[subscriptions]\n\n[[subscriptions.sources]]\nname = \"{primary_name}\"\nurl = \"{primary_url}\"\n"
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
