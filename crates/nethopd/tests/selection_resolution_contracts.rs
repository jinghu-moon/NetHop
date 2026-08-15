use std::{collections::BTreeMap, fs};

use nethop_core::{GenerationNodeRecord, GenerationNodeRegistry};
use nethopd::{
    ActiveTerminal, ActiveTerminalSnapshot, GroupState, NodeSelectionIntent, NodeSelectionStore,
    SelectionDiagnosticCode, StableNodeId, join_node_snapshot, resolve_active_terminal,
};
use tempfile::tempdir;

fn registry() -> GenerationNodeRegistry {
    GenerationNodeRegistry::new(vec![
        GenerationNodeRecord::new(
            "nh1s-0123456789abcdef",
            "internal-terminal",
            "Tokyo",
            "vless",
            vec!["src_0123456789abcdef0123456789abcdef".into()],
            true,
        )
        .unwrap(),
    ])
    .unwrap()
}

fn group(tag: &str, now: &str) -> GroupState {
    GroupState::new(tag, Some(now.into()), vec![now.into()]).unwrap()
}

#[test]
fn missing_store_defaults_to_auto_and_old_tag_schema_is_rejected() {
    let directory = tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let path = root.join("selection.v1.json");
    let store = NodeSelectionStore::new(&path).unwrap();
    assert_eq!(store.load().unwrap(), (NodeSelectionIntent::Auto, 0));
    fs::write(
        &path,
        r#"{"version":1,"intent":{"mode":"manual","selected_tag":"internal-terminal"},"changed_at":1}"#,
    )
    .unwrap();
    assert!(store.load().is_err());
}

#[test]
fn active_terminal_resolution_accepts_only_selector_to_terminal_or_builtin() {
    let registry = registry();
    let mut groups = BTreeMap::from([(
        "nethop-select".into(),
        group("nethop-select", "internal-terminal"),
    )]);
    assert_eq!(
        resolve_active_terminal("nethop-select", &groups, &registry),
        ActiveTerminal::Node(StableNodeId::new("nh1s-0123456789abcdef").unwrap())
    );
    groups.insert("nethop-select".into(), group("nethop-select", "direct"));
    assert_eq!(
        resolve_active_terminal("nethop-select", &groups, &registry),
        ActiveTerminal::Direct
    );
    groups.insert(
        "nethop-select".into(),
        group("nethop-select", "unknown-group"),
    );
    assert_eq!(
        resolve_active_terminal("nethop-select", &groups, &registry),
        ActiveTerminal::Unresolved(SelectionDiagnosticCode::ActiveNodeUnresolved)
    );
}

#[test]
fn unresolved_active_never_falls_back_to_the_first_registry_node() {
    let snapshot = join_node_snapshot(
        &registry(),
        NodeSelectionIntent::Auto,
        ActiveTerminal::Unresolved(SelectionDiagnosticCode::ActiveNodeUnresolved),
        1,
    )
    .unwrap();
    assert!(snapshot.selection().active_node_id().is_none());
    assert!(matches!(
        snapshot.selection().active_terminal(),
        ActiveTerminalSnapshot::Unresolved {
            reason: SelectionDiagnosticCode::ActiveNodeUnresolved
        }
    ));
    assert!(snapshot.nodes().iter().all(|node| !node.is_active()));
}
