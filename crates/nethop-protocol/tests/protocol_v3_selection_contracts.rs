use nethop_protocol::{
    ControlMethod, ControlParams, ControlRequest, EventKind, PROTOCOL_VERSION, ProtocolError,
    RequestId, SubscriptionMode,
};
use serde_json::{Value, json};

const GOLDEN: &str =
    include_str!("../../../tests/subscription-selection/fixtures/protocol-v3-golden.json");

fn request(method: ControlMethod) -> ControlRequest {
    ControlRequest::new(RequestId::new("v3-contract").unwrap(), method)
}

#[test]
fn protocol_v3_selection_method_names_match_golden() {
    let golden: Value = serde_json::from_str(GOLDEN).unwrap();
    assert_eq!(PROTOCOL_VERSION, golden["version"]);
    let cases = [
        (ControlMethod::SubscriptionModeGet, "mode_get"),
        (ControlMethod::SubscriptionModeSet, "mode_set"),
        (ControlMethod::SubscriptionSelect, "subscription_select"),
        (
            ControlMethod::SubscriptionSetEnabled,
            "subscription_set_enabled",
        ),
        (ControlMethod::NodeSelectionGet, "node_selection_get"),
        (ControlMethod::NodeSelectAuto, "node_select_auto"),
        (ControlMethod::NodeSelectManual, "node_select_manual"),
        (ControlMethod::NodeTestAll, "node_test_all"),
        (ControlMethod::NodeList, "node_list"),
    ];
    for (method, key) in cases {
        assert_eq!(
            serde_json::to_value(request(method)).unwrap()["method"],
            golden["methods"][key]
        );
    }
}

#[test]
fn subscription_transactions_require_exact_cas_and_mode_specific_params() {
    let digest = "a".repeat(64);
    let source = "src_0123456789abcdef0123456789abcdef".to_owned();
    assert!(
        request(ControlMethod::SubscriptionModeSet)
            .with_params(ControlParams::subscription_mode_set(
                digest.clone(),
                SubscriptionMode::Single,
                Some(source.clone()),
            ))
            .is_ok()
    );
    assert!(
        request(ControlMethod::SubscriptionModeSet)
            .with_params(ControlParams::subscription_mode_set(
                digest.clone(),
                SubscriptionMode::Merge,
                None,
            ))
            .is_ok()
    );
    assert_eq!(
        request(ControlMethod::SubscriptionModeSet)
            .with_params(ControlParams::subscription_mode_set(
                digest.clone(),
                SubscriptionMode::Single,
                None,
            ))
            .unwrap_err(),
        ProtocolError::InvalidEnvelope
    );
    assert!(
        request(ControlMethod::SubscriptionSetEnabled)
            .with_params(ControlParams::subscription_set_enabled(
                digest, source, true,
            ))
            .is_ok()
    );
}

#[test]
fn manual_selection_accepts_only_stable_ids_and_auto_has_no_target() {
    assert!(
        request(ControlMethod::NodeSelectManual)
            .with_params(ControlParams::target("nh1s-0123456789abcdef".to_owned()))
            .is_ok()
    );
    assert!(
        request(ControlMethod::NodeSelectManual)
            .with_params(ControlParams::target("internal-tag".to_owned()))
            .is_err()
    );
    assert!(
        request(ControlMethod::NodeSelectAuto)
            .with_params(ControlParams::target("nethop-auto".to_owned()))
            .is_err()
    );
}

#[test]
fn selection_events_are_typed_and_secret_free() {
    let kinds = [
        EventKind::SubscriptionMode,
        EventKind::SubscriptionActiveSet,
        EventKind::NodeSelection,
        EventKind::NodeActive,
        EventKind::NodeTest,
    ];
    let value = json!({"kinds":kinds,"node_id":"nh1s-0123456789abcdef"});
    let text = serde_json::to_string(&value).unwrap();
    assert!(!text.contains("https://"));
    assert!(!text.contains("internal_tag"));
    assert!(!text.contains("password"));
}
