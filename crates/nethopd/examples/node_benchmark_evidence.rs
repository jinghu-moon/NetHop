use std::{
    alloc::{GlobalAlloc, Layout, System},
    env,
    io::{BufRead, BufReader, Read, Write},
    net::{Ipv4Addr, Shutdown, SocketAddrV4, TcpListener, TcpStream},
    process::{self, Child, Command, Stdio},
    sync::{
        Arc, Barrier,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use nethop_protocol::{BenchmarkStatus, BenchmarkTrigger};
use nethopd::{
    BenchmarkCandidate, BenchmarkEndpoint, MAX_BENCHMARK_CANDIDATES, OPERATION_DEADLINE,
    benchmark_engine_metrics, reset_benchmark_engine_metrics, spawn_benchmark,
};
use serde::Serialize;

const SECRET: &str = "0123456789abcdef0123456789abcdef";
const SUCCESS_SAMPLES: usize = 20;
const SLOW_SAMPLES: usize = 3;
const BOOTSTRAP_SAMPLES: usize = 100;
const SERVER_STALL: Duration = Duration::from_millis(4_600);

struct CountingAllocator;

static CURRENT_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_BYTES: AtomicUsize = AtomicUsize::new(0);

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            record_allocation(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        CURRENT_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let next = unsafe { System.realloc(pointer, layout, new_size) };
        if !next.is_null() {
            if new_size >= layout.size() {
                record_allocation(new_size - layout.size());
            } else {
                CURRENT_BYTES.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
            }
        }
        next
    }
}

fn record_allocation(size: usize) {
    let current = CURRENT_BYTES.fetch_add(size, Ordering::Relaxed) + size;
    PEAK_BYTES.fetch_max(current, Ordering::Relaxed);
}

#[derive(Clone, Copy)]
enum Scenario {
    Success,
    Mixed,
    Timeout,
}

impl Scenario {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "success" => Some(Self::Success),
            "mixed" => Some(Self::Mixed),
            "timeout" => Some(Self::Timeout),
            _ => None,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Mixed => "mixed",
            Self::Timeout => "timeout",
        }
    }
}

#[derive(Serialize)]
struct Sample {
    wall_ms: f64,
    report_ms: u32,
    bootstrap_ms: u32,
    bootstrap_micros: u64,
    succeeded: usize,
    timed_out: usize,
    failed: usize,
    peak_tasks: usize,
    peak_sockets: usize,
    residual_tasks: usize,
    residual_sockets: usize,
    peak_heap_delta_bytes: usize,
}

#[derive(Serialize)]
struct Percentiles {
    p50: f64,
    p95: f64,
    p99: f64,
}

#[derive(Serialize)]
struct CaseReport {
    scenario: &'static str,
    candidates: usize,
    samples: Vec<Sample>,
    wall_ms: Percentiles,
    bootstrap_ms: Percentiles,
}

#[derive(Serialize)]
struct ResourceSupport {
    engine_task_counter: bool,
    engine_socket_counter: bool,
    os_fd_count: bool,
    os_thread_count: bool,
    os_rss_kib: bool,
    note: &'static str,
}

#[derive(Serialize)]
struct EvidenceReport {
    schema: &'static str,
    target_os: &'static str,
    target_arch: &'static str,
    release_profile: bool,
    sample_policy: SamplePolicy,
    resource_support: ResourceSupport,
    cases: Vec<CaseReport>,
    bootstrap_100_micros: Percentiles,
    bootstrap_raw_micros: Vec<u64>,
    passed: bool,
}

#[derive(Serialize)]
struct SamplePolicy {
    success_samples: usize,
    mixed_samples: usize,
    timeout_samples: usize,
    bootstrap_samples: usize,
}

