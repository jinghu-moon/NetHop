#![cfg(feature = "fetch")]

use std::{env, fs, process};

use nethop_subscription::{
    CandidateAcceptance, CapabilityMatrix, FetchClient, FetchPolicy, FetchRequest, FormatHint,
    ParserLimits, RequestProfile, SourceCache, SourceId, SourceInput, convert_stable_sources,
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
    let Some(path) = env::args_os().nth(1) else {
        emit_error("missing_config_path");
    };
    let bytes = fs::read(path).unwrap_or_else(|_| emit_error("source_config_read_failed"));
    let config = serde_json::from_slice::<SourceConfig>(&bytes)
        .unwrap_or_else(|_| emit_error("source_config_json_failed"));
    if config.schema != "nethop-sources-v1" {
        emit_error("source_config_schema_failed");
    }
    let Some(source) = config.sources.into_iter().next() else {
        emit_error("source_missing");
    };
    let source_id = SourceId::new(source.id).unwrap_or_else(|_| emit_error("invalid_source_id"));
    let policy = FetchPolicy::default();
    let limits = ParserLimits::default();
    let request = FetchRequest::new(
        source_id.clone(),
        &source.url,
        &source.mirrors,
        source.request_profile,
        &policy,
    )
    .unwrap_or_else(|_| emit_error("invalid_fetch_request"));
    let client = FetchClient::new(policy, limits);
    let outcome = client
        .fetch(&request, &SourceCache::default(), |_| {
            CandidateAcceptance::Accepted
        })
        .unwrap_or_else(|error| emit_error(error.code().to_string()));
    let conversion = convert_stable_sources(
        vec![SourceInput {
            source_id,
            format_hint: source.expected_format,
            bytes: outcome.body().to_vec(),
        }],
        &limits,
        &CapabilityMatrix::default(),
    );
    println!(
        "{}",
        serde_json::json!({
            "ok": conversion.report.summary.source_success,
            "bytes": outcome.body().len(),
            "accepted": conversion.report.summary.accepted,
            "duplicate": conversion.report.summary.duplicate,
            "rejected": conversion.report.summary.rejected,
        })
    );
}

fn emit_error(code: impl AsRef<str>) -> ! {
    println!(
        "{}",
        serde_json::json!({ "ok": false, "code": code.as_ref() })
    );
    process::exit(2);
}
