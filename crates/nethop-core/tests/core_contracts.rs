use std::collections::BTreeMap;

use nethop_core::{
    Candidate, CaptureMode, CapturePolicy, CapturePolicyError, ClashApi, CoreDiagnosticCode,
    CoreError, GenerationId, GenerationStore, InterfacePolicy, ManagedConfig, ManagedLogLevel,
    ManagedOptions, ManagedOutboundMode, ManagedProfile, ManagedSelectorMode, RuntimeState,
    StateTransitionError, TerminalOutbound, TunStack,
};
use serde_json::json;

fn outbound(tag: &str) -> TerminalOutbound {
    TerminalOutbound::new(
        tag,
        "vless",
        BTreeMap::from([
            ("server".to_owned(), json!("example.com")),
            ("server_port".to_owned(), json!(443)),
        ]),
    )
    .expect("fixture outbound is valid")
}

#[test]
fn runtime_state_allows_only_declared_lifecycle_edges() {
    assert_eq!(
        RuntimeState::Init
            .transition(RuntimeState::Probing)
            .unwrap(),
        RuntimeState::Probing
    );
    assert_eq!(
        RuntimeState::RunningTproxy
            .transition(RuntimeState::RunningTun)
            .unwrap_err(),
        StateTransitionError::Invalid {
            from: RuntimeState::RunningTproxy,
            to: RuntimeState::RunningTun,
        }
    );
    assert_eq!(
        RuntimeState::RunningTproxy
            .transition(RuntimeState::Stopping)
            .unwrap(),
        RuntimeState::Stopping
    );
    assert_eq!(
        RuntimeState::Degraded
            .transition(RuntimeState::FailOpenDirect)
            .unwrap(),
        RuntimeState::FailOpenDirect
    );
    assert_eq!(
        RuntimeState::FailOpenDirect
            .transition(RuntimeState::CircuitOpen)
            .unwrap(),
        RuntimeState::CircuitOpen
    );
    assert!(
        RuntimeState::CircuitOpen
            .transition(RuntimeState::Probing)
            .is_err()
    );
}

#[test]
fn composer_generates_nodes_only_config_with_deterministic_bytes() {
    let config_a =
        ManagedConfig::from_outbounds(vec![outbound("node-b"), outbound("node-a")]).unwrap();
    let config_b =
        ManagedConfig::from_outbounds(vec![outbound("node-a"), outbound("node-b")]).unwrap();

    assert_eq!(config_a.bytes(), config_b.bytes());
    assert_eq!(config_a.node_count(), 2);
    let value: serde_json::Value = serde_json::from_slice(config_a.bytes()).unwrap();
    assert!(value.get("inbounds").is_none());
    assert!(value.get("route").is_none());
    assert_eq!(value["outbounds"].as_array().unwrap().len(), 2);
}

