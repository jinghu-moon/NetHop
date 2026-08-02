#![cfg(all(
    feature = "format-uri",
    feature = "format-base64",
    feature = "format-clash-yaml",
    feature = "format-singbox-json"
))]

use std::time::{Duration, Instant};

use nethop_subscription::{
    CapabilityMatrix, FormatHint, ParserLimits, SourceId, SourceInput, convert_stable_sources,
};
use sha2::{Digest, Sha256};

const WARMUP_RUNS: usize = 5;
const MEASURED_RUNS: usize = 20;

fn source_id(value: &str) -> SourceId {
    SourceId::new(value).expect("test source id")
}

fn fixture_lines(count: usize) -> String {
    (0..count)
        .map(|index| {
            format!("trojan://benchmark-secret-{index}@node-{index}.example:443#node-{index}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn fixture_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn run_fixture(bytes: Vec<u8>, hint: FormatHint) -> Duration {
    let start = Instant::now();
    let conversion = convert_stable_sources(
        vec![SourceInput {
            source_id: source_id("benchmark"),
            format_hint: hint,
            bytes,
        }],
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    );
    assert!(!conversion.nodes.is_empty());
    start.elapsed()
}

#[test]
fn deterministic_fixture_generator_has_stable_digest_and_bounded_node_count() {
    let first = fixture_lines(1_000);
    let second = fixture_lines(1_000);
    assert_eq!(first, second);
    assert_eq!(
        fixture_digest(first.as_bytes()),
        fixture_digest(second.as_bytes())
    );
    assert!(first.len() < ParserLimits::default().max_body_bytes());
}

#[test]
fn host_runner_uses_frozen_warmup_and_sample_counts_for_uri_and_base64() {
    let uri = fixture_lines(256).into_bytes();
    let base64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        fixture_lines(256).as_bytes(),
    )
    .into_bytes();
    for _ in 0..WARMUP_RUNS {
        let _ = run_fixture(uri.clone(), FormatHint::UriList);
        let _ = run_fixture(base64.clone(), FormatHint::Base64List);
    }
    let uri_samples = (0..MEASURED_RUNS)
        .map(|_| run_fixture(uri.clone(), FormatHint::UriList))
        .collect::<Vec<_>>();
    let base64_samples = (0..MEASURED_RUNS)
        .map(|_| run_fixture(base64.clone(), FormatHint::Base64List))
        .collect::<Vec<_>>();
    assert_eq!(uri_samples.len(), MEASURED_RUNS);
    assert_eq!(base64_samples.len(), MEASURED_RUNS);
}

#[test]
fn release_profile_and_parser_feature_closure_remain_frozen() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace root");
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).expect("workspace manifest");
    assert!(manifest.contains("opt-level = 3"));
    assert!(manifest.contains("lto = \"thin\""));
    assert!(manifest.contains("codegen-units = 1"));
    assert!(manifest.contains("strip = \"symbols\""));

    let crate_manifest = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"),
    )
    .expect("crate manifest");
    assert!(crate_manifest.contains("fetch = [\"parser\", \"dep:ureq\", \"dep:url\"]"));
    assert!(crate_manifest.contains("sha2 ="));
    assert!(!crate_manifest.contains("blake3 ="));
    assert!(!crate_manifest.contains("base64-simd"));
}
