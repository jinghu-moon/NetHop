use std::{
    collections::HashSet,
    fmt,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use http_body_util::{BodyExt, Empty, Limited};
use hyper::{Request, StatusCode, body::Bytes, client::conn::http1};
use hyper_util::rt::TokioIo;
use nethop_protocol::{
    BenchmarkDiagnostic, BenchmarkReport, BenchmarkStatus, BenchmarkTrigger, NodeProbeOutcome,
    NodeProbeState,
};
use serde::Deserialize;
use thiserror::Error;
use tokio::{net::TcpStream, task::JoinSet, time::Instant as TokioInstant};

#[cfg(feature = "benchmark-evidence")]
use std::sync::atomic::{AtomicU64, AtomicUsize};

pub const MAX_BENCHMARK_CANDIDATES: usize = 64;
pub const PROBE_CUTOFF: Duration = Duration::from_millis(4_500);
pub const OPERATION_DEADLINE: Duration = Duration::from_millis(4_900);
const RESPONSE_BODY_LIMIT: usize = 4 * 1024;
const RESPONSE_HEADER_LIMIT: usize = 8 * 1024;
const RESPONSE_HEADER_COUNT_LIMIT: usize = 32;
const PROBE_URL: &str = "https://www.gstatic.com/generate_204";

#[cfg(feature = "benchmark-evidence")]
static ACTIVE_PROBE_TASKS: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "benchmark-evidence")]
static PEAK_PROBE_TASKS: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "benchmark-evidence")]
static ACTIVE_PROBE_SOCKETS: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "benchmark-evidence")]
static PEAK_PROBE_SOCKETS: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "benchmark-evidence")]
static LAST_BOOTSTRAP_MICROS: AtomicU64 = AtomicU64::new(0);

#[cfg(feature = "benchmark-evidence")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BenchmarkEngineMetrics {
    pub active_tasks: usize,
    pub peak_tasks: usize,
    pub active_sockets: usize,
    pub peak_sockets: usize,
    pub bootstrap_micros: u64,
}

#[cfg(feature = "benchmark-evidence")]
pub fn reset_benchmark_engine_metrics() {
    ACTIVE_PROBE_TASKS.store(0, Ordering::Release);
    PEAK_PROBE_TASKS.store(0, Ordering::Release);
    ACTIVE_PROBE_SOCKETS.store(0, Ordering::Release);
    PEAK_PROBE_SOCKETS.store(0, Ordering::Release);
    LAST_BOOTSTRAP_MICROS.store(0, Ordering::Release);
}

#[cfg(feature = "benchmark-evidence")]
pub fn benchmark_engine_metrics() -> BenchmarkEngineMetrics {
    BenchmarkEngineMetrics {
        active_tasks: ACTIVE_PROBE_TASKS.load(Ordering::Acquire),
        peak_tasks: PEAK_PROBE_TASKS.load(Ordering::Acquire),
        active_sockets: ACTIVE_PROBE_SOCKETS.load(Ordering::Acquire),
        peak_sockets: PEAK_PROBE_SOCKETS.load(Ordering::Acquire),
        bootstrap_micros: LAST_BOOTSTRAP_MICROS.load(Ordering::Acquire),
    }
}

#[cfg(feature = "benchmark-evidence")]
enum EvidenceResource {
    Task,
    Socket,
}

#[cfg(feature = "benchmark-evidence")]
struct EvidenceGuard(EvidenceResource);

#[cfg(feature = "benchmark-evidence")]
impl EvidenceGuard {
    fn task() -> Self {
        increment_peak(&ACTIVE_PROBE_TASKS, &PEAK_PROBE_TASKS);
        Self(EvidenceResource::Task)
    }

    fn socket() -> Self {
        increment_peak(&ACTIVE_PROBE_SOCKETS, &PEAK_PROBE_SOCKETS);
        Self(EvidenceResource::Socket)
    }
}

#[cfg(feature = "benchmark-evidence")]
impl Drop for EvidenceGuard {
    fn drop(&mut self) {
        match self.0 {
            EvidenceResource::Task => {
                ACTIVE_PROBE_TASKS.fetch_sub(1, Ordering::AcqRel);
            }
            EvidenceResource::Socket => {
                ACTIVE_PROBE_SOCKETS.fetch_sub(1, Ordering::AcqRel);
            }
        }
    }
}

