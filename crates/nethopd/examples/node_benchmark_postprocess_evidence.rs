use std::{
    fs,
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener},
    process,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use nethop_core::{GenerationNodeRecord, GenerationNodeRegistry};
use nethop_protocol::{BenchmarkReport, BenchmarkTrigger, NodeProbeOutcome};
use nethopd::{
    ClashApiClient, ClashApiLimits, NodeSelectionIntent, NodeSelectionStore, OperationalControl,
};
use serde::Serialize;
use serde_json::json;
use tempfile::tempdir;

const SECRET: &str = "0123456789abcdef0123456789abcdef";
const SAMPLES: usize = 20;
const CANDIDATES: usize = 64;

#[derive(Serialize)]
struct Percentiles {
    p50: f64,
    p95: f64,
    p99: f64,
}

#[derive(Serialize)]
struct Report {
    schema: &'static str,
    samples_ms: Vec<f64>,
    elapsed_ms: Percentiles,
    request_count: usize,
    expected_request_count: usize,
    put_count: usize,
    expected_put_count: usize,
    passed: bool,
}

fn main() {
    if cfg!(debug_assertions) {
        fail("postprocess evidence must use the release profile");
    }
    let directory = tempdir().expect("temporary fixture directory is available");
    let root = directory
        .path()
        .canonicalize()
        .expect("fixture root exists");
    let generations = generation_root(&root);
    let requests = Arc::new(Mutex::new(Vec::new()));
    let (address, server) = serve_api(Arc::clone(&requests));
    let selection_store = NodeSelectionStore::new(root.join("selection.v1.json"))
        .expect("selection store path is valid");
    selection_store
        .save(&NodeSelectionIntent::Auto, 1)
        .expect("auto intent is stored");
    let api = ClashApiClient::new(address, SECRET, ClashApiLimits::default())
        .expect("fake API endpoint is valid");
    let mut control = OperationalControl::new(api, selection_store, root.join("diagnostics.json"))
        .expect("operational control is valid")
        .with_generation_root(generations)
        .expect("generation root is valid");
    let ids = (0..CANDIDATES).map(node_id).collect::<Vec<_>>();
    let mut current = 0;
    let mut samples = Vec::with_capacity(SAMPLES);
    for sample in 0..SAMPLES {
        let target = if current == 0 { 1 } else { 0 };
        let report = benchmark_report(sample as u64 + 1, current, target);
        let started = Instant::now();
        let selection = control
            .complete_benchmark_for_evidence(
                &report,
                &ids,
                50,
                Instant::now() + Duration::from_millis(500),
            )
            .unwrap_or_else(|error| fail(&format!("postprocess failed: {error}")));
        samples.push(started.elapsed().as_secs_f64() * 1000.0);
        if selection["active_node_id"] != ids[target] {
            fail("final selector snapshot does not contain the selected target");
        }
        current = target;
    }
    drop(control);
    server.join().expect("fake API server did not panic");
    let requests = requests.lock().expect("request capture is available");
    let put_count = requests
        .iter()
        .filter(|request| request.starts_with("PUT /proxies/nethop-select "))
        .count();
    let elapsed_ms = percentiles(&samples);
    let expected_request_count = SAMPLES * 4;
    let passed =
        elapsed_ms.p95 <= 100.0 && requests.len() == expected_request_count && put_count == SAMPLES;
    println!(
        "{}",
        serde_json::to_string(&Report {
            schema: "nethop-node-benchmark-postprocess-v1",
            samples_ms: samples,
            elapsed_ms,
            request_count: requests.len(),
            expected_request_count,
            put_count,
            expected_put_count: SAMPLES,
            passed,
        })
        .expect("report is serializable")
    );
    if !passed {
        process::exit(2);
    }
}

