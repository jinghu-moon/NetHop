#![cfg(feature = "format-singbox-json")]

use nethop_subscription::{
    AdapterOutput, CapabilityMatrix, DiagnosticCode, ParserLimits, ProxyProtocol,
    parse_singbox_json,
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
fn singbox_json_maps_all_seven_protocols_through_the_shared_semantic_gate() {
    let input = r#"[
      {"type":"vless","server":"vless.example","server_port":443,"uuid":"550e8400-e29b-41d4-a716-446655440000","tls":{"enabled":true}},
      {"type":"vmess","server":"vmess.example","server_port":443,"uuid":"550e8400-e29b-41d4-a716-446655440000","security":"auto","tls":{"enabled":true}},
      {"type":"shadowsocks","server":"ss.example","server_port":443,"method":"aes-128-gcm","password":"secret"},
      {"type":"trojan","server":"trojan.example","server_port":443,"password":"secret","tls":{"enabled":true}},
      {"type":"hysteria2","server":"hy2.example","server_port":443,"password":"secret","udp":true,"tls":{"enabled":true}},
      {"type":"tuic","server":"tuic.example","server_port":443,"uuid":"550e8400-e29b-41d4-a716-446655440000","password":"secret","udp":true,"tls":{"enabled":true}},
      {"type":"anytls","server":"anytls.example","server_port":443,"password":"secret","tls":{"enabled":true}}
    ]"#;
    let output = parse(input);
    assert_eq!(output.accepted_count(), 7);
    assert_eq!(output.rejected_count(), 0);
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
