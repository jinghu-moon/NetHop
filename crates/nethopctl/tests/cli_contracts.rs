use nethop_protocol::{
    ControlError, ControlRequest, ControlResponse, ErrorCode, ErrorDomain, RequestId,
};
use nethopctl::{
    CliCommand, CliError, ControlTransport, build_request, execute, execute_invocation,
    parse_command, parse_invocation, render_response, render_status_human,
};
use serde_json::json;

#[derive(Debug)]
struct FakeTransport {
    response: Option<ControlResponse>,
    observed: Vec<ControlRequest>,
}

#[test]
fn manager_commands_build_typed_bounded_requests() {
    let digest = "a".repeat(64);
    let validate = parse_invocation([
        "config",
        "validate",
        "--expected-digest",
        digest.as_str(),
        "--json",
    ])
    .unwrap();
    let request = build_request(
        &validate,
        RequestId::new("manager-validate").unwrap(),
        Some(json!({"schema_version":1})),
    )
    .unwrap();
    assert_eq!(
        request.method(),
        nethop_protocol::ControlMethod::ConfigValidate
    );
    assert_eq!(
        request.params().expected_config_digest(),
        Some(digest.as_str())
    );
    assert_eq!(request.params().document().unwrap()["schema_version"], 1);

    let mutation =
        parse_invocation(["config", "mutate", "--expected-digest", digest.as_str()]).unwrap();
    let request = build_request(
        &mutation,
        RequestId::new("manager-mutate").unwrap(),
        Some(json!({"type":"add_source","name":"Backup","url":"https://example.com/sub"})),
    )
    .unwrap();
    assert!(matches!(
        request.params().mutation_value(),
        Some(nethop_protocol::ConfigMutation::AddSource { .. })
    ));

    let hello = parse_invocation([
        "hello",
        "--manager-version",
        "0.1.0",
        "--protocol-min",
        "1",
        "--protocol-max",
        "1",
    ])
    .unwrap();
    let request = build_request(&hello, RequestId::new("manager-hello").unwrap(), None).unwrap();
    assert_eq!(request.params().manager_protocol_range(), Some((1, 1)));

    let events = parse_invocation(["events", "--kinds", "config,generation", "--jsonl"]).unwrap();
    let request = build_request(&events, RequestId::new("manager-events").unwrap(), None).unwrap();
    assert_eq!(request.params().event_kinds().unwrap().len(), 2);

    let preview = parse_invocation([
        "subscription",
        "import",
        "preview",
        "--text",
        "--format",
        "auto",
        "--expected-digest",
        digest.as_str(),
    ])
    .unwrap();
    let request = build_request(
        &preview,
        RequestId::new("import-preview").unwrap(),
        Some(json!({"content":"ss://example","format_hint":"auto"})),
    )
    .unwrap();
    assert_eq!(
        request.method(),
        nethop_protocol::ControlMethod::SubscriptionImportPreview
    );

    let apply = parse_invocation([
        "subscription",
        "import",
        "apply",
        "--text",
        "--expected-digest",
        digest.as_str(),
        "--candidate-digest",
        digest.as_str(),
    ])
    .unwrap();
    let request = build_request(
        &apply,
        RequestId::new("import-apply").unwrap(),
        Some(json!({"content":"ss://example"})),
    )
    .unwrap();
    assert_eq!(request.params().candidate_digest(), Some(digest.as_str()));
}

#[test]
fn backup_commands_use_file_input_and_preserve_cas_restore() {
    let digest = "b".repeat(64);
    let export = parse_invocation(["backup", "export", "--file", "backup.json"]).unwrap();
    assert_eq!(export.command(), CliCommand::BackupExport);
    assert_eq!(export.input_file(), Some("backup.json"));
    assert_eq!(
        build_request(&export, RequestId::new("backup-export").unwrap(), None)
            .unwrap()
            .method(),
        nethop_protocol::ControlMethod::ConfigExport
    );

    let restore = parse_invocation([
        "backup",
        "restore",
        "--file",
        "backup.json",
        "--expected-digest",
        digest.as_str(),
    ])
    .unwrap();
    let document = json!({"schema_version": 1, "service": {"enabled": false}});
    let request = build_request(
        &restore,
        RequestId::new("backup-restore").unwrap(),
        Some(document.clone()),
    )
    .unwrap();
    assert_eq!(
        request.method(),
        nethop_protocol::ControlMethod::ConfigApply
    );
    assert_eq!(
        request.params().expected_config_digest(),
        Some(digest.as_str())
    );
    assert_eq!(request.params().document(), Some(&document));

    assert!(parse_invocation(["backup", "export"]).is_err());
    assert!(parse_invocation(["backup", "export", "--text"]).is_err());
    assert!(parse_invocation(["backup", "restore", "--file", "backup.json"]).is_err());
}

