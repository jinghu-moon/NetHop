#![cfg(all(
    feature = "format-uri",
    feature = "format-base64",
    feature = "format-clash-yaml",
    feature = "format-singbox-json"
))]

use base64::Engine;
use nethop_subscription::{
    CapabilityMatrix, CompactStatus, ConversionReport, DiagnosticCode, FormatHint, NodeDiagnostic,
    NodeSpec, ParserLimits, Severity, SourceBatch, SourceId, SourceInput, SourceOutcome,
    canonical_node_bytes, compose_outbound, compose_outbounds_json, convert_stable_sources,
    dedupe_sources, fingerprint_node, validate_node_spec,
};

fn source_id(value: &str) -> SourceId {
    SourceId::new(value).unwrap()
}

fn spec(protocol: &str, name: &str) -> NodeSpec {
    let mut spec = NodeSpec::minimal(protocol, format!("{protocol}.example"), 443);
    spec.display_name = Some(name.into());
    match protocol {
        "vless" | "vmess" | "tuic" => {
            spec.uuid = Some("550e8400-e29b-41d4-a716-446655440000".into());
            if protocol == "tuic" {
                spec.password = Some("secret".into());
            }
        }
        _ => spec.password = Some("secret".into()),
    }
    match protocol {
        "shadowsocks" => spec.method = Some("aes-128-gcm".into()),
        "trojan" | "anytls" | "vless" | "vmess" => spec.tls = true,
        "hysteria2" | "tuic" => {
            spec.tls = true;
            spec.udp = true;
            spec.transport = Some("quic".into());
        }
        _ => {}
    }
    spec
}

fn node(protocol: &str, name: &str) -> nethop_subscription::ProxyNode {
    validate_node_spec(spec(protocol, name), &CapabilityMatrix::default())
        .unwrap()
        .node
}

#[test]
fn canonical_fingerprint_ignores_display_and_source_but_tracks_connection_semantics() {
    let first = node("trojan", "alpha");
    let second = node("trojan", "beta");
    assert_eq!(canonical_node_bytes(&first), canonical_node_bytes(&second));
    assert_eq!(fingerprint_node(&first), fingerprint_node(&second));

    let changed = node("anytls", "alpha");
    assert_ne!(fingerprint_node(&first), fingerprint_node(&changed));
    assert!(!format!("{:?}", fingerprint_node(&first)).contains("secret"));
}

#[test]
fn node_id_is_schema_tagged_and_truncated() {
    let fp = fingerprint_node(&node("shadowsocks", "node"));
    let id = fp.display_id().to_string();
    assert!(id.starts_with("nh1s-"));
    assert_eq!(id.len(), "nh1s-".len() + 16);
    assert!(!id.contains(&fp.hex()));
}

#[test]
fn dedupe_merges_names_and_source_refs_with_stable_order() {
    let mut one = spec("trojan", "alpha");
    one.source_ref = Some(nethop_subscription::SourceRef {
        source_id: source_id("b"),
        item_index: 2,
        format: FormatHint::UriList,
        line: Some(2),
    });
    let mut two = spec("trojan", "beta");
    two.source_ref = Some(nethop_subscription::SourceRef {
        source_id: source_id("a"),
        item_index: 1,
        format: FormatHint::UriList,
        line: Some(1),
    });
    let nodes = vec![
        SourceBatch {
            source_id: source_id("b"),
            nodes: vec![
                validate_node_spec(one, &CapabilityMatrix::default())
                    .unwrap()
                    .node,
            ],
            rejected: 0,
            warnings: 0,
        },
        SourceBatch {
            source_id: source_id("a"),
            nodes: vec![
                validate_node_spec(two, &CapabilityMatrix::default())
                    .unwrap()
                    .node,
            ],
            rejected: 0,
            warnings: 0,
        },
    ];
    let (deduped, outcomes) = dedupe_sources(nodes, &ParserLimits::default());
    assert_eq!(deduped.len(), 1);
    assert_eq!(deduped[0].source_refs.len(), 2);
    assert!(deduped[0].aliases.contains(&"alpha".into()));
    assert!(deduped[0].aliases.contains(&"beta".into()));
    assert_eq!(outcomes[&source_id("a")].duplicate, 1);
    assert!(outcomes[&source_id("a")].success());
}

#[test]
fn source_outcome_truth_table_treats_duplicate_only_as_success() {
    assert!(
        SourceOutcome {
            accepted: 1,
            duplicate: 0,
            rejected: 9,
            warnings: 0
        }
        .success()
    );
    assert!(
        SourceOutcome {
            accepted: 0,
            duplicate: 1,
            rejected: 9,
            warnings: 0
        }
        .success()
    );
    assert!(
        !SourceOutcome {
            accepted: 0,
            duplicate: 0,
            rejected: 1,
            warnings: 0
        }
        .success()
    );
}

