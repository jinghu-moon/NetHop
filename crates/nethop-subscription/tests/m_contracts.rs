use nethop_subscription::{
    ACTIVE_OUTBOUND_BASELINE, CONVERSION_NODE_LIMIT, CURRENT_FINGERPRINT_SCHEMA,
    CURRENT_REPORT_SCHEMA_VERSION, CandidateStatus, CapabilityMatrix, DiagnosticCode, FormatHint,
    IpcPayloadOrigin, MANAGED_ACTIVE_OUTBOUND_LIMIT, ParserIpcRequest, ParserIpcRequestError,
    ParserIpcResponse, ParserLimits, ReceivedAt, ReportCompatibility, ReportReadError,
    RequestProfile, SourceId, SourceInput, convert_stable_sources, read_versioned_report,
    write_versioned_report,
};

#[path = "support/fake_module_host.rs"]
mod fake_module_host;
use fake_module_host::{
    FakeGenerationStore, FakeHostError, FakeModuleParserHost, FakePeer, FakeRootManager,
};

#[test]
fn sing_box_1_13_15_connection_critical_mapping_is_not_silently_dropped() {
    let conversion = convert_stable_sources(
        vec![
            SourceInput {
                source_id: SourceId::new("mapping-vless").unwrap(),
                format_hint: FormatHint::UriList,
                bytes: b"vless://550e8400-e29b-41d4-a716-446655440000@vless.example:443?security=reality&pbk=fixture-public-key&sid=0123456789abcdef&fp=chrome#vless"
                    .to_vec(),
            },
            SourceInput {
                source_id: SourceId::new("mapping-tuic").unwrap(),
                format_hint: FormatHint::UriList,
                bytes: b"tuic://550e8400-e29b-41d4-a716-446655440000:fixture-password@tuic.example:443?congestion_control=bbr#tuic"
                    .to_vec(),
            },
            SourceInput {
                source_id: SourceId::new("mapping-hysteria2").unwrap(),
                format_hint: FormatHint::UriList,
                bytes: b"hysteria2://fixture-password@hy2.example:443?obfs=salamander&obfs-password=fixture-obfs-password#hy2"
                    .to_vec(),
            },
        ],
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    );
    assert_eq!(conversion.report.summary.accepted, 3);
    assert_eq!(conversion.report.summary.rejected, 0);

    let outbounds: serde_json::Value = serde_json::from_str(&conversion.outbounds_json).unwrap();
    let vless = &outbounds[0];
    assert_eq!(vless["tls"]["utls"]["enabled"], true);
    assert_eq!(vless["tls"]["reality"]["enabled"], true);
    assert_eq!(outbounds[1]["congestion_control"], "bbr");
    assert_eq!(outbounds[2]["obfs"]["type"], "salamander");
    assert_eq!(outbounds[2]["obfs"]["password"], "fixture-obfs-password");
}

#[test]
fn sing_box_1_13_15_mapping_manifest_is_strict_and_digest_pinned() {
    let manifest = include_str!("../manifests/sing-box-1.13.15-mapping.json");
    let matrix = CapabilityMatrix::from_manifest_json(manifest).unwrap();
    assert_eq!(matrix.schema_version, 1);
    assert_eq!(matrix.sing_box_version, "1.13.15");
    assert_eq!(matrix.sing_box_tag, "v1.13.15");
    assert_eq!(
        matrix.sing_box_commit,
        "3708fa18766cda1f11b77f6ed9c7bd61688f17df"
    );
    assert_eq!(matrix.go_version, "1.24.7");
    assert_eq!(
        matrix.mapping_digest(),
        "d11f0497be4c0d731a6ef75a2543b8fc502957bca1712903fbfcf33c4788b1d6"
    );
    assert_eq!(matrix.entry_count(), 31);

    let wrong_version = manifest.replacen("1.13.15", "1.14.0", 1);
    assert!(CapabilityMatrix::from_manifest_json(&wrong_version).is_err());
    let unknown_field = manifest.replacen(
        "\"schema_version\": 1,",
        "\"schema_version\": 1, \"development_field\": true,",
        1,
    );
    assert!(CapabilityMatrix::from_manifest_json(&unknown_field).is_err());
}

