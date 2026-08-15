use std::collections::BTreeSet;

use nethop_protocol::{EventKind, FrameCodec, PROTOCOL_VERSION, ProtocolError};
use serde_json::Value;

const FIXTURE: &str = include_str!("../../../tests/webui/fixtures/protocol-v1-before.json");

fn event_kind_name(kind: EventKind) -> &'static str {
    match kind {
        EventKind::Config => "config",
        EventKind::Runtime => "runtime",
        EventKind::Subscription => "subscription",
        EventKind::Generation => "generation",
        EventKind::Network => "network",
        EventKind::Traffic => "traffic",
        EventKind::SubscriptionMode => "subscription_mode",
        EventKind::SubscriptionActiveSet => "subscription_active_set",
        EventKind::NodeSelection => "node_selection",
        EventKind::NodeActive => "node_active",
        EventKind::NodeTest => "node_test",
    }
}

#[test]
fn protocol_v1_before_golden_is_frozen_and_rejected_by_v2() {
    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(fixture["protocol_version"], 1);
    assert_eq!(PROTOCOL_VERSION, 5);

    let frames = fixture["frames"].as_object().unwrap();
    for value in frames.values() {
        let payload = serde_json::to_vec(value).unwrap();
        let mut framed = (payload.len() as u32).to_be_bytes().to_vec();
        framed.extend_from_slice(&payload);
        assert_eq!(
            FrameCodec::decode(&framed).unwrap_err(),
            ProtocolError::UnsupportedVersion
        );
    }
    let expected = [
        EventKind::Config,
        EventKind::Runtime,
        EventKind::Subscription,
        EventKind::Generation,
        EventKind::Network,
    ];
    let events = fixture["events"].as_array().unwrap();
    let kinds = events
        .iter()
        .map(|event| event["event_kind"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    let expected_names = expected.map(event_kind_name).into_iter().collect();
    assert_eq!(kinds, expected_names);
    assert!(events.iter().all(|event| event["payload"].is_object()));
}
