use std::collections::BTreeMap;

use nethop_core::{
    Candidate, CaptureMode, CapturePolicy, CapturePolicyError, CoreDiagnosticCode, CoreError,
    GenerationId, GenerationStore, ManagedConfig, RuntimeState, StateTransitionError,
    TerminalOutbound,
};
use serde_json::json;

fn outbound(tag: &str) -> TerminalOutbound {
    TerminalOutbound::new(
        tag,
        "vless",
        BTreeMap::from([
            ("server".to_owned(), json!("example.com")),
            ("server_port".to_owned(), json!(443)),
        ]),
    )
    .expect("fixture outbound is valid")
}

#[test]
fn runtime_state_allows_only_declared_lifecycle_edges() {
    assert_eq!(
        RuntimeState::Init
            .transition(RuntimeState::Probing)
            .unwrap(),
        RuntimeState::Probing
    );
    assert_eq!(
        RuntimeState::RunningTproxy
            .transition(RuntimeState::RunningTun)
            .unwrap_err(),
        StateTransitionError::Invalid {
            from: RuntimeState::RunningTproxy,
            to: RuntimeState::RunningTun,
        }
    );
    assert_eq!(
        RuntimeState::RunningTproxy
            .transition(RuntimeState::Stopping)
            .unwrap(),
        RuntimeState::Stopping
    );
    assert_eq!(
        RuntimeState::Degraded
            .transition(RuntimeState::FailOpenDirect)
            .unwrap(),
        RuntimeState::FailOpenDirect
    );
    assert_eq!(
        RuntimeState::FailOpenDirect
            .transition(RuntimeState::CircuitOpen)
            .unwrap(),
        RuntimeState::CircuitOpen
    );
    assert!(
        RuntimeState::CircuitOpen
            .transition(RuntimeState::Probing)
            .is_err()
    );
}

#[test]
fn composer_generates_nodes_only_config_with_deterministic_bytes() {
    let config_a =
        ManagedConfig::from_outbounds(vec![outbound("node-b"), outbound("node-a")]).unwrap();
    let config_b =
        ManagedConfig::from_outbounds(vec![outbound("node-a"), outbound("node-b")]).unwrap();

    assert_eq!(config_a.bytes(), config_b.bytes());
    assert_eq!(config_a.node_count(), 2);
    let value: serde_json::Value = serde_json::from_slice(config_a.bytes()).unwrap();
    assert!(value.get("inbounds").is_none());
    assert!(value.get("route").is_none());
    assert_eq!(value["outbounds"].as_array().unwrap().len(), 2);
}

#[test]
fn composer_rejects_reserved_top_level_semantics() {
    let result = TerminalOutbound::new(
        "node-a",
        "vless",
        BTreeMap::from([("inbounds".to_owned(), json!([]))]),
    );
    assert_eq!(
        result.unwrap_err(),
        nethop_core::ComposerError::ReservedField("inbounds".into())
    );
}

#[test]
fn generation_store_keeps_previous_generation_when_validation_fails() {
    let directory = tempfile::tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    let first = Candidate::new(
        GenerationId::new(1).unwrap(),
        ManagedConfig::from_outbounds(vec![outbound("one")]).unwrap(),
    );
    store.publish(&first, |_| Ok(())).unwrap();

    let second = Candidate::new(
        GenerationId::new(2).unwrap(),
        ManagedConfig::from_outbounds(vec![outbound("two")]).unwrap(),
    );
    let error = store
        .publish(&second, |_| Err(CoreError::ValidationFailed))
        .unwrap_err();

    assert_eq!(error, CoreError::ValidationFailed);
    assert_eq!(
        store.current_generation().unwrap(),
        Some(GenerationId::new(1).unwrap())
    );
    assert!(!directory.path().join("generations/2").exists());
}