#[test]
fn managed_composer_generates_tproxy_profile_with_controlled_topology() {
    let policy = CapturePolicy::new(
        CaptureMode::Tproxy,
        true,
        Some(7893),
        Some(0x4e48),
        vec![1001, 1002],
        vec![],
    )
    .unwrap();
    let profile = ManagedProfile::new(
        policy,
        vec![outbound("node-b"), outbound("node-a")],
        ClashApi::new("127.0.0.1:9090", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap(),
    )
    .unwrap();
    let config = ManagedConfig::from_profile(profile).unwrap();
    let value: serde_json::Value = serde_json::from_slice(config.bytes()).unwrap();

    assert_eq!(value["inbounds"][0]["type"], "tproxy");
    assert_eq!(value["inbounds"][0]["listen"], "::");
    assert_eq!(value["inbounds"][0]["listen_port"], 7893);
    assert_eq!(value["route"]["final"], "nethop-select");
    assert_eq!(
        value["experimental"]["clash_api"]["external_controller"],
        "127.0.0.1:9090"
    );
    assert_eq!(value["outbounds"][0]["tag"], "direct");
    assert_eq!(value["outbounds"][1]["tag"], "block");
    assert_eq!(value["outbounds"][2]["tag"], "nethop-auto");
    assert_eq!(value["outbounds"][3]["tag"], "nethop-select");
    assert_eq!(value["dns"]["servers"][0]["type"], "https");
    assert_eq!(value["dns"]["servers"][0]["tag"], "dns-direct");
    assert_eq!(value["dns"]["servers"][0]["server"], "223.5.5.5");
    assert_eq!(value["dns"]["servers"][0]["server_port"], 443);
    assert_eq!(value["dns"]["servers"][0]["path"], "/dns-query");
    assert_eq!(
        value["dns"]["servers"][0]["headers"]["Host"],
        "dns.alidns.com"
    );
    assert_eq!(
        value["dns"]["servers"][0]["tls"]["server_name"],
        "dns.alidns.com"
    );
    assert_eq!(value["dns"]["servers"][1]["type"], "https");
    assert_eq!(value["dns"]["servers"][1]["tag"], "dns-proxy");
    assert_eq!(value["dns"]["servers"][1]["server"], "1.1.1.1");
    assert_eq!(value["dns"]["servers"][1]["detour"], "nethop-select");
    assert_eq!(value["dns"]["final"], "dns-proxy");
    assert_eq!(value["dns"]["servers"][0]["tag"], "dns-direct");
    assert!(value["dns"]["rules"].as_array().is_some_and(|rules| {
        rules.iter().any(|rule| {
            rule["rule_set"] == serde_json::json!(["nethop-cn-domain"])
                && rule["server"] == "dns-direct"
        })
    }));
    assert_eq!(value["dns"]["strategy"], "prefer_ipv4");
    assert_eq!(value["dns"]["disable_cache"], false);
    assert_eq!(value["dns"]["cache_capacity"], 4096);
    assert_eq!(value["route"]["default_domain_resolver"], "dns-direct");
    assert!(value["route"]["rule_set"].as_array().is_some_and(|sets| {
        sets.iter().any(|set| {
            set["tag"] == "nethop-cn-domain"
                && set["type"] == "local"
                && set["format"] == "binary"
                && set["path"] == "/data/adb/nethop/rulesets/cn-domain.srs"
        }) && sets.iter().any(|set| {
            set["tag"] == "nethop-cn-ip"
                && set["type"] == "local"
                && set["format"] == "binary"
                && set["path"] == "/data/adb/nethop/rulesets/cn-ip.srs"
        })
    }));
    assert!(
        value["route"]["rules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|rule| rule["action"] == "hijack-dns"
                && rule["type"] == "logical"
                && rule["mode"] == "or"
                && rule["rules"].as_array().is_some_and(|rules| {
                    rules.iter().any(|item| item["protocol"] == "dns")
                        && rules.iter().any(|item| item["port"] == 53)
                }))
    );
    assert!(
        value["route"]["rules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|rule| {
                rule["rule_set"] == serde_json::json!(["nethop-cn-domain"])
                    && rule["outbound"] == "direct"
            })
    );
    assert!(
        value["route"]["rules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|rule| {
                rule["rule_set"] == serde_json::json!(["nethop-cn-ip"])
                    && rule["outbound"] == "direct"
            })
    );
    assert_eq!(config.node_count(), 2);
}

#[test]
fn managed_options_control_urltest_logging_and_route_without_raw_json() {
    let capture = CapturePolicy::new(
        CaptureMode::Tproxy,
        true,
        Some(7893),
        Some(0x4e48),
        vec![],
        vec![0],
    )
    .unwrap();
    let profile = ManagedProfile::new(
        capture,
        vec![outbound("node-b"), outbound("node-a")],
        ClashApi::new("127.0.0.1:9090", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap(),
    )
    .unwrap()
    .with_options(
        ManagedOptions::new(
            ManagedOutboundMode::Rule,
            ManagedSelectorMode::Manual,
            15,
            75,
            1,
            ManagedLogLevel::Debug,
            true,
            false,
            vec!["203.0.113.0/24".to_owned()],
            vec!["192.0.2.0/24".to_owned()],
        )
        .unwrap(),
    );
    let value: serde_json::Value =
        serde_json::from_slice(ManagedConfig::from_profile(profile).unwrap().bytes()).unwrap();

    assert_eq!(value["log"]["level"], "debug");
    assert_eq!(value["outbounds"][2]["interval"], "15m");
    assert_eq!(value["outbounds"][2]["tolerance"], 75);
    assert_eq!(
        value["outbounds"][2]["outbounds"].as_array().unwrap().len(),
        1
    );
    assert_eq!(value["outbounds"][3]["default"], "node-a");
    assert!(
        value["route"]["rules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|rule| {
                rule["ip_cidr"] == serde_json::json!(["203.0.113.0/24"])
                    && rule["outbound"] == "nethop-select"
            })
    );
    assert!(
        value["route"]["rules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|rule| {
                rule["ip_cidr"] == serde_json::json!(["192.0.2.0/24"])
                    && rule["outbound"] == "direct"
            })
    );
}

