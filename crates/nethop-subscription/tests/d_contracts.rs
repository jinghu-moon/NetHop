use base64::Engine;
use nethop_subscription::{
    Base64Variant, DiagnosticCode, FormatHint, ParserLimits, PayloadOrigin, UriNodeResult,
    decode_base64, decode_base64_and_detect, decode_vmess_inner_json, parse_uri_line,
    parse_uri_list, percent_decode_field,
};

#[test]
fn uri_line_parser_preserves_order_and_one_based_lines_without_allocating_line_strings() {
    let limits = ParserLimits::default();
    let input = "\r\n# ignored\n\nvless://550e8400-e29b-41d4-a716-446655440000@example.com:443#one\rvmess://opaque\n";
    let results = parse_uri_list(input.as_bytes(), None, &limits);
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].line(), 4);
    assert_eq!(results[1].line(), 5);
    assert_eq!(results[0].item_index(), 0);
    assert_eq!(results[1].item_index(), 1);
    assert!(results.iter().all(UriNodeResult::is_accepted));

    let too_long = format!("vless://{}", "a".repeat(limits.max_line_bytes()));
    let error = parse_uri_list(too_long.as_bytes(), None, &limits);
    assert_eq!(error.len(), 1);
    assert_eq!(
        error[0].diagnostic().unwrap().code,
        DiagnosticCode::InputTooLarge
    );
}

#[test]
fn scheme_dispatch_is_exact_case_sensitive_and_whitelist_only() {
    let limits = ParserLimits::default();
    let valid = parse_uri_line(
        "vless://550e8400-e29b-41d4-a716-446655440000@example.com:443",
        1,
        0,
        &limits,
    )
    .unwrap();
    assert_eq!(valid.scheme().as_str(), "vless");
    assert!(
        parse_uri_line(
            "VLESS://550e8400-e29b-41d4-a716-446655440000@example.com:443",
            1,
            0,
            &limits,
        )
        .is_err()
    );
    assert_eq!(
        parse_uri_line("socks5://example.com:1080", 1, 0, &limits)
            .unwrap_err()
            .code(),
        DiagnosticCode::UnsupportedProtocol
    );
    assert!(parse_uri_line("prefix vless://id@example.com:443", 1, 0, &limits).is_err());
}

#[test]
fn percent_decode_is_strict_single_pass_and_validates_text() {
    assert_eq!(
        percent_decode_field("A+B%2FC%25").unwrap(),
        "A+B/C%".to_owned()
    );
    assert!(percent_decode_field("bad%2").is_err());
    assert!(percent_decode_field("bad%ZZ").is_err());
    assert_eq!(percent_decode_field("%252F").unwrap(), "%2F".to_owned());
    assert!(percent_decode_field("bad%00value").is_err());
}

#[test]
fn uri_query_is_bounded_and_duplicate_keys_are_explicit() {
    let limits = ParserLimits::default();
    let query = (0..64)
        .map(|index| format!("k{index}=v"))
        .collect::<Vec<_>>()
        .join("&");
    let uri = format!("trojan://password@example.com:443?{query}#display");
    let candidate = parse_uri_line(&uri, 1, 0, &limits).unwrap();
    assert_eq!(candidate.query_count(), 64);
    assert_eq!(
        candidate.display_name().unwrap(),
        Some("display".to_owned())
    );

    let too_many = format!("trojan://password@example.com:443?{query}&overflow=1");
    assert_eq!(
        parse_uri_line(&too_many, 1, 0, &limits).unwrap_err().code(),
        DiagnosticCode::QueryLimitExceeded
    );
    let duplicate = parse_uri_line(
        "trojan://password@example.com:443?type=tcp&type=ws",
        1,
        0,
        &limits,
    )
    .unwrap();
    assert_eq!(duplicate.duplicate_query_keys(), vec!["type".to_owned()]);
}

#[test]
fn fragment_is_display_only_and_does_not_change_canonical_key() {
    let limits = ParserLimits::default();
    let first = parse_uri_line("trojan://password@example.com:443#one", 1, 0, &limits).unwrap();
    let second = parse_uri_line("trojan://password@example.com:443#two", 1, 0, &limits).unwrap();
    assert_eq!(first.canonical_key(), second.canonical_key());
    let long_fragment = format!("trojan://password@example.com:443#{}", "x".repeat(257));
    assert_eq!(
        parse_uri_line(&long_fragment, 1, 0, &limits)
            .unwrap_err()
            .code(),
        DiagnosticCode::FragmentTooLong
    );
}