#[test]
fn generation_store_publishes_manifest_and_current_pointer_atomically() {
    let directory = tempfile::tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    let config = ManagedConfig::from_outbounds(vec![outbound("one")]).unwrap();
    let candidate = Candidate::new(GenerationId::new(7).unwrap(), config.clone());

    store
        .publish(&candidate, |bytes| {
            assert_eq!(bytes, config.bytes());
            Ok(())
        })
        .unwrap();

    let generation = directory.path().join("generations/7");
    assert_eq!(
        std::fs::read_to_string(directory.path().join("current")).unwrap(),
        "7\n"
    );
    assert_eq!(
        std::fs::read(generation.join("config.json")).unwrap(),
        config.bytes()
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(generation.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["generation"], 7);
    assert_eq!(manifest["node_count"], 1);
}

#[test]
fn generation_lifecycle_does_not_activate_before_explicit_commit() {
    let directory = tempfile::tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    let candidate = Candidate::new(
        GenerationId::new(11).unwrap(),
        ManagedConfig::from_outbounds(vec![outbound("eleven")]).unwrap(),
    );

    let prepared = store.prepare_candidate(&candidate).unwrap();
    assert!(prepared.config_path().is_file());
    assert_eq!(store.current_generation().unwrap(), None);

    let sealed = store.seal_candidate(&prepared).unwrap();
    assert!(sealed.config_path().is_file());
    assert_eq!(store.current_generation().unwrap(), None);

    store.commit_generation(&sealed).unwrap();
    assert_eq!(
        store.current_generation().unwrap(),
        Some(candidate.generation())
    );
}

#[test]
fn generation_discard_and_rollback_preserve_a_valid_active_target() {
    let directory = tempfile::tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    let first = Candidate::new(
        GenerationId::new(1).unwrap(),
        ManagedConfig::from_outbounds(vec![outbound("one")]).unwrap(),
    );
    store.publish(&first, |_| Ok(())).unwrap();

    let second = Candidate::new(
        GenerationId::new(2).unwrap(),
        ManagedConfig::from_outbounds(vec![outbound("two")]).unwrap(),
    );
    let prepared = store.prepare_candidate(&second).unwrap();
    store.discard_prepared(prepared).unwrap();
    assert!(!directory.path().join("generations/2").exists());

    let prepared = store.prepare_candidate(&second).unwrap();
    let sealed = store.seal_candidate(&prepared).unwrap();
    store.commit_generation(&sealed).unwrap();
    store.rollback_to(first.generation()).unwrap();
    assert_eq!(
        store.current_generation().unwrap(),
        Some(first.generation())
    );
    store.discard_sealed(sealed).unwrap();
    assert!(!directory.path().join("generations/2").exists());
}

#[test]
fn rollback_rejects_a_generation_whose_config_no_longer_matches_manifest() {
    let directory = tempfile::tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    let first = Candidate::new(
        GenerationId::new(1).unwrap(),
        ManagedConfig::from_outbounds(vec![outbound("one")]).unwrap(),
    );
    store.publish(&first, |_| Ok(())).unwrap();
    std::fs::write(
        directory.path().join("generations/1/config.json"),
        b"{\"outbounds\":[]}",
    )
    .unwrap();

    let error = store.rollback_to(first.generation()).unwrap_err();
    assert_eq!(error.code(), CoreDiagnosticCode::GenerationPublishFailed);
}

#[test]
fn generation_id_zero_is_rejected() {
    assert_eq!(
        GenerationId::new(0).unwrap_err(),
        CoreError::InvalidGenerationId
    );
    assert_eq!(
        CoreDiagnosticCode::GenerationPublishFailed.as_str(),
        "generation_publish_failed"
    );
}

#[test]
fn capture_policy_is_shared_and_deterministic_for_uid_selection() {
    let policy = CapturePolicy::new(
        CaptureMode::Tproxy,
        true,
        Some(12345),
        Some(0x4e48),
        vec![1002, 1001, 1002],
        vec![1003],
    )
    .unwrap();
    assert_eq!(policy.include_uids(), [1001, 1002]);
    assert!(policy.captures_uid(1001));
    assert!(!policy.captures_uid(1003));
    assert!(!policy.captures_uid(1004));
}

#[test]
fn capture_policy_rejects_missing_tproxy_primitives_and_overlap() {
    assert_eq!(
        CapturePolicy::new(CaptureMode::Tproxy, true, Some(12345), None, vec![], vec![])
            .unwrap_err(),
        CapturePolicyError::MissingTproxyMark
    );
    assert_eq!(
        CapturePolicy::new(
            CaptureMode::Tproxy,
            true,
            Some(12345),
            Some(1),
            vec![1001],
            vec![1001]
        )
        .unwrap_err(),
        CapturePolicyError::OverlappingUidPolicy
    );
}
