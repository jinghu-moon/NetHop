#![cfg(feature = "format-singbox-json")]

use nethop_subscription::{
    AdapterOutput, CapabilityMatrix, DiagnosticCode, FormatHint, ParserLimits, ProxyProtocol,
    SourceId, SourceInput, convert_stable_sources, parse_singbox_json,
};

fn parse(input: &str) -> AdapterOutput {
    parse_singbox_json(
        input.as_bytes(),
        None,
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    )
    .unwrap()
}

const TROJAN: &str = r#"{"type":"trojan","tag":"node","server":"example.com","server_port":443,"password":"secret","tls":{"enabled":true}}"#;

#[test]
fn stable_conversion_preserves_per_source_outcomes_after_global_dedupe() {
    let first = SourceId::new("first-source").unwrap();
    let second = SourceId::new("second-source").unwrap();
    let bytes = b"trojan://fixture@example.com:443?security=tls#node\n".to_vec();
    let conversion = convert_stable_sources(
        vec![
            SourceInput {
                source_id: first.clone(),
                format_hint: FormatHint::UriList,
                bytes: bytes.clone(),
            },
            SourceInput {
                source_id: second.clone(),
                format_hint: FormatHint::UriList,
                bytes,
            },
        ],
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    );

    assert_eq!(conversion.source_outcomes[&first].accepted, 1);
    assert_eq!(conversion.source_outcomes[&second].duplicate, 1);
    assert!(conversion.source_outcomes[&second].success());
}

#[test]
fn shadowsocks_udp_over_tcp_is_preserved_by_the_controlled_mapping() {
    let input = br#"{
      "outbounds": [
        {
          "type": "shadowsocks",
          "tag": "uot-bool",
          "server": "one.example",
          "server_port": 443,
          "method": "chacha20-ietf-poly1305",
          "password": "fixture",
          "udp_over_tcp": true
        },
        {
          "type": "shadowsocks",
          "tag": "uot-object",
          "server": "two.example",
          "server_port": 443,
          "method": "chacha20-ietf-poly1305",
          "password": "fixture",
          "udp_over_tcp": { "enabled": true, "version": 1 }
        }
      ]
    }"#;
    let conversion = convert_stable_sources(
        vec![SourceInput {
            source_id: SourceId::new("singbox-uot").unwrap(),
            format_hint: FormatHint::SingboxJson,
            bytes: input.to_vec(),
        }],
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    );

    assert_eq!(conversion.report.summary.accepted, 2);
    assert_eq!(conversion.report.summary.rejected, 0);
    let outbounds: serde_json::Value = serde_json::from_str(&conversion.outbounds_json).unwrap();
    assert_eq!(outbounds[0]["udp_over_tcp"], true);
    assert_eq!(outbounds[1]["udp_over_tcp"]["enabled"], true);
    assert_eq!(outbounds[1]["udp_over_tcp"]["version"], 1);
}

#[test]
fn udp_over_tcp_rejects_unknown_or_non_shadowsocks_semantics() {
    for input in [
        br#"{"outbounds":[{"type":"shadowsocks","tag":"bad","server":"one.example","server_port":443,"method":"chacha20-ietf-poly1305","password":"fixture","udp_over_tcp":{"enabled":true,"unknown":1}}]}"#.as_slice(),
        br#"{"outbounds":[{"type":"trojan","tag":"bad","server":"one.example","server_port":443,"password":"fixture","tls":{"enabled":true},"udp_over_tcp":true}]}"#.as_slice(),
    ] {
        let conversion = convert_stable_sources(
            vec![SourceInput {
                source_id: SourceId::new("singbox-uot-rejected").unwrap(),
                format_hint: FormatHint::SingboxJson,
                bytes: input.to_vec(),
            }],
            &ParserLimits::default(),
            &CapabilityMatrix::default(),
        );
        assert_eq!(conversion.report.summary.accepted, 0);
        assert_eq!(conversion.report.summary.rejected, 1);
        assert_eq!(
            conversion.report.diagnostic_counts[&DiagnosticCode::UnsupportedSemantics],
            1
        );
    }
}

