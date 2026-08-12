use std::{
    fs,
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddrV4, TcpListener},
    thread,
    time::Duration,
};

use nethop_core::{
    CaptureMode, CapturePolicy, GenerationNodeRecord, GenerationNodeRegistry, RuntimeState,
};
use nethop_protocol::{ControlMethod, ControlParams};
use nethopd::{
    ClashApiClient, ClashApiError, ClashApiLimits, NodeSelectionIntent, NodeSelectionStore,
    OperationalControl, OperationalControlError, ReplayResult, SelectionModelError, StableNodeId,
};
use tempfile::tempdir;

const SECRET: &str = "0123456789abcdef0123456789abcdef";

fn serve(responses: Vec<(u16, String)>) -> (SocketAddrV4, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = match listener.local_addr().unwrap() {
        std::net::SocketAddr::V4(address) => address,
        _ => unreachable!(),
    };
    let handle = thread::spawn(move || {
        let mut requests = Vec::new();
        for (status, body) in responses {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0_u8; 2048];
            loop {
                let count = stream.read(&mut buffer).unwrap();
                if count == 0 {
                    break;
                }
                bytes.extend_from_slice(&buffer[..count]);
                if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..end + 4]);
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().ok())
                                .flatten()
                        })
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            requests.push(String::from_utf8(bytes).unwrap());
            let reason = if status == 200 { "OK" } else { "No Content" };
            write!(
                stream,
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
        requests
    });
    (address, handle)
}

fn api(address: SocketAddrV4) -> ClashApiClient {
    ClashApiClient::new(address, SECRET, ClashApiLimits::default()).unwrap()
}

fn selector_document(now: &str, members: &[&str]) -> String {
    let mut proxies = serde_json::Map::new();
    proxies.insert(
        "nethop-select".to_owned(),
        serde_json::json!({"type":"Selector","now":now,"all":members}),
    );
    for member in members {
        proxies.insert(
            (*member).to_owned(),
            serde_json::json!({"type": if *member == "nethop-auto" {"URLTest"} else {"VLESS"}}),
        );
    }
    serde_json::json!({"proxies":proxies}).to_string()
}

#[test]
fn operational_failures_have_stable_node_and_core_diagnostics() {
    use nethop_protocol::ErrorDomain;

    let cases = [
        (
            OperationalControlError::UnknownNode,
            ErrorDomain::Node,
            "SELECTION-STALE",
        ),
        (
            OperationalControlError::GenerationUnavailable,
            ErrorDomain::Node,
            "ACTIVE-UNRESOLVED",
        ),
        (
            OperationalControlError::Selection(SelectionModelError::InvalidNodeId),
            ErrorDomain::Node,
            "INVALID-ID",
        ),
        (
            OperationalControlError::ClashApi(ClashApiError::Unavailable),
            ErrorDomain::Core,
            "CONTROL-UNAVAILABLE",
        ),
        (
            OperationalControlError::ClashApi(ClashApiError::Rejected),
            ErrorDomain::Core,
            "CONTROL-REJECTED",
        ),
        (
            OperationalControlError::ClashApi(ClashApiError::InvalidResponse),
            ErrorDomain::Core,
            "CONTROL-INVALID-RESPONSE",
        ),
    ];
    for (error, expected_domain, expected_detail) in cases {
        assert_eq!(
            error.control_diagnostic(),
            (expected_domain, expected_detail)
        );
    }
}

fn generation_root(root: &std::path::Path, tags: &[&str]) -> std::path::PathBuf {
    let generations = root.join("generations");
    fs::create_dir(&generations).unwrap();
    fs::create_dir(generations.join("7")).unwrap();
    fs::write(generations.join("current"), "7\n").unwrap();
    let records = tags
        .iter()
        .map(|tag| {
            GenerationNodeRecord::new(
                *tag,
                *tag,
                format!("Node {tag}"),
                "vless",
                vec!["src_0123456789abcdef0123456789abcdef".into()],
                false,
            )
            .unwrap()
        })
        .collect();
    let registry = GenerationNodeRegistry::new(records).unwrap();
    fs::write(
        generations.join("7/nodes.json"),
        serde_json::to_vec(&registry).unwrap(),
    )
    .unwrap();
    generations
}

