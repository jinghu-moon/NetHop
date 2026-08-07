#![cfg(feature = "fetch")]

use nethop_subscription::{
    CandidateAcceptance, FetchClient, FetchDiagnosticCode, FetchError, FetchPolicy,
    FetchPolicyError, FetchRequest, ParserLimits, RequestProfile, SourceId, SourceUrlError,
    UREQ_SECURITY_ADAPTER_VERSION, validate_source_url,
};

#[test]
fn production_fetch_rejects_loopback_during_resolution() {
    let policy = FetchPolicy::default();
    let request = FetchRequest::new(
        SourceId::new("ssrf-test").unwrap(),
        "https://127.0.0.1/subscription?token=token-canary",
        std::iter::empty::<&str>(),
        RequestProfile::NetHopGeneric,
        &policy,
    )
    .unwrap();
    let client = FetchClient::new(policy, ParserLimits::default());
    let error = client
        .fetch(&request, &Default::default(), |_| {
            CandidateAcceptance::Accepted
        })
        .unwrap_err();
    assert_eq!(error, FetchError::Policy(FetchPolicyError::DeniedAddress));
    assert_eq!(error.code(), FetchDiagnosticCode::SsrfDenied);
    let rendered = error.to_string();
    assert!(!rendered.contains("token-canary"));
    assert!(!rendered.contains("127.0.0.1"));
}

#[test]
fn url_admission_handles_idna_ipv6_and_rejects_http_without_network() {
    let idna = validate_source_url("https://例子.测试/subscription").unwrap();
    assert!(idna.host_str().unwrap().starts_with("xn--"));
    assert!(validate_source_url("https://[2606:4700:4700::1111]:443/sub").is_ok());
    assert_eq!(
        validate_source_url("http://token-canary@example.com/sub").unwrap_err(),
        SourceUrlError::NonHttps
    );
}

#[test]
fn security_adapter_version_is_exactly_pinned_in_manifest_and_lock() {
    assert_eq!(UREQ_SECURITY_ADAPTER_VERSION, "3.3.0");
    let manifest = include_str!("../Cargo.toml");
    let lock = include_str!("../../../Cargo.lock");
    assert!(manifest.contains("ureq = { version = \"=3.3.0\""));
    assert!(lock.contains("name = \"ureq\"\nversion = \"3.3.0\""));
    assert!(!manifest.contains("features = [\"rustls\", \"gzip\"]"));
}