#[test]
fn singbox_json_accepts_audited_v2ray_plugin_options() {
    let output = parse(
        r#"[{"type":"shadowsocks","server":"ss.example","server_port":443,"method":"aes-128-gcm","password":"secret","plugin":"v2ray-plugin","plugin_opts":"mode=websocket;host=edge.example;path=/ws;tls=true;mux=1"}]"#,
    );
    assert_eq!(output.accepted_count(), 1);
    assert_eq!(output.rejected_count(), 0);
}

#[test]
fn singbox_json_accepts_config_array_and_single_outbound_shapes() {
    let config = format!(r#"{{"log":{{"level":"info"}},"outbounds":[{TROJAN}]}}"#);
    let array = format!("[{TROJAN}]");
    for input in [config.as_str(), array.as_str(), TROJAN] {
        let output = parse(input);
        assert_eq!(output.accepted_count(), 1);
        assert_eq!(output.rejected_count(), 0);
        assert_eq!(
            output.nodes[0].node.as_ref().unwrap().protocol(),
            ProxyProtocol::Trojan
        );
    }
    assert!(
        parse_singbox_json(
            br#"{"route":{"rules":[]}}"#,
            None,
            &ParserLimits::default(),
            &CapabilityMatrix::default()
        )
        .is_err()
    );
}

#[test]
fn json_duplicate_critical_fields_reject_only_the_affected_outbound() {
    let input = format!(
        r#"[{TROJAN},{{"type":"trojan","server":"bad.example","server_port":443,"password":"one","password":"two","tls":{{"enabled":true}}}}]"#
    );
    let output = parse(&input);
    assert_eq!(output.accepted_count(), 1);
    assert_eq!(output.rejected_count(), 1);
    assert_eq!(
        output.nodes[1].diagnostic.as_ref().unwrap().code,
        DiagnosticCode::DuplicateCredentialKey
    );
}

#[test]
fn json_reads_only_terminal_outbounds_and_summarizes_other_sections() {
    let input = format!(
        r#"{{
          "dns":{{"servers":[]}},
          "inbounds":[{{"type":"tun","interface_name":"secret-path"}}],
          "route":{{"rules":[]}},
          "services":[],
          "experimental":{{}},
          "outbounds":[
            {{"type":"selector","tag":"group","outbounds":["node"]}},
            {{"type":"direct","tag":"direct"}},
            {TROJAN}
          ]
        }}"#
    );
    let output = parse(&input);
    assert_eq!(output.accepted_count(), 1);
    assert_eq!(output.nodes.len(), 1);
    assert_eq!(output.diagnostics.len(), 1);
    assert_eq!(
        output.diagnostics[0].code,
        DiagnosticCode::NonNodeSectionIgnored
    );
    assert!(!format!("{output:?}").contains("secret-path"));
}

#[test]
fn singbox_json_maps_all_nine_protocols_through_the_shared_semantic_gate() {
    let input = r#"[
      {"type":"vless","server":"vless.example","server_port":443,"uuid":"550e8400-e29b-41d4-a716-446655440000","tls":{"enabled":true}},
      {"type":"vmess","server":"vmess.example","server_port":443,"uuid":"550e8400-e29b-41d4-a716-446655440000","security":"auto","tls":{"enabled":true}},
      {"type":"shadowsocks","server":"ss.example","server_port":443,"method":"aes-128-gcm","password":"secret"},
      {"type":"trojan","server":"trojan.example","server_port":443,"password":"secret","tls":{"enabled":true}},
      {"type":"hysteria2","server":"hy2.example","server_port":443,"password":"secret","udp":true,"tls":{"enabled":true}},
      {"type":"tuic","server":"tuic.example","server_port":443,"uuid":"550e8400-e29b-41d4-a716-446655440000","password":"secret","udp":true,"tls":{"enabled":true}},
      {"type":"anytls","server":"anytls.example","server_port":443,"password":"secret","tls":{"enabled":true}},
      {"type":"http","server":"http.example","server_port":8080},
      {"type":"socks","server":"socks.example","server_port":1080,"network":["tcp","udp"]}
    ]"#;
    let output = parse(input);
    assert_eq!(output.accepted_count(), 9);
    assert_eq!(output.rejected_count(), 0);
}

