use std::io::Cursor;

use nethop_protocol::{
    ApplicationPolicyMode, ApplicationTarget, ConfigMutation, ControlError, ControlMethod,
    ControlParams, ControlRequest, ControlResponse, ErrorCode, ErrorDomain, EventKind, FrameCodec,
    LogChannel, MAX_FRAME_BYTES, PROTOCOL_VERSION, ProtocolError, RequestId, StreamFrame,
    StreamKind, SubscriptionFormatHint, SubscriptionRequestProfile, SubscriptionSourceFilter,
    SubscriptionSourcePatch, SubscriptionSourceSettings, WireFrame,
};
use serde_json::json;

fn request_id() -> RequestId {
    RequestId::new("req-001").unwrap()
}

#[test]
fn request_golden_uses_big_endian_length_and_v6_json() {
    let frame = WireFrame::Request(ControlRequest::new(request_id(), ControlMethod::StatusGet));
    let encoded = FrameCodec::encode(&frame).unwrap();
    let length = u32::from_be_bytes(encoded[..4].try_into().unwrap()) as usize;
    assert_eq!(length, encoded.len() - 4);
    assert_eq!(
        &encoded[4..],
        br#"{"version":6,"request_id":"req-001","method":"status.get","params":{}}"#
    );
    assert_eq!(FrameCodec::decode(&encoded).unwrap(), frame);
}

#[test]
fn request_rejects_unknown_fields_versions_and_unbounded_ids() {
    let unknown =
        br#"{"version":6,"request_id":"r","method":"status.get","params":{},"admin":true}"#;
    let mut framed = (unknown.len() as u32).to_be_bytes().to_vec();
    framed.extend_from_slice(unknown);
    assert_eq!(
        FrameCodec::decode(&framed),
        Err(ProtocolError::InvalidEnvelope)
    );

    let version = br#"{"version":1,"request_id":"r","method":"status.get","params":{}}"#;
    let mut framed = (version.len() as u32).to_be_bytes().to_vec();
    framed.extend_from_slice(version);
    assert_eq!(
        FrameCodec::decode(&framed),
        Err(ProtocolError::UnsupportedVersion)
    );
    assert_eq!(
        RequestId::new("x".repeat(65)).unwrap_err(),
        ProtocolError::InvalidRequestId
    );
}

#[test]
fn response_requires_exactly_one_result_or_error_branch() {
    let success = WireFrame::Response(ControlResponse::success(
        request_id(),
        Some(7),
        json!({"state":"running"}),
    ));
    assert_eq!(
        FrameCodec::decode(&FrameCodec::encode(&success).unwrap()).unwrap(),
        success
    );

    let error = ControlError::new(
        ErrorCode::new(ErrorDomain::Auth, "ROOT-REQUIRED").unwrap(),
        "root caller required",
    )
    .unwrap();
    let failure = WireFrame::Response(ControlResponse::failure(request_id(), Some(7), error));
    assert_eq!(
        FrameCodec::decode(&FrameCodec::encode(&failure).unwrap()).unwrap(),
        failure
    );

    let invalid = br#"{"version":6,"request_id":"r","ok":true,"generation":1,"error":{"code":"NH-AUTH-DENIED","message":"denied"}}"#;
    let mut framed = (invalid.len() as u32).to_be_bytes().to_vec();
    framed.extend_from_slice(invalid);
    assert_eq!(
        FrameCodec::decode(&framed),
        Err(ProtocolError::InvalidEnvelope)
    );
}

#[test]
fn all_stable_error_domains_round_trip() {
    for domain in [
        ErrorDomain::Config,
        ErrorDomain::Source,
        ErrorDomain::Subscription,
        ErrorDomain::Capability,
        ErrorDomain::Network,
        ErrorDomain::Core,
        ErrorDomain::Node,
        ErrorDomain::Stats,
        ErrorDomain::Auth,
    ] {
        let code = ErrorCode::new(domain, "FAILED").unwrap();
        assert!(code.as_str().starts_with("NH-"));
    }
    assert_eq!(
        ErrorCode::new(ErrorDomain::Core, "lowercase").unwrap_err(),
        ProtocolError::InvalidErrorCode
    );
}