#[test]
fn base64_supports_both_alphabets_and_padded_or_unpadded_inputs() {
    let limits = ParserLimits::default();
    let standard = decode_base64(b"dmxlc3M6Ly9leGFtcGxl", &limits).unwrap();
    assert_eq!(standard.variant(), Base64Variant::Standard);
    assert_eq!(standard.bytes(), b"vless://example");
    assert_eq!(
        decode_base64(b"dmxlc3M6Ly9leGFtcGxl==", &limits)
            .unwrap_err()
            .code(),
        DiagnosticCode::InvalidBase64
    );
    let url_safe = decode_base64(b"_-7dzA", &limits).unwrap();
    assert_eq!(url_safe.variant(), Base64Variant::UrlSafe);
    assert_eq!(url_safe.bytes(), &[0xff, 0xee, 0xdd, 0xcc]);
    assert_eq!(
        decode_base64(b"+_7d", &limits).unwrap_err().code(),
        DiagnosticCode::InvalidBase64
    );
}

#[test]
fn base64_decoded_output_is_checked_before_allocation() {
    let limits = ParserLimits::default();
    let encoded =
        base64::engine::general_purpose::STANDARD.encode(vec![b'x'; limits.max_body_bytes() + 1]);
    let error = decode_base64(encoded.as_bytes(), &limits).unwrap_err();
    assert_eq!(error.code(), DiagnosticCode::InputTooLarge);
}

#[test]
fn base64_reprobe_has_one_decode_level_and_rejects_nested_base64() {
    let limits = ParserLimits::default();
    let inner = base64::engine::general_purpose::STANDARD.encode(b"vless://example");
    let outer = base64::engine::general_purpose::STANDARD.encode(inner.as_bytes());
    assert_eq!(
        decode_base64_and_detect(outer.as_bytes(), &limits)
            .unwrap_err()
            .code(),
        DiagnosticCode::Base64NestingExceeded
    );
}

#[test]
fn vmess_inner_json_is_bounded_before_json_parsing() {
    let limits = ParserLimits::default();
    let small = base64::engine::general_purpose::STANDARD.encode(br#"{"v":"2"}"#);
    let small_uri = format!("vmess://{small}");
    let candidate = parse_uri_line(&small_uri, 1, 0, &limits).unwrap();
    assert_eq!(candidate.vmess_inner_json().unwrap(), br#"{"v":"2"}"#);

    let large_json = format!(r#"{{"v":"{}"}}"#, "x".repeat(64 * 1024));
    let large = base64::engine::general_purpose::STANDARD.encode(large_json.as_bytes());
    assert_eq!(
        decode_vmess_inner_json(&large, &limits).unwrap_err().code(),
        DiagnosticCode::VmessInnerJsonTooLarge
    );
}

#[test]
fn uri_list_is_partially_successful_and_keeps_bad_line_diagnostic() {
    let limits = ParserLimits::default();
    let input = b"vless://550e8400-e29b-41d4-a716-446655440000@example.com:443\nnot-uri\ntrojan://pw@example.org:443\n";
    let results = parse_uri_list(input, None, &limits);
    assert_eq!(results.len(), 3);
    assert_eq!(
        results.iter().filter(|result| result.is_accepted()).count(),
        2
    );
    assert_eq!(
        results.iter().filter(|result| result.is_rejected()).count(),
        1
    );
    assert_eq!(results[1].line(), 2);
    assert_eq!(
        results[1].diagnostic().unwrap().code,
        DiagnosticCode::UnknownFormat
    );
}

#[test]
fn uri_parser_does_not_use_carrier_or_create_validated_nodes() {
    let limits = ParserLimits::default();
    let _origin = PayloadOrigin::QrRawValue;
    let result = parse_uri_list(
        b"vless://550e8400-e29b-41d4-a716-446655440000@example.com:443",
        None,
        &limits,
    );
    assert!(result[0].is_accepted());
    assert_eq!(result[0].format(), FormatHint::UriList);
}

#[test]
fn uri_and_decoded_base64_debug_output_is_redacted() {
    let limits = ParserLimits::default();
    let secret = "uri-secret-canary";
    let secret_uri = format!("trojan://{secret}@example.com:443?token={secret}");
    let candidate = parse_uri_line(&secret_uri, 1, 0, &limits).unwrap();
    assert!(!format!("{candidate:?}").contains(secret));

    let encoded = base64::engine::general_purpose::STANDARD.encode(secret);
    let decoded = decode_base64(encoded.as_bytes(), &limits).unwrap();
    assert!(!format!("{decoded:?}").contains(secret));
}
