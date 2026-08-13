use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddrV4, TcpListener},
    thread,
    time::Duration,
};

use nethopd::{ClashApiClient, ClashApiError, ClashApiLimits};

const SECRET: &str = "0123456789abcdef0123456789abcdef";

fn serve(responses: Vec<(u16, String)>) -> (SocketAddrV4, thread::JoinHandle<Vec<String>>) {
    serve_delayed(
        responses
            .into_iter()
            .map(|(status, body)| (status, body, Duration::ZERO))
            .collect(),
    )
}

fn serve_delayed(
    responses: Vec<(u16, String, Duration)>,
) -> (SocketAddrV4, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = match listener.local_addr().unwrap() {
        std::net::SocketAddr::V4(address) => address,
        _ => unreachable!(),
    };
    let handle = thread::spawn(move || {
        let mut requests = Vec::new();
        for (status, body, delay) in responses {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0_u8; 4096];
            loop {
                let count = stream.read(&mut buffer).unwrap();
                if count == 0 {
                    break;
                }
                bytes.extend_from_slice(&buffer[..count]);
                if let Some(header_end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..header_end + 4]);
                    let content_length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().ok())
                                .flatten()
                        })
                        .unwrap_or(0);
                    if bytes.len() >= header_end + 4 + content_length {
                        break;
                    }
                }
            }
            requests.push(String::from_utf8(bytes).unwrap());
            thread::sleep(delay);
            let reason = if status == 200 { "OK" } else { "Error" };
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

fn client(address: SocketAddrV4) -> ClashApiClient {
    ClashApiClient::new(address, SECRET, ClashApiLimits::default()).unwrap()
}

#[test]
fn client_is_loopback_only_and_redacts_its_secret() {
    assert!(matches!(
        ClashApiClient::new(
            SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 9090),
            SECRET,
            ClashApiLimits::default()
        ),
        Err(ClashApiError::InvalidEndpoint)
    ));
    let (address, server) = serve(Vec::new());
    let client = client(address);
    let debug = format!("{client:?}");
    assert!(debug.contains("[REDACTED]"));
    assert!(!debug.contains(SECRET));
    assert!(server.join().unwrap().is_empty());
}

#[test]
fn group_snapshot_uses_secret_and_never_serializes_proxy_credentials() {
    let body = serde_json::json!({
        "proxies": {
            "nethop-select": {"type":"Selector","now":"nh1s-fedcba9876543210","all":["direct","nh1s-0123456789abcdef","nh1s-fedcba9876543210"]},
            "nh1s-0123456789abcdef": {"type":"VLESS","alive":true,"history":[{"delay":42}]},
            "nh1s-fedcba9876543210": {"type":"Trojan","alive":false,"history":[]},
            "credential-shaped-field": {"type":"VLESS","password":"must-not-escape"}
        }
    })
    .to_string();
    let (address, server) = serve(vec![(200, body)]);
    let snapshot = client(address).group_snapshot().unwrap();
    assert_eq!(
        snapshot
            .terminal("nh1s-0123456789abcdef")
            .unwrap()
            .latency_ms(),
        Some(42)
    );
    let requests = server.join().unwrap();
    assert!(requests[0].starts_with("GET /proxies HTTP/1.1\r\n"));
    assert!(
        requests[0]
            .to_ascii_lowercase()
            .contains(&format!("authorization: bearer {SECRET}"))
    );
    assert!(!format!("{snapshot:?}").contains("must-not-escape"));
}

#[test]
fn group_snapshot_preserves_only_internal_group_relations_and_observations() {
    let body = serde_json::json!({
        "proxies": {
            "nethop-select": {"type":"Selector","now":"internal-node","all":["internal-node"]},
            "internal-node": {"type":"VLESS","alive":true,"history":[{"delay":41}]}
        }
    })
    .to_string();
    let (address, server) = serve(vec![(200, body)]);
    let snapshot = client(address).group_snapshot().unwrap();
    assert_eq!(
        snapshot.groups()["nethop-select"].now(),
        Some("internal-node")
    );
    let terminal = snapshot.terminal("internal-node").unwrap();
    assert_eq!(terminal.kind(), "VLESS");
    assert_eq!(terminal.latency_ms(), Some(41));
    assert_eq!(terminal.alive(), Some(true));
    assert!(format!("{snapshot:?}").find(SECRET).is_none());
    assert_eq!(server.join().unwrap().len(), 1);
}

