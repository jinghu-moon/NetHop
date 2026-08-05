use nethop_protocol::{
    ControlError, ControlRequest, ControlResponse, ErrorCode, ErrorDomain, RequestId,
};
use nethopctl::{
    CliCommand, CliError, ControlTransport, build_request, execute, execute_invocation,
    parse_command, parse_invocation, render_response,
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
        parse_command(["config", "reload"]).unwrap(),
        CliCommand::ConfigReload
    );
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
fn wait_and_if_needed_are_accepted_only_for_bounded_mutations() {
    let reload = parse_invocation(["config", "reload", "--wait"]).unwrap();
    assert_eq!(reload.command(), CliCommand::ConfigReload);
    assert!(reload.wait());
    assert!(!reload.if_needed());

    let update = parse_invocation(["update", "--if-needed", "--wait"]).unwrap();
    assert_eq!(update.command(), CliCommand::Update);
    assert!(update.wait());
    assert!(update.if_needed());
    assert!(parse_invocation(["status", "--wait"]).is_err());
    assert!(parse_invocation(["start", "--if-needed"]).is_err());

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