fn main() {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|argument| argument == "--serve") {
        serve(&args[1..]);
        return;
    }
    if cfg!(debug_assertions) {
        fail("evidence runner must use the release profile");
    }
    let mut cases = Vec::new();
    for candidates in [1, 16, 27, MAX_BENCHMARK_CANDIDATES] {
        cases.push(run_case(Scenario::Success, candidates, SUCCESS_SAMPLES));
    }
    cases.push(run_case(
        Scenario::Mixed,
        MAX_BENCHMARK_CANDIDATES,
        SLOW_SAMPLES,
    ));
    cases.push(run_case(
        Scenario::Timeout,
        MAX_BENCHMARK_CANDIDATES,
        SLOW_SAMPLES,
    ));
    let bootstrap_case = run_case(Scenario::Success, 1, BOOTSTRAP_SAMPLES);
    let bootstrap_raw_micros = bootstrap_case
        .samples
        .iter()
        .map(|sample| sample.bootstrap_micros)
        .collect::<Vec<_>>();
    let bootstrap_100_micros = percentiles(
        &bootstrap_raw_micros
            .iter()
            .map(|value| *value as f64)
            .collect::<Vec<_>>(),
    );
    let passed = cases.iter().all(case_passed)
        && bootstrap_raw_micros.len() == BOOTSTRAP_SAMPLES
        && bootstrap_case.samples.iter().all(sample_resources_passed);
    let report = EvidenceReport {
        schema: "nethop-node-benchmark-host-release-v1",
        target_os: env::consts::OS,
        target_arch: env::consts::ARCH,
        release_profile: true,
        sample_policy: SamplePolicy {
            success_samples: SUCCESS_SAMPLES,
            mixed_samples: SLOW_SAMPLES,
            timeout_samples: SLOW_SAMPLES,
            bootstrap_samples: BOOTSTRAP_SAMPLES,
        },
        resource_support: ResourceSupport {
            engine_task_counter: true,
            engine_socket_counter: true,
            os_fd_count: false,
            os_thread_count: false,
            os_rss_kib: false,
            note: "engine counters and heap are authoritative here; Android supplies OS FD/thread/RSS evidence",
        },
        cases,
        bootstrap_100_micros,
        bootstrap_raw_micros,
        passed,
    };
    println!(
        "{}",
        serde_json::to_string(&report).expect("evidence report is serializable")
    );
    if !passed {
        process::exit(2);
    }
}

fn run_case(scenario: Scenario, candidates: usize, count: usize) -> CaseReport {
    let (endpoint, mut server) = start_server(scenario, candidates, count);
    let mut samples = Vec::with_capacity(count);
    for generation in 1..=count {
        reset_benchmark_engine_metrics();
        let baseline_heap = CURRENT_BYTES.load(Ordering::Relaxed);
        PEAK_BYTES.store(baseline_heap, Ordering::Relaxed);
        let started = Instant::now();
        let mut job = spawn_benchmark(
            endpoint.clone(),
            (0..candidates).map(candidate).collect(),
            BenchmarkTrigger::Manual,
            generation as u64,
        )
        .unwrap_or_else(|error| fail(&format!("benchmark start failed: {error}")));
        let report = loop {
            if let Some(report) = job.try_complete() {
                break report;
            }
            if started.elapsed() > OPERATION_DEADLINE + Duration::from_millis(100) {
                fail("benchmark job did not converge within its deadline");
            }
            thread::sleep(Duration::from_millis(1));
        };
        let wall = started.elapsed();
        let metrics = benchmark_engine_metrics();
        let peak_heap = PEAK_BYTES
            .load(Ordering::Relaxed)
            .saturating_sub(baseline_heap);
        validate_report(scenario, candidates, &report);
        samples.push(Sample {
            wall_ms: wall.as_secs_f64() * 1000.0,
            report_ms: report.elapsed_ms,
            bootstrap_ms: report.bootstrap_ms,
            bootstrap_micros: metrics.bootstrap_micros,
            succeeded: report.succeeded,
            timed_out: report.timed_out,
            failed: report.failed,
            peak_tasks: metrics.peak_tasks,
            peak_sockets: metrics.peak_sockets,
            residual_tasks: metrics.active_tasks,
            residual_sockets: metrics.active_sockets,
            peak_heap_delta_bytes: peak_heap,
        });
    }
    let status = server
        .wait()
        .unwrap_or_else(|error| fail(&format!("fake core wait failed: {error}")));
    if !status.success() {
        fail("fake core exited unsuccessfully");
    }
    let wall_ms = percentiles(
        &samples
            .iter()
            .map(|sample| sample.wall_ms)
            .collect::<Vec<_>>(),
    );
    let bootstrap_ms = percentiles(
        &samples
            .iter()
            .map(|sample| f64::from(sample.bootstrap_ms))
            .collect::<Vec<_>>(),
    );
    CaseReport {
        scenario: scenario.name(),
        candidates,
        samples,
        wall_ms,
        bootstrap_ms,
    }
}

fn validate_report(scenario: Scenario, candidates: usize, report: &nethopd::BenchmarkReport) {
    let expected_success = match scenario {
        Scenario::Success => candidates,
        Scenario::Mixed => candidates.div_ceil(2),
        Scenario::Timeout => 0,
    };
    let expected_timeout = candidates - expected_success;
    let expected_status = match scenario {
        Scenario::Success => BenchmarkStatus::Success,
        Scenario::Mixed => BenchmarkStatus::Partial,
        Scenario::Timeout => BenchmarkStatus::Failed,
    };
    if report.status != expected_status
        || report.tested != candidates
        || report.succeeded != expected_success
        || report.timed_out != expected_timeout
        || report.failed != 0
    {
        fail("benchmark report does not match the controlled scenario");
    }
}

