use std::{
    fs,
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddrV4, TcpListener},
    thread,
    time::Duration,
};

use nethop_core::{CaptureMode, CapturePolicy, RuntimeState};
use nethop_protocol::{ControlMethod, ControlParams};
use nethopd::{ClashApiClient, ClashApiLimits, OperationalControl, ReplayResult, SelectorStore};
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
fn selector_store_is_strict_bounded_and_replaces_state() {
    let directory = tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let path = root.join("selector.v1.json");
    let store = SelectorStore::new(&path).unwrap();
    assert_eq!(store.load().unwrap(), None);
    store.save("nh1s-a").unwrap();
    assert_eq!(store.load().unwrap().as_deref(), Some("nh1s-a"));
    store.save("nh1s-b").unwrap();
    assert_eq!(store.load().unwrap().as_deref(), Some("nh1s-b"));

    fs::write(&path, r#"{"schema_version":2,"selected_tag":"nh1s-a"}"#).unwrap();
    assert!(store.load().is_err());
    assert!(SelectorStore::new("relative.json").is_err());
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
        SelectorStore::new(root.join("selector.v1.json")).unwrap(),
        root.join("diagnostics-latest.json"),
    )
    .unwrap();
    let status = control.status_document();
    assert_eq!(status["core_api"], "available");
    assert_eq!(status["selector"]["selected"], "nh1s-0123456789abcdef");
    assert_eq!(status["selector"]["candidate_count"], 1);
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
        SelectorStore::new(root.join("selector.v1.json")).unwrap(),
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
    let store = SelectorStore::new(root.join("selector.v1.json")).unwrap();
    store.save("nh1s-0123456789abcdef").unwrap();
    let (address, server) = serve(vec![
        (
            200,
            selector_document("nethop-auto", &["nethop-auto", "nh1s-0123456789abcdef"]),
        ),
        (204, String::new()),
    ]);
    let mut control =
        OperationalControl::new(api(address), store, root.join("diagnostics-latest.json")).unwrap();
    assert_eq!(control.replay_selection().unwrap(), ReplayResult::Restored);
    let requests = server.join().unwrap();
    assert!(requests[1].ends_with(r#"{"name":"nh1s-0123456789abcdef"}"#));
}

#[test]
fn replay_falls_back_to_auto_when_a_node_disappears() {
    let directory = tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let state_path = root.join("selector.v1.json");
    let store = SelectorStore::new(&state_path).unwrap();
    store.save("nh1s-gone").unwrap();
    let selector = selector_document(
        "nh1s-0123456789abcdef",
        &["nethop-auto", "nh1s-0123456789abcdef"],
    );
    let (address, server) = serve(vec![(200, selector), (204, String::new())]);
    let mut control =
        OperationalControl::new(api(address), store, root.join("diagnostics-latest.json")).unwrap();
    assert_eq!(
        control.replay_selection().unwrap(),
        ReplayResult::FellBackToAuto
    );
    assert_eq!(
        SelectorStore::new(state_path)
            .unwrap()
            .load()
            .unwrap()
            .as_deref(),
        Some("nethop-auto")
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
        SelectorStore::new(root.join("selector.v1.json")).unwrap(),
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
        SelectorStore::new(root.join("selector.v1.json")).unwrap(),
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
    let (address, server) = serve(Vec::new());
    let mut control = OperationalControl::new(
        api(address),
        SelectorStore::new(root.join("selector.v1.json")).unwrap(),
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
