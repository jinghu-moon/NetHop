use std::{collections::BTreeMap, str::FromStr};

use nethop_subscription::*;

fn now() -> ReceivedAt {
    ReceivedAt {
        wall_clock_unix_ms: 1,
        monotonic_nanos: 2,
    }
}

fn source_id() -> SourceId {
    SourceId::new("source-a").unwrap()
}

fn endpoint() -> Endpoint {
    Endpoint::new("example.com", 443).unwrap()
}

fn uuid() -> UuidValue {
    UuidValue::parse("550e8400-e29b-41d4-a716-446655440000").unwrap()
}

fn baseline_node(
    protocol: ProxyProtocol,
    transport: TransportOptions,
    tls: bool,
    udp: bool,
) -> UnvalidatedNode {
    let credentials = match protocol {
        ProxyProtocol::Vless => Credentials::Vless { uuid: uuid() },
        ProxyProtocol::Vmess => Credentials::Vmess {
            uuid: uuid(),
            alter_id: 0,
            security: BoundedText::new("auto", 32).unwrap(),
        },
        ProxyProtocol::Shadowsocks => Credentials::Shadowsocks {
            method: BoundedText::new("2022-blake3-aes-128-gcm", 64).unwrap(),
            password: SecretString::new("password"),
            plugin: None,
        },
        ProxyProtocol::Trojan => Credentials::Trojan {
            password: SecretString::new("password"),
        },
        ProxyProtocol::Hysteria2 => Credentials::Hysteria2 {
            password: SecretString::new("password"),
            obfs: None,
        },
        ProxyProtocol::Tuic => Credentials::Tuic {
            uuid: uuid(),
            password: SecretString::new("password"),
        },
        ProxyProtocol::AnyTls => Credentials::AnyTls {
            password: SecretString::new("password"),
        },
    };
    UnvalidatedNode {
        display_name: DisplayName::new("node").unwrap(),
        protocol,
        endpoint: endpoint(),
        credentials,
        tls: TlsOptions {
            enabled: tls,
            ..TlsOptions::default()
        },
        transport,
        protocol_options: ProtocolOptions::None,
        capabilities: Capabilities {
            tcp: true,
            udp,
            ipv6: false,
            quic: matches!(protocol, ProxyProtocol::Hysteria2 | ProxyProtocol::Tuic),
            tls,
        },
        source_refs: vec![SourceRef {
            source_id: source_id(),
            item_index: 0,
            format: FormatHint::Auto,
            line: Some(1),
        }],
    }
}

#[test]
fn parser_limits_match_frozen_budget_and_reject_expansion() {
    let limits = ParserLimits::default();
    assert_eq!(limits.max_body_bytes(), 5 * 1024 * 1024);
    assert_eq!(limits.max_nodes(), 10_000);
    assert_eq!(limits.max_line_bytes(), 16 * 1024);
    assert_eq!(limits.max_depth(), 64);
    assert_eq!(limits.max_string_bytes(), 64 * 1024);
    assert_eq!(limits.max_report_bytes(), 8 * 1024 * 1024);
    assert!(ParserLimits::new(5 * 1024 * 1024 + 1, 10_000, 16 * 1024, 64, 64 * 1024).is_err());
    let json = serde_json::to_value(limits).unwrap();
    assert_eq!(json["max_body_bytes"], 5 * 1024 * 1024);
}

#[test]
fn all_payload_origins_share_bounded_content_model() {
    let limits = ParserLimits::default();
    let origins = [
        PayloadOrigin::QrRawValue,
        PayloadOrigin::LocalFile {
            display_name: Some("subscription.yaml".into()),
        },
        PayloadOrigin::PastedText,
        PayloadOrigin::HttpResponse {
            metadata: FetchMetadata {
                status_code: 200,
                declared_content_type: Some("text/plain".into()),
                response_bytes: 3,
                final_scheme: HttpScheme::Https,
            },
        },
    ];
    for origin in origins {
        let payload = ImportPayload::new(
            origin,
            b"ss://example".to_vec(),
            FormatHint::Auto,
            Some(source_id()),
            None,
            now(),
            &limits,
        )
        .unwrap();
        assert_eq!(payload.bytes(), b"ss://example");
        assert!(payload.metadata().content_digest.hex().len() == 64);
    }
    assert!(
        ImportPayload::new(
            PayloadOrigin::PastedText,
            vec![0; 5 * 1024 * 1024 + 1],
            FormatHint::Auto,
            None,
            None,
            now(),
            &limits
        )
        .is_err()
    );
}

