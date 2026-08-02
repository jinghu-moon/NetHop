#![cfg(feature = "format-clash-yaml")]

use nethop_subscription::{
    CapabilityMatrix, DiagnosticCode, ParserLimits, ProxyProtocol, parse_clash_yaml, yaml_options,
};

fn parse(input: &str) -> nethop_subscription::AdapterOutput {
    parse_clash_yaml(
        input.as_bytes(),
        None,
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    )
    .unwrap()
}

#[test]
fn yaml_options_freeze_budget_alias_and_policy_limits() {
    let options = yaml_options(&ParserLimits::default());
    let budget = options.budget.unwrap();
    assert_eq!(budget.max_events, 200_000);
    assert_eq!(budget.max_documents, 1);
    assert_eq!(budget.max_depth, 64);
    assert_eq!(budget.max_aliases, 1_024);
    assert_eq!(budget.max_anchors, 1_024);
    assert_eq!(budget.max_merge_keys, 0);
    assert!(!options.with_snippet);
    assert!(matches!(
        options.merge_keys,
        serde_saphyr::MergeKeyPolicy::Error
    ));
}

#[test]
fn clash_yaml_reads_only_inline_proxies_and_maps_all_protocols() {
    let output = parse(
        r#"
proxies:
  - { name: vless, type: vless, server: vless.example, port: 443, uuid: 550e8400-e29b-41d4-a716-446655440000, tls: true }
  - { name: vmess, type: vmess, server: vmess.example, port: 443, uuid: 550e8400-e29b-41d4-a716-446655440000, tls: true }
  - { name: ss, type: ss, server: ss.example, port: 443, cipher: aes-128-gcm, password: secret }
  - { name: trojan, type: trojan, server: trojan.example, port: 443, password: secret, tls: true }
  - { name: hy2, type: hysteria2, server: hy2.example, port: 443, password: secret, tls: true, network: quic, udp: true }
  - { name: tuic, type: tuic, server: tuic.example, port: 443, uuid: 550e8400-e29b-41d4-a716-446655440000, password: secret, tls: true, network: quic, udp: true }
  - { name: anytls, type: anytls, server: anytls.example, port: 443, password: secret, tls: true }
proxy-groups: [{ name: ignored, type: select }]
rules: [MATCH,DIRECT]
script: ignored
"#,
    );
    assert_eq!(output.accepted_count(), 7);
    assert_eq!(output.rejected_count(), 0);
    assert!(output.diagnostics.len() <= 4);
    assert_eq!(
        output.nodes[0].node.as_ref().unwrap().protocol(),
        ProxyProtocol::Vless
    );
}

#[test]
fn provider_only_and_non_node_sections_are_never_fetched_or_imported() {
    let output = parse(
        r#"
proxy-providers:
  remote:
    type: http
    url: https://token.example.invalid/subscription
rules: [MATCH,DIRECT]
"#,
    );
    assert_eq!(output.accepted_count(), 0);
    assert!(output.diagnostics.len() <= 3);
    assert!(
        output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::ClashInlineProxiesMissing)
    );
    assert!(
        output.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == DiagnosticCode::ClashProxyProvidersNotImported
        })
    );
    assert!(!format!("{:?}", output).contains("token.example.invalid"));
}

#[test]
fn duplicate_merge_tag_and_alias_attacks_have_stable_source_failures() {
    for (input, expected) in [
        (
            "proxies:\n  - { type: trojan, type: ss, server: example.com, port: 443, password: secret, tls: true }\n",
            DiagnosticCode::DuplicateKey,
        ),
        (
            "proxies:\n  - <<: { type: trojan, server: example.com, port: 443, password: secret, tls: true }\n",
            DiagnosticCode::YamlMergeKeyUnsupported,
        ),
        (
            "proxies:\n  - !include secret.yaml\n",
            DiagnosticCode::InvalidYaml,
        ),
    ] {
        let error = parse_clash_yaml(
            input.as_bytes(),
            None,
            &ParserLimits::default(),
            &CapabilityMatrix::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, expected);
    }
}

#[test]
fn yaml_semantic_failures_are_per_node_and_critical_unknowns_are_rejected() {
    let output = parse(
        r#"
proxies:
  - { name: good, type: trojan, server: example.com, port: 443, password: secret, tls: true }
  - { name: bad, type: trojan, server: bad.example, port: 443, password: secret, tls: false }
  - { name: xhttp, type: vless, server: xhttp.example, port: 443, uuid: 550e8400-e29b-41d4-a716-446655440000, tls: true, xhttp-opts: { mode: auto } }
"#,
    );
    assert_eq!(output.accepted_count(), 1);
    assert_eq!(output.rejected_count(), 2);
    assert_eq!(
        output.nodes[1].diagnostic.as_ref().unwrap().code,
        DiagnosticCode::InvalidTlsCombination
    );
    assert_eq!(
        output.nodes[2].diagnostic.as_ref().unwrap().code,
        DiagnosticCode::UnsupportedSemantics
    );
}

#[test]
fn yaml_alias_and_document_limits_fail_before_node_mapping() {
    let aliases = std::iter::repeat_n("*base", 1_025)
        .collect::<Vec<_>>()
        .join(",");
    let bomb = format!(
        "base: &base {{ name: node, type: trojan, server: example.com, port: 443, password: secret, tls: true }}\nproxies: [{aliases}]\n"
    );
    let error = parse_clash_yaml(
        bomb.as_bytes(),
        None,
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    )
    .unwrap_err();
    assert_eq!(error.code, DiagnosticCode::YamlAliasLimitExceeded);

    let multi = "proxies: []\n---\nproxies: []\n";
    let error = parse_clash_yaml(
        multi.as_bytes(),
        None,
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    )
    .unwrap_err();
    assert!(matches!(
        error.code,
        DiagnosticCode::YamlNodeLimitExceeded | DiagnosticCode::InvalidYaml
    ));
}

#[test]
fn harmless_unknown_yaml_fields_are_bounded_warnings() {
    let output = parse(
        r#"
proxies:
  - { name: good, type: trojan, server: example.com, port: 443, password: secret, tls: true, icon: display-only }
"#,
    );
    assert_eq!(output.accepted_count(), 1);
    assert_eq!(output.nodes[0].warnings.len(), 1);
    assert_eq!(
        output.nodes[0].warnings[0].code,
        DiagnosticCode::UnknownField
    );
}