#[test]
fn singbox_json_preserves_v2ray_transport_hosts() {
    let input = r#"[
      {
        "type":"vless",
        "tag":"ws-host",
        "server":"vless.example",
        "server_port":443,
        "uuid":"550e8400-e29b-41d4-a716-446655440000",
        "tls":{"enabled":true},
        "transport":{"type":"ws","path":"/ws","headers":{"Host":"edge.example"}}
      },
      {
        "type":"vmess",
        "tag":"http-hosts",
        "server":"vmess.example",
        "server_port":443,
        "uuid":"550e8400-e29b-41d4-a716-446655440000",
        "security":"auto",
        "tls":{"enabled":true},
        "transport":{"type":"http","path":"/http","host":["edge.example","backup.example"]}
      }
    ]"#;
    let conversion = convert_stable_sources(
        vec![SourceInput {
            source_id: SourceId::new("transport-hosts").unwrap(),
            format_hint: FormatHint::SingboxJson,
            bytes: input.as_bytes().to_vec(),
        }],
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    );
    assert_eq!(conversion.report.summary.accepted, 2);
    assert_eq!(conversion.report.summary.rejected, 0);
    let outbounds: serde_json::Value = serde_json::from_str(&conversion.outbounds_json).unwrap();
    assert_eq!(outbounds[0]["transport"]["headers"]["Host"], "edge.example");
    assert_eq!(outbounds[1]["transport"]["host"][0], "edge.example");
    assert_eq!(outbounds[1]["transport"]["host"][1], "backup.example");
}

#[test]
fn singbox_json_preserves_hysteria2_tuic_and_anytls_session_options() {
    let input = r#"[
      {
        "type":"hysteria2","tag":"hy2","server":"hy2.example","server_port":443,
        "password":"secret","server_ports":["443","5000-6000"],"hop_interval":"30s",
        "up_mbps":100,"down_mbps":500,"tls":{"enabled":true}
      },
      {
        "type":"tuic","tag":"tuic","server":"tuic.example","server_port":443,
        "uuid":"550e8400-e29b-41d4-a716-446655440000","password":"secret",
        "congestion_control":"bbr","udp_relay_mode":"native","udp_over_stream":true,
        "zero_rtt_handshake":true,"heartbeat":"10s","tls":{"enabled":true}
      },
      {
        "type":"anytls","tag":"anytls","server":"anytls.example","server_port":443,
        "password":"secret","idle_session_check_interval":"30s",
        "idle_session_timeout":"2m","min_idle_session":2,"tls":{"enabled":true}
      }
    ]"#;
    let conversion = convert_stable_sources(
        vec![SourceInput {
            source_id: SourceId::new("session-options").unwrap(),
            format_hint: FormatHint::SingboxJson,
            bytes: input.as_bytes().to_vec(),
        }],
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    );
    assert_eq!(conversion.report.summary.accepted, 3);
    assert_eq!(conversion.report.summary.rejected, 0);
    let outbounds: serde_json::Value = serde_json::from_str(&conversion.outbounds_json).unwrap();
    assert_eq!(outbounds[0]["server_ports"][1], "5000-6000");
    assert_eq!(outbounds[0]["hop_interval"], "30s");
    assert_eq!(outbounds[1]["udp_relay_mode"], "native");
    assert_eq!(outbounds[1]["zero_rtt_handshake"], true);
    assert_eq!(outbounds[2]["idle_session_timeout"], "2m");
    assert_eq!(outbounds[2]["min_idle_session"], 2);
}

#[test]
fn singbox_json_maps_audited_http_and_socks_outbounds() {
    let output = parse(
        r#"[
  {"type":"http","tag":"http","server":"http.example","server_port":8443,
   "username":"fixture-user","password":"fixture-password","path":"/connect",
   "headers":{"X-Fixture":"bounded"},"tls":{"enabled":true,"server_name":"proxy.example"}},
  {"type":"socks","tag":"socks","server":"socks.example","server_port":1080,
   "version":"5","username":"fixture-user","password":"fixture-password",
   "network":["tcp","udp"]}
]"#,
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
    assert!(output.nodes[1].node.as_ref().unwrap().capabilities().udp);
}