#[test]
fn core_version_check_maps_to_the_read_only_typed_method() {
    let invocation = parse_invocation(["core", "version-check"]).unwrap();
    assert_eq!(invocation.command(), CliCommand::CoreVersionCheck);
    assert_eq!(
        build_request(
            &invocation,
            RequestId::new("core-version-check").unwrap(),
            None,
        )
        .unwrap()
        .method(),
        nethop_protocol::ControlMethod::CoreVersionCheck
    );
    assert!(parse_invocation(["core", "version-check", "--wait"]).is_err());
}

impl ControlTransport for FakeTransport {
    fn exchange(&mut self, request: &ControlRequest) -> Result<ControlResponse, CliError> {
        self.observed.push(request.clone());
        self.response.take().ok_or(CliError::ConnectionFailed)
    }
}

#[test]
fn command_parser_is_exact_and_maps_only_minimal_methods() {
    assert_eq!(parse_command(["status"]).unwrap(), CliCommand::Status);
    assert_eq!(parse_command(["start"]).unwrap(), CliCommand::Start);
    assert_eq!(parse_command(["stop"]).unwrap(), CliCommand::Stop);
    assert_eq!(parse_command(["probe"]).unwrap(), CliCommand::Probe);
    assert_eq!(parse_command(["update"]).unwrap(), CliCommand::Update);
    assert_eq!(
        parse_command(["ruleset", "status"]).unwrap(),
        CliCommand::RuleSetStatus
    );
    assert_eq!(
        parse_command(["ruleset", "update"]).unwrap(),
        CliCommand::RuleSetUpdate
    );
    assert_eq!(
        parse_command(["config", "reload"]).unwrap(),
        CliCommand::ConfigReload
    );
    assert_eq!(
        parse_command(["node", "list"]).unwrap(),
        CliCommand::NodeList
    );
    assert_eq!(
        parse_command(["node", "test"]).unwrap(),
        CliCommand::NodeTest
    );
    assert_eq!(
        parse_command(["node", "select"]).unwrap(),
        CliCommand::NodeSelect
    );
    assert_eq!(
        parse_command(["connections"]).unwrap(),
        CliCommand::ConnectionsGet
    );
    assert_eq!(
        parse_command(["connection", "close"]).unwrap(),
        CliCommand::ConnectionClose
    );
    assert_eq!(
        parse_command(["connections", "close-all"]).unwrap(),
        CliCommand::ConnectionsCloseAll
    );
    assert_eq!(parse_command(["logs", "get"]).unwrap(), CliCommand::LogsGet);
    assert_eq!(
        parse_command(["logs", "tail"]).unwrap(),
        CliCommand::LogsTail
    );
    assert_eq!(
        parse_command(["logs", "clear"]).unwrap(),
        CliCommand::LogsClear
    );
    assert_eq!(
        parse_command(["subscription", "add"]).unwrap(),
        CliCommand::SubscriptionAdd
    );
    assert_eq!(
        parse_command(["application", "add-package"]).unwrap(),
        CliCommand::ApplicationAddPackage
    );
    assert_eq!(
        parse_command(["network", "set"]).unwrap(),
        CliCommand::NetworkSet
    );
    assert_eq!(
        parse_command(["diagnose"]).unwrap(),
        CliCommand::DiagnosticsBundle
    );
    assert_eq!(
        parse_command(["topology"]).unwrap(),
        CliCommand::TopologyGet
    );
    assert_eq!(parse_command(["traffic"]).unwrap(), CliCommand::TrafficGet);
    assert_eq!(
        parse_command(["status", "extra"]).unwrap_err(),
        CliError::Usage
    );
    assert_eq!(
        parse_command(std::iter::empty::<&str>()).unwrap_err(),
        CliError::Usage
    );
}