#[test]
fn test_all_exposes_only_stable_ids_from_the_active_generation() {
    let directory = tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let first = "nh1s-0123456789abcdef";
    let second = "nh1s-fedcba9876543210";
    let (address, server) = serve(vec![
        (
            200,
            serde_json::json!({
                second: 87,
                "direct": 1,
                "nh1s-aaaaaaaaaaaaaaaa": 22,
                first: 42,
            })
            .to_string(),
        ),
        (
            200,
            selector_document("nethop-auto", &["nethop-auto", first, second]),
        ),
    ]);
    let generations = generation_root(&root, &[first, second]);
    let mut control = OperationalControl::new(
        api(address),
        NodeSelectionStore::new(root.join("selection.v1.json")).unwrap(),
        root.join("diagnostics-latest.json"),
    )
    .unwrap()
    .with_generation_root(generations)
    .unwrap();
    let policy = CapturePolicy::new(
        CaptureMode::Tproxy,
        true,
        Some(7893),
        Some(131_072),
        Vec::new(),
        vec![0],
    )
    .unwrap();

    let result = control
        .handle(
            ControlMethod::NodeTestAll,
            &ControlParams::default(),
            RuntimeState::RunningTproxy,
            None,
            &policy,
        )
        .unwrap();
    assert_eq!(
        result["results"],
        serde_json::json!([
            {"id": first, "latency_ms": 42},
            {"id": second, "latency_ms": 87},
        ])
    );
    assert_eq!(result["selection"]["intent"]["mode"], "auto");
    assert_eq!(server.join().unwrap().len(), 2);
}

#[test]
fn selector_store_is_strict_bounded_and_replaces_state() {
    let directory = tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let path = root.join("selection.v1.json");
    let store = NodeSelectionStore::new(&path).unwrap();
    assert_eq!(store.load().unwrap(), (NodeSelectionIntent::Auto, 0));
    let first = StableNodeId::new("nh1s-0123456789abcdef").unwrap();
    store
        .save(
            &NodeSelectionIntent::Manual {
                node_id: first.clone(),
            },
            1,
        )
        .unwrap();
    assert_eq!(
        store.load().unwrap(),
        (NodeSelectionIntent::Manual { node_id: first }, 1)
    );
    store.reset_auto(2).unwrap();
    assert_eq!(store.load().unwrap(), (NodeSelectionIntent::Auto, 2));

    fs::write(&path, r#"{"schema_version":3,"selected_tag":"nh1s-a"}"#).unwrap();
    assert!(store.load().is_err());
    assert!(NodeSelectionStore::new("relative.json").is_err());
}

#[test]
fn manager_operational_status_is_compact_and_secret_free() {
    let directory = tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let connections = serde_json::json!({
        "connections": [{
            "id":"id-1",
            "metadata":{"network":"tcp","host":"example.com","destinationPort":443,"password":"never"},
            "chains":["nethop-select","nh1s-0123456789abcdef"],
            "upload":1,
            "download":2
        }]
    })
    .to_string();
    let (address, server) = serve(vec![
        (
            200,
            selector_document(
                "nh1s-0123456789abcdef",
                &["nethop-auto", "nh1s-0123456789abcdef"],
            ),
        ),
        (200, connections),
    ]);
    let mut control = OperationalControl::new(
        api(address),
        NodeSelectionStore::new(root.join("selection.v1.json")).unwrap(),
        root.join("diagnostics-latest.json"),
    )
    .unwrap();
    let status = control.status_document();
    assert_eq!(status["core_api"], "available");
    assert!(status["selector"].get("selected").is_none());
    assert_eq!(status["selector"]["candidate_count"], 0);
    assert_eq!(status["active_connection_count"], 1);
    assert!(!serde_json::to_string(&status).unwrap().contains("never"));
    assert_eq!(server.join().unwrap().len(), 2);
}

#[test]
fn runtime_metrics_use_core_totals_without_external_public_ip_requests() {
    let directory = tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let (address, server) = serve(vec![(
        200,
        serde_json::json!({"connections":[],"uploadTotal":1234,"downloadTotal":5678}).to_string(),
    )]);
    let control = OperationalControl::new(
        api(address),
        NodeSelectionStore::new(root.join("selection.v1.json")).unwrap(),
        root.join("diagnostics-latest.json"),
    )
    .unwrap();
    let metrics = control.metrics_document(
        None,
        Duration::from_secs(90),
        RuntimeState::RunningTproxy,
        None,
    );
    assert_eq!(metrics["uptime_seconds"], 90);
    assert_eq!(metrics["traffic"]["upload_bytes"], 1234);
    assert_eq!(metrics["traffic"]["download_bytes"], 5678);
    assert!(metrics["outbound"]["public_ip"].is_null());
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].starts_with("GET /connections "));
}