#[test]
fn singbox_socks_rejects_unknown_version_and_http_rejects_transport_semantics() {
    let output = parse(
        r#"[
  {"type":"socks","server":"socks.example","server_port":1080,"version":"6"},
  {"type":"http","server":"http.example","server_port":8080,"transport":{"type":"ws"}}
]"#,
    );
    assert_eq!(output.rejected_count(), 2);
    assert!(output.nodes.iter().all(|item| {
        item.diagnostic.as_ref().unwrap().code == DiagnosticCode::UnsupportedSemantics
            || item.diagnostic.as_ref().unwrap().code == DiagnosticCode::UnsupportedTransport
    }));
}

#[test]
fn hysteria2_server_ports_reject_invalid_ranges_before_composition() {
    let output = parse(
        r#"[{"type":"hysteria2","server":"hy2.example","server_port":443,"password":"secret","server_ports":["6000-5000"],"tls":{"enabled":true}}]"#,
    );
    assert_eq!(output.accepted_count(), 0);
    assert_eq!(output.rejected_count(), 1);
    assert_eq!(
        output.nodes[0].diagnostic.as_ref().unwrap().code,
        DiagnosticCode::UnsupportedSemantics
    );
}

#[test]
fn json_unknown_field_policy_matches_yaml_semantics() {
    let input = r#"[
      {"type":"trojan","server":"good.example","server_port":443,"password":"secret","tls":{"enabled":true},"icon":"display-only"},
      {"type":"vless","server":"bad.example","server_port":443,"uuid":"550e8400-e29b-41d4-a716-446655440000","tls":{"enabled":true},"detour":"unsafe-reference"}
    ]"#;
    let output = parse(input);
    assert_eq!(output.accepted_count(), 1);
    assert_eq!(output.rejected_count(), 1);
    assert_eq!(
        output.nodes[0].warnings[0].code,
        DiagnosticCode::UnknownField
    );
    assert_eq!(
        output.nodes[1].diagnostic.as_ref().unwrap().code,
        DiagnosticCode::UnsupportedSemantics
    );
}

#[test]
fn json_mixed_outbounds_keep_order_and_partial_success() {
    let input = format!(
        r#"[{TROJAN},{{"type":"trojan","server":"bad.example","server_port":443,"tls":{{"enabled":true}}}},{{"type":"wireguard","tag":"unsupported","server":"wg.example","server_port":443}}]"#
    );
    let output = parse(&input);
    assert_eq!(output.nodes.len(), 3);
    assert_eq!(output.accepted_count(), 1);
    assert_eq!(output.rejected_count(), 2);
    assert_eq!(output.nodes[0].item_index, 0);
    assert_eq!(output.nodes[2].item_index, 2);
    assert_eq!(
        output.nodes[2].diagnostic.as_ref().unwrap().code,
        DiagnosticCode::UnsupportedProtocol
    );
}

#[test]
fn singbox_wireguard_endpoint_is_reported_as_a_non_node_section() {
    let output = parse(
        r#"{
  "endpoints": [{
    "type": "wireguard",
    "tag": "wg-endpoint",
    "address": ["10.0.0.2/32"],
    "private_key": "fixture-private-key",
    "peers": [{"address":"wg.example","port":51820,"public_key":"fixture-public-key"}]
  }],
  "outbounds": [
    {"type":"trojan","server":"good.example","server_port":443,"password":"secret","tls":{"enabled":true}}
  ]
}"#,
    );
    assert_eq!(output.accepted_count(), 1);
    assert!(
        output
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == DiagnosticCode::NonNodeSectionIgnored })
    );
}

#[test]
fn json_depth_and_string_budgets_are_checked_before_typed_mapping() {
    let deep = format!("{}0{}", "[".repeat(65), "]".repeat(65));
    assert_eq!(
        parse_singbox_json(
            deep.as_bytes(),
            None,
            &ParserLimits::default(),
            &CapabilityMatrix::default(),
        )
        .unwrap_err()
        .code,
        DiagnosticCode::InputTooLarge
    );
    let long = format!(
        r#"{{"type":"trojan","tag":"{}"}}"#,
        "x".repeat(64 * 1024 + 1)
    );
    assert_eq!(
        parse_singbox_json(
            long.as_bytes(),
            None,
            &ParserLimits::default(),
            &CapabilityMatrix::default(),
        )
        .unwrap_err()
        .code,
        DiagnosticCode::InputTooLarge
    );
}