#[test]
fn operational_commands_build_only_bounded_typed_params() {
    let digest = "a".repeat(64);
    let list = parse_invocation(["node", "list", "edge", "--limit", "16"]).unwrap();
    let request = build_request(&list, RequestId::new("node-list").unwrap(), None).unwrap();
    assert_eq!(request.method(), nethop_protocol::ControlMethod::NodeList);
    assert_eq!(request.params().query_value(), Some("edge"));
    assert_eq!(request.params().limit(), Some(16));

    let connections = parse_invocation(["connections", "--limit", "8"]).unwrap();
    let request =
        build_request(&connections, RequestId::new("connections").unwrap(), None).unwrap();
    assert_eq!(request.params().query_value(), None);
    assert_eq!(request.params().limit(), Some(8));

    for (arguments, method, target) in [
        (
            vec!["node", "test", "node-a"],
            nethop_protocol::ControlMethod::NodeTest,
            "node-a",
        ),
        (
            vec!["node", "select", "node-b"],
            nethop_protocol::ControlMethod::NodeSelect,
            "node-b",
        ),
        (
            vec!["connection", "close", "connection-id"],
            nethop_protocol::ControlMethod::ConnectionClose,
            "connection-id",
        ),
        (
            vec!["node", "export", "nh1s-0123456789abcdef"],
            nethop_protocol::ControlMethod::NodeExport,
            "nh1s-0123456789abcdef",
        ),
    ] {
        let invocation = parse_invocation(arguments).unwrap();
        let request = build_request(&invocation, RequestId::new("target").unwrap(), None).unwrap();
        assert_eq!(request.method(), method);
        assert_eq!(request.params().target_value(), Some(target));
    }

    assert!(parse_invocation(["node", "test"]).is_err());
    assert!(parse_invocation(["node", "list", "a", "b"]).is_err());
    assert!(parse_invocation(["connections", "--limit", "0"]).is_err());
    assert!(parse_invocation(["diagnose", "extra"]).is_err());

    let close_all = parse_invocation(["connections", "close-all"]).unwrap();
    assert_eq!(
        build_request(&close_all, RequestId::new("close-all").unwrap(), None)
            .unwrap()
            .method(),
        nethop_protocol::ControlMethod::ConnectionsCloseAll
    );
    let logs = parse_invocation(["logs", "get", "--limit", "12"]).unwrap();
    let request = build_request(&logs, RequestId::new("logs").unwrap(), None).unwrap();
    assert_eq!(request.method(), nethop_protocol::ControlMethod::LogsGet);
    assert_eq!(request.params().limit(), Some(12));
    let tail = parse_invocation(["logs", "tail", "--kinds", "runtime,network"]).unwrap();
    assert_eq!(
        build_request(&tail, RequestId::new("tail").unwrap(), None)
            .unwrap()
            .method(),
        nethop_protocol::ControlMethod::EventsSubscribe
    );
    assert!(parse_invocation(["logs", "clear", "--limit", "1"]).is_err());

    let remove = parse_invocation([
        "node",
        "remove",
        "nh1s-0123456789abcdef",
        "--expected-digest",
        digest.as_str(),
    ])
    .unwrap();
    let request = build_request(&remove, RequestId::new("node-remove").unwrap(), None).unwrap();
    assert!(matches!(
        request.params().mutation_value(),
        Some(nethop_protocol::ConfigMutation::RemoveNode { node_id })
            if node_id == "nh1s-0123456789abcdef"
    ));

    let add = parse_invocation([
        "subscription",
        "add",
        "Primary",
        "https://example.com/sub",
        "--expected-digest",
        digest.as_str(),
    ])
    .unwrap();
    let request = build_request(&add, RequestId::new("sub-add").unwrap(), None).unwrap();
    assert!(matches!(
        request.params().mutation_value(),
        Some(nethop_protocol::ConfigMutation::AddSource { name, url })
            if name == "Primary" && url == "https://example.com/sub"
    ));

    let network = parse_invocation([
        "network",
        "set",
        "logging.level",
        "debug",
        "--expected-digest",
        digest.as_str(),
    ])
    .unwrap();
    let request = build_request(&network, RequestId::new("network-set").unwrap(), None).unwrap();
    assert!(matches!(
        request.params().mutation_value(),
        Some(nethop_protocol::ConfigMutation::SetScalarField { field_id, value })
            if field_id == "logging.level" && value == &json!("debug")
    ));
}