#[test]
fn managed_domain_overrides_have_explicit_route_and_dns_precedence() {
    let capture = CapturePolicy::new(
        CaptureMode::Tproxy,
        true,
        Some(7893),
        Some(0x4e48),
        vec![],
        vec![0],
    )
    .unwrap();
    let options = ManagedOptions::default()
        .with_domain_rules(
            vec!["proxy.example".into()],
            vec!["direct.example".into()],
            vec!["blocked.example".into()],
        )
        .unwrap();
    let profile = ManagedProfile::new(
        capture,
        vec![outbound("node-a")],
        ClashApi::new("127.0.0.1:9090", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap(),
    )
    .unwrap()
    .with_options(options);
    let value: serde_json::Value =
        serde_json::from_slice(ManagedConfig::from_profile(profile).unwrap().bytes()).unwrap();
    let rules = value["route"]["rules"].as_array().unwrap();
    let domain_rules = rules
        .iter()
        .filter(|rule| rule.get("domain_suffix").is_some())
        .collect::<Vec<_>>();
    assert_eq!(domain_rules[0]["outbound"], "block");
    assert_eq!(domain_rules[1]["outbound"], "nethop-select");
    assert_eq!(domain_rules[2]["outbound"], "direct");
    let dns_rules = value["dns"]["rules"].as_array().unwrap();
    assert!(dns_rules.iter().any(|rule| {
        rule["domain_suffix"] == json!(["proxy.example"]) && rule["server"] == "dns-proxy"
    }));
    assert!(dns_rules.iter().any(|rule| {
        rule["domain_suffix"] == json!(["direct.example"]) && rule["server"] == "dns-direct"
    }));

    assert!(
        ManagedOptions::default()
            .with_domain_rules(vec!["Bad Domain".into()], vec![], vec![])
            .is_err()
    );
}

#[test]
fn managed_outbound_modes_have_distinct_dns_and_cn_routing_semantics() {
    let compose = |mode| {
        let capture = CapturePolicy::new(
            CaptureMode::Tproxy,
            true,
            Some(7893),
            Some(0x4e48),
            vec![],
            vec![0],
        )
        .unwrap();
        let options = ManagedOptions::new(
            mode,
            ManagedSelectorMode::Urltest,
            10,
            50,
            64,
            ManagedLogLevel::Warn,
            true,
            true,
            vec![],
            vec![],
        )
        .unwrap();
        let profile = ManagedProfile::new(
            capture,
            vec![outbound("node-a")],
            ClashApi::new("127.0.0.1:9090", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap(),
        )
        .unwrap()
        .with_options(options);
        serde_json::from_slice::<serde_json::Value>(
            ManagedConfig::from_profile(profile).unwrap().bytes(),
        )
        .unwrap()
    };

    let rule = compose(ManagedOutboundMode::Rule);
    assert_eq!(rule["route"]["final"], "nethop-select");
    assert_eq!(rule["dns"]["final"], "dns-proxy");
    assert_eq!(rule["route"]["rule_set"].as_array().unwrap().len(), 2);
    assert_eq!(rule["dns"]["rules"].as_array().unwrap().len(), 1);

    let global = compose(ManagedOutboundMode::Global);
    assert_eq!(global["route"]["final"], "nethop-select");
    assert_eq!(global["dns"]["final"], "dns-proxy");
    assert!(global["route"]["rule_set"].as_array().unwrap().is_empty());
    assert!(global["dns"]["rules"].as_array().unwrap().is_empty());

    let direct = compose(ManagedOutboundMode::Direct);
    assert_eq!(direct["route"]["final"], "direct");
    assert_eq!(direct["dns"]["final"], "dns-direct");
    assert!(direct["route"]["rule_set"].as_array().unwrap().is_empty());
    assert!(direct["dns"]["rules"].as_array().unwrap().is_empty());
}

#[test]
fn managed_composer_generates_tun_stack_without_tproxy_fields() {
    let policy = CapturePolicy::new(CaptureMode::Tun, true, None, None, vec![], vec![]).unwrap();
    let profile = ManagedProfile::new(
        policy,
        vec![outbound("node-a")],
        ClashApi::new("127.0.0.1:9090", "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap(),
    )
    .unwrap()
    .with_tun_stack(TunStack::System);
    let value: serde_json::Value =
        serde_json::from_slice(ManagedConfig::from_profile(profile).unwrap().bytes()).unwrap();

    assert_eq!(value["inbounds"][0]["type"], "tun");
    assert_eq!(value["inbounds"][0]["interface_name"], "nethop0");
    assert_eq!(value["inbounds"][0]["stack"], "system");
    assert!(value["inbounds"][0].get("listen_port").is_none());
}

#[test]
fn managed_composer_rejects_non_loopback_api_and_leaks_no_secret_in_debug() {
    assert!(ClashApi::new("0.0.0.0:9090", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").is_err());
    let api = ClashApi::new("127.0.0.1:9090", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
    let policy =
        CapturePolicy::new(CaptureMode::Direct, false, None, None, vec![], vec![]).unwrap();
    let profile = ManagedProfile::new(policy, vec![outbound("node-a")], api).unwrap();
    assert!(!format!("{profile:?}").contains("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
    let config = ManagedConfig::from_profile(profile).unwrap();
    assert!(!format!("{config:?}").contains("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
}

#[test]
fn managed_composer_is_order_independent_and_enforces_owned_tags() {
    let policy = || {
        CapturePolicy::new(
            CaptureMode::Tproxy,
            true,
            Some(7893),
            Some(0x4e48),
            vec![],
            vec![],
        )
        .unwrap()
    };
    let api = || ClashApi::new("127.0.0.1:9090", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
    let left = ManagedConfig::from_profile(
        ManagedProfile::new(
            policy(),
            vec![outbound("node-b"), outbound("node-a")],
            api(),
        )
        .unwrap(),
    )
    .unwrap();
    let right = ManagedConfig::from_profile(
        ManagedProfile::new(
            policy(),
            vec![outbound("node-a"), outbound("node-b")],
            api(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(left.bytes(), right.bytes());

    assert_eq!(
        ManagedProfile::new(policy(), vec![outbound("direct")], api()).unwrap_err(),
        nethop_core::ComposerError::ReservedTag
    );
}

#[test]
fn managed_composer_bounds_active_nodes_and_redacts_terminal_fields() {
    let policy =
        CapturePolicy::new(CaptureMode::Direct, false, None, None, vec![], vec![]).unwrap();
    let api = ClashApi::new("127.0.0.1:9090", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
    let nodes = (0..2_001)
        .map(|index| outbound(&format!("node-{index}")))
        .collect();
    assert_eq!(
        ManagedProfile::new(policy, nodes, api).unwrap_err(),
        nethop_core::ComposerError::TooManyOutbounds
    );

    let secret_node = TerminalOutbound::new(
        "secret-node",
        "trojan",
        BTreeMap::from([("password".to_owned(), json!("credential-canary"))]),
    )
    .unwrap();
    assert!(!format!("{secret_node:?}").contains("credential-canary"));
}

#[test]
fn composer_rejects_reserved_top_level_semantics() {
    let result = TerminalOutbound::new(
        "node-a",
        "vless",
        BTreeMap::from([("inbounds".to_owned(), json!([]))]),
    );
    assert_eq!(
        result.unwrap_err(),
        nethop_core::ComposerError::ReservedField("inbounds".into())
    );
}

#[test]
fn generation_store_keeps_previous_generation_when_validation_fails() {
    let directory = tempfile::tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    let first = Candidate::new(
        GenerationId::new(1).unwrap(),
        ManagedConfig::from_outbounds(vec![outbound("one")]).unwrap(),
    );
    store.publish(&first, |_| Ok(())).unwrap();

    let second = Candidate::new(
        GenerationId::new(2).unwrap(),
        ManagedConfig::from_outbounds(vec![outbound("two")]).unwrap(),
    );
    let error = store
        .publish(&second, |_| Err(CoreError::ValidationFailed))
        .unwrap_err();

    assert_eq!(error, CoreError::ValidationFailed);
    assert_eq!(
        store.current_generation().unwrap(),
        Some(GenerationId::new(1).unwrap())
    );
    assert!(!directory.path().join("generations/2").exists());
}

#[test]
fn generation_store_publishes_manifest_and_current_pointer_atomically() {
    let directory = tempfile::tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    let config = ManagedConfig::from_outbounds(vec![outbound("one")]).unwrap();
    let candidate = Candidate::new(GenerationId::new(7).unwrap(), config.clone());

    store
        .publish(&candidate, |bytes| {
            assert_eq!(bytes, config.bytes());
            Ok(())
        })
        .unwrap();

    let generation = directory.path().join("generations/7");
    assert_eq!(
        std::fs::read_to_string(directory.path().join("generations/current")).unwrap(),
        "7\n"
    );
    assert_eq!(
        std::fs::read(generation.join("config.json")).unwrap(),
        config.bytes()
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(generation.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["generation"], 7);
    assert_eq!(manifest["node_count"], 1);
}

#[cfg(unix)]
#[test]
fn generation_store_enforces_private_directory_and_file_modes() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("store");
    let store = GenerationStore::new(&root).unwrap();
    let candidate = Candidate::new(
        GenerationId::new(1).unwrap(),
        ManagedConfig::from_outbounds(vec![outbound("one")]).unwrap(),
    );
    store.publish(&candidate, |_| Ok(())).unwrap();

    for path in [
        &root,
        &root.join("generations"),
        &root.join("generations/1"),
    ] {
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o700,
            "{} must be private",
            path.display()
        );
    }
    for path in [
        root.join("generations/1/config.json"),
        root.join("generations/1/manifest.json"),
        root.join("generations/current"),
    ] {
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600,
            "{} must be private",
            path.display()
        );
    }
}

#[test]
fn generation_lifecycle_does_not_activate_before_explicit_commit() {
    let directory = tempfile::tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    let candidate = Candidate::new(
        GenerationId::new(11).unwrap(),
        ManagedConfig::from_outbounds(vec![outbound("eleven")]).unwrap(),
    );

    let prepared = store.prepare_candidate(&candidate).unwrap();
    assert!(prepared.config_path().is_file());
    assert_eq!(store.current_generation().unwrap(), None);

    let sealed = store.seal_candidate(&prepared).unwrap();
    assert!(sealed.config_path().is_file());
    assert_eq!(store.current_generation().unwrap(), None);

    store.commit_generation(&sealed).unwrap();
    assert_eq!(
        store.current_generation().unwrap(),
        Some(candidate.generation())
    );
}

#[test]
fn generation_discard_and_rollback_preserve_a_valid_active_target() {
    let directory = tempfile::tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    let first = Candidate::new(
        GenerationId::new(1).unwrap(),
        ManagedConfig::from_outbounds(vec![outbound("one")]).unwrap(),
    );
    store.publish(&first, |_| Ok(())).unwrap();

    let second = Candidate::new(
        GenerationId::new(2).unwrap(),
        ManagedConfig::from_outbounds(vec![outbound("two")]).unwrap(),
    );
    let prepared = store.prepare_candidate(&second).unwrap();
    store.discard_prepared(prepared).unwrap();
    assert!(!directory.path().join("generations/2").exists());

    let prepared = store.prepare_candidate(&second).unwrap();
    let sealed = store.seal_candidate(&prepared).unwrap();
    store.commit_generation(&sealed).unwrap();
    store.rollback_to(first.generation()).unwrap();
    assert_eq!(
        store.current_generation().unwrap(),
        Some(first.generation())
    );
    store.discard_sealed(sealed).unwrap();
    assert!(!directory.path().join("generations/2").exists());
}

#[test]
fn rollback_rejects_a_generation_whose_config_no_longer_matches_manifest() {
    let directory = tempfile::tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    let first = Candidate::new(
        GenerationId::new(1).unwrap(),
        ManagedConfig::from_outbounds(vec![outbound("one")]).unwrap(),
    );
    store.publish(&first, |_| Ok(())).unwrap();
    std::fs::write(
        directory.path().join("generations/1/config.json"),
        b"{\"outbounds\":[]}",
    )
    .unwrap();

    let error = store.rollback_to(first.generation()).unwrap_err();
    assert_eq!(error.code(), CoreDiagnosticCode::GenerationPublishFailed);
}

#[test]
fn current_generation_is_reopened_only_after_manifest_verification() {
    let directory = tempfile::tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    assert!(store.current_sealed_generation().unwrap().is_none());
    let candidate = Candidate::new(
        GenerationId::new(9).unwrap(),
        ManagedConfig::from_outbounds(vec![outbound("nine")]).unwrap(),
    );
    store.publish(&candidate, |_| Ok(())).unwrap();
    let current = store.current_sealed_generation().unwrap().unwrap();
    assert_eq!(current.generation(), candidate.generation());
    assert!(current.config_path().is_file());

    std::fs::write(current.config_path(), b"{\"outbounds\":[]}").unwrap();
    assert!(store.current_sealed_generation().is_err());
}

#[test]
fn generation_id_zero_is_rejected() {
    assert_eq!(
        GenerationId::new(0).unwrap_err(),
        CoreError::InvalidGenerationId
    );
    assert_eq!(
        CoreDiagnosticCode::GenerationPublishFailed.as_str(),
        "generation_publish_failed"
    );
}

#[test]
fn capture_policy_is_shared_and_deterministic_for_uid_selection() {
    let policy = CapturePolicy::new(
        CaptureMode::Tproxy,
        true,
        Some(12345),
        Some(0x4e48),
        vec![1002, 1001, 1002],
        vec![1003],
    )
    .unwrap();
    assert_eq!(policy.include_uids(), [1001, 1002]);
    assert!(policy.captures_uid(1001));
    assert!(!policy.captures_uid(1003));
    assert!(!policy.captures_uid(1004));
}

#[test]
fn capture_policy_rejects_missing_tproxy_primitives_and_overlap() {
    assert_eq!(
        CapturePolicy::new(CaptureMode::Tproxy, true, Some(12345), None, vec![], vec![])
            .unwrap_err(),
        CapturePolicyError::MissingTproxyMark
    );
    assert_eq!(
        CapturePolicy::new(
            CaptureMode::Tproxy,
            true,
            Some(12345),
            Some(1),
            vec![1001],
            vec![1001]
        )
        .unwrap_err(),
        CapturePolicyError::OverlappingUidPolicy
    );
}

#[test]
fn interface_policy_is_bounded_and_cannot_disable_every_interface() {
    let policy =
        InterfacePolicy::new(false, true, vec!["wlan*".into()], vec!["wlan-test".into()]).unwrap();
    assert!(!policy.mobile());
    assert!(policy.wifi());
    assert_eq!(policy.include(), ["wlan*"]);
    assert_eq!(policy.exclude(), ["wlan-test"]);
    assert_eq!(
        InterfacePolicy::new(false, false, Vec::new(), Vec::new()).unwrap_err(),
        CapturePolicyError::InvalidInterfacePolicy
    );
    assert_eq!(
        InterfacePolicy::new(true, true, vec!["bad/name".into()], Vec::new()).unwrap_err(),
        CapturePolicyError::InvalidInterfacePolicy
    );
}