#[test]
fn stream_has_explicit_item_end_and_error_frames() {
    let item = WireFrame::Stream(StreamFrame::item(request_id(), 1, json!({"bytes":10})));
    let end = WireFrame::Stream(StreamFrame::end(request_id(), 2));
    let error = WireFrame::Stream(StreamFrame::error(
        request_id(),
        3,
        ControlError::new(
            ErrorCode::new(ErrorDomain::Core, "UNAVAILABLE").unwrap(),
            "core unavailable",
        )
        .unwrap(),
    ));
    for frame in [item, end, error] {
        assert_eq!(
            FrameCodec::decode(&FrameCodec::encode(&frame).unwrap()).unwrap(),
            frame
        );
    }
    let end = StreamFrame::end(request_id(), 2);
    assert_eq!(end.kind(), StreamKind::End);
    assert_eq!(end.sequence(), 2);
}

#[test]
fn reader_rejects_oversized_length_before_allocating_payload() {
    let mut input = Cursor::new(((MAX_FRAME_BYTES + 1) as u32).to_be_bytes());
    assert_eq!(
        FrameCodec::read_from(&mut input),
        Err(ProtocolError::FrameTooLarge)
    );
}

#[test]
fn codec_rejects_trailing_bytes_invalid_utf8_and_truncated_io() {
    let request = WireFrame::Request(ControlRequest::new(request_id(), ControlMethod::StatusGet));
    let mut encoded = FrameCodec::encode(&request).unwrap();
    encoded.push(0);
    assert_eq!(
        FrameCodec::decode(&encoded),
        Err(ProtocolError::InvalidFrameLength)
    );

    let invalid_utf8 = [0, 0, 0, 1, 0xff];
    assert_eq!(
        FrameCodec::decode(&invalid_utf8),
        Err(ProtocolError::InvalidUtf8)
    );
    let mut truncated = Cursor::new([0, 0, 0, 4, b'{']);
    assert_eq!(
        FrameCodec::read_from(&mut truncated),
        Err(ProtocolError::Io)
    );
}

#[test]
fn protocol_version_is_frozen() {
    assert_eq!(PROTOCOL_VERSION, 6);
}

#[test]
fn control_error_details_are_optional_and_bounded_by_the_outer_frame() {
    let legacy = ControlError::new(
        ErrorCode::new(ErrorDomain::Config, "CONFLICT").unwrap(),
        "requested service is unavailable",
    )
    .unwrap();
    assert!(legacy.details().is_none());
    assert!(!serde_json::to_string(&legacy).unwrap().contains("details"));

    let detailed = ControlError::with_details(
        ErrorCode::new(ErrorDomain::Config, "CONFLICT").unwrap(),
        "requested service is unavailable",
        json!({
            "observed_config_digest": "a".repeat(64),
            "changed_field_ids": [],
            "requires_reload": true,
        }),
    )
    .unwrap();
    assert_eq!(detailed.details().unwrap()["requires_reload"], true);
    assert_eq!(
        serde_json::from_str::<ControlError>(&serde_json::to_string(&detailed).unwrap()).unwrap(),
        detailed
    );
}

#[test]
fn subscription_update_is_a_bounded_v6_empty_params_command() {
    let frame = WireFrame::Request(ControlRequest::new(
        request_id(),
        ControlMethod::SubscriptionUpdate,
    ));
    let encoded = FrameCodec::encode(&frame).unwrap();
    assert_eq!(
        &encoded[4..],
        br#"{"version":6,"request_id":"req-001","method":"subscription.update","params":{}}"#
    );
    assert_eq!(FrameCodec::decode(&encoded).unwrap(), frame);
}

