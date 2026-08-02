use std::collections::BTreeMap;
use std::io::{self, Read};

use nethop_subscription::{
    CandidateStatus, CapabilityMatrix, DiagnosticCode, Digest, FormatHint, ParserIpcResponse,
    ParserLimits, SourceId, SourceInput, convert_stable_sources, detect_bytes,
};

fn main() {
    let limits = ParserLimits::default();
    let mut body = Vec::new();
    io::stdin()
        .take((limits.max_body_bytes() + 1) as u64)
        .read_to_end(&mut body)
        .expect("stdin read failed");
    if body.len() > limits.max_body_bytes() {
        emit_error("body_too_large");
    }

    let digest = Digest::sha256(&body).hex();
    let detected = match detect_bytes(&body, FormatHint::Auto, &limits) {
        Ok(value) => value,
        Err(error) => {
            println!(
                "{}",
                serde_json::json!({
                    "bytes": body.len(),
                    "digest_prefix": &digest[..12],
                    "detected": null,
                    "diagnostic": diagnostic_name(error.code()),
                })
            );
            return;
        }
    };
    let format = detected.format();
    if std::env::args().nth(1).as_deref() == Some("schema") {
        emit_schema(&body, format, &digest);
        return;
    }
    let parse_format = match format {
        FormatHint::UriList
        | FormatHint::Base64List
        | FormatHint::ClashYaml
        | FormatHint::SingboxJson => Some(format),
        #[cfg(feature = "format-surfboard")]
        FormatHint::IniProfile => Some(FormatHint::SurfboardIni),
        _ => None,
    };
    let Some(parse_format) = parse_format else {
        println!(
            "{}",
            serde_json::json!({
                "bytes": body.len(),
                "digest_prefix": &digest[..12],
                "detected": format_name(format),
                "parser": "experimental",
            })
        );
        return;
    };

    let input_bytes = body.len();
    let matrix = CapabilityMatrix::default();
    let conversion = convert_stable_sources(
        vec![SourceInput {
            source_id: SourceId::new("stdin-probe").expect("static source id is valid"),
            format_hint: parse_format,
            bytes: body,
        }],
        &limits,
        &matrix,
    );
    let response = ParserIpcResponse::from_conversion(
        SourceId::new("probe-request").expect("static request id is valid"),
        &conversion,
        &limits,
    )
    .expect("bounded conversion must produce an IPC response");
    let candidate_state = match response.candidate() {
        CandidateStatus::Ready { .. } => "ready",
        CandidateStatus::AcceptedZero => "accepted_zero",
        CandidateStatus::Rejected { .. } => "rejected",
    };
    let mut protocol_counts = BTreeMap::<&str, usize>::new();
    for item in &conversion.report.items {
        if let Some(protocol) = item.protocol {
            *protocol_counts.entry(protocol.as_str()).or_default() += 1;
        }
    }
    println!(
        "{}",
        serde_json::json!({
            "bytes": input_bytes,
            "digest_prefix": &digest[..12],
            "detected": format_name(format),
            "accepted": conversion.report.summary.accepted,
            "duplicate": conversion.report.summary.duplicate,
            "rejected": conversion.report.summary.rejected,
            "diagnostic_counts": conversion.report.diagnostic_counts,
            "protocol_counts": protocol_counts,
            "source_success": conversion.report.summary.source_success,
            "ipc_schema_version": response.schema_version(),
            "mapping_digest": matrix.mapping_digest(),
            "candidate_state": candidate_state,
        })
    );
}

fn emit_schema(body: &[u8], format: FormatHint, digest: &str) {
    let document =
        match format {
            #[cfg(feature = "format-clash-yaml")]
            FormatHint::ClashYaml => serde_saphyr::from_slice::<serde_json::Value>(body)
                .expect("validated YAML must decode"),
            #[cfg(not(feature = "format-clash-yaml"))]
            FormatHint::ClashYaml => {
                println!(
                    "{}",
                    serde_json::json!({
                        "digest_prefix": &digest[..12],
                        "detected": format_name(format),
                        "schema": "feature_disabled",
                    })
                );
                return;
            }
            FormatHint::SingboxJson => serde_json::from_slice::<serde_json::Value>(body)
                .expect("validated JSON must decode"),
            _ => {
                println!(
                    "{}",
                    serde_json::json!({
                        "digest_prefix": &digest[..12],
                        "detected": format_name(format),
                        "schema": "not_structured",
                    })
                );
                return;
            }
        };
    let nodes = match format {
        FormatHint::ClashYaml => document
            .get("proxies")
            .and_then(serde_json::Value::as_array),
        FormatHint::SingboxJson => document
            .get("outbounds")
            .and_then(serde_json::Value::as_array)
            .or_else(|| document.as_array()),
        _ => None,
    };
    let mut key_counts = BTreeMap::<String, usize>::new();
    let mut type_counts = BTreeMap::<String, usize>::new();
    for node in nodes.into_iter().flatten() {
        let Some(object) = node.as_object() else {
            continue;
        };
        for key in object.keys() {
            *key_counts.entry(key.clone()).or_default() += 1;
        }
        if let Some(protocol) = object.get("type").and_then(serde_json::Value::as_str) {
            *type_counts.entry(protocol.to_owned()).or_default() += 1;
        }
    }
    println!(
        "{}",
        serde_json::json!({
            "digest_prefix": &digest[..12],
            "detected": format_name(format),
            "node_keys": key_counts,
            "protocol_counts": type_counts,
        })
    );
}

fn format_name(format: FormatHint) -> &'static str {
    match format {
        FormatHint::Auto => "auto",
        FormatHint::UriList => "uri_list",
        FormatHint::Base64List => "base64_list",
        FormatHint::ClashYaml => "clash_yaml",
        FormatHint::SingboxJson => "singbox_json",
        FormatHint::IniProfile => "ini_profile",
        FormatHint::SurfboardIni => "surfboard_ini",
    }
}

fn diagnostic_name(code: DiagnosticCode) -> &'static str {
    match code {
        DiagnosticCode::UnknownFormat => "unknown_format",
        DiagnosticCode::AmbiguousFormat => "ambiguous_format",
        DiagnosticCode::InvalidJson => "invalid_json",
        DiagnosticCode::InvalidYaml => "invalid_yaml",
        DiagnosticCode::EmptyInput => "empty_input",
        _ => "detection_failed",
    }
}

fn emit_error(code: &str) -> ! {
    println!("{}", serde_json::json!({ "error": code }));
    std::process::exit(2)
}
