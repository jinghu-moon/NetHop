use nethop_subscription::{
    Base64Alphabet, Base64Padding, DetectionError, DiagnosticCode, EvidenceStrength, FormatHint,
    ImportPayload, ParserLimits, PayloadOrigin, ReceivedAt, detect_bytes, detect_format,
    normalize_bytes,
};

fn now() -> ReceivedAt {
    ReceivedAt {
        wall_clock_unix_ms: 1,
        monotonic_nanos: 2,
    }
}

fn payload(bytes: &[u8], hint: FormatHint) -> ImportPayload {
    ImportPayload::new(
        PayloadOrigin::PastedText,
        bytes.to_vec(),
        hint,
        None,
        None,
        now(),
        &ParserLimits::default(),
    )
    .unwrap()
}

#[test]
fn raw_payload_limit_is_checked_before_normalization_or_detection() {
    let limits = ParserLimits::default();
    let below = vec![b'A'; limits.max_body_bytes() - 1];
    let exact = vec![b'A'; limits.max_body_bytes()];
    let above = vec![b'A'; limits.max_body_bytes() + 1];

    assert!(normalize_bytes(&below, &limits).is_ok());
    assert!(normalize_bytes(&exact, &limits).is_ok());
    assert_eq!(
        detect_bytes(&above, FormatHint::Auto, &limits)
            .unwrap_err()
            .code(),
        DiagnosticCode::InputTooLarge
    );
}

#[test]
fn normalization_strips_bom_and_unifies_line_iteration_without_touching_credentials() {
    let limits = ParserLimits::default();
    let input = b"\xEF\xBB\xBF  vless://User+Case/%2F?token=A+B/C%2f\r\nvmess://AbC+/=\r  ";
    let normalized = normalize_bytes(input, &limits).unwrap();

    assert_eq!(
        normalized.as_str(),
        "vless://User+Case/%2F?token=A+B/C%2f\r\nvmess://AbC+/="
    );
    let lines: Vec<_> = normalized.lines().map(|line| line.text()).collect();
    assert_eq!(
        lines,
        ["vless://User+Case/%2F?token=A+B/C%2f", "vmess://AbC+/="]
    );
    assert!(std::ptr::eq(
        normalized.as_str().as_ptr(),
        input[5..].as_ptr()
    ));
}

#[test]
fn normalization_rejects_invalid_utf8_and_nul_without_lossy_conversion() {
    let limits = ParserLimits::default();
    assert_eq!(
        normalize_bytes(&[0xff, 0xfe], &limits).unwrap_err().code(),
        DiagnosticCode::InvalidUtf8
    );
    assert_eq!(
        normalize_bytes(b"vless://ok\0hidden", &limits)
            .unwrap_err()
            .code(),
        DiagnosticCode::NulByte
    );
}

