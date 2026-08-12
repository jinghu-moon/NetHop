#![cfg(feature = "fetch")]

use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use nethop_subscription::{
    ContentEncoding, FetchAgentConfig, FetchDiagnosticCode, FetchEndpointKind, FetchPolicy,
    FetchPolicyError, FetchRequest, LocalFetchProxy, ParserLimits, RequestProfile, SourceCache,
    SourceId, SourceUrlError, UREQ_SECURITY_ADAPTER_VERSION, decode_response_body,
    is_denied_ssrf_address, next_redirect, validate_peer_address, validate_peer_in_approved_set,
    validate_resolved_addresses, validate_response_limits, validate_source_url,
};

#[test]
fn local_fetch_proxy_is_loopback_only_and_redacts_credentials() {
    let proxy = LocalFetchProxy::new(
        "127.0.0.1:7894".parse().unwrap(),
        "nethop",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    )
    .unwrap();
    assert_eq!(proxy.endpoint().to_string(), "127.0.0.1:7894");
    assert!(!format!("{proxy:?}").contains("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
    assert!(
        LocalFetchProxy::new(
            "0.0.0.0:7894".parse().unwrap(),
            "nethop",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .is_err()
    );
    assert!(LocalFetchProxy::new("127.0.0.1:7894".parse().unwrap(), "nethop", "short").is_err());
}

#[test]
fn fetch_accepts_only_https_urls_without_user_info() {
    assert!(validate_source_url("https://subscription.example/path").is_ok());
    assert_eq!(
        validate_source_url("http://subscription.example").unwrap_err(),
        SourceUrlError::NonHttps
    );
    assert_eq!(
        validate_source_url("https://token@subscription.example").unwrap_err(),
        SourceUrlError::UserInfo
    );
}

#[test]
fn ssrf_policy_rejects_private_and_link_local_addresses_at_resolution_and_peer_time() {
    for address in [
        "0.0.0.1".parse().unwrap(),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
        "100.64.0.1".parse().unwrap(),
        IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254)),
        "192.0.2.1".parse().unwrap(),
        "198.18.0.1".parse().unwrap(),
        "198.51.100.1".parse().unwrap(),
        "203.0.113.1".parse().unwrap(),
        "240.0.0.1".parse().unwrap(),
        IpAddr::V6(Ipv6Addr::LOCALHOST),
        "::ffff:127.0.0.1".parse().unwrap(),
        "100::1".parse().unwrap(),
        "fd00::1".parse().unwrap(),
        "fe80::1".parse().unwrap(),
        "2001:db8::1".parse().unwrap(),
    ] {
        assert!(is_denied_ssrf_address(address));
        assert_eq!(
            validate_peer_address(address).unwrap_err(),
            FetchPolicyError::DeniedAddress
        );
    }
    let public: IpAddr = "1.1.1.1".parse().unwrap();
    assert!(validate_resolved_addresses(&[public]).is_ok());
    assert_eq!(
        validate_resolved_addresses(&[public, IpAddr::V4(Ipv4Addr::LOCALHOST)]).unwrap_err(),
        FetchPolicyError::DeniedAddress
    );

    assert!(validate_peer_in_approved_set(public, &[public]).is_ok());
    assert_eq!(
        validate_peer_in_approved_set("8.8.8.8".parse().unwrap(), &[public]).unwrap_err(),
        FetchPolicyError::PeerMismatch
    );
}

#[test]
fn redirects_are_manual_bounded_and_revalidated() {
    let policy = FetchPolicy::default();
    let source = validate_source_url("https://subscription.example/one").unwrap();
    let redirect = next_redirect(&source, "/two", 0, &policy).unwrap();
    assert_eq!(redirect.as_str(), "https://subscription.example/two");
    assert_eq!(
        next_redirect(&source, "http://blocked.example", 0, &policy).unwrap_err(),
        FetchPolicyError::NonHttps
    );
    assert_eq!(
        next_redirect(&source, "/four", policy.max_redirects, &policy).unwrap_err(),
        FetchPolicyError::RedirectLimit
    );
}

#[test]
fn response_limits_apply_to_headers_encoded_and_decoded_bodies() {
    let policy = FetchPolicy::default();
    let limits = ParserLimits::default();
    assert!(validate_response_limits(0, 1, 1, &policy, &limits).is_ok());
    assert_eq!(
        validate_response_limits(policy.max_response_header_bytes + 1, 1, 1, &policy, &limits)
            .unwrap_err(),
        FetchPolicyError::HeadersTooLarge
    );
    assert_eq!(
        validate_response_limits(1, 1, limits.max_body_bytes() + 1, &policy, &limits).unwrap_err(),
        FetchPolicyError::BodyTooLarge
    );
}

#[test]
fn fetch_agent_config_is_https_only_bounded_and_pool_free() {
    let policy = FetchPolicy::default();
    let config = FetchAgentConfig::from_policy(&policy);
    assert!(config.https_only);
    assert_eq!(config.max_redirects, 0);
    assert_eq!(config.max_idle_connections, 0);
    assert_eq!(config.max_idle_connections_per_host, 0);
    assert_eq!(config.max_response_header_bytes, 64 * 1024);
    assert!(config.tls_verification);
    assert!(!config.environment_proxy);
    assert_eq!(UREQ_SECURITY_ADAPTER_VERSION, "3.3.0");
}

#[test]
fn identity_and_gzip_bodies_have_independent_encoded_and_decoded_limits() {
    let limits = ParserLimits::new(128, 100, 1024, 16, 1024).unwrap();
    assert_eq!(
        decode_response_body(&b"abc"[..], ContentEncoding::Identity, Some(3), &limits).unwrap(),
        b"abc"
    );

    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder
        .write_all(b"trojan://secret@example.com:443")
        .unwrap();
    let gzip = encoder.finish().unwrap();
    assert_eq!(
        decode_response_body(
            gzip.as_slice(),
            ContentEncoding::Gzip,
            Some(gzip.len()),
            &limits,
        )
        .unwrap(),
        b"trojan://secret@example.com:443"
    );

    let oversized = vec![b'x'; limits.max_body_bytes() + 1];
    assert_eq!(
        decode_response_body(
            oversized.as_slice(),
            ContentEncoding::Identity,
            None,
            &limits,
        )
        .unwrap_err(),
        FetchPolicyError::BodyTooLarge
    );

    let mut bomb = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    bomb.write_all(&oversized).unwrap();
    let bomb = bomb.finish().unwrap();
    assert!(bomb.len() < limits.max_body_bytes());
    assert_eq!(
        decode_response_body(
            bomb.as_slice(),
            ContentEncoding::Gzip,
            Some(bomb.len()),
            &limits,
        )
        .unwrap_err(),
        FetchPolicyError::BodyTooLarge
    );

    assert_eq!(
        decode_response_body(
            &[0x1f, 0x8b, 0x08, 0x00][..],
            ContentEncoding::Gzip,
            Some(4),
            &limits,
        )
        .unwrap_err(),
        FetchPolicyError::InvalidResponseMetadata
    );
}

#[test]
fn fetch_request_bounds_mirrors_without_deriving_format_from_urls() {
    let policy = FetchPolicy::default();
    let request = FetchRequest::new(
        SourceId::new("source-1").unwrap(),
        "https://primary.example/subscription",
        [
            "https://mirror-1.example/subscription",
            "https://mirror-2.example/subscription",
        ],
        RequestProfile::Mihomo,
        &policy,
    )
    .unwrap();
    assert_eq!(request.endpoints().len(), 3);
    assert_eq!(request.endpoints()[0].kind(), FetchEndpointKind::Primary);
    assert_eq!(request.endpoints()[1].kind(), FetchEndpointKind::Mirror);
    assert_eq!(
        request.profile().accept(),
        "application/yaml, text/yaml, */*"
    );

    assert_eq!(
        FetchRequest::new(
            SourceId::new("source-1").unwrap(),
            "https://primary.example/subscription",
            [
                "https://one.example",
                "https://two.example",
                "https://three.example",
                "https://four.example",
            ],
            RequestProfile::NetHopGeneric,
            &policy,
        )
        .unwrap_err(),
        FetchPolicyError::TooManyMirrors
    );
}

#[test]
fn fetch_diagnostic_codes_are_stable_and_redacted() {
    let codes = [
        FetchDiagnosticCode::Network,
        FetchDiagnosticCode::Timeout,
        FetchDiagnosticCode::SsrfDenied,
        FetchDiagnosticCode::PeerMismatch,
        FetchDiagnosticCode::RedirectRejected,
        FetchDiagnosticCode::BodyTooLarge,
        FetchDiagnosticCode::UnsupportedContentEncoding,
        FetchDiagnosticCode::CacheMiss,
    ];
    for code in codes {
        let rendered = code.to_string();
        assert!(!rendered.contains("token-canary"));
        assert!(!rendered.contains("subscription.example"));
        assert!(!rendered.contains('/'));
    }
}

#[test]
fn cache_keeps_last_known_good_and_conditional_headers_without_parser_coupling() {
    let mut cache = SourceCache::default();
    assert_eq!(
        cache.apply_not_modified().unwrap_err(),
        FetchPolicyError::CacheMiss
    );
    cache
        .apply_success(
            b"trojan://secret@example.com:443".to_vec(),
            Some("\"opaque-etag\"".into()),
            Some("Mon, 01 Jan 2024 00:00:00 GMT".into()),
            &ParserLimits::default(),
        )
        .unwrap();
    assert_eq!(
        cache.conditional_headers(),
        vec![
            ("If-None-Match", "\"opaque-etag\""),
            ("If-Modified-Since", "Mon, 01 Jan 2024 00:00:00 GMT"),
        ]
    );
    assert_eq!(
        cache.apply_not_modified().unwrap(),
        b"trojan://secret@example.com:443"
    );
    assert_eq!(RequestProfile::NetHopGeneric.user_agent(), "NetHop/0.1");
    assert_eq!(RequestProfile::Mihomo.user_agent(), "clash.meta");
    assert_eq!(RequestProfile::ClashStandard.user_agent(), "clash");
    assert_eq!(RequestProfile::Surfboard.user_agent(), "Surfboard");
    assert_eq!(RequestProfile::SingBox.user_agent(), "sing-box");
    assert_eq!(RequestProfile::SingBoxAndroid.user_agent(), "SFA");
    assert_eq!(
        RequestProfile::ClashStandard.accept(),
        "application/yaml, text/yaml, */*"
    );
    assert_eq!(RequestProfile::Surfboard.accept(), "text/plain, */*");
    assert_eq!(
        RequestProfile::SingBoxAndroid.accept(),
        "application/json, */*"
    );
}