#[test]
fn alioth_fixture_records_the_actual_prebuilt_sing_box_identity() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/device/alioth-parser-integration.json"
    ))
    .unwrap();
    let core = &fixture["build"]["sing_box_core"];
    assert_eq!(core["origin"], "official_prebuilt");
    assert_eq!(core["version"], "1.13.15");
    assert_eq!(core["revision"], "3708fa18766cda1f11b77f6ed9c7bd61688f17df");
    assert_eq!(core["go"], "1.25.12");
    let tags = core["tags"].as_array().unwrap();
    for required in ["with_gvisor", "with_quic", "with_utls", "with_wireguard"] {
        assert!(
            tags.iter().any(|tag| tag == required),
            "missing tag {required}"
        );
    }
}

#[test]
fn sing_box_check_fixture_covers_exactly_the_nine_parser_protocols() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/mapping/sing-box-1.13.15-check.json")).unwrap();
    let mut types = fixture["outbounds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|outbound| outbound["type"].as_str().unwrap())
        .collect::<Vec<_>>();
    types.sort_unstable();
    assert_eq!(
        types,
        [
            "anytls",
            "http",
            "hysteria2",
            "shadowsocks",
            "socks",
            "trojan",
            "tuic",
            "vless",
            "vmess"
        ]
    );
}

#[test]
#[cfg(feature = "format-singbox-json")]
fn sing_box_check_fixture_round_trips_all_nine_protocols_through_parser() {
    let conversion = convert_stable_sources(
        vec![SourceInput {
            source_id: SourceId::new("sing-box-check-fixture").unwrap(),
            format_hint: FormatHint::SingboxJson,
            bytes: include_bytes!("fixtures/mapping/sing-box-1.13.15-check.json").to_vec(),
        }],
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    );
    assert_eq!(
        conversion.report.summary.accepted, 9,
        "diagnostics={:?}",
        conversion.report.diagnostic_counts
    );
    assert_eq!(conversion.report.summary.rejected, 0);
}

#[test]
fn fake_magisk_and_kernelsu_hosts_enforce_root_timeout_and_candidate_contract() {
    let frame = include_bytes!("fixtures/ipc/parser-request-v1.json");
    let mut responses = Vec::new();
    for manager in [FakeRootManager::Magisk, FakeRootManager::KernelSu] {
        let host = FakeModuleParserHost::new(manager, 30_000);
        assert_eq!(host.manager(), manager);
        assert_eq!(
            host.handle(FakePeer { uid: 10_000 }, 1, frame),
            Err(FakeHostError::PermissionDenied)
        );
        assert_eq!(
            host.handle(FakePeer { uid: 0 }, 30_001, frame),
            Err(FakeHostError::TimedOut)
        );
        let response = host.handle(FakePeer { uid: 0 }, 1, frame).unwrap();
        let json: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["candidate"]["state"], "ready");
        assert!(json.get("outbounds_json").is_none());
        assert!(!response.contains("fixture-password"));
        responses.push(response);
    }
    assert_eq!(responses[0], responses[1]);
}

#[test]
fn active_outbound_and_conversion_limits_have_distinct_boundary_semantics() {
    assert_eq!(ACTIVE_OUTBOUND_BASELINE, 500);
    assert_eq!(MANAGED_ACTIVE_OUTBOUND_LIMIT, 2_000);
    assert_eq!(CONVERSION_NODE_LIMIT, 10_000);

    for count in [ACTIVE_OUTBOUND_BASELINE, MANAGED_ACTIVE_OUTBOUND_LIMIT] {
        let conversion = conversion_with_nodes(count);
        assert_eq!(conversion.nodes.len(), count);
        let response = ParserIpcResponse::from_conversion(
            SourceId::new(format!("boundary-{count}")).unwrap(),
            &conversion,
            &ParserLimits::default(),
        )
        .unwrap();
        assert!(matches!(
            response.candidate(),
            CandidateStatus::Ready { node_count, .. } if *node_count == count
        ));
    }

    for count in [MANAGED_ACTIVE_OUTBOUND_LIMIT + 1, CONVERSION_NODE_LIMIT] {
        let conversion = conversion_with_nodes(count);
        assert_eq!(conversion.nodes.len(), count);
        let response = ParserIpcResponse::from_conversion(
            SourceId::new(format!("boundary-{count}")).unwrap(),
            &conversion,
            &ParserLimits::default(),
        )
        .unwrap();
        assert_eq!(
            response.candidate(),
            &CandidateStatus::Rejected {
                code: DiagnosticCode::ActiveLimitExceeded
            }
        );
    }

    let overflow = conversion_with_nodes(CONVERSION_NODE_LIMIT + 1);
    assert!(overflow.nodes.is_empty());
    assert_eq!(overflow.report.summary.accepted, 0);
    assert_eq!(
        overflow
            .report
            .diagnostic_counts
            .get(&DiagnosticCode::NodeLimitExceeded),
        Some(&1)
    );
}

