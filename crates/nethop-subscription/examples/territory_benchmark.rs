use std::{hint::black_box, time::Instant};

use nethop_subscription::infer_display_territory;
use serde::Serialize;
use sha2::{Digest, Sha256};

const NAME_COUNT: usize = 2_000;
const WARMUP_RUNS: usize = 5;
const SAMPLE_RUNS: usize = 20;

#[derive(Serialize)]
struct Report {
    schema: &'static str,
    name_count: usize,
    max_name_bytes: usize,
    warmup_runs: usize,
    sample_runs: usize,
    samples_us: Vec<u128>,
    p50_us: u128,
    p95_us: u128,
    result_sha256: String,
    passed: bool,
}

fn main() {
    let names = fixture();
    for _ in 0..WARMUP_RUNS {
        black_box(run(&names));
    }
    let mut samples = Vec::with_capacity(SAMPLE_RUNS);
    let mut result = Vec::new();
    for _ in 0..SAMPLE_RUNS {
        let started = Instant::now();
        result = run(&names);
        samples.push(started.elapsed().as_micros());
    }
    let mut ordered = samples.clone();
    ordered.sort_unstable();
    let p50 = percentile(&ordered, 50);
    let p95 = percentile(&ordered, 95);
    let report = Report {
        schema: "nethop-territory-benchmark-v1",
        name_count: names.len(),
        max_name_bytes: names.iter().map(String::len).max().unwrap_or_default(),
        warmup_runs: WARMUP_RUNS,
        sample_runs: SAMPLE_RUNS,
        samples_us: samples,
        p50_us: p50,
        p95_us: p95,
        result_sha256: hex(&Sha256::digest(&result)),
        passed: p95 <= 5_000,
    };
    println!(
        "{}",
        serde_json::to_string(&report).expect("benchmark report")
    );
    if !report.passed {
        std::process::exit(1);
    }
}

fn fixture() -> Vec<String> {
    const PREFIXES: [&str; 10] = [
        "JP-Tokyo",
        "US-Los Angeles",
        "SG-Singapore",
        "HK-Hong Kong",
        "RO-Bucharest",
        "NL-Amsterdam",
        "日本-优化",
        "新加坡-低延迟",
        "Fast-B2",
        "STATUS",
    ];
    (0..NAME_COUNT)
        .map(|index| {
            let prefix = PREFIXES[index % PREFIXES.len()];
            let suffix = "-controlled-benchmark-padding".repeat(3);
            format!("{prefix}-{index:04}{suffix}")
        })
        .collect()
}

fn run(names: &[String]) -> Vec<u8> {
    names
        .iter()
        .map(|name| {
            infer_display_territory([name.as_str()])
                .map(|code| code.as_str().as_bytes()[0])
                .unwrap_or_default()
        })
        .collect()
}

fn percentile(samples: &[u128], percentile: usize) -> u128 {
    samples[((samples.len() - 1) * percentile).div_ceil(100)]
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
