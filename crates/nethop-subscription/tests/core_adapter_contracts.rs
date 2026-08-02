use nethop_core::ManagedConfig;
use nethop_subscription::{
    CapabilityMatrix, FormatHint, ParserLimits, SourceId, SourceInput, adapt_terminal_outbound,
    adapt_terminal_outbounds, compose_outbound, convert_stable_sources,
};

fn conversion() -> nethop_subscription::StableConversion {
    convert_stable_sources(
        vec![SourceInput {
            source_id: SourceId::new("core-adapter").unwrap(),
            format_hint: FormatHint::UriList,
            bytes: b"trojan://fixture-password@example.com:443?security=tls#node\n".to_vec(),
        }],
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    )
}

#[test]
fn adapter_preserves_the_audited_parser_outbound_exactly() {
    let conversion = conversion();
    assert_eq!(conversion.nodes.len(), 1);
    let expected = compose_outbound(&conversion.nodes[0]);
    let terminal = adapt_terminal_outbound(&conversion.nodes[0]).unwrap();
    let managed = ManagedConfig::from_outbounds(vec![terminal]).unwrap();
    let actual: serde_json::Value = serde_json::from_slice(managed.bytes()).unwrap();

    assert_eq!(actual["outbounds"][0], expected);
}

#[test]
fn adapter_accepts_only_unique_deduplicated_terminal_tags() {
    let conversion = conversion();
    let duplicate = vec![conversion.nodes[0].clone(), conversion.nodes[0].clone()];

    assert_eq!(
        adapt_terminal_outbounds(&duplicate).unwrap_err(),
        nethop_subscription::TerminalOutboundAdapterError::DuplicateTag
    );
}