#[test]
fn last_known_good_changes_only_after_a_ready_candidate_is_committed() {
    let limits = ParserLimits::default();
    let mut store = FakeGenerationStore::default();
    let ready = ParserIpcResponse::from_conversion(
        SourceId::new("ready-request").unwrap(),
        &conversion_with_nodes(1),
        &limits,
    )
    .unwrap();
    assert!(store.commit_ready(&ready));
    let active = store.current_digest().unwrap().to_owned();
    assert_eq!(store.source_cache_digest(), Some(active.as_str()));

    let zero = convert_stable_sources(
        vec![SourceInput {
            source_id: SourceId::new("zero-source").unwrap(),
            format_hint: FormatHint::UriList,
            bytes: b"# no nodes".to_vec(),
        }],
        &limits,
        &CapabilityMatrix::default(),
    );
    let zero_response =
        ParserIpcResponse::from_conversion(SourceId::new("zero-request").unwrap(), &zero, &limits)
            .unwrap();
    assert_eq!(zero_response.candidate(), &CandidateStatus::AcceptedZero);
    assert!(!store.commit_ready(&zero_response));

    let unsafe_response = ParserIpcResponse::from_conversion(
        SourceId::new("unsafe-request").unwrap(),
        &conversion_with_nodes(MANAGED_ACTIVE_OUTBOUND_LIMIT + 1),
        &limits,
    )
    .unwrap();
    assert!(!store.commit_ready(&unsafe_response));

    let host = FakeModuleParserHost::new(FakeRootManager::Magisk, 30_000);
    assert_eq!(
        host.handle(FakePeer { uid: 0 }, 1, b"not-json"),
        Err(FakeHostError::InvalidRequest)
    );
    assert_eq!(store.current_digest(), Some(active.as_str()));
    assert_eq!(store.source_cache_digest(), Some(active.as_str()));
}

#[test]
fn cross_environment_manifest_freezes_schema_mapping_and_limits() {
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/device/cross-environment-compatibility.json"
    ))
    .unwrap();
    assert_eq!(manifest["parser_ipc_schema_version"], 1);
    assert_eq!(manifest["fingerprint_schema"], CURRENT_FINGERPRINT_SCHEMA);
    assert_eq!(
        manifest["mapping_digest"],
        CapabilityMatrix::default().mapping_digest()
    );
    assert_eq!(
        manifest["limits"]["active_baseline"],
        ACTIVE_OUTBOUND_BASELINE
    );
    assert_eq!(
        manifest["limits"]["managed_active"],
        MANAGED_ACTIVE_OUTBOUND_LIMIT
    );
    assert_eq!(
        manifest["limits"]["conversion_nodes"],
        CONVERSION_NODE_LIMIT
    );
    let environments = manifest["environments"].as_array().unwrap();
    assert_eq!(environments.len(), 3);
    assert!(
        environments
            .iter()
            .all(|environment| environment["status"] == "passed")
    );
}

fn conversion_with_nodes(count: usize) -> nethop_subscription::StableConversion {
    let bytes = (0..count)
        .map(|index| format!("trojan://fixture-{index}@node-{index}.example:443"))
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes();
    convert_stable_sources(
        vec![SourceInput {
            source_id: SourceId::new(format!("nodes-{count}")).unwrap(),
            format_hint: FormatHint::UriList,
            bytes,
        }],
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    )
}

#[test]
fn android_request_profiles_have_stable_wire_names() {
    for (wire_name, expected) in [
        ("mihomo", RequestProfile::Mihomo),
        ("clash_standard", RequestProfile::ClashStandard),
        ("surfboard", RequestProfile::Surfboard),
        ("sing_box", RequestProfile::SingBox),
        ("sing_box_android", RequestProfile::SingBoxAndroid),
    ] {
        let value = serde_json::to_value(expected).unwrap();
        assert_eq!(value, serde_json::json!(wire_name));
        assert_eq!(
            serde_json::from_value::<RequestProfile>(value).unwrap(),
            expected
        );
    }

    for unsupported in [
        "stash",
        "surge",
        "shadowrocket",
        "sing_box_tv_os",
        "quantumult_x",
    ] {
        assert!(serde_json::from_value::<RequestProfile>(serde_json::json!(unsupported)).is_err());
    }
}