#[test]
fn source_metadata_contains_digests_but_no_raw_secret() {
    let limits = ParserLimits::default();
    let secret = "token=do-not-leak";
    let payload = ImportPayload::from_text(
        PayloadOrigin::PastedText,
        secret.into(),
        FormatHint::Auto,
        Some(source_id()),
        now(),
        &limits,
    )
    .unwrap();
    let metadata = payload.metadata();
    let encoded = serde_json::to_string(&metadata).unwrap();
    assert!(!encoded.contains(secret));
    assert!(!encoded.contains("token="));
    assert_eq!(metadata.origin_kind, PayloadOriginKind::PastedText);
}

#[test]
fn import_payload_debug_and_file_name_are_redacted_or_rejected() {
    let limits = ParserLimits::default();
    let payload = ImportPayload::from_text(
        PayloadOrigin::PastedText,
        "token=secret".into(),
        FormatHint::Auto,
        None,
        now(),
        &limits,
    )
    .unwrap();
    let debug = format!("{payload:?}");
    assert!(!debug.contains("token=secret"));
    assert!(
        ImportPayload::new(
            PayloadOrigin::LocalFile {
                display_name: Some("C:\\secret.yaml".into()),
            },
            b"x".to_vec(),
            FormatHint::Auto,
            None,
            None,
            now(),
            &limits,
        )
        .is_err()
    );
}

#[test]
fn secret_string_redacts_debug_display_and_serialization() {
    let secret = SecretString::new("uuid=550e8400-e29b-41d4-a716-446655440000");
    assert_eq!(format!("{secret:?}"), "<redacted>");
    assert_eq!(secret.to_string(), "<redacted>");
    assert!(!serde_json::to_string(&secret).unwrap().contains("550e8400"));
}

#[test]
fn diagnostic_codes_are_stable_and_unknown_is_forward_compatible() {
    let codes = [
        DiagnosticCode::EmptyInput,
        DiagnosticCode::InputTooLarge,
        DiagnosticCode::InvalidUtf8,
        DiagnosticCode::UnknownFormat,
        DiagnosticCode::InvalidYaml,
        DiagnosticCode::DuplicateKey,
        DiagnosticCode::UnsupportedProtocol,
        DiagnosticCode::InvalidEndpoint,
        DiagnosticCode::ActiveLimitExceeded,
    ];
    let names: Vec<_> = codes.iter().map(DiagnosticCode::as_str).collect();
    assert_eq!(
        names.len(),
        names.iter().collect::<std::collections::HashSet<_>>().len()
    );
    assert_eq!(DiagnosticCode::parse("future_code").as_str(), "future_code");
    assert!(DiagnosticCode::from_str("future_code").is_err());
    assert_eq!(
        serde_json::to_string(&DiagnosticCode::InvalidEndpoint).unwrap(),
        "\"invalid_endpoint\""
    );
}

#[test]
fn source_location_is_one_based_and_bounded() {
    assert!(SourceLocation::new(0, Some(0), Some(1), None).is_err());
    assert!(SourceLocation::new(0, Some(1), Some(1), Some("a".repeat(257))).is_err());
    let location =
        SourceLocation::new(3, Some(1), Some(2), Some("proxies[0].server".into())).unwrap();
    assert_eq!(location.line, Some(1));
    assert_eq!(location.column, Some(2));
}

#[test]
fn node_diagnostic_allows_only_bounded_non_secret_parameters() {
    let diagnostic = NodeDiagnostic::new(DiagnosticCode::InvalidEndpoint, Severity::Error)
        .with_parameter("field", "server")
        .with_parameter("password", "secret-canary")
        .with_parameter("field", "safe");
    let encoded = serde_json::to_string(&diagnostic).unwrap();
    assert!(encoded.contains("safe"));
    assert!(!encoded.contains("secret-canary"));
    assert!(!diagnostic.parameters.contains_key("password"));
}