#[test]
fn report_is_compact_bounded_and_preserves_counts() {
    let mut report = ConversionReport {
        summary: Default::default(),
        items: vec![nethop_subscription::CompactItemReport {
            index: 0,
            status: CompactStatus::Rejected,
            protocol: None,
            node_id: None,
            codes: vec![DiagnosticCode::InvalidCredential],
        }],
        diagnostics: (0..1_500)
            .map(|_| NodeDiagnostic::new(DiagnosticCode::InvalidCredential, Severity::Error))
            .collect(),
        diagnostic_counts: Default::default(),
    };
    report.summary.rejected = 1_500;
    let json = report.bounded_json(&ParserLimits::default());
    assert!(json.len() <= ParserLimits::default().max_report_bytes());
    assert!(!json.contains("password"));
}

#[test]
fn compose_generates_terminal_fragments_for_all_protocols_deterministically() {
    let batches = vec![SourceBatch {
        source_id: source_id("s"),
        nodes: [
            "vless",
            "vmess",
            "shadowsocks",
            "trojan",
            "hysteria2",
            "tuic",
            "anytls",
        ]
        .into_iter()
        .map(|protocol| node(protocol, protocol))
        .collect(),
        rejected: 0,
        warnings: 0,
    }];
    let (deduped, _) = dedupe_sources(batches, &ParserLimits::default());
    let json = compose_outbounds_json(&deduped);
    assert_eq!(json, compose_outbounds_json(&deduped));
    assert!(json.contains("\"type\":\"vless\""));
    assert!(json.contains("\"type\":\"anytls\""));
    let fragment = compose_outbound(&deduped[0]);
    assert!(fragment.get("tag").is_some());
}

#[test]
fn compose_preserves_http_and_socks_connection_semantics() {
    let mut http = NodeSpec::minimal("http", "http.example", 8443);
    http.display_name = Some("http".into());
    http.username = Some("fixture-user".into());
    http.password = Some("fixture-password".into());
    http.tls = true;
    http.http_path = Some("/connect".into());
    http.http_headers
        .insert("X-Fixture".into(), "bounded".into());

    let mut socks = NodeSpec::minimal("socks", "socks.example", 1080);
    socks.display_name = Some("socks".into());
    socks.socks_version = Some("5".into());
    socks.udp = true;

    let batch = SourceBatch {
        source_id: source_id("http-socks"),
        nodes: vec![
            validate_node_spec(http, &CapabilityMatrix::default())
                .unwrap()
                .node,
            validate_node_spec(socks, &CapabilityMatrix::default())
                .unwrap()
                .node,
        ],
        rejected: 0,
        warnings: 0,
    };
    let (nodes, _) = dedupe_sources(vec![batch], &ParserLimits::default());
    let json = compose_outbounds_json(&nodes);
    assert!(json.contains("\"type\":\"http\""));
    assert!(json.contains("\"path\":\"/connect\""));
    assert!(json.contains("\"type\":\"socks\""));
    assert!(json.contains("\"version\":\"5\""));
    assert!(json.contains("\"network\":[\"tcp\",\"udp\"]"));
}

#[test]
fn stable_conversion_handles_uri_base64_yaml_and_json_without_fetch_or_full_config() {
    let limits = ParserLimits::default();
    let uri = SourceInput {
        source_id: source_id("uri"),
        format_hint: FormatHint::UriList,
        bytes: b"trojan://secret@trojan.example:443#uri".to_vec(),
    };
    let wrapped = base64::engine::general_purpose::STANDARD
        .encode(b"trojan://secret@trojan.example:443#base64");
    let base64_source = SourceInput {
        source_id: source_id("base64"),
        format_hint: FormatHint::Base64List,
        bytes: wrapped.into_bytes(),
    };
    let yaml = SourceInput {
        source_id: source_id("yaml"),
        format_hint: FormatHint::ClashYaml,
        bytes: br#"proxies:
  - { name: yaml, type: trojan, server: trojan.example, port: 443, password: secret, tls: true }
"#
        .to_vec(),
    };
    let json = SourceInput {
        source_id: source_id("json"),
        format_hint: FormatHint::SingboxJson,
        bytes: br#"[{"type":"trojan","tag":"json","server":"trojan.example","server_port":443,"password":"secret","tls":{"enabled":true}}]"#.to_vec(),
    };
    let conversion = convert_stable_sources(
        vec![uri, base64_source, yaml, json],
        &limits,
        &CapabilityMatrix::default(),
    );
    assert_eq!(conversion.nodes.len(), 1);
    assert_eq!(conversion.report.summary.accepted, 1);
    assert_eq!(conversion.report.summary.duplicate, 3);
    assert!(conversion.report.summary.source_success);
    assert!(conversion.outbounds_json.starts_with('['));
    assert!(!conversion.outbounds_json.contains("inbounds"));
}
