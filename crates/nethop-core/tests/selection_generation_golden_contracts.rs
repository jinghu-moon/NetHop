use std::{collections::BTreeMap, fs};

use nethop_core::{
    Candidate, CaptureMode, CapturePolicy, ClashApi, GenerationId, GenerationNodeRecord,
    GenerationNodeRegistry, GenerationStore, ManagedConfig, ManagedLogLevel, ManagedOptions,
    ManagedOutboundMode, ManagedProfile, TerminalOutbound,
};
use serde_json::{Value, json};
use tempfile::tempdir;

const SINGLE: &str =
    include_str!("../../../tests/subscription-selection/fixtures/generation-single-v1.json");
const MERGE: &str =
    include_str!("../../../tests/subscription-selection/fixtures/generation-merge-v1.json");

fn outbound(tag: &str, port: u16) -> TerminalOutbound {
    TerminalOutbound::new(
        tag,
        "vless",
        BTreeMap::from([
            ("server".into(), json!("example.com")),
            ("server_port".into(), json!(port)),
            ("uuid".into(), json!("00000000-0000-4000-8000-000000000001")),
        ]),
    )
    .unwrap()
}

fn projection(auto_pool: Vec<String>, source_ids: Vec<String>) -> Value {
    let profile = ManagedProfile::new(
        CapturePolicy::new(CaptureMode::Direct, false, None, None, vec![], vec![]).unwrap(),
        vec![
            outbound("nh1s-1111111111111111", 443),
            outbound("nh1s-2222222222222222", 8443),
        ],
        auto_pool.clone(),
        ClashApi::new("127.0.0.1:19090", "fixture-secret-32-bytes-long-000").unwrap(),
    )
    .unwrap()
    .with_options(
        ManagedOptions::new(
            ManagedOutboundMode::Direct,
            10,
            50,
            64,
            ManagedLogLevel::Warn,
            true,
            false,
            vec![],
            vec![],
        )
        .unwrap(),
    );
    let config = ManagedConfig::from_profile(profile).unwrap();
    let registry = GenerationNodeRegistry::new(
        ["nh1s-1111111111111111", "nh1s-2222222222222222"]
            .into_iter()
            .map(|id| {
                GenerationNodeRecord::new(
                    id,
                    id,
                    id,
                    "vless",
                    source_ids.clone(),
                    auto_pool.iter().any(|candidate| candidate == id),
                )
                .unwrap()
            })
            .collect(),
    )
    .unwrap();
    let candidate = Candidate::new(GenerationId::new(1).unwrap(), config)
        .with_node_registry(registry)
        .unwrap()
        .bind_sources("a".repeat(64), source_ids.clone())
        .unwrap();
    let directory = tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    let prepared = store.prepare_candidate(&candidate).unwrap();
    let config: Value = serde_json::from_slice(&fs::read(prepared.config_path()).unwrap()).unwrap();
    let registry: Value =
        serde_json::from_slice(&fs::read(prepared.node_registry_path()).unwrap()).unwrap();
    let manifest: Value = serde_json::from_slice(
        &fs::read(prepared.config_path().with_file_name("manifest.json")).unwrap(),
    )
    .unwrap();
    let outbounds = config["outbounds"].as_array().unwrap();
    let group = |tag: &str| {
        outbounds
            .iter()
            .find(|outbound| outbound["tag"] == tag)
            .unwrap()
    };
    let registry_auto = registry["records"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|record| record["auto_candidate"] == true)
        .map(|record| record["stable_node_id"].clone())
        .collect::<Vec<_>>();
    json!({
        "auto_pool": group("nethop-auto")["outbounds"],
        "selector": group("nethop-select")["outbounds"],
        "default": group("nethop-select")["default"],
        "interrupt_exist_connections": group("nethop-select")["interrupt_exist_connections"],
        "registry_auto": registry_auto,
        "node_count": manifest["node_count"],
        "source_ids": manifest["source_ids"],
    })
}

#[test]
fn single_generation_projection_matches_golden() {
    let expected: Value = serde_json::from_str(SINGLE).unwrap();
    assert_eq!(
        projection(
            vec!["nh1s-1111111111111111".into()],
            vec!["src_11111111111111111111111111111111".into()],
        ),
        expected
    );
}

#[test]
fn merge_generation_projection_matches_golden() {
    let expected: Value = serde_json::from_str(MERGE).unwrap();
    assert_eq!(
        projection(
            vec![
                "nh1s-1111111111111111".into(),
                "nh1s-2222222222222222".into(),
            ],
            vec![
                "src_11111111111111111111111111111111".into(),
                "src_22222222222222222222222222222222".into(),
            ],
        ),
        expected
    );
}