#[cfg(feature = "benchmark-evidence")]
fn increment_peak(active: &AtomicUsize, peak: &AtomicUsize) {
    let current = active.fetch_add(1, Ordering::AcqRel) + 1;
    peak.fetch_max(current, Ordering::AcqRel);
}

#[derive(Clone, PartialEq, Eq)]
pub struct BenchmarkCandidate {
    node_id: String,
    internal_tag: String,
}

impl BenchmarkCandidate {
    pub fn new(
        node_id: impl Into<String>,
        internal_tag: impl Into<String>,
    ) -> Result<Self, BenchmarkError> {
        let node_id = node_id.into();
        let internal_tag = internal_tag.into();
        if !valid_stable_node_id(&node_id)
            || internal_tag.is_empty()
            || internal_tag.len() > 128
            || internal_tag.chars().any(char::is_control)
        {
            return Err(BenchmarkError::InvalidCandidates);
        }
        Ok(Self {
            node_id,
            internal_tag,
        })
    }

    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    pub fn internal_tag(&self) -> &str {
        &self.internal_tag
    }
}

impl fmt::Debug for BenchmarkCandidate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BenchmarkCandidate")
            .field("node_id", &self.node_id)
            .field("internal_tag", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone)]
pub struct BenchmarkEndpoint {
    address: SocketAddrV4,
    authorization: String,
}

impl BenchmarkEndpoint {
    pub fn new(address: SocketAddrV4, secret: &str) -> Result<Self, BenchmarkError> {
        if *address.ip() != Ipv4Addr::LOCALHOST
            || address.port() == 0
            || !(16..=128).contains(&secret.len())
            || secret.chars().any(char::is_control)
        {
            return Err(BenchmarkError::InvalidEndpoint);
        }
        Ok(Self {
            address,
            authorization: format!("Bearer {secret}"),
        })
    }
}