#[test]
fn parser_ipc_request_v1_golden_is_bounded_and_converts_to_import_payload() {
    let bytes = include_bytes!("fixtures/ipc/parser-request-v1.json");
    let limits = ParserLimits::default();
    let request = ParserIpcRequest::from_json(bytes, &limits).unwrap();

    assert_eq!(request.schema_version(), 1);
    assert_eq!(request.request_id().as_str(), "req-001");
    assert_eq!(request.source_id().as_str(), "source-001");
    assert_eq!(request.expected_format(), FormatHint::UriList);
    assert_eq!(request.request_profile(), RequestProfile::NetHopGeneric);
    assert_eq!(request.origin(), &IpcPayloadOrigin::PastedText);

    let payload = request
        .to_import_payload(
            ReceivedAt {
                wall_clock_unix_ms: 1,
                monotonic_nanos: 2,
            },
            &limits,
        )
        .unwrap();
    assert_eq!(
        payload.bytes(),
        b"trojan://fixture-password@example.com:443"
    );
    assert_eq!(payload.expected_format(), FormatHint::UriList);
    assert_eq!(payload.source_id(), Some(request.source_id()));
}

#[test]
fn parser_ipc_rejects_unknown_or_security_bypass_fields() {
    let template = include_str!("fixtures/ipc/parser-request-v1.json");
    for field in [
        r#""active_limit":5000"#,
        r#""allow_private_network":true"#,
        r#""full_config":{"inbounds":[]}"#,
        r#""skip_sing_box_check":true"#,
        r#""script":"run()""#,
    ] {
        let tampered = template.replacen("{", &format!("{{{field},"), 1);
        let error =
            ParserIpcRequest::from_json(tampered.as_bytes(), &ParserLimits::default()).unwrap_err();
        assert_eq!(error, ParserIpcRequestError::InvalidRequest);
    }
}

#[test]
fn parser_ipc_frame_payload_and_origin_metadata_are_validated() {
    let limits = ParserLimits::new(16, 16, 64, 8, 1024).unwrap();
    let oversized_frame = vec![b' '; nethop_subscription::MAX_PARSER_IPC_FRAME_BYTES + 1];
    assert_eq!(
        ParserIpcRequest::from_json(&oversized_frame, &limits).unwrap_err(),
        ParserIpcRequestError::FrameTooLarge
    );

    let oversized_payload = include_str!("fixtures/ipc/parser-request-v1.json").replace(
        "dHJvamFuOi8vZml4dHVyZS1wYXNzd29yZEBleGFtcGxlLmNvbTo0NDM=",
        "YWFhYWFhYWFhYWFhYWFhYWFhYQ==",
    );
    assert_eq!(
        ParserIpcRequest::from_json(oversized_payload.as_bytes(), &limits).unwrap_err(),
        ParserIpcRequestError::PayloadTooLarge
    );

    let insecure_fetch = include_str!("fixtures/ipc/parser-request-v1.json").replace(
        r#""kind": "pasted_text""#,
        r#""kind": "http_response", "status_code": 200, "declared_content_type": "application/yaml", "final_scheme": "http""#,
    );
    assert_eq!(
        ParserIpcRequest::from_json(insecure_fetch.as_bytes(), &ParserLimits::default())
            .unwrap_err(),
        ParserIpcRequestError::InsecureHttpResponse
    );
}

#[test]
fn parser_ipc_response_exposes_only_bounded_report_and_candidate_summary() {
    let limits = ParserLimits::default();
    let conversion = convert_stable_sources(
        vec![SourceInput {
            source_id: SourceId::new("source-001").unwrap(),
            format_hint: FormatHint::UriList,
            bytes: b"trojan://fixture-password@example.com:443".to_vec(),
        }],
        &limits,
        &CapabilityMatrix::default(),
    );
    let response =
        ParserIpcResponse::from_conversion(SourceId::new("req-001").unwrap(), &conversion, &limits)
            .unwrap();

    assert!(matches!(
        response.candidate(),
        CandidateStatus::Ready { node_count: 1, .. }
    ));
    let json = response.to_json(&limits).unwrap();
    assert!(json.len() <= limits.max_report_bytes());
    assert!(!json.contains("fixture-password"));
    assert!(!json.contains("outbounds_json"));
    assert!(!json.contains("inbounds"));
    assert!(!json.contains("route"));
}

#[test]
fn parser_ipc_schema_has_no_socket_or_runtime_implementation() {
    let source = include_str!("../src/ipc.rs");
    assert!(!source.contains("UnixListener"));
    assert!(!source.contains("TcpListener"));
    assert!(!source.contains("tokio"));
    assert!(!source.contains("interprocess"));
}

