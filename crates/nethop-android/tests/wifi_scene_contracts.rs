use nethop_android::{
    CapabilityError, CommandWifiFactsSource, ProbeBackend, ProbeCommand, ProbeOutput,
    WifiFactsSource, WifiNetworkFacts, WifiSceneAction, WifiSceneError, WifiSceneMatcher,
    WifiSceneRule,
};

#[test]
fn wifi_facts_and_rules_never_expose_ssid_or_bssid_in_debug() {
    let facts = WifiNetworkFacts::new(
        Some("Private Home".into()),
        Some("aa:bb:cc:dd:ee:ff".into()),
    )
    .unwrap();
    let rule = WifiSceneRule::new(
        "home",
        Some("Private Home".into()),
        Some("aa:bb:cc:dd:ee:ff".into()),
        WifiSceneAction::DisableProxy,
    )
    .unwrap();
    let debug = format!("{facts:?} {rule:?}");
    assert!(!debug.contains("Private Home"));
    assert!(!debug.contains("aa:bb"));
    assert!(debug.contains("[REDACTED]"));
}

#[test]
fn bssid_specific_rule_wins_and_only_emits_a_typed_reconcile_decision() {
    let matcher = WifiSceneMatcher::new(vec![
        WifiSceneRule::new(
            "office-default",
            Some("Office".into()),
            None,
            WifiSceneAction::EnableProxy,
        )
        .unwrap(),
        WifiSceneRule::new(
            "office-trusted-ap",
            Some("Office".into()),
            Some("00:11:22:33:44:55".into()),
            WifiSceneAction::DisableProxy,
        )
        .unwrap(),
    ])
    .unwrap();
    let facts =
        WifiNetworkFacts::new(Some("Office".into()), Some("00:11:22:33:44:55".into())).unwrap();
    let decision = matcher.evaluate(&facts).unwrap();
    assert_eq!(decision.scene_id(), "office-trusted-ap");
    assert_eq!(decision.action(), WifiSceneAction::DisableProxy);
    assert!(!decision.action().service_enabled());
    assert!(decision.requires_reconcile());
}

#[test]
fn wifi_scene_inputs_are_bounded_unique_and_fail_closed_when_ambiguous() {
    assert_eq!(
        WifiNetworkFacts::new(Some("<unknown ssid>".into()), None).unwrap_err(),
        WifiSceneError::NetworkUnavailable
    );
    assert!(WifiNetworkFacts::new(Some("x".repeat(33)), None).is_err());
    assert!(WifiNetworkFacts::new(None, Some("not-a-bssid".into())).is_err());

    let first = WifiSceneRule::new(
        "one",
        Some("Office".into()),
        None,
        WifiSceneAction::EnableProxy,
    )
    .unwrap();
    let duplicate = WifiSceneRule::new(
        "two",
        Some("Office".into()),
        None,
        WifiSceneAction::DisableProxy,
    )
    .unwrap();
    assert_eq!(
        WifiSceneMatcher::new(vec![first, duplicate]).unwrap_err(),
        WifiSceneError::DuplicateRule
    );
}

#[test]
fn android_wifi_status_parser_is_bounded_and_redacted() {
    let facts = WifiNetworkFacts::from_android_status(
        "Wi-Fi is enabled\nWifiInfo: SSID: \"Private Home\", BSSID: aa:bb:cc:dd:ee:ff, RSSI: -50",
    )
    .unwrap();
    let matcher = WifiSceneMatcher::new(vec![
        WifiSceneRule::new(
            "home",
            Some("Private Home".into()),
            Some("aa:bb:cc:dd:ee:ff".into()),
            WifiSceneAction::DisableProxy,
        )
        .unwrap(),
    ])
    .unwrap();
    assert_eq!(matcher.evaluate(&facts).unwrap().scene_id(), "home");
    assert!(!format!("{facts:?}").contains("Private Home"));
    assert_eq!(
        WifiNetworkFacts::from_android_status(&"x".repeat(64 * 1024 + 1)).unwrap_err(),
        WifiSceneError::InvalidStatus
    );
}

struct WifiProbe;

impl ProbeBackend for WifiProbe {
    fn run(&mut self, command: ProbeCommand) -> Result<ProbeOutput, CapabilityError> {
        assert_eq!(command, ProbeCommand::WifiStatus);
        Ok(ProbeOutput::new(
            true,
            "WifiInfo: SSID: Office, BSSID: 00:11:22:33:44:55",
            "",
        ))
    }
}

#[test]
fn command_source_uses_only_the_typed_wifi_status_probe() {
    let facts = CommandWifiFactsSource::new(WifiProbe).current().unwrap();
    let matcher = WifiSceneMatcher::new(vec![
        WifiSceneRule::new(
            "office",
            Some("Office".into()),
            None,
            WifiSceneAction::EnableProxy,
        )
        .unwrap(),
    ])
    .unwrap();
    assert_eq!(matcher.evaluate(&facts).unwrap().scene_id(), "office");
}
