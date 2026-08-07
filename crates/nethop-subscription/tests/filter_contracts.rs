use nethop_subscription::{
    CapabilityMatrix, DiagnosticCode, FilteredSourceInput, FormatHint, NodeFilter, ParserLimits,
    ProxyProtocol, SourceId, SourceInput, convert_filtered_sources, convert_stable_sources,
};

fn source(bytes: &[u8]) -> SourceInput {
    SourceInput {
        source_id: SourceId::new("source-filter").unwrap(),
        format_hint: FormatHint::UriList,
        bytes: bytes.to_vec(),
    }
}

fn fixture() -> &'static [u8] {
    b"vless://550e8400-e29b-41d4-a716-446655440000@us.example:443?security=tls#US-Fast\n\
trojan://secret@jp.example:443?security=tls#JP-Slow\n\
ss://aes-128-gcm:secret@backup.example:443#US-Backup\n"
}

#[test]
fn default_filter_preserves_the_previous_conversion_result() {
    let limits = ParserLimits::default();
    let matrix = CapabilityMatrix::default();
    let before = convert_stable_sources(vec![source(fixture())], &limits, &matrix);
    let after = convert_filtered_sources(
        vec![FilteredSourceInput {
            source: source(fixture()),
            filter: NodeFilter::default(),
        }],
        &limits,
        &matrix,
    );
    assert_eq!(after.nodes, before.nodes);
    assert_eq!(after.outbounds_json, before.outbounds_json);
    assert_eq!(after.report.summary, before.report.summary);
}

#[test]
fn source_filter_combines_name_include_exclude_and_protocol_allowlist() {
    let filter = NodeFilter::new(
        vec!["us-".into()],
        vec!["backup".into()],
        vec![ProxyProtocol::Vless, ProxyProtocol::Shadowsocks],
    )
    .unwrap();
    let conversion = convert_filtered_sources(
        vec![FilteredSourceInput {
            source: source(fixture()),
            filter,
        }],
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    );
    assert_eq!(conversion.nodes.len(), 1);
    assert_eq!(conversion.nodes[0].node.protocol(), ProxyProtocol::Vless);
    assert_eq!(conversion.nodes[0].node.display_name().as_str(), "US-Fast");
    assert_eq!(conversion.report.summary.accepted, 1);
    assert_eq!(conversion.report.summary.rejected, 2);
}

#[test]
fn empty_after_filter_is_a_stable_source_failure_without_outbounds() {
    let conversion = convert_filtered_sources(
        vec![FilteredSourceInput {
            source: source(fixture()),
            filter: NodeFilter::new(vec!["missing".into()], Vec::new(), Vec::new()).unwrap(),
        }],
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    );
    assert!(conversion.nodes.is_empty());
    assert_eq!(conversion.outbounds_json, "[]");
    assert!(!conversion.report.summary.source_success);
    assert_eq!(
        conversion
            .report
            .diagnostic_counts
            .get(&DiagnosticCode::SourceFilteredEmpty),
        Some(&1)
    );
}

#[test]
fn filter_rules_are_bounded_unique_and_reject_control_characters() {
    assert!(NodeFilter::new(vec!["x".into(); 33], Vec::new(), Vec::new()).is_err());
    assert!(NodeFilter::new(vec!["x".into(), "x".into()], Vec::new(), Vec::new()).is_err());
    assert!(NodeFilter::new(vec!["x\n".into()], Vec::new(), Vec::new()).is_err());
    assert!(NodeFilter::new(Vec::new(), Vec::new(), vec![ProxyProtocol::Vless; 2]).is_err());
    assert!(
        NodeFilter::new_with_node_ids(
            Vec::new(),
            Vec::new(),
            vec!["nh1s-0123456789ABCDEF".into()],
            Vec::new(),
        )
        .is_err()
    );
}

#[test]
fn stable_node_id_filter_removes_exactly_the_selected_fingerprint() {
    let limits = ParserLimits::default();
    let matrix = CapabilityMatrix::default();
    let baseline = convert_stable_sources(vec![source(fixture())], &limits, &matrix);
    let removed = baseline.nodes[1].node_id.to_string();
    let filter =
        NodeFilter::new_with_node_ids(Vec::new(), Vec::new(), vec![removed.clone()], Vec::new())
            .unwrap();
    let conversion = convert_filtered_sources(
        vec![FilteredSourceInput {
            source: source(fixture()),
            filter,
        }],
        &limits,
        &matrix,
    );
    assert_eq!(conversion.nodes.len(), baseline.nodes.len() - 1);
    assert!(
        conversion
            .nodes
            .iter()
            .all(|node| node.node_id.as_str() != removed)
    );
}
