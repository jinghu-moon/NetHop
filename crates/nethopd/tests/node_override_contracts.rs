#![cfg(feature = "subscription-update")]

use std::fs;

use nethop_core::{CaptureMode, CapturePolicy, ClashApi, GenerationId, ManagedOptions, TunStack};
use nethop_subscription::{
    CapabilityMatrix, FormatHint, ParserLimits, SourceId, SourceInput, convert_stable_sources,
};
use nethopd::{
    CandidateBuildProfile, NodeOverride, NodeOverrideError, NodeOverrideSet, NodeOverrideStore,
    StableNodeId, SubscriptionMode, build_candidate_with_overrides,
};
use serde_json::json;
use tempfile::tempdir;

fn node_id() -> StableNodeId {
    StableNodeId::new("nh1s-0123456789abcdef").unwrap()
}

fn valid_override() -> NodeOverride {
    NodeOverride::new(
        node_id(),
        "编辑后的东京节点",
        json!({
            "type": "trojan",
            "server": "edge.example.com",
            "server_port": 443,
            "password": "private-password",
            "tls": {"enabled": true, "server_name": "edge.example.com"}
        }),
    )
    .unwrap()
}

#[test]
fn override_keeps_original_stable_id_and_redacts_credentials_from_debug() {
    let value = valid_override();
    assert_eq!(value.node_id(), &node_id());
    assert_eq!(value.protocol(), "trojan");
    assert_eq!(value.terminal_outbound().unwrap().tag(), node_id().as_str());
    assert!(!format!("{value:?}").contains("private-password"));
}

#[test]
fn override_rejects_group_protocol_detour_and_mismatched_tag() {
    for outbound in [
        json!({"type":"selector","server":"edge.example.com","server_port":443}),
        json!({"type":"trojan","server":"edge.example.com","server_port":443,"password":"x","detour":"direct"}),
        json!({"tag":"nh1s-fedcba9876543210","type":"trojan","server":"edge.example.com","server_port":443,"password":"x"}),
    ] {
        assert_eq!(
            NodeOverride::new(node_id(), "node", outbound).unwrap_err(),
            NodeOverrideError::InvalidOutbound
        );
    }
}

#[test]
fn private_store_round_trips_and_rejects_unknown_schema_fields() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("node-overrides.json");
    let store = NodeOverrideStore::new(path.clone()).unwrap();
    let mut values = NodeOverrideSet::default();
    values.upsert(valid_override()).unwrap();

    store.replace(&values).unwrap();
    let loaded = store.load().unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(
        loaded.get(&node_id()).unwrap().display_name(),
        "编辑后的东京节点"
    );
    assert!(!String::from_utf8_lossy(&fs::read(&path).unwrap()).contains("tag"));

    fs::write(
        &path,
        br#"{"schema":"nethop-node-overrides-v1","entries":[],"unexpected":true}"#,
    )
    .unwrap();
    assert_eq!(store.load().unwrap_err(), NodeOverrideError::InvalidFile);
}

#[test]
fn candidate_replays_override_without_changing_identity_or_source_attribution() {
    let source_id = SourceId::new("src_11111111111111111111111111111111").unwrap();
    let conversion = convert_stable_sources(
        vec![SourceInput {
            source_id: source_id.clone(),
            format_hint: FormatHint::UriList,
            bytes: b"trojan://old-secret@old.example.com:443#Tokyo".to_vec(),
        }],
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    );
    let stable_id = StableNodeId::new(conversion.nodes[0].node_id.as_str()).unwrap();
    let mut overrides = NodeOverrideSet::default();
    overrides
        .upsert(
            NodeOverride::new(
                stable_id.clone(),
                "日本 东京高级线路",
                json!({
                    "type": "trojan",
                    "server": "new.example.com",
                    "server_port": 8443,
                    "password": "new-secret",
                    "tls": {"enabled": true, "server_name": "new.example.com"}
                }),
            )
            .unwrap(),
        )
        .unwrap();

    let candidate = build_candidate_with_overrides(
        GenerationId::new(1).unwrap(),
        &conversion,
        CandidateBuildProfile::new(
            CapturePolicy::new(CaptureMode::Direct, false, None, None, vec![], vec![]).unwrap(),
            ClashApi::new("127.0.0.1:9090", "x".repeat(32)).unwrap(),
            TunStack::System,
            ManagedOptions::default(),
        ),
        SubscriptionMode::Single,
        std::slice::from_ref(&source_id),
        &overrides,
    )
    .unwrap();

    let config: serde_json::Value = serde_json::from_slice(candidate.config().bytes()).unwrap();
    let terminal = config["outbounds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["tag"] == stable_id.as_str())
        .unwrap();
    assert_eq!(terminal["server"], "new.example.com");
    assert_eq!(terminal["server_port"], 8443);
    let record = candidate
        .node_registry()
        .unwrap()
        .records()
        .first()
        .unwrap();
    assert_eq!(record.stable_node_id(), stable_id.as_str());
    assert_eq!(record.display_name(), "日本 东京高级线路");
    assert_eq!(record.source_ids(), [source_id.as_str().to_owned()]);
    assert_eq!(record.display_territory_code().unwrap().as_str(), "JP");
}