#[test]
fn local_import_preview_and_apply_are_digest_bound_documents() {
    let document = json!({"content":"ss://example","format_hint":"auto"});
    let preview = ControlRequest::new(request_id(), ControlMethod::SubscriptionImportPreview)
        .with_params(ControlParams::import_document(
            "a".repeat(64),
            None,
            document.clone(),
        ))
        .unwrap();
    assert_eq!(preview.params().candidate_digest(), None);
    let candidate_digest = "b".repeat(64);
    let apply = ControlRequest::new(request_id(), ControlMethod::SubscriptionImportApply)
        .with_params(ControlParams::import_document(
            "a".repeat(64),
            Some(candidate_digest.clone()),
            document,
        ))
        .unwrap();
    assert_eq!(
        apply.params().candidate_digest(),
        Some(candidate_digest.as_str())
    );
    assert!(
        ControlRequest::new(request_id(), ControlMethod::SubscriptionImportApply)
            .with_params(ControlParams::import_document(
                "a".repeat(64),
                None,
                json!({"content":"x"}),
            ))
            .is_err()
    );
}

#[test]
fn config_reload_is_a_bounded_v6_empty_params_command() {
    let frame = WireFrame::Request(ControlRequest::new(
        request_id(),
        ControlMethod::ConfigReload,
    ));
    let encoded = FrameCodec::encode(&frame).unwrap();
    assert_eq!(
        &encoded[4..],
        br#"{"version":6,"request_id":"req-001","method":"config.reload","params":{}}"#
    );
    assert_eq!(FrameCodec::decode(&encoded).unwrap(), frame);
}

#[test]
fn config_export_has_a_stable_wire_name_and_accepts_only_empty_params() {
    let request = ControlRequest::new(request_id(), ControlMethod::ConfigExport);
    let encoded = serde_json::to_value(&request).unwrap();
    assert_eq!(encoded["method"], "config.export");
    assert!(
        request
            .clone()
            .with_params(ControlParams::default())
            .is_ok()
    );

    let invalid = request.with_params(ControlParams::event_subscription(vec![EventKind::Config]));
    assert_eq!(invalid.unwrap_err(), ProtocolError::InvalidEnvelope);
}

#[test]
fn core_version_check_has_a_stable_wire_name_and_accepts_only_empty_params() {
    let request = ControlRequest::new(request_id(), ControlMethod::CoreVersionCheck);
    let encoded = serde_json::to_value(&request).unwrap();
    assert_eq!(encoded["method"], "core.version_check");
    assert!(
        request
            .clone()
            .with_params(ControlParams::default())
            .is_ok()
    );
    assert_eq!(
        request
            .with_params(ControlParams::event_subscription(vec![EventKind::Runtime]))
            .unwrap_err(),
        ProtocolError::InvalidEnvelope
    );
}

#[test]
fn rule_set_methods_have_stable_names_and_only_update_accepts_wait() {
    let status = ControlRequest::new(request_id(), ControlMethod::RuleSetStatus);
    let status_value = serde_json::to_value(status).unwrap();
    assert_eq!(status_value["method"], "ruleset.status");

    let update = ControlRequest::new(request_id(), ControlMethod::RuleSetUpdate)
        .with_params(ControlParams::new(true, false))
        .unwrap();
    let update_value = serde_json::to_value(update).unwrap();
    assert_eq!(update_value["method"], "ruleset.update");
    assert_eq!(update_value["params"]["wait"], true);

    assert!(
        ControlRequest::new(request_id(), ControlMethod::RuleSetStatus)
            .with_params(ControlParams::new(true, false))
            .is_err()
    );
}

