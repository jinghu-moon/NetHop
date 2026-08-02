use std::hint::black_box;
use std::time::Instant;

use base64::Engine;
use nethop_subscription::{
    CapabilityMatrix, FormatHint, ParserLimits, SourceId, SourceInput, StableConversion,
    convert_stable_sources, fingerprint_node,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

const NODE_COUNT: usize = 10_000;
const WARMUP_RUNS: usize = 5;
const SAMPLE_RUNS: usize = 20;
const UUID: &str = "550e8400-e29b-41d4-a716-446655440000";

#[derive(Serialize)]
struct CaseReport {
    name: &'static str,
    fixture_bytes: usize,
    fixture_sha256: String,
    source_count: usize,
    expected_items: usize,
    warmup_runs: usize,
    sample_runs: usize,
    samples_us: Vec<u128>,
    p50_us: u128,
    p95_us: u128,
    accepted: usize,
    rejected: usize,
    duplicate: usize,
    fingerprint_pass_us: u128,
}

#[derive(Serialize)]
struct BenchmarkReport {
    schema_version: u32,
    target: &'static str,
    node_count: usize,
    parser_body_limit: usize,
    cases: Vec<CaseReport>,
    process_vm_hwm_kib: Option<u64>,
}

struct Fixture {
    name: &'static str,
    sources: Vec<SourceInput>,
    total_bytes: usize,
    digest: String,
}

fn main() {
    let limits = ParserLimits::default();
    let requested_case = std::env::args().nth(1);
    if requested_case.as_deref() == Some("baseline") {
        println!(
            "{}",
            serde_json::json!({
                "schema_version": 1,
                "target": std::env::consts::ARCH,
                "process_vm_hwm_kib": vm_hwm_kib(),
            })
        );
        return;
    }
    let fixtures = match requested_case.as_deref() {
        None => all_fixtures(),
        Some(name) => vec![fixture_by_name(name)],
    };
    let cases = fixtures
        .into_iter()
        .map(|fixture| measure(fixture, &limits))
        .collect();
    let report = BenchmarkReport {
        schema_version: 1,
        target: std::env::consts::ARCH,
        node_count: NODE_COUNT,
        parser_body_limit: limits.max_body_bytes(),
        cases,
        process_vm_hwm_kib: vm_hwm_kib(),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("benchmark report must serialize")
    );
}

fn all_fixtures() -> Vec<Fixture> {
    vec![
        uri_fixture(450, false),
        uri_fixture(320, true),
        json_fixture(),
        yaml_fixture(),
        multi_source_fixture(),
    ]
}

fn fixture_by_name(name: &str) -> Fixture {
    match name {
        "uri_list" => uri_fixture(450, false),
        "base64_uri_list" => uri_fixture(320, true),
        "singbox_json" => json_fixture(),
        "clash_yaml" => yaml_fixture(),
        "multi_source" => multi_source_fixture(),
        _ => panic!("unknown benchmark case: {name}"),
    }
}

fn measure(fixture: Fixture, limits: &ParserLimits) -> CaseReport {
    let matrix = CapabilityMatrix::default();
    for _ in 0..WARMUP_RUNS {
        let conversion = run_once(&fixture.sources, limits, &matrix);
        black_box(conversion.outbounds_json.len());
    }
    let mut samples = Vec::with_capacity(SAMPLE_RUNS);
    let mut counts = (0, 0, 0);
    for _ in 0..SAMPLE_RUNS {
        let sources = fixture.sources.clone();
        let started = Instant::now();
        let conversion = convert_stable_sources(sources, limits, &matrix);
        samples.push(started.elapsed().as_micros());
        counts = (
            conversion.report.summary.accepted,
            conversion.report.summary.rejected,
            conversion.report.summary.duplicate,
        );
        black_box(conversion.outbounds_json.len());
    }
    let mut ordered = samples.clone();
    ordered.sort_unstable();
    let verification = run_once(&fixture.sources, limits, &matrix);
    let fingerprint_started = Instant::now();
    for node in &verification.nodes {
        black_box(fingerprint_node(&node.node));
    }
    let fingerprint_pass_us = fingerprint_started.elapsed().as_micros();
    CaseReport {
        name: fixture.name,
        fixture_bytes: fixture.total_bytes,
        fixture_sha256: fixture.digest,
        source_count: fixture.sources.len(),
        expected_items: NODE_COUNT,
        warmup_runs: WARMUP_RUNS,
        sample_runs: SAMPLE_RUNS,
        samples_us: samples,
        p50_us: percentile(&ordered, 50),
        p95_us: percentile(&ordered, 95),
        accepted: counts.0,
        rejected: counts.1,
        duplicate: counts.2,
        fingerprint_pass_us,
    }
}

fn run_once(
    sources: &[SourceInput],
    limits: &ParserLimits,
    matrix: &CapabilityMatrix,
) -> StableConversion {
    convert_stable_sources(sources.to_vec(), limits, matrix)
}

fn uri_fixture(padding: usize, base64_wrapped: bool) -> Fixture {
    let text = uri_lines(0, NODE_COUNT, padding);
    let bytes = if base64_wrapped {
        base64::engine::general_purpose::STANDARD
            .encode(text.as_bytes())
            .into_bytes()
    } else {
        text.into_bytes()
    };
    let name = if base64_wrapped {
        "base64_uri_list"
    } else {
        "uri_list"
    };
    fixture(
        name,
        vec![source(
            name,
            if base64_wrapped {
                FormatHint::Base64List
            } else {
                FormatHint::UriList
            },
            bytes,
        )],
    )
}

fn multi_source_fixture() -> Fixture {
    let sources = (0..4)
        .map(|source_index| {
            let start = source_index * (NODE_COUNT / 4);
            source(
                &format!("multi-{source_index}"),
                FormatHint::UriList,
                uri_lines(start, NODE_COUNT / 4, 450).into_bytes(),
            )
        })
        .collect();
    fixture("multi_source", sources)
}

fn uri_lines(start: usize, count: usize, padding: usize) -> String {
    let padding = "x".repeat(padding);
    (start..start + count)
        .map(|index| {
            if index % 10 == 0 {
                format!("invalid-{index}")
            } else {
                format!(
                    "trojan://bench-{index}@node-{index}.example:443?benchmark={padding}#node-{index}"
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn json_fixture() -> Fixture {
    let padding = "x".repeat(360);
    let nodes = (0..NODE_COUNT)
        .map(|index| {
            if index % 10 == 0 {
                format!(r#"{{"type":"wireguard","tag":"invalid-{index}"}}"#)
            } else {
                json_node(index, &padding)
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    fixture(
        "singbox_json",
        vec![source(
            "singbox-json",
            FormatHint::SingboxJson,
            format!("[{nodes}]").into_bytes(),
        )],
    )
}

fn json_node(index: usize, padding: &str) -> String {
    let server = format!("node-{index}.example");
    match index % 7 {
        0 => format!(
            r#"{{"type":"vless","server":"{server}","server_port":443,"uuid":"{UUID}","tls":{{"enabled":true}},"icon":"{padding}"}}"#
        ),
        1 => format!(
            r#"{{"type":"vmess","server":"{server}","server_port":443,"uuid":"{UUID}","security":"auto","tls":{{"enabled":true}},"icon":"{padding}"}}"#
        ),
        2 => format!(
            r#"{{"type":"shadowsocks","server":"{server}","server_port":443,"method":"aes-128-gcm","password":"bench-{index}","icon":"{padding}"}}"#
        ),
        3 => format!(
            r#"{{"type":"trojan","server":"{server}","server_port":443,"password":"bench-{index}","tls":{{"enabled":true}},"icon":"{padding}"}}"#
        ),
        4 => format!(
            r#"{{"type":"hysteria2","server":"{server}","server_port":443,"password":"bench-{index}","udp":true,"tls":{{"enabled":true}},"icon":"{padding}"}}"#
        ),
        5 => format!(
            r#"{{"type":"tuic","server":"{server}","server_port":443,"uuid":"{UUID}","password":"bench-{index}","udp":true,"tls":{{"enabled":true}},"icon":"{padding}"}}"#
        ),
        _ => format!(
            r#"{{"type":"anytls","server":"{server}","server_port":443,"password":"bench-{index}","tls":{{"enabled":true}},"icon":"{padding}"}}"#
        ),
    }
}

fn yaml_fixture() -> Fixture {
    let padding = "x".repeat(360);
    let mut text = String::from("proxies:\n");
    for index in 0..NODE_COUNT {
        if index % 10 == 0 {
            text.push_str(&format!(
                "  - {{ name: invalid-{index}, type: wireguard }}\n"
            ));
        } else {
            text.push_str("  - ");
            text.push_str(&yaml_node(index, &padding));
            text.push('\n');
        }
    }
    fixture(
        "clash_yaml",
        vec![source(
            "clash-yaml",
            FormatHint::ClashYaml,
            text.into_bytes(),
        )],
    )
}

fn yaml_node(index: usize, padding: &str) -> String {
    let server = format!("node-{index}.example");
    let common = format!("server: {server}, port: 443, icon: {padding}");
    match index % 7 {
        0 => format!("{{ name: node-{index}, type: vless, {common}, uuid: {UUID}, tls: true }}"),
        1 => format!("{{ name: node-{index}, type: vmess, {common}, uuid: {UUID}, tls: true }}"),
        2 => format!(
            "{{ name: node-{index}, type: ss, {common}, cipher: aes-128-gcm, password: bench-{index} }}"
        ),
        3 => format!(
            "{{ name: node-{index}, type: trojan, {common}, password: bench-{index}, tls: true }}"
        ),
        4 => format!(
            "{{ name: node-{index}, type: hysteria2, {common}, password: bench-{index}, tls: true, network: quic, udp: true }}"
        ),
        5 => format!(
            "{{ name: node-{index}, type: tuic, {common}, uuid: {UUID}, password: bench-{index}, tls: true, network: quic, udp: true }}"
        ),
        _ => format!(
            "{{ name: node-{index}, type: anytls, {common}, password: bench-{index}, tls: true }}"
        ),
    }
}

fn source(id: &str, format_hint: FormatHint, bytes: Vec<u8>) -> SourceInput {
    SourceInput {
        source_id: SourceId::new(id).expect("benchmark source id"),
        format_hint,
        bytes,
    }
}

fn fixture(name: &'static str, sources: Vec<SourceInput>) -> Fixture {
    let total_bytes = sources.iter().map(|source| source.bytes.len()).sum();
    let mut hasher = Sha256::new();
    for source in &sources {
        hasher.update(&source.bytes);
    }
    Fixture {
        name,
        sources,
        total_bytes,
        digest: hex(&hasher.finalize()),
    }
}

fn percentile(samples: &[u128], percentile: usize) -> u128 {
    let index = ((samples.len() - 1) * percentile).div_ceil(100);
    samples[index]
}

fn vm_hwm_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|line| {
        let value = line.strip_prefix("VmHWM:")?.trim();
        value.split_whitespace().next()?.parse().ok()
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
