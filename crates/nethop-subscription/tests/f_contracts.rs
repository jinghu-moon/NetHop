#![cfg(feature = "format-clash-yaml")]

use nethop_subscription::{
    CapabilityMatrix, Credentials, DiagnosticCode, ParserLimits, ProtocolOptions, ProxyProtocol,
    parse_clash_yaml, yaml_options,
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
fn clash_shadowsocks_maps_audited_obfs_and_udp_over_tcp_semantics() {
    let output = parse(
        r#"
proxies:
  - name: ss-obfs
    type: ss
    server: ss.example
    port: 443
    cipher: aes-128-gcm
    password: secret
    plugin: obfs
    plugin-opts:
      mode: tls
      host: edge.example
    udp: true
    udp-over-tcp: true
"#,
    );

    assert_eq!(output.accepted_count(), 1);
    assert_eq!(output.rejected_count(), 0);
    let node = output.nodes[0].node.as_ref().unwrap();
    let Credentials::Shadowsocks {
        plugin: Some(plugin),
        ..
    } = node.credentials()
    else {
        panic!("expected Shadowsocks obfs plugin");
    };
    assert_eq!(plugin.name.as_str(), "obfs-local");
    assert_eq!(plugin.options["obfs"].as_str(), "tls");
    assert_eq!(plugin.options["obfs-host"].as_str(), "edge.example");
    assert_eq!(
        node.protocol_options(),
        &ProtocolOptions::Shadowsocks {
            udp_over_tcp: Some(nethop_subscription::UdpOverTcpOptions {
                enabled: true,
                version: 0,
            }),
        }
    );
}

#[test]
fn clash_yaml_preserves_hysteria2_tuic_and_anytls_options() {
    let output = parse(
        r#"
proxies:
  - name: hy2
    type: hysteria2
    server: hy2.example
    port: 443
    password: secret
    tls: true
    network: quic
    ports: 443,5000-6000
    hop-interval: 30s
    up: 100 Mbps
    down: 500 Mbps
  - name: tuic
    type: tuic
    server: tuic.example
    port: 443
    uuid: 550e8400-e29b-41d4-a716-446655440000
    password: secret
    tls: true
    network: quic
    congestion-controller: bbr
    udp-relay-mode: native
    udp-over-stream: true
    zero-rtt: true
    heartbeat-interval: 10s
  - name: anytls
    type: anytls
    server: anytls.example
    port: 443
    password: secret
    tls: true
    idle-session-check-interval: 30s
    idle-session-timeout: 2m
    min-idle-session: 2
"#,
    );
    assert_eq!(output.accepted_count(), 3);
    assert_eq!(output.rejected_count(), 0);
    let conversion = nethop_subscription::convert_stable_sources(
        vec![nethop_subscription::SourceInput {
            source_id: nethop_subscription::SourceId::new("clash-options").unwrap(),
            format_hint: nethop_subscription::FormatHint::ClashYaml,
            bytes: br#"
proxies:
  - { name: hy2, type: hysteria2, server: hy2.example, port: 443, password: secret, tls: true, network: quic, ports: "443,5000-6000", hop-interval: 30s, up: "100 Mbps", down: "500 Mbps" }
"#
                .to_vec(),
        }],
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    );
    let outbounds: serde_json::Value = serde_json::from_str(&conversion.outbounds_json).unwrap();
    assert!(outbounds[0].get("server_port").is_none());
    assert_eq!(outbounds[0]["server_ports"][1], "5000-6000");
    assert_eq!(outbounds[0]["up_mbps"], 100);
    assert_eq!(outbounds[0]["down_mbps"], 500);
}

#[test]
fn clash_yaml_maps_audited_http_and_socks5_fields() {
    let output = parse(
        r#"proxies:
  - name: http
    type: http
    server: http.example
    port: 8443
    username: fixture-user
    password: fixture-password
    tls: true
    sni: proxy.example
    headers: { X-Fixture: bounded }
  - name: socks
    type: socks5
    server: socks.example
    port: 1080
    username: fixture-user
    password: fixture-password
    udp: true
"#,
    );
    assert_eq!(output.accepted_count(), 2);
    assert_eq!(
        output.nodes[0].node.as_ref().unwrap().protocol().as_str(),
        "http"
    );
    assert_eq!(
        output.nodes[1].node.as_ref().unwrap().protocol().as_str(),
        "socks"
    );
}

#[test]
fn clash_socks_tls_and_local_certificate_fields_are_not_silently_dropped() {
    let output = parse(
        r#"proxies:
  - { name: socks-tls, type: socks5, server: socks.example, port: 1080, tls: true }
  - { name: http-cert, type: http, server: http.example, port: 8443, tls: true, certificate: /data/local/cert.pem }
"#,
    );
    assert_eq!(output.rejected_count(), 2);
    assert!(output.nodes.iter().all(|item| {
        item.diagnostic.as_ref().unwrap().code == DiagnosticCode::UnsupportedSemantics
            || item.diagnostic.as_ref().unwrap().code == DiagnosticCode::InvalidTlsCombination
    }));
}

#[test]
fn clash_yaml_rejects_mihomo_certificate_fingerprint_without_silent_downgrade() {
    let output = parse(
        r#"
proxies:
  - name: hy2-pin
    type: hysteria2
    server: hy2.example
    port: 443
    password: secret
    tls: true
    fingerprint: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
"#,
    );
    assert_eq!(output.accepted_count(), 0);
    assert_eq!(output.rejected_count(), 1);
    assert_eq!(
        output.nodes[0].diagnostic.as_ref().unwrap().code,
        DiagnosticCode::UnsupportedSemantics
    );
}

#[test]
fn clash_yaml_normalizes_mihomo_numeric_intervals_to_sing_box_durations() {
    let conversion = nethop_subscription::convert_stable_sources(
        vec![nethop_subscription::SourceInput {
            source_id: nethop_subscription::SourceId::new("clash-numeric-intervals").unwrap(),
            format_hint: nethop_subscription::FormatHint::ClashYaml,
            bytes: br#"
proxies:
  - name: hy2
    type: hysteria2
    server: hy2.example
    port: 443
    password: secret
    tls: true
    hop-interval: 15
  - name: tuic
    type: tuic
    server: tuic.example
    port: 443
    uuid: 550e8400-e29b-41d4-a716-446655440000
    password: secret
    tls: true
    heartbeat-interval: 10000
"#
            .to_vec(),
        }],
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    );
    assert_eq!(conversion.report.summary.accepted, 2);
    let outbounds: serde_json::Value = serde_json::from_str(&conversion.outbounds_json).unwrap();
    assert_eq!(outbounds[0]["hop_interval"], "15s");
    assert_eq!(outbounds[1]["heartbeat"], "10000ms");
}

#[test]
fn clash_shadowsocks_rejects_unknown_obfs_plugin_option() {
    let output = parse(
        r#"
proxies:
  - name: ss-obfs
    type: ss
    server: ss.example
    port: 443
    cipher: aes-128-gcm
    password: secret
    plugin: obfs
    plugin-opts:
      mode: tls
      unexpected: value
"#,
    );

    assert_eq!(output.accepted_count(), 0);
    assert_eq!(output.rejected_count(), 1);
    assert_eq!(
        output.nodes[0].diagnostic.as_ref().unwrap().code,
        DiagnosticCode::UnsupportedSemantics
    );
}

#[test]
fn clash_shadowsocks_rejects_unknown_plugin_name() {
    let output = parse(
        r#"
proxies:
  - name: ss-plugin
    type: ss
    server: ss.example
    port: 443
    cipher: aes-128-gcm
    password: secret
    plugin: arbitrary-plugin
"#,
    );

    assert_eq!(output.accepted_count(), 0);
    assert_eq!(output.rejected_count(), 1);
    assert_eq!(
        output.nodes[0].diagnostic.as_ref().unwrap().code,
        DiagnosticCode::UnsupportedSemantics
    );
}

#[test]
fn clash_shadowsocks_maps_audited_v2ray_plugin_options() {
    let conversion = nethop_subscription::convert_stable_sources(
        vec![nethop_subscription::SourceInput {
            source_id: nethop_subscription::SourceId::new("ss-v2ray-plugin").unwrap(),
            format_hint: nethop_subscription::FormatHint::ClashYaml,
            bytes: br#"
proxies:
  - name: ss-v2ray
    type: ss
    server: ss.example
    port: 443
    cipher: aes-128-gcm
    password: secret
    plugin: v2ray-plugin
    plugin-opts:
      mode: websocket
      host: edge.example
      path: /ws
      tls: true
      mux: 1
"#
            .to_vec(),
        }],
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    );
    assert_eq!(conversion.report.summary.accepted, 1);
    assert_eq!(conversion.report.summary.rejected, 0);
    let outbounds: serde_json::Value = serde_json::from_str(&conversion.outbounds_json).unwrap();
    assert_eq!(outbounds[0]["plugin"], "v2ray-plugin");
    assert_eq!(
        outbounds[0]["plugin_opts"],
        "host=edge.example;mode=websocket;mux=1;path=/ws;tls=true"
    );
}

#[test]
fn clash_shadowsocks_rejects_non_boolean_udp_over_tcp() {
    let error = parse_clash_yaml(
        br#"
proxies:
  - name: ss-uot
    type: ss
    server: ss.example
    port: 443
    cipher: aes-128-gcm
    password: secret
    udp-over-tcp: enabled
"#,
        None,
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    )
    .unwrap_err();

    assert_eq!(error.code, DiagnosticCode::InvalidYaml);
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
fn yaml_pathological_nesting_is_rejected_before_recursive_deserialization() {
    let nested = "[".repeat(65) + &"]".repeat(65);
    let input = format!("proxies: {nested}\n");
    let error = parse_clash_yaml(
        input.as_bytes(),
        None,
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    )
    .unwrap_err();
    assert_eq!(error.code, DiagnosticCode::YamlNodeLimitExceeded);

    let multiline = format!(
        "proxies: {}\n",
        std::iter::repeat_n("[", 65).collect::<Vec<_>>().join("\n")
    );
    let error = parse_clash_yaml(
        multiline.as_bytes(),
        None,
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    )
    .unwrap_err();
    assert_eq!(error.code, DiagnosticCode::YamlNodeLimitExceeded);

    let tagged = "proxies: !include remote.yaml\n";
    let error = parse_clash_yaml(
        tagged.as_bytes(),
        None,
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    )
    .unwrap_err();
    assert_eq!(error.code, DiagnosticCode::InvalidYaml);
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
