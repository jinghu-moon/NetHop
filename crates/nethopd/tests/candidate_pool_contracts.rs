#![cfg(feature = "subscription-update")]

use std::collections::HashSet;

use nethop_subscription::SourceId;
use nethopd::{
    CandidatePoolNode, NodeAttribution, StableNodeId, SubscriptionMode, build_candidate_pools,
};

fn source(number: usize) -> SourceId {
    SourceId::new(format!("src_{number:032x}")).unwrap()
}

fn node(number: usize, sources: &[usize]) -> CandidatePoolNode {
    CandidatePoolNode::new(
        StableNodeId::new(format!("nh1s-{number:016x}")).unwrap(),
        NodeAttribution::new(sources.iter().copied().map(source)).unwrap(),
    )
}

fn ids(values: &[nethopd::StableNodeId]) -> Vec<&str> {
    values.iter().map(StableNodeId::as_str).collect()
}

#[test]
fn d001_single_pool_is_stable_sorted_and_bounded() {
    let active = [source(1)];
    let first = vec![node(3, &[1]), node(1, &[1]), node(2, &[1])];
    let reverse = first.iter().cloned().rev().collect::<Vec<_>>();
    let expected = ["nh1s-0000000000000001", "nh1s-0000000000000002"];
    for input in [&first, &reverse] {
        let pools = build_candidate_pools(SubscriptionMode::Single, &active, input, 2).unwrap();
        assert_eq!(ids(pools.auto()), expected);
        assert_eq!(pools.manual().len(), 3);
        assert_eq!(pools.contributions()[0].candidates(), 2);
    }
}

#[test]
fn d002_merge_round_robin_prevents_a_large_first_source_from_starving_others() {
    let active = [source(1), source(2), source(3)];
    let mut nodes = (0..20)
        .map(|index| node(100 + index, &[1]))
        .collect::<Vec<_>>();
    nodes.push(node(1, &[2]));
    nodes.push(node(2, &[3]));
    let pools = build_candidate_pools(SubscriptionMode::Merge, &active, &nodes, 6).unwrap();
    assert_eq!(
        pools
            .contributions()
            .iter()
            .map(|entry| entry.candidates())
            .collect::<Vec<_>>(),
        [4, 1, 1]
    );
    assert!(ids(pools.auto()).contains(&"nh1s-0000000000000001"));
    assert!(ids(pools.auto()).contains(&"nh1s-0000000000000002"));
}

#[test]
fn d003_shared_nodes_consume_one_slot_and_advance_every_source_queue() {
    let active = [source(1), source(2)];
    let nodes = [node(1, &[1, 2]), node(2, &[1]), node(3, &[2])];
    let pools = build_candidate_pools(SubscriptionMode::Merge, &active, &nodes, 3).unwrap();
    assert_eq!(pools.auto().len(), 3);
    assert_eq!(
        pools.auto().iter().collect::<HashSet<_>>().len(),
        pools.auto().len()
    );
    assert_eq!(
        ids(pools.auto()),
        [
            "nh1s-0000000000000001",
            "nh1s-0000000000000003",
            "nh1s-0000000000000002",
        ]
    );
}

#[test]
fn d004_empty_sources_do_not_block_non_empty_sources() {
    let active = [source(1), source(2), source(3)];
    let nodes = [node(1, &[2]), node(2, &[2])];
    let pools = build_candidate_pools(SubscriptionMode::Merge, &active, &nodes, 64).unwrap();
    assert_eq!(pools.auto().len(), 2);
    assert_eq!(
        pools
            .contributions()
            .iter()
            .map(|entry| entry.candidates())
            .collect::<Vec<_>>(),
        [0, 2, 0]
    );
}

#[test]
fn d005_limits_cover_one_sixteen_sixty_four_and_two_hundred_fifty_six() {
    let active = (1..=16).map(source).collect::<Vec<_>>();
    let nodes = (0..512)
        .map(|index| node(index, &[index % 16 + 1]))
        .collect::<Vec<_>>();
    for limit in [1, 16, 64, 256] {
        let pools = build_candidate_pools(SubscriptionMode::Merge, &active, &nodes, limit).unwrap();
        assert_eq!(pools.auto().len(), limit);
    }
    assert!(build_candidate_pools(SubscriptionMode::Merge, &active, &nodes, 0).is_err());
    assert!(build_candidate_pools(SubscriptionMode::Merge, &active, &nodes, 257).is_err());
}

#[test]
fn d006_source_reordering_has_a_predictable_tie_break() {
    let nodes = [node(1, &[1]), node(2, &[2])];
    let forward =
        build_candidate_pools(SubscriptionMode::Merge, &[source(1), source(2)], &nodes, 1).unwrap();
    let reverse =
        build_candidate_pools(SubscriptionMode::Merge, &[source(2), source(1)], &nodes, 1).unwrap();
    assert_eq!(ids(forward.auto()), ["nh1s-0000000000000001"]);
    assert_eq!(ids(reverse.auto()), ["nh1s-0000000000000002"]);
}

#[test]
fn d007_manual_pool_keeps_every_active_deduplicated_node() {
    let nodes = (0..300).map(|index| node(index, &[1])).collect::<Vec<_>>();
    let pools = build_candidate_pools(SubscriptionMode::Single, &[source(1)], &nodes, 64).unwrap();
    assert_eq!(pools.auto().len(), 64);
    assert_eq!(pools.manual().len(), 300);
}

#[test]
fn d008_ten_thousand_generated_inputs_preserve_pool_properties() {
    let mut state = 0x9e37_79b9_u64;
    for sample in 0..10_000 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let source_count = (state as usize % 4) + 1;
        let node_count = source_count + ((state >> 8) as usize % 28);
        let active = (1..=source_count).map(source).collect::<Vec<_>>();
        let mut nodes = (0..node_count)
            .map(|index| node(sample * 64 + index, &[index % source_count + 1]))
            .collect::<Vec<_>>();
        let limit = source_count.max(((state >> 16) as usize % 16) + 1);
        let expected = build_candidate_pools(
            if source_count == 1 {
                SubscriptionMode::Single
            } else {
                SubscriptionMode::Merge
            },
            &active,
            &nodes,
            limit,
        )
        .unwrap();
        nodes.rotate_left((state as usize) % node_count);
        nodes.reverse();
        let repeated = build_candidate_pools(
            if source_count == 1 {
                SubscriptionMode::Single
            } else {
                SubscriptionMode::Merge
            },
            &active,
            &nodes,
            limit,
        )
        .unwrap();
        assert_eq!(expected.auto(), repeated.auto());
        assert_eq!(expected.manual(), repeated.manual());
        assert!(expected.auto().len() <= limit);
        assert_eq!(
            expected.auto().iter().collect::<HashSet<_>>().len(),
            expected.auto().len()
        );
        assert!(
            expected
                .contributions()
                .iter()
                .all(|entry| entry.candidates() >= 1)
        );
    }
}