#[test]
fn wait_and_if_needed_are_accepted_only_for_bounded_mutations() {
    let reload = parse_invocation(["config", "reload", "--wait"]).unwrap();
    assert_eq!(reload.command(), CliCommand::ConfigReload);
    assert!(reload.wait());
    assert!(!reload.if_needed());

    let update = parse_invocation(["update", "--if-needed", "--wait"]).unwrap();
    assert_eq!(update.command(), CliCommand::Update);
    assert!(update.wait());
    assert!(update.if_needed());
    let rule_set_update = parse_invocation(["ruleset", "update", "--wait"]).unwrap();
    assert_eq!(rule_set_update.command(), CliCommand::RuleSetUpdate);
    assert!(rule_set_update.wait());
    assert!(parse_invocation(["ruleset", "status", "--wait"]).is_err());
    let source_id = "src_01010101010101010101010101010101";
    let source_update = parse_invocation(["update", "--source", source_id, "--wait"]).unwrap();
    assert_eq!(source_update.source_id(), Some(source_id));
    assert_eq!(
        build_request(
            &source_update,
            RequestId::new("source-update").unwrap(),
            None,
        )
        .unwrap()
        .params()
        .source_id(),
        Some(source_id)
    );
    assert!(parse_invocation(["update", "--source", "Primary"]).is_err());
    assert!(parse_invocation(["status", "--wait"]).is_err());
    assert!(parse_invocation(["start", "--if-needed"]).is_err());
    let human_status = parse_invocation(["status", "--human"]).unwrap();
    assert!(human_status.human());
    assert!(parse_invocation(["update", "--human"]).is_err());
    assert!(parse_invocation(["status", "--human", "--json"]).is_err());

    let request_id = RequestId::new("wait-options").unwrap();
    let mut transport = FakeTransport {
        response: Some(ControlResponse::success(
            request_id.clone(),
            None,
            json!({}),
        )),
        observed: Vec::new(),
    };
    execute_invocation(&mut transport, update, request_id).unwrap();
    assert!(transport.observed[0].params().wait());
    assert!(transport.observed[0].params().if_needed());
}

#[test]
fn human_status_is_bounded_and_surfaces_a_core_update_without_raw_json() {
    let response = ControlResponse::success(
        RequestId::new("human-status").unwrap(),
        Some(7),
        json!({
            "state": "running_tproxy",
            "generation": 7,
            "last_update": "succeeded",
            "core_update": {
                "current": "1.13.15",
                "latest": "1.13.16",
                "availability": "available"
            },
            "dns_split": {"mode":"strict","dns_split":"degraded_private_dns"}
        }),
    );

    assert_eq!(
        render_status_human(&response).unwrap(),
        "NetHop status\nState: running_tproxy\nGeneration: 7\nSubscription: succeeded\nDNS split: degraded (strict Private DNS); disable Private DNS for split DNS\nCore: 1.13.15\nCore update: 1.13.16 available; update the NetHop module"
    );

    let injected = ControlResponse::success(
        RequestId::new("human-status-invalid").unwrap(),
        None,
        json!({
            "state": "running_tproxy\nsecret",
            "generation": null,
            "last_update": "never",
            "core_update": {"state":"never_checked","current":"1.13.15"}
        }),
    );
    assert_eq!(
        render_status_human(&injected).unwrap_err(),
        CliError::InvalidResponse
    );
}

#[test]
fn client_sends_one_typed_request_and_preserves_daemon_response() {
    let request_id = RequestId::new("ctl-test").unwrap();
    let response =
        ControlResponse::success(request_id.clone(), Some(9), json!({"state":"running"}));
    let mut transport = FakeTransport {
        response: Some(response.clone()),
        observed: Vec::new(),
    };
    let actual = execute(&mut transport, CliCommand::Status, request_id).unwrap();
    assert_eq!(actual, response);
    assert_eq!(transport.observed.len(), 1);
    assert_eq!(transport.observed[0].method(), CliCommand::Status.method());
    assert_eq!(
        render_response(&actual).unwrap(),
        r#"{"version":1,"request_id":"ctl-test","ok":true,"generation":9,"result":{"state":"running"}}"#
    );
}

#[test]
fn client_rejects_mismatched_response_id_and_keeps_daemon_failures_structured() {
    let mismatched = ControlResponse::success(RequestId::new("other").unwrap(), None, json!({}));
    let mut transport = FakeTransport {
        response: Some(mismatched),
        observed: Vec::new(),
    };
    assert_eq!(
        execute(
            &mut transport,
            CliCommand::Probe,
            RequestId::new("expected").unwrap(),
        )
        .unwrap_err(),
        CliError::ResponseRequestMismatch
    );

    let request_id = RequestId::new("failed").unwrap();
    let failure = ControlResponse::failure(
        request_id.clone(),
        Some(3),
        ControlError::new(
            ErrorCode::new(ErrorDomain::Capability, "UNAVAILABLE").unwrap(),
            "capability unavailable",
        )
        .unwrap(),
    );
    let mut transport = FakeTransport {
        response: Some(failure),
        observed: Vec::new(),
    };
    let response = execute(&mut transport, CliCommand::Probe, request_id).unwrap();
    assert!(!response.ok());
    assert_eq!(
        response.error().unwrap().code().as_str(),
        "NH-CAP-UNAVAILABLE"
    );
}
