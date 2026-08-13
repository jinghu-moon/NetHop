use std::collections::BTreeMap;

use nethop_core::{
    Candidate, CaptureMode, CapturePolicy, ClashApi, GenerationId, GenerationNodeRecord,
    GenerationNodeRegistry, GenerationStore, ManagedConfig, ManagedProfile, TerminalOutbound,
};

fn outbound(tag: &str) -> TerminalOutbound {
    TerminalOutbound::new(tag, "trojan", BTreeMap::new()).unwrap()
}

fn record(id: &str, auto_candidate: bool) -> GenerationNodeRecord {
    GenerationNodeRecord::new(
        id,
        id,
        format!("Node {id}"),
        "trojan",
        vec!["src_11111111111111111111111111111111".into()],
        auto_candidate,
    )
    .unwrap()
}

fn managed_config() -> ManagedConfig {
    let capture =
        CapturePolicy::new(CaptureMode::Direct, false, None, None, vec![], vec![]).unwrap();
    let profile = ManagedProfile::new(
        capture,
        vec![
            outbound("nh1s-1111111111111111"),
            outbound("nh1s-2222222222222222"),
        ],
        vec!["nh1s-2222222222222222".into()],
        ClashApi::new("127.0.0.1:9090", "a".repeat(32)).unwrap(),
    )
    .unwrap();
    ManagedConfig::from_profile(profile).unwrap()
}

fn candidate() -> Candidate {
    Candidate::new(GenerationId::new(1).unwrap(), managed_config())
        .with_node_registry(
            GenerationNodeRegistry::new(vec![
                record("nh1s-1111111111111111", false),
                record("nh1s-2222222222222222", true),
            ])
            .unwrap(),
        )
        .unwrap()
}

#[test]
fn composer_uses_explicit_auto_pool_but_keeps_every_terminal_selectable() {
    let value: serde_json::Value = serde_json::from_slice(managed_config().bytes()).unwrap();
    let selector = &value["outbounds"][2];
    assert_eq!(selector["default"], "nh1s-2222222222222222");
    assert_eq!(
        selector["outbounds"],
        serde_json::json!(["nh1s-1111111111111111", "nh1s-2222222222222222"])
    );
    assert_eq!(selector["interrupt_exist_connections"], false);
}

#[test]
fn registry_is_bounded_strict_and_has_bidirectional_lookup() {
    let candidate = candidate();
    let registry = candidate.node_registry().unwrap();
    assert_eq!(registry.records().len(), 2);
    assert_eq!(registry.auto_pool(), ["nh1s-2222222222222222"]);
    assert_eq!(
        registry
            .by_stable_id("nh1s-2222222222222222")
            .unwrap()
            .internal_tag(),
        "nh1s-2222222222222222"
    );
    assert!(
        registry
            .by_internal_tag("nh1s-2222222222222222")
            .unwrap()
            .auto_candidate()
    );
    let mut value = serde_json::to_value(registry).unwrap();
    value["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<GenerationNodeRegistry>(value).is_err());
    assert!(
        GenerationNodeRecord::new(
            "nh1s-1111111111111111",
            "tag",
            "x".repeat(129),
            "trojan",
            vec!["src_11111111111111111111111111111111".into()],
            false,
        )
        .is_err()
    );
}

#[test]
fn generation_manifest_seals_and_verifies_registry_digest() {
    let directory = tempfile::tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    let candidate = candidate();
    store.publish(&candidate, |_| Ok(())).unwrap();
    let generation = directory.path().join("generations/1");
    assert!(generation.join("nodes.json").is_file());
    assert!(candidate.manifest().node_registry_sha256.is_some());
    assert_eq!(
        store
            .read_node_registry(candidate.generation())
            .unwrap()
            .records()
            .len(),
        2
    );

    std::fs::write(generation.join("nodes.json"), b"{}").unwrap();
    assert!(store.verify_generation(candidate.generation()).is_err());
}

#[test]
fn missing_registry_is_rejected_when_manifest_requires_it() {
    let directory = tempfile::tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    let candidate = candidate();
    store.publish(&candidate, |_| Ok(())).unwrap();
    std::fs::remove_file(directory.path().join("generations/1/nodes.json")).unwrap();
    assert!(store.verify_generation(candidate.generation()).is_err());
}

#[cfg(unix)]
#[test]
fn symlink_or_public_registry_is_rejected() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    for symlink_case in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let store = GenerationStore::new(directory.path()).unwrap();
        let candidate = candidate();
        store.publish(&candidate, |_| Ok(())).unwrap();
        let registry = directory.path().join("generations/1/nodes.json");
        if symlink_case {
            let replacement = directory.path().join("replacement.json");
            std::fs::copy(&registry, &replacement).unwrap();
            std::fs::remove_file(&registry).unwrap();
            symlink(replacement, &registry).unwrap();
        } else {
            std::fs::set_permissions(&registry, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        assert!(store.verify_generation(candidate.generation()).is_err());
    }
}