#[test]
fn operational_methods_use_stable_wire_names_and_scoped_params() {
    let cases = [
        (ControlMethod::NodeList, "node.list"),
        (ControlMethod::NodeTest, "node.test"),
        (ControlMethod::NodeTestAll, "node.test_all"),
        (ControlMethod::NodeSelectionGet, "node.selection_get"),
        (ControlMethod::NodeSelectAuto, "node.select_auto"),
        (ControlMethod::NodeSelectManual, "node.select_manual"),
        (ControlMethod::NodeExport, "node.export"),
        (ControlMethod::ConnectionsGet, "connections.get"),
        (ControlMethod::ConnectionClose, "connection.close"),
        (ControlMethod::ConnectionsCloseAll, "connections.close_all"),
        (ControlMethod::LogsGet, "logs.get"),
        (ControlMethod::LogsClear, "logs.clear"),
        (ControlMethod::DiagnosticsBundle, "diagnostics.bundle"),
        (ControlMethod::TopologyGet, "topology.get"),
        (ControlMethod::TrafficGet, "traffic.get"),
        (ControlMethod::MetricsGet, "metrics.get"),
    ];
    for (method, wire_name) in cases {
        let params = match method {
            ControlMethod::NodeSelectManual => {
                ControlParams::target("nh1s-0123456789abcdef".to_owned())
            }
            ControlMethod::NodeTest
            | ControlMethod::NodeExport
            | ControlMethod::ConnectionClose => ControlParams::target("stable-id".to_owned()),
            ControlMethod::NodeList | ControlMethod::ConnectionsGet => {
                ControlParams::list(Some("edge".to_owned()), Some(32))
            }
            ControlMethod::LogsGet => ControlParams::logs(Some(LogChannel::Core), Some(32)),
            _ => ControlParams::default(),
        };
        let request = WireFrame::Request(
            ControlRequest::new(request_id(), method)
                .with_params(params)
                .unwrap(),
        );
        let encoded = FrameCodec::encode(&request).unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&encoded[4..]).unwrap();
        assert_eq!(payload["method"], wire_name);
        assert_eq!(FrameCodec::decode(&encoded).unwrap(), request);
    }
}

#[test]
fn operational_targets_are_required_bounded_and_method_scoped() {
    for method in [
        ControlMethod::NodeTest,
        ControlMethod::NodeSelectManual,
        ControlMethod::NodeExport,
        ControlMethod::ConnectionClose,
    ] {
        assert_eq!(
            ControlRequest::new(request_id(), method)
                .with_params(ControlParams::default())
                .unwrap_err(),
            ProtocolError::InvalidEnvelope
        );
        assert_eq!(
            ControlRequest::new(request_id(), method)
                .with_params(ControlParams::target("x".repeat(129)))
                .unwrap_err(),
            ProtocolError::InvalidEnvelope
        );
    }
    assert_eq!(
        ControlRequest::new(request_id(), ControlMethod::StatusGet)
            .with_params(ControlParams::target("node".to_owned()))
            .unwrap_err(),
        ProtocolError::InvalidEnvelope
    );

    let request = ControlRequest::new(request_id(), ControlMethod::NodeTestAll)
        .with_params(ControlParams::default())
        .unwrap();
    assert_eq!(request.method(), ControlMethod::NodeTestAll);
}

#[test]
fn operational_list_filters_are_optional_bounded_and_method_scoped() {
    for method in [ControlMethod::NodeList, ControlMethod::ConnectionsGet] {
        ControlRequest::new(request_id(), method)
            .with_params(ControlParams::list(None, Some(1)))
            .unwrap();
        ControlRequest::new(request_id(), method)
            .with_params(ControlParams::list(Some("x".repeat(128)), Some(128)))
            .unwrap();
        assert_eq!(
            ControlRequest::new(request_id(), method)
                .with_params(ControlParams::list(Some("x".repeat(129)), None))
                .unwrap_err(),
            ProtocolError::InvalidEnvelope
        );
        assert_eq!(
            ControlRequest::new(request_id(), method)
                .with_params(ControlParams::list(None, Some(0)))
                .unwrap_err(),
            ProtocolError::InvalidEnvelope
        );
    }
    ControlRequest::new(request_id(), ControlMethod::LogsGet)
        .with_params(ControlParams::logs(Some(LogChannel::Service), Some(128)))
        .unwrap();
    assert!(
        ControlRequest::new(request_id(), ControlMethod::LogsGet)
            .with_params(ControlParams::logs(Some(LogChannel::Subscription), Some(0)))
            .is_err()
    );
    assert_eq!(
        ControlRequest::new(request_id(), ControlMethod::TopologyGet)
            .with_params(ControlParams::list(None, Some(10)))
            .unwrap_err(),
        ProtocolError::InvalidEnvelope
    );
    assert!(
        ControlRequest::new(request_id(), ControlMethod::LogsGet)
            .with_params(ControlParams::list(Some("events".into()), Some(10)))
            .is_err()
    );
    assert!(
        ControlRequest::new(request_id(), ControlMethod::StatusGet)
            .with_params(ControlParams::logs(Some(LogChannel::Core), None))
            .is_err()
    );
}