impl fmt::Debug for BenchmarkEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BenchmarkEndpoint")
            .field("address", &self.address)
            .field("authorization", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoSelectionDecision {
    Keep,
    Switch { node_id: String },
}

pub fn choose_auto_target(
    ordered_node_ids: &[String],
    outcomes: &[NodeProbeOutcome],
    current_node_id: Option<&str>,
    tolerance_ms: u32,
) -> AutoSelectionDecision {
    let delay = |node_id: &str| {
        outcomes
            .iter()
            .find(|outcome| outcome.node_id == node_id && outcome.state == NodeProbeState::Success)
            .and_then(|outcome| outcome.latency_ms)
    };
    let mut selected = current_node_id
        .and_then(delay)
        .map(|latency| (current_node_id.expect("current ID exists"), latency));
    for node_id in ordered_node_ids {
        let Some(candidate_delay) = delay(node_id) else {
            continue;
        };
        match selected {
            None => selected = Some((node_id.as_str(), candidate_delay)),
            Some((_, selected_delay))
                if selected_delay > candidate_delay.saturating_add(tolerance_ms) =>
            {
                selected = Some((node_id.as_str(), candidate_delay));
            }
            Some(_) => {}
        }
    }
    match selected {
        Some((node_id, _)) if Some(node_id) != current_node_id => AutoSelectionDecision::Switch {
            node_id: node_id.to_owned(),
        },
        _ => AutoSelectionDecision::Keep,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BenchmarkError {
    #[error("benchmark endpoint is invalid")]
    InvalidEndpoint,
    #[error("benchmark candidate set is invalid")]
    InvalidCandidates,
    #[error("benchmark runtime could not be created")]
    Runtime,
}

pub fn validate_candidates(candidates: &[BenchmarkCandidate]) -> Result<(), BenchmarkError> {
    if candidates.is_empty() || candidates.len() > MAX_BENCHMARK_CANDIDATES {
        return Err(BenchmarkError::InvalidCandidates);
    }
    let node_ids = candidates
        .iter()
        .map(BenchmarkCandidate::node_id)
        .collect::<HashSet<_>>();
    let tags = candidates
        .iter()
        .map(BenchmarkCandidate::internal_tag)
        .collect::<HashSet<_>>();
    if node_ids.len() != candidates.len() || tags.len() != candidates.len() {
        return Err(BenchmarkError::InvalidCandidates);
    }
    Ok(())
}

pub fn run_benchmark(
    endpoint: BenchmarkEndpoint,
    candidates: Vec<BenchmarkCandidate>,
    trigger: BenchmarkTrigger,
    generation: u64,
) -> Result<BenchmarkReport, BenchmarkError> {
    run_benchmark_with_cancel(
        endpoint,
        candidates,
        trigger,
        generation,
        PROBE_CUTOFF,
        Arc::new(AtomicBool::new(false)),
    )
}

#[cfg(test)]
fn run_benchmark_with_cutoff(
    endpoint: BenchmarkEndpoint,
    candidates: Vec<BenchmarkCandidate>,
    trigger: BenchmarkTrigger,
    generation: u64,
    cutoff: Duration,
) -> Result<BenchmarkReport, BenchmarkError> {
    run_benchmark_with_cancel(
        endpoint,
        candidates,
        trigger,
        generation,
        cutoff,
        Arc::new(AtomicBool::new(false)),
    )
}

fn run_benchmark_with_cancel(
    endpoint: BenchmarkEndpoint,
    candidates: Vec<BenchmarkCandidate>,
    trigger: BenchmarkTrigger,
    generation: u64,
    cutoff: Duration,
    cancelled: Arc<AtomicBool>,
) -> Result<BenchmarkReport, BenchmarkError> {
    run_benchmark_from(
        endpoint,
        candidates,
        trigger,
        generation,
        cutoff,
        cancelled,
        Instant::now(),
    )
}

fn run_benchmark_from(
    endpoint: BenchmarkEndpoint,
    candidates: Vec<BenchmarkCandidate>,
    trigger: BenchmarkTrigger,
    generation: u64,
    cutoff: Duration,
    cancelled: Arc<AtomicBool>,
    started: Instant,
) -> Result<BenchmarkReport, BenchmarkError> {
    validate_candidates(&candidates)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .map_err(|_| BenchmarkError::Runtime)?;
    Ok(runtime.block_on(run_async(BenchmarkRun {
        endpoint,
        candidates,
        trigger,
        generation,
        started,
        probe_cutoff: cutoff,
        cancelled,
    })))
}

struct BenchmarkRun {
    endpoint: BenchmarkEndpoint,
    candidates: Vec<BenchmarkCandidate>,
    trigger: BenchmarkTrigger,
    generation: u64,
    started: Instant,
    probe_cutoff: Duration,
    cancelled: Arc<AtomicBool>,
}

pub fn spawn_benchmark(
    endpoint: BenchmarkEndpoint,
    candidates: Vec<BenchmarkCandidate>,
    trigger: BenchmarkTrigger,
    generation: u64,
) -> Result<BenchmarkJob, BenchmarkError> {
    spawn_benchmark_with_wake(endpoint, candidates, trigger, generation, None)
}

pub fn spawn_benchmark_with_wake(
    endpoint: BenchmarkEndpoint,
    candidates: Vec<BenchmarkCandidate>,
    trigger: BenchmarkTrigger,
    generation: u64,
    wake: Option<mpsc::Sender<()>>,
) -> Result<BenchmarkJob, BenchmarkError> {
    validate_candidates(&candidates)?;
    let (sender, receiver) = mpsc::sync_channel(1);
    let started = Instant::now();
    let cancelled = Arc::new(AtomicBool::new(false));
    let thread_cancelled = Arc::clone(&cancelled);
    let handle = thread::Builder::new()
        .name("nethop-node-bench".to_owned())
        .spawn(move || {
            let report = catch_unwind(AssertUnwindSafe(|| {
                run_benchmark_from(
                    endpoint,
                    candidates,
                    trigger,
                    generation,
                    PROBE_CUTOFF,
                    thread_cancelled,
                    started,
                )
            }))
            .ok()
            .and_then(Result::ok)
            .unwrap_or_else(|| {
                BenchmarkReport::internal_error(trigger, generation, duration_ms(started.elapsed()))
            });
            let _ = sender.send(report);
            if let Some(wake) = wake {
                let _ = wake.send(());
            }
        })
        .map_err(|_| BenchmarkError::Runtime)?;
    Ok(BenchmarkJob {
        receiver,
        handle: Some(handle),
        started,
        trigger,
        generation,
        cancelled,
    })
}

pub struct BenchmarkJob {
    receiver: mpsc::Receiver<BenchmarkReport>,
    handle: Option<thread::JoinHandle<()>>,
    started: Instant,
    trigger: BenchmarkTrigger,
    generation: u64,
    cancelled: Arc<AtomicBool>,
}

impl BenchmarkJob {
    pub fn deadline(&self) -> Instant {
        self.started + OPERATION_DEADLINE
    }

    pub fn remaining(&self) -> Duration {
        self.deadline().saturating_duration_since(Instant::now())
    }

    pub fn try_complete(&mut self) -> Option<BenchmarkReport> {
        match self.receiver.try_recv() {
            Ok(report) => {
                self.join();
                Some(report)
            }
            Err(mpsc::TryRecvError::Empty) if self.started.elapsed() < OPERATION_DEADLINE => None,
            Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => {
                self.cancelled.store(true, Ordering::Release);
                self.join();
                Some(BenchmarkReport::internal_error(
                    self.trigger,
                    self.generation,
                    duration_ms(self.started.elapsed()),
                ))
            }
        }
    }

    fn join(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }

    pub fn cancel(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        self.join();
    }
}

impl Drop for BenchmarkJob {
    fn drop(&mut self) {
        self.cancel();
    }
}

async fn run_async(run: BenchmarkRun) -> BenchmarkReport {
    let BenchmarkRun {
        endpoint,
        candidates,
        trigger,
        generation,
        started,
        probe_cutoff,
        cancelled,
    } = run;
    #[cfg(feature = "benchmark-evidence")]
    LAST_BOOTSTRAP_MICROS.store(
        started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64,
        Ordering::Release,
    );
    let bootstrap_ms = duration_ms(started.elapsed());
    let candidate_ids = candidates
        .iter()
        .map(|candidate| candidate.node_id.clone())
        .collect::<Vec<_>>();
    let cutoff = TokioInstant::now() + probe_cutoff.saturating_sub(started.elapsed());
    let mut tasks = JoinSet::new();
    for candidate in candidates {
        let endpoint = endpoint.clone();
        tasks.spawn(async move {
            #[cfg(feature = "benchmark-evidence")]
            let _task_guard = EvidenceGuard::task();
            let node_id = candidate.node_id.clone();
            match probe_one(&endpoint, &candidate.internal_tag, cutoff).await {
                Ok(delay) => Ok(NodeProbeOutcome::success(node_id, delay)
                    .expect("validated candidate and delay produce a valid outcome")),
                Err(ProbeError::Outcome(state)) => Ok(NodeProbeOutcome::failed(node_id, state)
                    .expect("validated candidate and failure state produce a valid outcome")),
                Err(ProbeError::Unauthorized) => Err(()),
            }
        });
    }
    let mut outcomes = Vec::with_capacity(tasks.len());
    let mut unauthorized = false;
    loop {
        if cancelled.load(Ordering::Acquire) {
            tasks.shutdown().await;
            break;
        }
        let poll_deadline = cutoff.min(TokioInstant::now() + Duration::from_millis(25));
        match tokio::time::timeout_at(poll_deadline, tasks.join_next()).await {
            Ok(Some(Ok(Ok(outcome)))) => outcomes.push(outcome),
            Ok(Some(Ok(Err(())))) => {
                unauthorized = true;
                tasks.shutdown().await;
                break;
            }
            Ok(Some(Err(_))) => {}
            Ok(None) => break,
            Err(_) if TokioInstant::now() < cutoff => continue,
            Err(_) => {
                tasks.shutdown().await;
                break;
            }
        }
    }
    let mut completed = outcomes
        .drain(..)
        .map(|outcome| (outcome.node_id.clone(), outcome))
        .collect::<std::collections::HashMap<_, _>>();
    outcomes = candidate_ids
        .into_iter()
        .map(|node_id| {
            completed.remove(&node_id).unwrap_or_else(|| {
                NodeProbeOutcome::failed(node_id, NodeProbeState::Timeout)
                    .expect("validated candidate produces a valid timeout outcome")
            })
        })
        .collect();
    let mut report = BenchmarkReport::from_outcomes(
        trigger,
        generation,
        bootstrap_ms,
        duration_ms(started.elapsed()),
        outcomes,
    )
    .expect("validated benchmark inputs produce a valid report");
    if unauthorized {
        report.status = BenchmarkStatus::Failed;
        report.diagnostic = Some(BenchmarkDiagnostic::Unauthorized);
    }
    report
}

enum ProbeError {
    Outcome(NodeProbeState),
    Unauthorized,
}

async fn probe_one(
    endpoint: &BenchmarkEndpoint,
    internal_tag: &str,
    cutoff: TokioInstant,
) -> Result<u32, ProbeError> {
    let stream =
        tokio::time::timeout_at(cutoff, TcpStream::connect(SocketAddr::V4(endpoint.address)))
            .await
            .map_err(|_| ProbeError::Outcome(NodeProbeState::Timeout))?
            .map_err(|_| ProbeError::Outcome(NodeProbeState::Unavailable))?;
    #[cfg(feature = "benchmark-evidence")]
    let _socket_guard = EvidenceGuard::socket();
    let mut builder = http1::Builder::new();
    builder
        .max_buf_size(RESPONSE_HEADER_LIMIT)
        .max_headers(RESPONSE_HEADER_COUNT_LIMIT);
    let (mut sender, connection) =
        tokio::time::timeout_at(cutoff, builder.handshake(TokioIo::new(stream)))
            .await
            .map_err(|_| ProbeError::Outcome(NodeProbeState::Timeout))?
            .map_err(|_| ProbeError::Outcome(NodeProbeState::ProtocolError))?;
    let request = Request::builder()
        .method("GET")
        .uri(format!(
            "/proxies/{}/delay?timeout={}&url={}",
            encode_component(internal_tag),
            cutoff
                .saturating_duration_since(TokioInstant::now())
                .as_millis(),
            encode_component(PROBE_URL)
        ))
        .header("Host", endpoint.address.to_string())
        .header("Authorization", &endpoint.authorization)
        .header("Accept", "application/json")
        .header("Connection", "close")
        .body(Empty::<Bytes>::new())
        .map_err(|_| ProbeError::Outcome(NodeProbeState::ProtocolError))?;
    let request_and_body = async move {
        let response = sender
            .send_request(request)
            .await
            .map_err(|_| ProbeError::Outcome(NodeProbeState::ProtocolError))?;
        if response.status() == StatusCode::UNAUTHORIZED
            || response.status() == StatusCode::FORBIDDEN
        {
            return Err(ProbeError::Unauthorized);
        }
        if response.status() == StatusCode::GATEWAY_TIMEOUT {
            return Err(ProbeError::Outcome(NodeProbeState::Timeout));
        }
        if response.status() == StatusCode::SERVICE_UNAVAILABLE {
            return Err(ProbeError::Outcome(NodeProbeState::Unavailable));
        }
        if response.status() != StatusCode::OK {
            return Err(ProbeError::Outcome(NodeProbeState::ProtocolError));
        }
        let body = Limited::new(response.into_body(), RESPONSE_BODY_LIMIT)
            .collect()
            .await
            .map_err(|_| ProbeError::Outcome(NodeProbeState::ProtocolError))?
            .to_bytes();
        let response: DelayResponse = serde_json::from_slice(&body)
            .map_err(|_| ProbeError::Outcome(NodeProbeState::ProtocolError))?;
        if response.delay == 0 || response.delay > u32::from(u16::MAX) {
            return Err(ProbeError::Outcome(NodeProbeState::ProtocolError));
        }
        Ok(response.delay)
    };
    tokio::time::timeout_at(cutoff, async move {
        tokio::pin!(request_and_body);
        tokio::pin!(connection);
        tokio::select! {
            biased;
            result = &mut request_and_body => result,
            result = &mut connection => match result {
                Ok(()) => request_and_body.await,
                Err(_) => Err(ProbeError::Outcome(NodeProbeState::ProtocolError)),
            },
        }
    })
    .await
    .map_err(|_| ProbeError::Outcome(NodeProbeState::Timeout))?
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DelayResponse {
    delay: u32,
}

fn valid_stable_node_id(value: &str) -> bool {
    value.len() == 21
        && value.starts_with("nh1s-")
        && value[5..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn encode_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

fn duration_ms(value: Duration) -> u32 {
    u32::try_from(value.as_millis()).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
        sync::{Arc, Barrier, Mutex},
    };

    fn outcome(id: &str, delay: Option<u32>) -> NodeProbeOutcome {
        match delay {
            Some(delay) => NodeProbeOutcome::success(id, delay).unwrap(),
            None => NodeProbeOutcome::failed(id, NodeProbeState::Timeout).unwrap(),
        }
    }

    #[test]
    fn tolerance_is_strict_and_order_is_stable() {
        let a = "nh1s-000000000000000a";
        let b = "nh1s-000000000000000b";
        let c = "nh1s-000000000000000c";
        let ids = vec![a.to_owned(), b.to_owned(), c.to_owned()];
        let boundary = vec![outcome(a, Some(150)), outcome(b, Some(100))];
        assert_eq!(
            choose_auto_target(&ids, &boundary, Some(a), 50),
            AutoSelectionDecision::Keep
        );
        assert_eq!(
            choose_auto_target(&ids, &boundary, Some(a), 49),
            AutoSelectionDecision::Switch {
                node_id: b.to_owned()
            }
        );
        let results = vec![
            outcome(a, Some(150)),
            outcome(b, Some(100)),
            outcome(c, Some(49)),
        ];
        assert_eq!(
            choose_auto_target(&ids, &results, None, 50),
            AutoSelectionDecision::Switch {
                node_id: c.to_owned()
            }
        );
        assert_eq!(
            choose_auto_target(&ids, &[outcome(a, None)], Some(a), 50),
            AutoSelectionDecision::Keep
        );
    }

    #[test]
    fn candidate_set_is_bounded_unique_and_redacted() {
        let one = BenchmarkCandidate::new("nh1s-0123456789abcdef", "private-terminal").unwrap();
        assert!(!format!("{one:?}").contains("private-terminal"));
        assert_eq!(
            validate_candidates(&[]),
            Err(BenchmarkError::InvalidCandidates)
        );
        assert_eq!(
            validate_candidates(&[one.clone(), one]),
            Err(BenchmarkError::InvalidCandidates)
        );
    }

    #[test]
    fn hyper_connection_is_driven_and_response_is_bounded() {
        let (endpoint, requests, server) = fake_server(1, false);
        let report = run_benchmark_with_cutoff(
            endpoint,
            vec![candidate(0)],
            BenchmarkTrigger::Manual,
            7,
            Duration::from_secs(1),
        )
        .unwrap();

        assert_eq!(report.status, BenchmarkStatus::Success, "{report:?}");
        assert_eq!(report.nodes[0].latency_ms, Some(42));
        server.join().unwrap();
        let request = &requests.lock().unwrap()[0];
        assert!(request.starts_with("GET /proxies/terminal-0/delay?timeout="));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer 0123456789abcdef")
        );
    }

    #[test]
    fn complete_keep_alive_response_does_not_wait_for_eof() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = match listener.local_addr().unwrap() {
            SocketAddr::V4(address) => address,
            SocketAddr::V6(_) => unreachable!(),
        };
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
            }
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 12\r\nConnection: keep-alive\r\n\r\n{\"delay\":42}",
                )
                .unwrap();
            thread::sleep(Duration::from_millis(300));
        });
        let started = Instant::now();
        let report = run_benchmark_with_cutoff(
            BenchmarkEndpoint::new(address, "0123456789abcdef").unwrap(),
            vec![candidate(0)],
            BenchmarkTrigger::Manual,
            8,
            Duration::from_secs(1),
        )
        .unwrap();
        let elapsed = started.elapsed();
        server.join().unwrap();
        assert_eq!(report.nodes[0].latency_ms, Some(42));
        assert!(elapsed < Duration::from_millis(250));
    }

    #[test]
    fn candidates_start_concurrently_and_share_one_cutoff() {
        for count in [1, 16, 27, 64] {
            let (endpoint, requests, server) = fake_server(count, true);
            let started = Instant::now();
            let report = run_benchmark_with_cutoff(
                endpoint,
                (0..count).map(candidate).collect(),
                BenchmarkTrigger::Manual,
                8,
                Duration::from_millis(250),
            )
            .unwrap();

            assert!(started.elapsed() < Duration::from_secs(5), "count={count}");
            assert_eq!(report.tested, count);
            assert_eq!(report.timed_out, count);
            server.join().unwrap();
            assert_eq!(requests.lock().unwrap().len(), count);
        }
    }

    #[test]
    fn report_order_follows_candidate_order_not_completion_order() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = match listener.local_addr().unwrap() {
            SocketAddr::V4(address) => address,
            SocketAddr::V6(_) => unreachable!(),
        };
        let server = thread::spawn(move || {
            let mut workers = Vec::new();
            for stream in listener.incoming().take(2) {
                workers.push(thread::spawn(move || {
                    let mut stream = stream.unwrap();
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut request = String::new();
                    loop {
                        let mut line = String::new();
                        reader.read_line(&mut line).unwrap();
                        request.push_str(&line);
                        if line == "\r\n" || line.is_empty() {
                            break;
                        }
                    }
                    let (delay, wait) = if request.contains("terminal-0") {
                        (80, Duration::from_millis(80))
                    } else {
                        (20, Duration::ZERO)
                    };
                    thread::sleep(wait);
                    let body = format!(r#"{{"delay":{delay}}}"#);
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .unwrap();
                }));
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });

        let report = run_benchmark_with_cutoff(
            BenchmarkEndpoint::new(address, "0123456789abcdef").unwrap(),
            vec![candidate(1), candidate(0)],
            BenchmarkTrigger::Manual,
            8,
            Duration::from_secs(1),
        )
        .unwrap();

        server.join().unwrap();
        assert_eq!(report.nodes[0].node_id, "nh1s-0000000000000001");
        assert_eq!(report.nodes[0].latency_ms, Some(20));
        assert_eq!(report.nodes[1].node_id, "nh1s-0000000000000000");
        assert_eq!(report.nodes[1].latency_ms, Some(80));
    }

    #[test]
    fn status_body_and_header_failures_are_bounded_and_typed() {
        let cases = [
            (504, r#"{}"#, NodeProbeState::Timeout),
            (503, r#"{}"#, NodeProbeState::Unavailable),
            (500, r#"{}"#, NodeProbeState::ProtocolError),
            (200, r#"{"delay":0}"#, NodeProbeState::ProtocolError),
            (200, r#"{"delay":65536}"#, NodeProbeState::ProtocolError),
        ];
        for (status, body, expected) in cases {
            let (endpoint, server) = raw_server(format!(
                "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ));
            let report = run_benchmark_with_cutoff(
                endpoint,
                vec![candidate(0)],
                BenchmarkTrigger::Manual,
                9,
                Duration::from_secs(1),
            )
            .unwrap();
            assert_eq!(report.nodes[0].state, expected);
            server.join().unwrap();
        }

        let (endpoint, server) = raw_server(format!(
            "HTTP/1.1 200 OK\r\nX-Oversized: {}\r\nContent-Length: 12\r\n\r\n{{\"delay\":42}}",
            "x".repeat(RESPONSE_HEADER_LIMIT)
        ));
        let report = run_benchmark_with_cutoff(
            endpoint,
            vec![candidate(0)],
            BenchmarkTrigger::Manual,
            10,
            Duration::from_secs(1),
        )
        .unwrap();
        assert_eq!(report.nodes[0].state, NodeProbeState::ProtocolError);
        server.join().unwrap();

        let body = format!(
            "{{\"delay\":42,\"padding\":\"{}\"}}",
            "x".repeat(RESPONSE_BODY_LIMIT)
        );
        let (endpoint, server) = raw_server(format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ));
        let report = run_benchmark_with_cutoff(
            endpoint,
            vec![candidate(0)],
            BenchmarkTrigger::Manual,
            11,
            Duration::from_secs(1),
        )
        .unwrap();
        assert_eq!(report.nodes[0].state, NodeProbeState::ProtocolError);
        server.join().unwrap();
    }

    #[test]
    fn unauthorized_is_an_immediate_round_diagnostic() {
        for status in [401, 403] {
            let (endpoint, server) = raw_server(format!(
                "HTTP/1.1 {status} Unauthorized\r\nContent-Length: 0\r\n\r\n"
            ));
            let report = run_benchmark_with_cutoff(
                endpoint,
                vec![candidate(0)],
                BenchmarkTrigger::Manual,
                12,
                Duration::from_secs(1),
            )
            .unwrap();
            assert_eq!(report.status, BenchmarkStatus::Failed);
            assert_eq!(report.diagnostic, Some(BenchmarkDiagnostic::Unauthorized));
            assert!(!format!("{report:?}").contains("0123456789abcdef"));
            server.join().unwrap();
        }
    }

    #[test]
    fn cancelling_a_job_joins_the_named_thread_before_the_cutoff() {
        let (endpoint, requests, server) = fake_server(1, true);
        let mut job =
            spawn_benchmark(endpoint, vec![candidate(0)], BenchmarkTrigger::Manual, 13).unwrap();
        let wait_started = Instant::now();
        while requests.lock().unwrap().is_empty() {
            assert!(wait_started.elapsed() < Duration::from_secs(1));
            thread::yield_now();
        }
        let started = Instant::now();
        job.cancel();
        assert!(started.elapsed() < Duration::from_secs(1));
        server.join().unwrap();
    }

    #[test]
    fn completed_job_wakes_worker_and_disconnected_wake_is_harmless() {
        let (endpoint, _, server) = fake_server(1, false);
        let (wake, receiver) = mpsc::channel();
        let mut job = spawn_benchmark_with_wake(
            endpoint,
            vec![candidate(0)],
            BenchmarkTrigger::Manual,
            14,
            Some(wake),
        )
        .unwrap();
        receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        let report = job.try_complete().expect("wake follows result send");
        assert_eq!(report.status, BenchmarkStatus::Success);
        server.join().unwrap();

        let (endpoint, _, server) = fake_server(1, false);
        let (wake, receiver) = mpsc::channel();
        drop(receiver);
        let mut job = spawn_benchmark_with_wake(
            endpoint,
            vec![candidate(0)],
            BenchmarkTrigger::Periodic,
            15,
            Some(wake),
        )
        .unwrap();
        let started = Instant::now();
        let report = loop {
            if let Some(report) = job.try_complete() {
                break report;
            }
            assert!(started.elapsed() < OPERATION_DEADLINE);
            thread::yield_now();
        };
        assert_eq!(report.status, BenchmarkStatus::Success);
        server.join().unwrap();
    }

    fn candidate(index: usize) -> BenchmarkCandidate {
        BenchmarkCandidate::new(format!("nh1s-{index:016x}"), format!("terminal-{index}")).unwrap()
    }

    fn fake_server(
        expected: usize,
        stall: bool,
    ) -> (
        BenchmarkEndpoint,
        Arc<Mutex<Vec<String>>>,
        thread::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = match listener.local_addr().unwrap() {
            SocketAddr::V4(address) => address,
            SocketAddr::V6(_) => unreachable!(),
        };
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&requests);
        let barrier = Arc::new(Barrier::new(expected));
        let server = thread::spawn(move || {
            let mut workers = Vec::new();
            for stream in listener.incoming().take(expected) {
                let mut stream = stream.unwrap();
                let captured = Arc::clone(&captured);
                let barrier = Arc::clone(&barrier);
                workers.push(thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut request = String::new();
                    loop {
                        let mut line = String::new();
                        reader.read_line(&mut line).unwrap();
                        request.push_str(&line);
                        if line == "\r\n" || line.is_empty() {
                            break;
                        }
                    }
                    captured.lock().unwrap().push(request);
                    barrier.wait();
                    if stall {
                        thread::sleep(Duration::from_millis(400));
                    } else {
                        stream
                            .write_all(
                                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 12\r\nConnection: close\r\n\r\n{\"delay\":42}",
                            )
                            .unwrap();
                    }
                }));
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
        (
            BenchmarkEndpoint::new(address, "0123456789abcdef").unwrap(),
            requests,
            server,
        )
    }

    fn raw_server(response: String) -> (BenchmarkEndpoint, thread::JoinHandle<()>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = match listener.local_addr().unwrap() {
            SocketAddr::V4(address) => address,
            SocketAddr::V6(_) => unreachable!(),
        };
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
            }
            stream.write_all(response.as_bytes()).unwrap();
        });
        (
            BenchmarkEndpoint::new(address, "0123456789abcdef").unwrap(),
            server,
        )
    }
}