#[test]
fn node_test_percent_encodes_the_path_and_parses_delay() {
    let (address, server) = serve(vec![(200, r#"{"delay":87}"#.to_owned())]);
    let delay = client(address).test_node("node / one").unwrap();
    assert_eq!(delay.delay_ms, 87);
    let requests = server.join().unwrap();
    assert!(requests[0].starts_with(
        "GET /proxies/node%20%2F%20one/delay?timeout=5000&url=http%3A%2F%2Fwww.gstatic.com%2Fgenerate_204 HTTP/1.1\r\n"
    ));
}

#[test]
fn node_selection_validates_membership_before_putting_selector() {
    let list = serde_json::json!({
        "proxies": {
            "nethop-select": {"now":"nh1s-0123456789abcdef","all":["nh1s-0123456789abcdef","nh1s-fedcba9876543210"]},
            "nh1s-0123456789abcdef": {"type":"VLESS"},
            "nh1s-fedcba9876543210": {"type":"Trojan"}
        }
    })
    .to_string();
    let (address, server) = serve(vec![(200, list), (204, String::new())]);
    client(address)
        .select_node("nh1s-fedcba9876543210")
        .unwrap();
    let requests = server.join().unwrap();
    assert!(requests[1].starts_with("PUT /proxies/nethop-select HTTP/1.1\r\n"));
    assert!(requests[1].ends_with(r#"{"name":"nh1s-fedcba9876543210"}"#));
}

#[test]
fn connections_are_compact_filterable_and_close_uses_encoded_id() {
    let body = serde_json::json!({
        "connections": [{
            "id":"id / 1",
            "metadata": {"network":"tcp","host":"example.com","destinationIP":"203.0.113.8","destinationPort":443,"process":"tv.example.app","password":"never"},
            "chains":["nethop-select","nh1s-a"],
            "upload":12,
            "download":34
        }]
    })
    .to_string();
    let (address, server) = serve(vec![(200, body), (204, String::new())]);
    let client = client(address);
    let connections = client.connections(Some("example"), Some(1)).unwrap();
    assert_eq!(connections[0].target, "example.com:443");
    assert_eq!(connections[0].download_bytes, 34);
    assert!(
        !serde_json::to_string(&connections)
            .unwrap()
            .contains("never")
    );
    client.close_connection("id / 1").unwrap();
    let requests = server.join().unwrap();
    assert!(requests[1].starts_with("DELETE /connections/id%20%2F%201 HTTP/1.1\r\n"));
}

#[test]
fn close_all_uses_the_bounded_clash_endpoint() {
    let (address, server) = serve(vec![(204, String::new())]);
    let client = client(address);
    client.close_all_connections().unwrap();
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].starts_with("DELETE /connections HTTP/1.1\r\n"));
}

#[test]
fn traffic_sample_reads_one_bounded_authenticated_chunk() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = match listener.local_addr().unwrap() {
        std::net::SocketAddr::V4(address) => address,
        _ => unreachable!(),
    };
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        loop {
            let count = stream.read(&mut buffer).unwrap();
            request.extend_from_slice(&buffer[..count]);
            if request.windows(4).any(|part| part == b"\r\n\r\n") {
                break;
            }
        }
        let body = br#"{"up":123,"down":456}
"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n",
            body.len()
        )
        .unwrap();
        stream.write_all(body).unwrap();
        stream.write_all(b"\r\n").unwrap();
        String::from_utf8(request).unwrap()
    });

    let sample = client(address).traffic_sample().unwrap();
    assert_eq!((sample.up, sample.down), (123, 456));
    let request = server.join().unwrap();
    assert!(request.starts_with("GET /traffic HTTP/1.1\r\n"));
    assert!(
        request
            .to_ascii_lowercase()
            .contains(&format!("authorization: bearer {SECRET}"))
    );
}

#[test]
fn response_size_and_json_shape_are_strictly_bounded() {
    let limits = ClashApiLimits::new(Duration::from_secs(1), 1024).unwrap();
    let (address, server) = serve(vec![(200, "x".repeat(1025))]);
    let api = ClashApiClient::new(address, SECRET, limits).unwrap();
    assert!(matches!(
        api.group_snapshot(),
        Err(ClashApiError::ResponseTooLarge)
    ));
    server.join().unwrap();

    let (address, server) = serve(vec![(200, "{}".to_owned())]);
    assert!(matches!(
        client(address).group_snapshot(),
        Err(ClashApiError::InvalidResponse)
    ));
    server.join().unwrap();
}