#[test]
fn qr_ipc_accepts_only_confirmed_utf8_raw_values() {
    let confirmed_text = qr_request_json("trojan://fixture-password@example.com:443", true);
    let request =
        ParserIpcRequest::from_json(confirmed_text.as_bytes(), &ParserLimits::default()).unwrap();
    assert_eq!(
        request.origin(),
        &IpcPayloadOrigin::QrRawValue {
            user_confirmed: true
        }
    );

    let image_bytes = serde_json::json!({
        "schema_version": 1,
        "request_id": "qr-002",
        "source_id": "qr-source",
        "origin": { "kind": "qr_raw_value", "user_confirmed": true },
        "expected_format": "uri_list",
        "request_profile": "net_hop_generic",
        "source_url_digest": null,
        "payload_base64": "iVBORw0KGgo="
    })
    .to_string();
    assert_eq!(
        ParserIpcRequest::from_json(image_bytes.as_bytes(), &ParserLimits::default()).unwrap_err(),
        ParserIpcRequestError::InvalidQrRawValue
    );

    let unconfirmed_url = qr_request_json("https://subscription.example/sub", false);
    assert_eq!(
        ParserIpcRequest::from_json(unconfirmed_url.as_bytes(), &ParserLimits::default())
            .unwrap_err(),
        ParserIpcRequestError::UnconfirmedUrl
    );
}

#[test]
fn qr_ipc_rejects_display_text_and_image_carriers() {
    for kind in ["qr_display_text", "qr_image_bytes"] {
        let value = serde_json::json!({
            "schema_version": 1,
            "request_id": "qr-003",
            "source_id": "qr-source",
            "origin": { "kind": kind, "user_confirmed": true },
            "expected_format": "uri_list",
            "request_profile": "net_hop_generic",
            "source_url_digest": null,
            "payload_base64": "dHJvamFuOi8vZml4dHVyZS1wYXNzd29yZEBleGFtcGxlLmNvbTo0NDM="
        })
        .to_string();
        assert_eq!(
            ParserIpcRequest::from_json(value.as_bytes(), &ParserLimits::default()).unwrap_err(),
            ParserIpcRequestError::InvalidRequest
        );
    }
}

#[test]
fn report_reader_marks_legacy_reports_and_requires_current_fingerprint_schema() {
    let conversion = convert_stable_sources(
        vec![SourceInput {
            source_id: SourceId::new("legacy-source").unwrap(),
            format_hint: FormatHint::UriList,
            bytes: b"trojan://fixture-password@example.com:443".to_vec(),
        }],
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    );
    let legacy = serde_json::to_vec(&conversion.report).unwrap();
    assert_eq!(
        read_versioned_report(&legacy).unwrap().compatibility(),
        ReportCompatibility::LegacyRebuildRequired
    );

    let current = write_versioned_report(&conversion.report).unwrap();
    let parsed = read_versioned_report(&current).unwrap();
    assert_eq!(parsed.compatibility(), ReportCompatibility::Current);
    assert_eq!(parsed.schema_version(), CURRENT_REPORT_SCHEMA_VERSION);
    assert_eq!(parsed.fingerprint_schema(), CURRENT_FINGERPRINT_SCHEMA);

    let wrong_fingerprint = String::from_utf8(current)
        .unwrap()
        .replace(CURRENT_FINGERPRINT_SCHEMA, "nh-fp-blake3-v1");
    assert_eq!(
        read_versioned_report(wrong_fingerprint.as_bytes()).unwrap_err(),
        ReportReadError::FingerprintSchemaMismatch
    );

    let wrong_version = String::from_utf8(write_versioned_report(&conversion.report).unwrap())
        .unwrap()
        .replace("\"schema_version\":1", "\"schema_version\":2");
    assert_eq!(
        read_versioned_report(wrong_version.as_bytes()).unwrap_err(),
        ReportReadError::UnsupportedSchema
    );
}

fn qr_request_json(raw_value: &str, user_confirmed: bool) -> String {
    use base64::Engine;

    serde_json::json!({
        "schema_version": 1,
        "request_id": "qr-001",
        "source_id": "qr-source",
        "origin": { "kind": "qr_raw_value", "user_confirmed": user_confirmed },
        "expected_format": "uri_list",
        "request_profile": "net_hop_generic",
        "source_url_digest": null,
        "payload_base64": base64::engine::general_purpose::STANDARD.encode(raw_value)
    })
    .to_string()
}
