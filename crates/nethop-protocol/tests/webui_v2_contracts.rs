use nethop_protocol::{
    ControlMethod, ControlParams, ControlRequest, ControlResponse, EventKind, FrameCodec,
    MAX_WEBUI_ARRAY_ITEMS, MAX_WEBUI_DIAGNOSTIC_BYTES, MAX_WEBUI_STDERR_BYTES,
    MAX_WEBUI_STDOUT_BYTES, MAX_WEBUI_STRING_BYTES, PROTOCOL_VERSION, ProtocolError, RequestId,
    WebUiErrorKind, WebUiPayloadNamespace, WebUiPayloadOperation, WireFrame,
};
use serde_json::{Value, json};

fn request_id() -> RequestId {
    RequestId::new("webui-v5").unwrap()
}

#[test]
fn v3_freezes_traffic_payload_methods_and_stable_errors() {
    assert_eq!(PROTOCOL_VERSION, 6);
    assert_eq!(serde_json::to_value(EventKind::Traffic).unwrap(), "traffic");
    assert_eq!(MAX_WEBUI_STDOUT_BYTES, 1024 * 1024);
    assert_eq!(MAX_WEBUI_STDERR_BYTES, 64 * 1024);
    assert_eq!(MAX_WEBUI_DIAGNOSTIC_BYTES, 256 * 1024);
    assert_eq!(MAX_WEBUI_ARRAY_ITEMS, 10_000);
    assert_eq!(MAX_WEBUI_STRING_BYTES, 64 * 1024);
    assert_eq!(WebUiErrorKind::Incompatible.code(), "NH-CORE-INCOMPATIBLE");
    assert_eq!(WebUiErrorKind::Timeout.code(), "NH-CORE-TIMEOUT");
    assert_eq!(WebUiErrorKind::Conflict.code(), "NH-CONFIG-CONFLICT");
    assert_eq!(
        WebUiErrorKind::InvalidPayload.code(),
        "NH-CONFIG-INVALID-PAYLOAD"
    );
    assert_eq!(WebUiErrorKind::LimitExceeded.code(), "NH-CORE-LIMIT");
    assert_eq!(WebUiErrorKind::Unavailable.code(), "NH-CORE-UNAVAILABLE");
}

#[test]
fn payload_methods_have_typed_bounded_wire_shapes() {
    let handle = format!("p_{}", "a".repeat(32));
    let cases = [
        (
            ControlMethod::WebUiPayloadCreate,
            ControlParams::payload_create(WebUiPayloadNamespace::Config),
            "webui.payload.create",
        ),
        (
            ControlMethod::WebUiPayloadAppend,
            ControlParams::payload_append(
                WebUiPayloadNamespace::Subscription,
                handle.clone(),
                "e30=".into(),
            ),
            "webui.payload.append",
        ),
        (
            ControlMethod::WebUiPayloadCommit,
            ControlParams::payload_commit(
                WebUiPayloadNamespace::Backup,
                handle.clone(),
                WebUiPayloadOperation::BackupRestore,
            ),
            "webui.payload.commit",
        ),
        (
            ControlMethod::WebUiPayloadRemove,
            ControlParams::payload_remove(WebUiPayloadNamespace::Config, handle),
            "webui.payload.remove",
        ),
    ];
    for (method, params, wire_name) in cases {
        let request = ControlRequest::new(request_id(), method)
            .with_params(params)
            .unwrap();
        assert_eq!(serde_json::to_value(&request).unwrap()["method"], wire_name);
        let frame = WireFrame::Request(request);
        assert_eq!(
            FrameCodec::decode(&FrameCodec::encode(&frame).unwrap()).unwrap(),
            frame
        );
    }
}

#[test]
fn private_payload_config_mutate_is_a_stable_allowlisted_operation() {
    let params = ControlParams::payload_commit(
        WebUiPayloadNamespace::Config,
        "p_0123456789abcdef0123456789abcdef".into(),
        WebUiPayloadOperation::ConfigMutate,
    );
    let request = ControlRequest::new(
        RequestId::new("webui-config-mutate").unwrap(),
        ControlMethod::WebUiPayloadCommit,
    )
    .with_params(params)
    .unwrap();
    let wire = serde_json::to_value(request).unwrap();
    assert_eq!(wire["params"]["payload"]["operation"], "config_mutate");
}

#[test]
fn payload_commit_rejects_cross_namespace_operations() {
    let params = ControlParams::payload_commit(
        WebUiPayloadNamespace::Subscription,
        "p_0123456789abcdef0123456789abcdef".into(),
        WebUiPayloadOperation::ConfigMutate,
    );
    assert!(
        ControlRequest::new(request_id(), ControlMethod::WebUiPayloadCommit)
            .with_params(params)
            .is_err()
    );
}

#[test]
fn payload_wire_rejects_unknown_namespace_traversal_handle_and_oversized_chunk() {
    let invalid_values = [
        json!({"version":PROTOCOL_VERSION,"request_id":"webui-v4","method":"webui.payload.create","params":{"payload":{"namespace":"../config"}}}),
        json!({"version":PROTOCOL_VERSION,"request_id":"webui-v4","method":"webui.payload.remove","params":{"payload":{"namespace":"config","handle":"../outside"}}}),
        json!({"version":PROTOCOL_VERSION,"request_id":"webui-v4","method":"webui.payload.remove","params":{"payload":{"namespace":"config","handle":"p_ABCDEF0123456789abcdef0123456789"}}}),
        json!({"version":PROTOCOL_VERSION,"request_id":"webui-v4","method":"webui.payload.append","params":{"payload":{"namespace":"config","handle":format!("p_{}", "a".repeat(32)),"chunk":"A".repeat(16 * 1024 + 1)}}}),
    ];
    for value in invalid_values {
        let payload = serde_json::to_vec(&value).unwrap();
        let mut framed = (payload.len() as u32).to_be_bytes().to_vec();
        framed.extend_from_slice(&payload);
        assert_eq!(
            FrameCodec::decode(&framed),
            Err(ProtocolError::InvalidEnvelope)
        );
    }
}

#[test]
fn response_payload_rejects_oversized_strings_arrays_and_depth() {
    let oversized_string = ControlResponse::success(
        request_id(),
        None,
        json!({"value":"x".repeat(MAX_WEBUI_STRING_BYTES + 1)}),
    );
    assert_eq!(
        FrameCodec::encode(&WireFrame::Response(oversized_string)),
        Err(ProtocolError::InvalidEnvelope)
    );

    let oversized_array = ControlResponse::success(
        request_id(),
        None,
        Value::Array(vec![Value::Null; MAX_WEBUI_ARRAY_ITEMS + 1]),
    );
    assert_eq!(
        FrameCodec::encode(&WireFrame::Response(oversized_array)),
        Err(ProtocolError::InvalidEnvelope)
    );

    let mut deep = Value::Null;
    for _ in 0..34 {
        deep = json!({"next":deep});
    }
    let deep = ControlResponse::success(request_id(), None, deep);
    assert_eq!(
        FrameCodec::encode(&WireFrame::Response(deep)),
        Err(ProtocolError::InvalidEnvelope)
    );
}