#[test]
fn expected_format_is_a_constraint_not_a_parser_override() {
    let limits = ParserLimits::default();
    let valid = detect_bytes(b"vless://id@example.com:443", FormatHint::UriList, &limits).unwrap();
    assert_eq!(valid.format(), FormatHint::UriList);

    let mismatch =
        detect_bytes(br#"{"outbounds":[]}"#, FormatHint::ClashYaml, &limits).unwrap_err();
    assert_eq!(mismatch.code(), DiagnosticCode::FormatHintMismatch);

    let arbitrary = detect_bytes(b"not a subscription", FormatHint::UriList, &limits).unwrap_err();
    assert_eq!(arbitrary.code(), DiagnosticCode::FormatHintMismatch);
}

#[test]
fn json_requires_supported_top_level_structure_and_beats_weak_text_evidence() {
    let limits = ParserLimits::default();
    for input in [
        br#"{"outbounds":[{"type":"vless","tag":"n"}]}"#.as_slice(),
        br#"[{"type":"trojan","tag":"n"}]"#.as_slice(),
    ] {
        let detected = detect_bytes(input, FormatHint::Auto, &limits).unwrap();
        assert_eq!(detected.format(), FormatHint::SingboxJson);
        assert_eq!(detected.strength(), EvidenceStrength::Strong);
    }
    assert_eq!(
        detect_bytes(br#"{"message":"dmxlc3M6Ly8="}"#, FormatHint::Auto, &limits)
            .unwrap_err()
            .code(),
        DiagnosticCode::UnknownFormat
    );
}

#[test]
fn yaml_requires_a_top_level_proxies_sequence() {
    let limits = ParserLimits::default();
    let yaml =
        b"proxies:\n  - name: node\n    type: vless\n    server: example.com\n    port: 443\n";
    let detected = detect_bytes(yaml, FormatHint::Auto, &limits).unwrap();
    assert_eq!(detected.format(), FormatHint::ClashYaml);
    assert_eq!(detected.strength(), EvidenceStrength::Strong);

    assert_eq!(
        detect_bytes(
            b"this sentence happens to contain proxies: but is not YAML",
            FormatHint::Auto,
            &limits,
        )
        .unwrap_err()
        .code(),
        DiagnosticCode::UnknownFormat
    );
}

#[test]
fn uri_evidence_requires_a_whitelisted_scheme_at_line_start() {
    let limits = ParserLimits::default();
    let supported = b"# comment\n  vless://id@example.com:443\nsocks5://ignored.example:1080";
    assert_eq!(
        detect_bytes(supported, FormatHint::Auto, &limits)
            .unwrap()
            .format(),
        FormatHint::UriList
    );

    for unsupported in [
        b"socks5://example.com:1080".as_slice(),
        b"https://example.test/?next=vless://id@example.com:443".as_slice(),
        b"prefix vless://id@example.com:443".as_slice(),
    ] {
        assert_eq!(
            detect_bytes(unsupported, FormatHint::Auto, &limits)
                .unwrap_err()
                .code(),
            DiagnosticCode::UnknownFormat
        );
    }
}

#[test]
fn base64_is_only_weak_evidence_and_records_alphabet_and_padding() {
    let limits = ParserLimits::default();
    let standard =
        detect_bytes(b"dmxlc3M6Ly9leGFtcGxlLmNvbQ==", FormatHint::Auto, &limits).unwrap();
    assert_eq!(standard.format(), FormatHint::Base64List);
    assert_eq!(standard.strength(), EvidenceStrength::Weak);
    assert_eq!(
        standard.base64_details().unwrap().alphabet,
        Base64Alphabet::Standard
    );
    assert_eq!(
        standard.base64_details().unwrap().padding,
        Base64Padding::Present
    );

    let url_safe = detect_bytes(b"_-7dzA", FormatHint::Auto, &limits).unwrap();
    assert_eq!(
        url_safe.base64_details().unwrap().alphabet,
        Base64Alphabet::UrlSafe
    );
    assert_eq!(
        url_safe.base64_details().unwrap().padding,
        Base64Padding::Missing
    );
}

#[test]
fn conflicting_strong_evidence_is_ambiguous_instead_of_order_dependent() {
    let limits = ParserLimits::default();
    let input = b"proxies:\n  - name: node\n    type: vless\nvless://id@example.com:443\n";
    let error = detect_bytes(input, FormatHint::Auto, &limits).unwrap_err();
    assert_eq!(error.code(), DiagnosticCode::AmbiguousFormat);
    assert_eq!(error.candidates().len(), 2);
}

#[test]
fn malformed_json_is_terminal_and_never_falls_back() {
    let error = detect_bytes(
        br#"{"outbounds":["#,
        FormatHint::Auto,
        &ParserLimits::default(),
    )
    .unwrap_err();
    assert_eq!(error.code(), DiagnosticCode::InvalidJson);
    assert_eq!(error.terminal_format(), Some(FormatHint::SingboxJson));
}

#[test]
fn malformed_yaml_is_terminal_and_never_falls_back() {
    let error = detect_bytes(
        b"proxies:\n  - [unterminated\n",
        FormatHint::Auto,
        &ParserLimits::default(),
    )
    .unwrap_err();
    assert_eq!(error.code(), DiagnosticCode::InvalidYaml);
    assert_eq!(error.terminal_format(), Some(FormatHint::ClashYaml));
}

#[test]
fn public_detector_is_carrier_independent_and_preserves_hint_errors() {
    let limits = ParserLimits::default();
    let content = b"vless://id@example.com:443";
    let origins = [
        PayloadOrigin::QrRawValue,
        PayloadOrigin::LocalFile {
            display_name: Some("nodes.txt".into()),
        },
        PayloadOrigin::PastedText,
    ];
    for origin in origins {
        let candidate = ImportPayload::new(
            origin,
            content.to_vec(),
            FormatHint::Auto,
            None,
            None,
            now(),
            &limits,
        )
        .unwrap();
        assert_eq!(
            detect_format(&candidate, &limits).unwrap().format(),
            FormatHint::UriList
        );
    }

    let mismatch = payload(content, FormatHint::SingboxJson);
    assert!(matches!(
        detect_format(&mismatch, &limits),
        Err(DetectionError::FormatHintMismatch { .. })
    ));
}
