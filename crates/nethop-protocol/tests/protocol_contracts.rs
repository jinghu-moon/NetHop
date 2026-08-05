use std::io::Cursor;

use nethop_protocol::{
    ControlError, ControlMethod, ControlParams, ControlRequest, ControlResponse, ErrorCode,
    ErrorDomain, FrameCodec, MAX_FRAME_BYTES, PROTOCOL_VERSION, ProtocolError, RequestId,
    StreamFrame, StreamKind, WireFrame,
};
use serde_json::json;

fn request_id() -> RequestId {
    RequestId::new("req-001").unwrap()
}

#[test]
fn request_golden_uses_big_endian_length_and_v1_json() {
    let frame = WireFrame::Request(ControlRequest::new(request_id(), ControlMethod::StatusGet));
    let encoded = FrameCodec::encode(&frame).unwrap();
    let length = u32::from_be_bytes(encoded[..4].try_into().unwrap()) as usize;
    assert_eq!(length, encoded.len() - 4);
    assert_eq!(
        &encoded[4..],
        br#"{"version":1,"request_id":"req-001","method":"status.get","params":{}}"#
    );
    assert_eq!(FrameCodec::decode(&encoded).unwrap(), frame);
}

#[test]
fn request_rejects_unknown_fields_versions_and_unbounded_ids() {
    let unknown =
        br#"{"version":1,"request_id":"r","method":"status.get","params":{},"admin":true}"#;
    let mut framed = (unknown.len() as u32).to_be_bytes().to_vec();
    framed.extend_from_slice(unknown);
    assert_eq!(
        FrameCodec::decode(&framed),
        Err(ProtocolError::InvalidEnvelope)
    );

    let version = br#"{"version":2,"request_id":"r","method":"status.get","params":{}}"#;
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

    let invalid = br#"{"version":1,"request_id":"r","ok":true,"generation":1,"error":{"code":"NH-AUTH-DENIED","message":"denied"}}"#;
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
    assert_eq!(PROTOCOL_VERSION, 1);
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
fn subscription_update_is_a_bounded_v1_empty_params_command() {
    let frame = WireFrame::Request(ControlRequest::new(
        request_id(),
        ControlMethod::SubscriptionUpdate,
    ));
    let encoded = FrameCodec::encode(&frame).unwrap();
    assert_eq!(
        &encoded[4..],
        br#"{"version":1,"request_id":"req-001","method":"subscription.update","params":{}}"#
    );
    assert_eq!(FrameCodec::decode(&encoded).unwrap(), frame);
}

#[test]
fn config_reload_is_a_bounded_v1_empty_params_command() {
    let frame = WireFrame::Request(ControlRequest::new(
        request_id(),
        ControlMethod::ConfigReload,
    ));
    let encoded = FrameCodec::encode(&frame).unwrap();
    assert_eq!(
        &encoded[4..],
        br#"{"version":1,"request_id":"req-001","method":"config.reload","params":{}}"#
    );
    assert_eq!(FrameCodec::decode(&encoded).unwrap(), frame);
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