#[test]
fn replay_restores_existing_selection() {
    let directory = tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let store = NodeSelectionStore::new(root.join("selection.v1.json")).unwrap();
    let node_id = StableNodeId::new("nh1s-0123456789abcdef").unwrap();
    store
        .save(&NodeSelectionIntent::Manual { node_id }, 1)
        .unwrap();
    let (address, server) = serve(vec![
        (
            200,
            selector_document("nethop-auto", &["nethop-auto", "nh1s-0123456789abcdef"]),
        ),
        (204, String::new()),
    ]);
    let generations = generation_root(&root, &["nh1s-0123456789abcdef"]);
    let mut control =
        OperationalControl::new(api(address), store, root.join("diagnostics-latest.json"))
            .unwrap()
            .with_generation_root(generations)
            .unwrap();
    assert_eq!(control.replay_selection().unwrap(), ReplayResult::Restored);
    let requests = server.join().unwrap();
    assert!(requests[1].ends_with(r#"{"name":"nh1s-0123456789abcdef"}"#));
}

#[test]
fn replay_falls_back_to_auto_when_a_node_disappears() {
    let directory = tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let state_path = root.join("selection.v1.json");
    let store = NodeSelectionStore::new(&state_path).unwrap();
    let gone = StableNodeId::new("nh1s-aaaaaaaaaaaaaaaa").unwrap();
    store
        .save(&NodeSelectionIntent::Manual { node_id: gone }, 1)
        .unwrap();
    let selector = selector_document(
        "nh1s-0123456789abcdef",
        &["nethop-auto", "nh1s-0123456789abcdef"],
    );
    let (address, server) = serve(vec![(200, selector), (204, String::new())]);
    let generations = generation_root(&root, &["nh1s-0123456789abcdef"]);
    let mut control =
        OperationalControl::new(api(address), store, root.join("diagnostics-latest.json"))
            .unwrap()
            .with_generation_root(generations)
            .unwrap();
    assert_eq!(
        control.replay_selection().unwrap(),
        ReplayResult::FellBackToAuto
    );
    assert_eq!(
        NodeSelectionStore::new(state_path)
            .unwrap()
            .load()
            .unwrap()
            .0,
        NodeSelectionIntent::Auto
    );
    let requests = server.join().unwrap();
    assert!(requests[1].ends_with(r#"{"name":"nethop-auto"}"#));
}

#[test]
fn diagnostics_bundle_is_compact_persisted_and_secret_free() {
    let directory = tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let path = root.join("diagnostics-latest.json");
    let (address, server) = serve(vec![
        (
            200,
            selector_document("nh1s-0123456789abcdef", &["nh1s-0123456789abcdef"]),
        ),
        (200, r#"{"connections":[]}"#.to_owned()),
    ]);
    let mut control = OperationalControl::new(
        api(address),
        NodeSelectionStore::new(root.join("selection.v1.json")).unwrap(),
        &path,
    )
    .unwrap();
    let policy = CapturePolicy::new(
        CaptureMode::Tproxy,
        true,
        Some(7893),
        Some(131_072),
        Vec::new(),
        vec![0],
    )
    .unwrap();
    let result = control
        .handle(
            ControlMethod::DiagnosticsBundle,
            &ControlParams::default(),
            RuntimeState::RunningTproxy,
            None,
            &policy,
        )
        .unwrap();
    assert_eq!(result["bundle"]["clash_api"]["available"], true);
    let persisted = fs::read_to_string(path).unwrap();
    assert!(persisted.contains("running_tproxy"));
    assert!(!persisted.contains(SECRET));
    server.join().unwrap();
}

#[test]
fn close_all_connections_is_exposed_as_one_typed_operation() {
    let directory = tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let (address, server) = serve(vec![(204, String::new())]);
    let mut control = OperationalControl::new(
        api(address),
        NodeSelectionStore::new(root.join("selection.v1.json")).unwrap(),
        root.join("diagnostics-latest.json"),
    )
    .unwrap();
    let policy = CapturePolicy::new(
        CaptureMode::Tproxy,
        true,
        Some(7893),
        Some(131_072),
        Vec::new(),
        vec![0],
    )
    .unwrap();
    let result = control
        .handle(
            ControlMethod::ConnectionsCloseAll,
            &ControlParams::default(),
            RuntimeState::RunningTproxy,
            None,
            &policy,
        )
        .unwrap();
    assert_eq!(result["closed_all"], true);
    server.join().unwrap();
}

#[test]
fn node_export_reads_only_the_active_generation_and_requires_an_exact_tag() {
    let directory = tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let generations = root.join("generations");
    fs::create_dir(&generations).unwrap();
    fs::create_dir(generations.join("7")).unwrap();
    fs::write(generations.join("current"), "7\n").unwrap();
    fs::write(
        generations.join("7/config.json"),
        serde_json::to_vec(&serde_json::json!({
            "outbounds":[
                {"type":"selector","tag":"nethop-select"},
                {"type":"trojan","tag":"nh1s-0123456789abcdef","server":"example.com","password":"explicit-export"}
            ]
        }))
        .unwrap(),
    )
    .unwrap();
    let registry = GenerationNodeRegistry::new(vec![
        GenerationNodeRecord::new(
            "nh1s-0123456789abcdef",
            "nh1s-0123456789abcdef",
            "Export node",
            "trojan",
            vec!["src_0123456789abcdef0123456789abcdef".into()],
            true,
        )
        .unwrap(),
    ])
    .unwrap();
    fs::write(
        generations.join("7/nodes.json"),
        serde_json::to_vec(&registry).unwrap(),
    )
    .unwrap();
    let (address, server) = serve(Vec::new());
    let mut control = OperationalControl::new(
        api(address),
        NodeSelectionStore::new(root.join("selection.v1.json")).unwrap(),
        root.join("diagnostics-latest.json"),
    )
    .unwrap()
    .with_generation_root(&generations)
    .unwrap();
    let policy = CapturePolicy::new(
        CaptureMode::Tproxy,
        true,
        Some(7893),
        Some(131_072),
        Vec::new(),
        vec![0],
    )
    .unwrap();
    let exported = control
        .handle(
            ControlMethod::NodeExport,
            &ControlParams::target("nh1s-0123456789abcdef".into()),
            RuntimeState::RunningTproxy,
            None,
            &policy,
        )
        .unwrap();
    assert_eq!(exported["generation"], 7);
    assert_eq!(exported["outbound"]["password"], "explicit-export");
    assert!(
        control
            .handle(
                ControlMethod::NodeExport,
                &ControlParams::target("nh1s-ffffffffffffffff".into()),
                RuntimeState::RunningTproxy,
                None,
                &policy,
            )
            .is_err()
    );
    server.join().unwrap();
}