fn generation_root(root: &std::path::Path) -> std::path::PathBuf {
    let generations = root.join("generations");
    fs::create_dir(&generations).expect("generations directory is created");
    fs::create_dir(generations.join("7")).expect("generation directory is created");
    fs::write(generations.join("current"), "7\n").expect("current generation is written");
    let records = (0..CANDIDATES)
        .map(|index| {
            let id = node_id(index);
            GenerationNodeRecord::new(
                &id,
                &id,
                format!("Node {index}"),
                "vless",
                vec!["src_0123456789abcdef0123456789abcdef".to_owned()],
                true,
            )
            .expect("fixture node is valid")
        })
        .collect();
    let registry = GenerationNodeRegistry::new(records).expect("fixture registry is valid");
    fs::write(
        generations.join("7/nodes.json"),
        serde_json::to_vec(&registry).expect("registry serializes"),
    )
    .expect("registry is written");
    generations
}

fn benchmark_report(generation: u64, current: usize, target: usize) -> BenchmarkReport {
    let outcomes = (0..CANDIDATES)
        .map(|index| {
            let latency = if index == target {
                20
            } else if index == current {
                200
            } else {
                180
            };
            NodeProbeOutcome::success(node_id(index), latency).expect("outcome is valid")
        })
        .collect();
    BenchmarkReport::from_outcomes(BenchmarkTrigger::Periodic, generation, 0, 10, outcomes)
        .expect("report is valid")
}

fn serve_api(requests: Arc<Mutex<Vec<String>>>) -> (SocketAddrV4, thread::JoinHandle<()>) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("fake API binds");
    let address = match listener.local_addr().expect("fake API address exists") {
        SocketAddr::V4(address) => address,
        SocketAddr::V6(_) => unreachable!(),
    };
    let server = thread::spawn(move || {
        let mut current = node_id(0);
        for _ in 0..SAMPLES * 4 {
            let (mut stream, _) = listener.accept().expect("fake API accepts");
            let request = read_request(&mut stream);
            if request.starts_with("PUT /proxies/nethop-select ") {
                let body = request.split("\r\n\r\n").nth(1).expect("PUT body exists");
                current = serde_json::from_str::<serde_json::Value>(body)
                    .expect("PUT body is JSON")["name"]
                    .as_str()
                    .expect("PUT body has name")
                    .to_owned();
                write!(
                    stream,
                    "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
                .expect("PUT response is written");
            } else {
                let body = selector_document(&current);
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .expect("GET response is written");
            }
            requests
                .lock()
                .expect("request capture locks")
                .push(request);
        }
    });
    (address, server)
}

fn read_request(stream: &mut std::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let count = stream.read(&mut buffer).expect("request is readable");
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..count]);
        let Some(headers_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&bytes[..headers_end + 4]);
        let length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0);
        if bytes.len() >= headers_end + 4 + length {
            break;
        }
    }
    String::from_utf8(bytes).expect("request is UTF-8")
}

fn selector_document(current: &str) -> String {
    let mut proxies = serde_json::Map::new();
    let ids = (0..CANDIDATES).map(node_id).collect::<Vec<_>>();
    proxies.insert(
        "nethop-select".to_owned(),
        json!({"type":"Selector","now":current,"all":ids}),
    );
    for index in 0..CANDIDATES {
        proxies.insert(node_id(index), json!({"type":"VLESS"}));
    }
    json!({"proxies":proxies}).to_string()
}

fn node_id(index: usize) -> String {
    format!("nh1s-{index:016x}")
}

fn percentiles(values: &[f64]) -> Percentiles {
    let mut ordered = values.to_vec();
    ordered.sort_by(f64::total_cmp);
    Percentiles {
        p50: percentile(&ordered, 50),
        p95: percentile(&ordered, 95),
        p99: percentile(&ordered, 99),
    }
}

fn percentile(ordered: &[f64], percentile: usize) -> f64 {
    let index = (ordered.len() * percentile).div_ceil(100).saturating_sub(1);
    ordered[index.min(ordered.len() - 1)]
}

fn fail(message: &str) -> ! {
    eprintln!("node benchmark postprocess evidence: {message}");
    process::exit(2);
}
