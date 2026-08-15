use nethop_core::DisplayTerritoryCode;
use nethop_subscription::SourceId;
use nethopd::{
    ActiveTerminalSnapshot, NodeListItem, NodeListSnapshot, NodeSelectionIntent,
    NodeSelectionSnapshot, SelectionDiagnosticCode, StableNodeId,
};

fn node_id(value: &str) -> StableNodeId {
    StableNodeId::new(value).unwrap()
}

#[test]
fn stable_node_id_accepts_only_parser_display_fingerprint_shape() {
    assert_eq!(
        node_id("nh1s-0123456789abcdef").as_str(),
        "nh1s-0123456789abcdef"
    );
    for invalid in [
        "",
        "daemon-private",
        "nh1s-0123456789ABCDEf",
        "nh1s-0123456789abcde",
        "nh1s-0123456789abcdef0",
        "nh1s-0123456789abcdeg",
    ] {
        assert!(StableNodeId::new(invalid).is_err(), "accepted {invalid}");
    }
}

#[test]
fn selection_snapshot_keeps_intent_separate_from_active_terminal() {
    let active = node_id("nh1s-0123456789abcdef");
    let automatic = NodeSelectionSnapshot::new(
        NodeSelectionIntent::Auto,
        ActiveTerminalSnapshot::Node {
            node_id: active.clone(),
        },
        1_786_200_000,
    );
    assert!(automatic.validate().is_ok());
    assert_eq!(automatic.active_node_id(), Some(&active));
    assert!(matches!(automatic.intent(), NodeSelectionIntent::Auto));

    let requested = node_id("nh1s-fedcba9876543210");
    let manual = NodeSelectionSnapshot::new(
        NodeSelectionIntent::Manual {
            node_id: requested.clone(),
        },
        ActiveTerminalSnapshot::Node { node_id: active },
        1_786_200_001,
    );
    assert!(matches!(
        manual.intent(),
        NodeSelectionIntent::Manual { node_id } if node_id == &requested
    ));
    assert_ne!(manual.active_node_id(), Some(&requested));

    let encoded = serde_json::to_vec(&manual).unwrap();
    assert_eq!(
        serde_json::from_slice::<NodeSelectionSnapshot>(&encoded).unwrap(),
        manual
    );
}

#[test]
fn node_list_has_explicit_requested_and_active_flags_without_group_nodes() {
    let source = SourceId::new("src_11111111111111111111111111111111").unwrap();
    let id = node_id("nh1s-0123456789abcdef");
    let item = NodeListItem::new(
        id.clone(),
        "Tokyo",
        "vless",
        vec![source],
        Some(42),
        Some(true),
        false,
        true,
        DisplayTerritoryCode::new("JP"),
    )
    .unwrap();
    let list = NodeListSnapshot::new(
        vec![item],
        NodeSelectionSnapshot::new(
            NodeSelectionIntent::Auto,
            ActiveTerminalSnapshot::Node { node_id: id },
            1,
        ),
    );
    let value = serde_json::to_value(&list).unwrap();
    assert!(value["nodes"][0].get("selected").is_none());
    assert_eq!(value["nodes"][0]["is_requested"], false);
    assert_eq!(value["nodes"][0]["is_active"], true);
    assert_eq!(value["nodes"][0]["display_territory_code"], "JP");
    assert_eq!(value["selection"]["intent"]["mode"], "auto");
    assert_eq!(value["selection"]["version"], 2);
    assert_eq!(value["selection"]["active_terminal"]["kind"], "node");
}

#[test]
fn selection_diagnostic_codes_are_complete_and_stable() {
    let codes = [
        SelectionDiagnosticCode::SubscriptionModeMismatch,
        SelectionDiagnosticCode::SingleSourceNotUnique,
        SelectionDiagnosticCode::NoActiveSource,
        SelectionDiagnosticCode::LastActiveSource,
        SelectionDiagnosticCode::TargetNotReady,
        SelectionDiagnosticCode::ModeTargetRequired,
        SelectionDiagnosticCode::NodeSelectionStale,
        SelectionDiagnosticCode::ActiveNodeUnresolved,
        SelectionDiagnosticCode::NodeTestPartial,
    ];
    let encoded = codes
        .iter()
        .map(|code| serde_json::to_value(code).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(encoded.len(), 9);
    assert_eq!(encoded[0], "NH-SUB-MODE-MISMATCH");
    assert_eq!(encoded[8], "NH-NODE-TEST-PARTIAL");
}
