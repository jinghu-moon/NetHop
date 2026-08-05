#![cfg(feature = "subscription-update")]

use std::{env, fs, io, path::PathBuf, process};

use nethop_core::{
    CaptureMode, CapturePolicy, ClashApi, GenerationId, GenerationStore, ManagedOptions, TunStack,
};
use nethop_subscription::{
    CandidateAcceptance, CapabilityMatrix, FetchClient, FetchPolicy, FetchRequest, FormatHint,
    ParserLimits, RequestProfile, SourceCache, SourceId, SourceInput, convert_stable_sources,
};
use nethopd::{
    CandidateChecker, CoreProcessLimits, CoreProcessRunner, HealthProbe, RunnerLimits,
    SingBoxCheckRunner, StartupLivenessProbe, build_candidate,
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceConfig {
    schema: String,
    sources: Vec<Source>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    id: String,
    url: String,
    #[serde(default)]
    mirrors: Vec<String>,
    expected_format: FormatHint,
    request_profile: RequestProfile,
}

fn main() {
    let mut args = env::args_os().skip(1);
    let Some(root) = args.next().map(PathBuf::from) else {
        emit_error("arguments", "missing_root");
    };
    let Some(sing_box) = args.next().map(PathBuf::from) else {
        emit_error("arguments", "missing_sing_box");
    };
    if args.next().is_some() || !root.is_absolute() || !sing_box.is_absolute() {
        emit_error("arguments", "invalid_path");
    }
    let config = serde_json::from_reader::<_, SourceConfig>(io::stdin())
        .unwrap_or_else(|_| emit_error("config", "invalid_json"));
    if config.schema != "nethop-sources-v1" {
        emit_error("config", "invalid_schema");
    }
    let Some(source) = config.sources.into_iter().next() else {
        emit_error("config", "source_missing");
    };
    let source_id =
        SourceId::new(source.id).unwrap_or_else(|_| emit_error("config", "invalid_source_id"));
    let policy = FetchPolicy::default();
    let limits = ParserLimits::default();
    let request = FetchRequest::new(
        source_id.clone(),
        &source.url,
        &source.mirrors,
        source.request_profile,
        &policy,
    )
    .unwrap_or_else(|_| emit_error("fetch", "invalid_request"));
    let mut cache = SourceCache::default();
    let client = FetchClient::new(policy, limits);
    let outcome = client
        .fetch(&request, &cache, |_| CandidateAcceptance::Accepted)
        .unwrap_or_else(|error| emit_error("fetch", error.code().to_string()));
    cache
        .commit(&outcome, &limits)
        .unwrap_or_else(|_| emit_error("fetch", "cache_commit_failed"));
    let conversion = convert_stable_sources(
        vec![SourceInput {
            source_id,
            format_hint: source.expected_format,
            bytes: outcome.body().to_vec(),
        }],
        &limits,
        &CapabilityMatrix::default(),
    );
    if !conversion.report.summary.source_success {
        emit_error("parse", "source_failed");
    }
    let capture = CapturePolicy::new(
        CaptureMode::Tproxy,
        true,
        Some(7893),
        Some(0x20_000),
        Vec::new(),
        vec![0],
    )
    .unwrap_or_else(|_| emit_error("compose", "capture_failed"));
    let clash_api = ClashApi::new("127.0.0.1:9090", "diagnostic-secret-32-bytes-long-00")
        .unwrap_or_else(|_| emit_error("compose", "api_failed"));
    let generation = GenerationId::new(1).expect("one is a valid generation");
    let candidate = build_candidate(
        generation,
        &conversion,
        capture,
        clash_api,
        TunStack::System,
        ManagedOptions::default(),
    )
    .unwrap_or_else(|_| emit_error("compose", "candidate_failed"));
    fs::create_dir_all(&root).unwrap_or_else(|_| emit_error("store", "root_create_failed"));
    let store =
        GenerationStore::new(&root).unwrap_or_else(|_| emit_error("store", "store_create_failed"));
    let prepared = store
        .prepare_candidate(&candidate)
        .unwrap_or_else(|_| emit_error("store", "prepare_failed"));
    let runner =
        SingBoxCheckRunner::new(&sing_box, store.generations_root(), RunnerLimits::default())
            .unwrap_or_else(|error| emit_error("check", error.code().as_str()));
    runner
        .check(&prepared.config_path())
        .unwrap_or_else(|error| emit_error("check", error.code().as_str()));
    let sealed = store
        .seal_candidate(&prepared)
        .unwrap_or_else(|_| emit_error("store", "seal_failed"));
    let launcher = CoreProcessRunner::new(
        &sing_box,
        store.generations_root(),
        CoreProcessLimits::default(),
    )
    .unwrap_or_else(|error| emit_error("start", error.code().as_str()));
    let mut process = launcher
        .start(&sealed.config_path())
        .unwrap_or_else(|error| emit_error("start", error.code().as_str()));
    StartupLivenessProbe::default()
        .wait_healthy(&mut process)
        .unwrap_or_else(|_| emit_error("health", "startup_liveness_failed"));
    process
        .stop()
        .unwrap_or_else(|error| emit_error("stop", error.code().as_str()));
    println!(
        "{}",
        serde_json::json!({
            "ok": true,
            "stage": "core_stopped",
            "accepted": conversion.report.summary.accepted,
            "duplicate": conversion.report.summary.duplicate,
            "rejected": conversion.report.summary.rejected,
            "node_count": candidate.config().node_count(),
        })
    );
}

fn emit_error(stage: &'static str, code: impl AsRef<str>) -> ! {
    println!(
        "{}",
        serde_json::json!({ "ok": false, "stage": stage, "code": code.as_ref() })
    );
    process::exit(2);
}