#[test]
fn close_all_and_log_clear_accept_only_empty_params() {
    for method in [ControlMethod::ConnectionsCloseAll, ControlMethod::LogsClear] {
        ControlRequest::new(request_id(), method)
            .with_params(ControlParams::default())
            .unwrap();
        assert!(
            ControlRequest::new(request_id(), method)
                .with_params(ControlParams::list(None, Some(1)))
                .is_err()
        );
    }
}

#[test]
fn operational_params_reject_unknown_fields() {
    let payload =
        br#"{"version":6,"request_id":"req-001","method":"node.list","params":{"offset":1}}"#;
    let mut framed = (payload.len() as u32).to_be_bytes().to_vec();
    framed.extend_from_slice(payload);
    assert_eq!(
        FrameCodec::decode(&framed),
        Err(ProtocolError::InvalidEnvelope)
    );
}

#[test]
fn bounded_wait_options_are_method_scoped_and_round_trip() {
    let request = ControlRequest::new(request_id(), ControlMethod::SubscriptionUpdate)
        .with_params(ControlParams::new(true, true))
        .unwrap();
    let encoded = FrameCodec::encode(&WireFrame::Request(request.clone())).unwrap();
    assert_eq!(
        FrameCodec::decode(&encoded).unwrap(),
        WireFrame::Request(request)
    );
    assert!(
        ControlRequest::new(request_id(), ControlMethod::StatusGet)
            .with_params(ControlParams::new(true, false))
            .is_err()
    );
}

#[test]
fn subscription_update_source_id_is_typed_and_method_scoped() {
    let source_id = "src_01010101010101010101010101010101".to_owned();
    let request = ControlRequest::new(request_id(), ControlMethod::SubscriptionUpdate)
        .with_params(ControlParams::subscription_update(
            true,
            false,
            Some(source_id.clone()),
        ))
        .unwrap();
    assert_eq!(request.params().source_id(), Some(source_id.as_str()));
    assert!(
        ControlRequest::new(request_id(), ControlMethod::StatusGet)
            .with_params(ControlParams::subscription_update(
                false,
                false,
                Some(source_id),
            ))
            .is_err()
    );
}

#[test]
fn manager_config_methods_require_bounded_cas_documents() {
    let document = json!({
        "schema_version": 1,
        "service": {"enabled": true},
        "subscriptions": {"sources": [{"name": "Primary", "url": ""}]}
    });
    for method in [ControlMethod::ConfigValidate, ControlMethod::ConfigApply] {
        let request = ControlRequest::new(request_id(), method)
            .with_params(ControlParams::config_document(
                "a".repeat(64),
                document.clone(),
            ))
            .unwrap();
        assert_eq!(
            FrameCodec::decode(&FrameCodec::encode(&WireFrame::Request(request.clone())).unwrap())
                .unwrap(),
            WireFrame::Request(request)
        );
    }
    assert!(
        ControlRequest::new(request_id(), ControlMethod::ConfigGet)
            .with_params(ControlParams::config_document(
                "a".repeat(64),
                document.clone()
            ))
            .is_err()
    );
    assert!(
        ControlRequest::new(request_id(), ControlMethod::ConfigApply)
            .with_params(ControlParams::config_document(
                "not-a-digest".into(),
                document
            ))
            .is_err()
    );
}

