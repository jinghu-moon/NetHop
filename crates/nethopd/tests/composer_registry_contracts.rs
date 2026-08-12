#![cfg(feature = "subscription-update")]

use nethop_core::{
    CaptureMode, CapturePolicy, ClashApi, GenerationId, ManagedLogLevel, ManagedOptions,
    ManagedOutboundMode, TunStack,
};
use nethop_subscription::{
    CapabilityMatrix, FormatHint, ParserLimits, SourceId, SourceInput, convert_stable_sources,
};
use nethopd::{SubscriptionMode, build_candidate};

#[test]
fn merge_candidate_uses_fair_pool_and_emits_complete_registry() {
    let one = SourceId::new("src_11111111111111111111111111111111").unwrap();
    let two = SourceId::new("src_22222222222222222222222222222222").unwrap();
    let conversion = convert_stable_sources(
        vec![
            SourceInput {
                source_id: one.clone(),
                format_hint: FormatHint::UriList,
                bytes: b"trojan://one@example.com:443#One\ntrojan://shared@example.com:443#Shared"
                    .to_vec(),
            },
            SourceInput {
                source_id: two.clone(),
                format_hint: FormatHint::UriList,
                bytes: b"trojan://two@example.com:443#Two\ntrojan://shared@example.com:443#Alias"
                    .to_vec(),
            },
        ],
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    );
    let options = ManagedOptions::new(
        ManagedOutboundMode::Rule,
        10,
        50,
        2,
        ManagedLogLevel::Warn,
        true,
        true,
        vec![],
        vec![],
    )
    .unwrap();
    let candidate = build_candidate(
        GenerationId::new(1).unwrap(),
        &conversion,
        nethopd::CandidateBuildProfile::new(
            CapturePolicy::new(CaptureMode::Direct, false, None, None, vec![], vec![]).unwrap(),
            ClashApi::new("127.0.0.1:9090", "a".repeat(32)).unwrap(),
            TunStack::System,
            options,
        ),
        SubscriptionMode::Merge,
        &[one.clone(), two.clone()],
    )
    .unwrap();
    let registry = candidate.node_registry().unwrap();
    assert_eq!(registry.records().len(), 3);
    assert_eq!(
        registry
            .records()
            .iter()
            .filter(|record| record.auto_candidate())
            .count(),
        2
    );
    let shared = registry
        .records()
        .iter()
        .find(|record| record.source_ids().len() == 2)
        .unwrap();
    assert_eq!(
        shared.source_ids(),
        [one.as_str().to_owned(), two.as_str().to_owned()]
    );
}
