use nethop_protocol::{
    ControlError, ControlRequest, ControlResponse, ErrorCode, ErrorDomain, RequestId,
};
use nethopctl::{CliCommand, CliError, ControlTransport, execute, parse_command, render_response};
use serde_json::json;

#[derive(Debug)]
struct FakeTransport {
    response: Option<ControlResponse>,
    observed: Vec<ControlRequest>,
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