#[test]
fn source_selection_mutation_is_typed_and_round_trips() {
    let request = ControlRequest::new(request_id(), ControlMethod::SubscriptionSelect)
        .with_params(ControlParams::subscription_select(
            "a".repeat(64),
            "src_0123456789abcdef0123456789abcdef".into(),
        ))
        .unwrap();
    let frame = WireFrame::Request(request);
    assert_eq!(
        FrameCodec::decode(&FrameCodec::encode(&frame).unwrap()).unwrap(),
        frame
    );
    assert_eq!(
        ControlRequest::new(request_id(), ControlMethod::SubscriptionSelect)
            .with_params(ControlParams::subscription_select(
                "a".repeat(64),
                "source".into(),
            ))
            .unwrap_err(),
        ProtocolError::InvalidEnvelope
    );
}

#[test]
fn application_target_mutations_are_typed_bounded_and_round_trip() {
    let targets = vec![
        ApplicationTarget::Package {
            android_user_id: 0,
            package: "com.example.video".into(),
        },
        ApplicationTarget::Uid { uid: 10123 },
    ];
    let request = ControlRequest::new(request_id(), ControlMethod::ConfigMutate)
        .with_params(ControlParams::mutation(
            "a".repeat(64),
            ConfigMutation::ReplaceApplicationTargets {
                targets: targets.clone(),
            },
        ))
        .unwrap();
    let frame = WireFrame::Request(request);
    assert_eq!(
        FrameCodec::decode(&FrameCodec::encode(&frame).unwrap()).unwrap(),
        frame
    );

    for invalid in [
        ApplicationTarget::Uid { uid: 0 },
        ApplicationTarget::Package {
            android_user_id: 0,
            package: "bad\npackage".into(),
        },
        ApplicationTarget::Package {
            android_user_id: 21_475,
            package: "com.example.work".into(),
        },
    ] {
        assert_eq!(
            ControlRequest::new(request_id(), ControlMethod::ConfigMutate)
                .with_params(ControlParams::mutation(
                    "a".repeat(64),
                    ConfigMutation::AddApplicationTarget { target: invalid },
                ))
                .unwrap_err(),
            ProtocolError::InvalidEnvelope
        );
    }
}

#[test]
fn source_advanced_settings_are_bounded_typed_and_round_trip() {
    let settings = SubscriptionSourceSettings {
        request_profile: SubscriptionRequestProfile::Mihomo,
        format_hint: SubscriptionFormatHint::ClashYaml,
        mirrors: vec!["https://mirror.example/sub".into()],
        filter: SubscriptionSourceFilter {
            include_names: vec!["Premium".into()],
            exclude_names: vec!["Expired".into()],
            protocols: vec!["trojan".into(), "vless".into()],
        },
    };
    let request = ControlRequest::new(request_id(), ControlMethod::ConfigMutate)
        .with_params(ControlParams::mutation(
            "a".repeat(64),
            ConfigMutation::AddSource {
                name: "Primary".into(),
                url: "https://one.example/sub".into(),
                settings: Some(Box::new(settings.clone())),
            },
        ))
        .unwrap();
    let frame = WireFrame::Request(request);
    assert_eq!(
        FrameCodec::decode(&FrameCodec::encode(&frame).unwrap()).unwrap(),
        frame
    );

    let invalid = SubscriptionSourcePatch {
        mirrors: Some(vec!["https://mirror.example/sub".into(); 5]),
        ..SubscriptionSourcePatch::default()
    };
    assert!(
        ControlRequest::new(request_id(), ControlMethod::ConfigMutate)
            .with_params(ControlParams::mutation(
                "a".repeat(64),
                ConfigMutation::UpdateSource {
                    source_id: "src_0123456789abcdef0123456789abcdef".into(),
                    name: None,
                    url: None,
                    enabled: None,
                    settings: Some(Box::new(invalid)),
                },
            ))
            .is_err()
    );
}