#[test]
fn protocol_and_transport_whitelists_reject_unknown_values() {
    for protocol in ProxyProtocol::ALL {
        assert!(protocol.as_str().parse::<ProxyProtocol>().is_ok());
    }
    assert!("http".parse::<ProxyProtocol>().is_err());
    assert!("wireguard".parse::<ProxyProtocol>().is_err());
    assert!("ws".parse::<TransportKind>().is_ok());
    assert!("xhttp".parse::<TransportKind>().is_err());
}

#[test]
fn endpoint_and_transport_fields_are_bounded_without_dns() {
    assert!(Endpoint::new("", 443).is_err());
    assert!(Endpoint::new("example.com", 0).is_err());
    assert!(Endpoint::new("example.com", 65535).is_ok());
    assert!(Endpoint::new("example com", 443).is_err());
    assert!(BoundedText::new("/path", 4).is_err());
    assert!(BoundedText::new("/path", 8).is_ok());
    assert!(
        TransportOptions::Grpc {
            service_name: BoundedText::new("proxy", 64).unwrap()
        }
        .kind()
            == TransportKind::Grpc
    );
}

#[test]
fn credential_variants_are_protocol_specific_and_uuid_is_strict() {
    assert_eq!(
        Credentials::Vless { uuid: uuid() }.protocol(),
        ProxyProtocol::Vless
    );
    assert!(UuidValue::parse("{550e8400-e29b-41d4-a716-446655440000}").is_err());
    assert!(UuidValue::parse("00000000-0000-0000-0000-000000000000").is_err());
    assert!(UuidValue::parse("550e8400e29b41d4a716446655440000").is_ok());
}

#[test]
fn tls_and_reality_options_are_typed_and_bounded() {
    let tls = TlsOptions {
        enabled: true,
        server_name: Some(BoundedText::new("example.com", 256).unwrap()),
        alpn: vec![BoundedText::new("h2", 64).unwrap()],
        reality: Some(RealityOptions {
            public_key: SecretString::new("public-key"),
            short_id: None,
            fingerprint: Some(BoundedText::new("chrome", 64).unwrap()),
        }),
        ..TlsOptions::default()
    };
    let debug = format!("{tls:?}");
    assert!(!debug.contains("public-key"));
    assert!(BoundedText::new("\u{7f}", 4).is_err());
}

#[test]
fn validated_proxy_node_cannot_bypass_credential_or_capability_checks() {
    let matrix = CapabilityMatrix::default();
    let valid = ProxyNode::validate(
        baseline_node(ProxyProtocol::Vless, TransportOptions::Tcp, true, false),
        &matrix,
    )
    .unwrap();
    assert_eq!(valid.protocol(), ProxyProtocol::Vless);
    assert_eq!(valid.endpoint().port(), 443);

    let mut mismatch = baseline_node(ProxyProtocol::Vless, TransportOptions::Tcp, true, false);
    mismatch.credentials = Credentials::Trojan {
        password: SecretString::new("password"),
    };
    assert!(ProxyNode::validate(mismatch, &matrix).is_err());

    let quic_without_tls = baseline_node(
        ProxyProtocol::Hysteria2,
        TransportOptions::Quic,
        false,
        true,
    );
    assert!(ProxyNode::validate(quic_without_tls, &matrix).is_err());
}

#[test]
fn capability_matrix_is_versioned_evidenced_and_deny_by_default() {
    let matrix = CapabilityMatrix::default();
    assert_eq!(matrix.sing_box_version, "1.13.15");
    assert!(matrix.entry_count() >= 7);
    let unsupported = CapabilityQuery {
        protocol: ProxyProtocol::Vless,
        transport: TransportKind::Quic,
        tls: true,
        reality: false,
        utls: false,
        udp: true,
        flow: Some("unknown".into()),
        plugin: None,
    };
    assert!(!matrix.supports(&unsupported));

    let invalid = CapabilityMatrix::new(1, "latest", vec![], vec![]);
    assert!(invalid.is_err());
    let missing_evidence = CapabilityMatrix::new(
        1,
        "1.13.15",
        vec![],
        vec![CapabilityEntry {
            query: unsupported,
            supported: true,
            evidence: None,
        }],
    );
    assert!(missing_evidence.is_err());
}

#[test]
fn capability_matrix_has_no_secret_fixture_fields() {
    let matrix = CapabilityMatrix::default();
    let mut map = BTreeMap::new();
    map.insert("matrix", serde_json::to_string(&matrix).unwrap());
    assert!(!map["matrix"].contains("password"));
}
