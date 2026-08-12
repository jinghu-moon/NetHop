use std::{collections::BTreeSet, fs, path::PathBuf};

use serde_json::Value;

fn baseline_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/subscription-selection")
}

#[test]
fn a001_a008_baseline_manifest_is_complete_bounded_and_secret_free() {
    let root = baseline_root();
    let bytes = fs::read(root.join("baseline-manifest-v1.json")).unwrap();
    assert!(bytes.len() <= 16 * 1024);
    let manifest: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(manifest["schema"], "nethop-selection-tdd-baseline-v1");

    let task_ids = manifest["task_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        task_ids,
        (1..=8)
            .map(|number| format!("A{number:03}"))
            .collect::<BTreeSet<_>>()
    );

    let behaviors = manifest["behaviors"].as_object().unwrap();
    assert_eq!(behaviors.len(), 10);
    for number in 1..=10 {
        assert!(behaviors.contains_key(&format!("B{number:02}")));
    }

    for fixture in manifest["fixtures"].as_array().unwrap() {
        let path = root.join(fixture.as_str().unwrap());
        let metadata = fs::metadata(&path).unwrap();
        assert!(metadata.is_file());
        assert!(metadata.len() <= 64 * 1024);
    }

    let serialized = String::from_utf8(bytes).unwrap().to_ascii_lowercase();
    for secret in ["glados", "baac5688", "f936155", "121525"] {
        assert!(!serialized.contains(secret));
    }
}

#[test]
fn a006_old_wire_inventory_remains_data_not_a_compatibility_path() {
    let root = baseline_root();
    let config = fs::read_to_string(root.join("fixtures/schema-v2-before.toml")).unwrap();
    assert!(config.contains("schema_version = 2"));
    assert!(config.contains("selector_mode = \"urltest\""));
    assert!(!config.contains("source_id"));

    let protocol: Value =
        serde_json::from_slice(&fs::read(root.join("fixtures/protocol-v2-before.json")).unwrap())
            .unwrap();
    assert_eq!(protocol["version"], 2);
    assert_eq!(protocol["params"]["mutation"]["type"], "select_source");

    let webui: Value =
        serde_json::from_slice(&fs::read(root.join("fixtures/webui-before.json")).unwrap())
            .unwrap();
    assert_eq!(webui["valid_behaviors"].as_array().unwrap().len(), 4);
    assert_eq!(webui["known_defects"].as_array().unwrap().len(), 4);
}

#[test]
fn a007_evidence_contract_requires_tdd_steps_and_forbids_secrets() {
    let manifest: Value = serde_json::from_slice(
        &fs::read(baseline_root().join("baseline-manifest-v1.json")).unwrap(),
    )
    .unwrap();
    let fields = manifest["evidence_fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect::<BTreeSet<_>>();
    for required in [
        "task_id",
        "fixture_sha256",
        "red",
        "green",
        "refactor",
        "verify",
    ] {
        assert!(fields.contains(required));
    }
    let forbidden = manifest["forbidden_evidence_fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert!(forbidden.contains("subscription_url"));
    assert!(forbidden.contains("api_secret"));
    assert!(fields.is_disjoint(&forbidden));
}