#[test]
fn application_policy_mutation_is_atomic_and_rejects_invalid_mode_target_pairs() {
    let targets = vec![ApplicationTarget::Package {
        android_user_id: 0,
        package: "com.example.video".into(),
    }];
    let request = ControlRequest::new(request_id(), ControlMethod::ConfigMutate)
        .with_params(ControlParams::mutation(
            "a".repeat(64),
            ConfigMutation::SetApplicationPolicy {
                mode: ApplicationPolicyMode::Whitelist,
                targets: targets.clone(),
            },
        ))
        .unwrap();
    let frame = WireFrame::Request(request);
    assert_eq!(
        FrameCodec::decode(&FrameCodec::encode(&frame).unwrap()).unwrap(),
        frame
    );

    for mutation in [
        ConfigMutation::SetApplicationPolicy {
            mode: ApplicationPolicyMode::All,
            targets,
        },
        ConfigMutation::SetApplicationPolicy {
            mode: ApplicationPolicyMode::Blacklist,
            targets: Vec::new(),
        },
    ] {
        assert_eq!(
            ControlRequest::new(request_id(), ControlMethod::ConfigMutate)
                .with_params(ControlParams::mutation("a".repeat(64), mutation))
                .unwrap_err(),
            ProtocolError::InvalidEnvelope
        );
    }
}

#[test]
fn remove_node_mutation_accepts_only_stable_lowercase_fingerprints() {
    let valid = ConfigMutation::RemoveNode {
        node_id: "nh1s-0123456789abcdef".into(),
    };
    ControlRequest::new(request_id(), ControlMethod::ConfigMutate)
        .with_params(ControlParams::mutation("a".repeat(64), valid))
        .unwrap();
    for node_id in ["node", "nh1s-0123456789ABCDEf", "nh1s-0123"] {
        assert!(
            ControlRequest::new(request_id(), ControlMethod::ConfigMutate)
                .with_params(ControlParams::mutation(
                    "a".repeat(64),
                    ConfigMutation::RemoveNode {
                        node_id: node_id.into(),
                    },
                ))
                .is_err()
        );
    }
}

#[test]
fn node_override_methods_require_stable_identity_and_bounded_terminal_document() {
    let valid = ControlRequest::new(request_id(), ControlMethod::NodeOverrideApply).with_params(
        ControlParams::node_override(
            "nh1s-0123456789abcdef".into(),
            nethop_protocol::NodeOverrideDocument {
                display_name: "东京节点".into(),
                outbound: json!({
                    "type":"trojan",
                    "server":"edge.example.com",
                    "server_port":443,
                    "password":"secret"
                }),
            },
        ),
    );
    assert!(valid.is_ok());

    assert!(
        ControlRequest::new(request_id(), ControlMethod::NodeOverrideApply)
            .with_params(ControlParams::node_override(
                "unstable".into(),
                nethop_protocol::NodeOverrideDocument {
                    display_name: "node".into(),
                    outbound: json!({"type":"trojan"}),
                },
            ))
            .is_err()
    );
    assert!(
        ControlRequest::new(request_id(), ControlMethod::NodeOverrideApply)
            .with_params(ControlParams::target("nh1s-0123456789abcdef".into()))
            .is_err()
    );
}

#[test]
fn protocol_hello_requires_an_explicit_manager_version_range() {
    let request = ControlRequest::new(request_id(), ControlMethod::ProtocolHello)
        .with_params(ControlParams::hello("manager-alpha".into(), 1, 1))
        .unwrap();
    assert_eq!(
        FrameCodec::decode(&FrameCodec::encode(&WireFrame::Request(request.clone())).unwrap())
            .unwrap(),
        WireFrame::Request(request)
    );
    assert!(
        ControlRequest::new(request_id(), ControlMethod::StatusGet)
            .with_params(ControlParams::hello("manager-alpha".into(), 1, 1))
            .is_err()
    );
}