fn case_passed(case: &CaseReport) -> bool {
    case.wall_ms.p95 <= 5_000.0
        && case.samples.iter().all(sample_resources_passed)
        && case.samples.iter().all(|sample| sample.wall_ms <= 5_000.0)
}

fn sample_resources_passed(sample: &Sample) -> bool {
    sample.peak_tasks <= MAX_BENCHMARK_CANDIDATES
        && sample.peak_sockets <= MAX_BENCHMARK_CANDIDATES
        && sample.residual_tasks == 0
        && sample.residual_sockets == 0
        && sample.peak_heap_delta_bytes <= 4 * 1024 * 1024
}

fn percentiles(samples: &[f64]) -> Percentiles {
    if samples.is_empty() {
        fail("percentiles require at least one sample");
    }
    let mut ordered = samples.to_vec();
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

fn candidate(index: usize) -> BenchmarkCandidate {
    BenchmarkCandidate::new(format!("nh1s-{index:016x}"), format!("terminal-{index}"))
        .expect("fixture candidate is valid")
}

fn start_server(
    scenario: Scenario,
    candidates: usize,
    rounds: usize,
) -> (BenchmarkEndpoint, Child) {
    let mut child =
        Command::new(env::current_exe().expect("evidence executable path is available"))
            .args([
                "--serve",
                scenario.name(),
                &candidates.to_string(),
                &rounds.to_string(),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap_or_else(|error| fail(&format!("fake core start failed: {error}")));
    let stdout = child.stdout.take().expect("fake core stdout is piped");
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .unwrap_or_else(|error| fail(&format!("fake core address read failed: {error}")));
    let address = line
        .trim()
        .parse::<SocketAddrV4>()
        .unwrap_or_else(|_| fail("fake core returned an invalid address"));
    drop(reader);
    (
        BenchmarkEndpoint::new(address, SECRET).expect("loopback evidence endpoint is valid"),
        child,
    )
}

fn serve(args: &[String]) {
    if args.len() != 3 {
        fail("fake core requires scenario, candidates and rounds");
    }
    let scenario = Scenario::parse(&args[0]).unwrap_or_else(|| fail("unknown fake core scenario"));
    let candidates = parse_count(&args[1]);
    let rounds = parse_count(&args[2]);
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .unwrap_or_else(|error| fail(&format!("fake core bind failed: {error}")));
    println!(
        "{}",
        listener
            .local_addr()
            .expect("bound listener has an address")
    );
    std::io::stdout().flush().expect("address is flushed");
    for _ in 0..rounds {
        let barrier = Arc::new(Barrier::new(candidates));
        let mut workers = Vec::with_capacity(candidates);
        for index in 0..candidates {
            let (stream, _) = listener
                .accept()
                .unwrap_or_else(|error| fail(&format!("fake core accept failed: {error}")));
            let barrier = Arc::clone(&barrier);
            workers.push(thread::spawn(move || {
                serve_request(stream, scenario, index, barrier);
            }));
        }
        for worker in workers {
            if worker.join().is_err() {
                fail("fake core worker panicked");
            }
        }
    }
}

fn serve_request(mut stream: TcpStream, scenario: Scenario, index: usize, barrier: Arc<Barrier>) {
    let mut headers = Vec::new();
    let mut buffer = [0_u8; 1024];
    loop {
        let count = stream.read(&mut buffer).expect("request is readable");
        if count == 0 {
            return;
        }
        headers.extend_from_slice(&buffer[..count]);
        if headers.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
        if headers.len() > 8 * 1024 {
            return;
        }
    }
    let request_headers = String::from_utf8_lossy(&headers).to_ascii_lowercase();
    let expected_authorization = format!("authorization: bearer {SECRET}").to_ascii_lowercase();
    if !request_headers.contains(&expected_authorization) {
        return;
    }
    barrier.wait();
    let succeeds = matches!(scenario, Scenario::Success)
        || matches!(scenario, Scenario::Mixed) && index % 2 == 0;
    if succeeds {
        let body = format!("{{\"delay\":{}}}", 20 + index);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .expect("response is writable");
    } else {
        thread::sleep(SERVER_STALL);
        let _ = stream.shutdown(Shutdown::Both);
    }
}

fn parse_count(value: &str) -> usize {
    value
        .parse::<usize>()
        .ok()
        .filter(|count| (1..=MAX_BENCHMARK_CANDIDATES.max(BOOTSTRAP_SAMPLES)).contains(count))
        .unwrap_or_else(|| fail("fake core count is invalid"))
}

fn fail(message: &str) -> ! {
    eprintln!("node benchmark evidence: {message}");
    process::exit(2);
}
