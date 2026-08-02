use nethop_subscription::{
    CapabilityMatrix, DiagnosticCode, FormatHint, NodeSpec, ProxyProtocol, SemanticError, SourceId,
    SourceRef, TransportKind, node_spec_from_uri, parse_uri_line, validate_node_spec,
};

fn spec(protocol: &str) -> NodeSpec {
    let mut spec = NodeSpec::minimal(protocol, "example.com", 443);
    spec.display_name = Some(format!("{protocol}-node"));
    match protocol {
        "vless" | "vmess" | "tuic" => {
            spec.uuid = Some("550e8400-e29b-41d4-a716-446655440000".into());
            if protocol == "tuic" {
                spec.password = Some("secret".into());
            }
        }
        _ => spec.password = Some("secret".into()),
    }
    match protocol {
        "shadowsocks" => spec.method = Some("aes-128-gcm".into()),
        "trojan" | "anytls" | "vless" | "vmess" => spec.tls = true,
        "hysteria2" | "tuic" => {
            spec.tls = true;
            spec.udp = true;
            spec.transport = Some("quic".into());
        }
        _ => {}
    }
    spec
}

#[test]
fn seven_protocols_are_validated_only_through_the_capability_matrix() {
    let matrix = CapabilityMatrix::default();
    for protocol in [
        "vless",
        "vmess",
        "shadowsocks",
        "trojan",
        "hysteria2",
        "tuic",
        "anytls",
    ] {
        let outcome = validate_node_spec(spec(protocol), &matrix).unwrap();
        assert_eq!(outcome.node.protocol().as_str(), protocol);
    }
}

#[test]
fn endpoint_uuid_tls_and_transport_share_one_semantic_result() {
    let matrix = CapabilityMatrix::default();
    let mut invalid_endpoint = spec("vless");
    invalid_endpoint.port = 0;
    assert_eq!(
        validate_node_spec(invalid_endpoint, &matrix).unwrap_err(),
        SemanticError::InvalidEndpoint
    );

    let mut invalid_uuid = spec("vmess");
    invalid_uuid.uuid = Some("not-a-uuid".into());
    assert_eq!(
        validate_node_spec(invalid_uuid, &matrix).unwrap_err(),
        SemanticError::InvalidCredential
    );

    let mut invalid_tls = spec("trojan");
    invalid_tls.tls = false;
    assert_eq!(
        validate_node_spec(invalid_tls, &matrix).unwrap_err(),
        SemanticError::InvalidTlsCombination
    );

    let mut invalid_transport = spec("tuic");
    invalid_transport.transport = Some("tcp".into());
    assert_eq!(
        validate_node_spec(invalid_transport, &matrix).unwrap_err(),
        SemanticError::UnsupportedTransport
    );
}

#[test]
fn protocol_specific_credentials_and_unknown_critical_semantics_are_rejected() {
    let matrix = CapabilityMatrix::default();
    let mut bad_method = spec("shadowsocks");
    bad_method.method = Some("rc4-md5".into());
    assert_eq!(
        validate_node_spec(bad_method, &matrix).unwrap_err(),
        SemanticError::UnsupportedSemantics
    );

    let mut plugin = spec("shadowsocks");
    plugin.plugin = Some("obfs-local".into());
    assert_eq!(
        validate_node_spec(plugin, &matrix).unwrap_err(),
        SemanticError::UnsupportedSemantics
    );

    let mut unknown = spec("vless");
    unknown.unknown_critical_field = Some("xhttp".into());
    assert_eq!(
        validate_node_spec(unknown, &matrix).unwrap_err(),
        SemanticError::UnsupportedSemantics
    );
    assert_eq!(
        SemanticError::UnsupportedSemantics.code(),
        DiagnosticCode::UnsupportedSemantics
    );
}

#[test]
fn reality_is_vless_tls_only_and_insecure_tls_is_observable() {
    let matrix = CapabilityMatrix::default();
    let mut reality = spec("vless");
    reality.reality_public_key = Some("reality-key".into());
    reality.client_fingerprint = Some("chrome".into());
    let outcome = validate_node_spec(reality, &matrix).unwrap();
    assert_eq!(outcome.node.protocol(), ProxyProtocol::Vless);

    let mut invalid_reality = spec("trojan");
    invalid_reality.reality_public_key = Some("reality-key".into());
    assert_eq!(
        validate_node_spec(invalid_reality, &matrix).unwrap_err(),
        SemanticError::InvalidTlsCombination
    );

    let mut insecure = spec("trojan");
    insecure.insecure = true;
    let outcome = validate_node_spec(insecure, &matrix).unwrap();
    assert_eq!(outcome.warnings.len(), 1);
    assert_eq!(outcome.warnings[0].code, DiagnosticCode::InsecureTls);
}

#[test]
fn protocol_and_transport_matrix_is_deny_by_default() {
    let matrix = CapabilityMatrix::default();
    let mut websocket = spec("vless");
    websocket.transport = Some("ws".into());
    websocket.path = Some("/ws".into());
    assert!(validate_node_spec(websocket, &matrix).is_ok());
    assert_eq!(TransportKind::WebSocket, "ws".parse().unwrap());

    let unsupported = NodeSpec::minimal("wireguard", "example.com", 443);
    assert_eq!(
        validate_node_spec(unsupported, &matrix).unwrap_err(),
        SemanticError::UnsupportedProtocol
    );
    assert_eq!(ProxyProtocol::AnyTls.as_str(), "anytls");
}

#[test]
fn vless_vmess_hysteria2_and_tuic_specific_semantics_are_enforced() {
    let matrix = CapabilityMatrix::default();

    let mut flow = spec("vless");
    flow.flow = Some("unknown-flow".into());
    assert_eq!(
        validate_node_spec(flow, &matrix).unwrap_err(),
        SemanticError::UnsupportedSemantics
    );

    let mut security = spec("vmess");
    security.vmess_security = Some("legacy-unknown".into());
    assert_eq!(
        validate_node_spec(security, &matrix).unwrap_err(),
        SemanticError::UnsupportedSemantics
    );

    let mut obfs = spec("hysteria2");
    obfs.obfs = Some("unknown-obfs".into());
    assert_eq!(
        validate_node_spec(obfs, &matrix).unwrap_err(),
        SemanticError::UnsupportedSemantics
    );

    let mut congestion = spec("tuic");
    congestion.congestion_control = Some("unknown-congestion".into());
    assert_eq!(
        validate_node_spec(congestion, &matrix).unwrap_err(),
        SemanticError::UnsupportedSemantics
    );
}

#[test]
fn uri_candidates_enter_the_same_semantic_gate_as_other_adapters() {
    let matrix = CapabilityMatrix::default();
    let limits = nethop_subscription::ParserLimits::default();
    let uri = parse_uri_line(
        "vless://550e8400-e29b-41d4-a716-446655440000@example.com:443?security=tls#node",
        1,
        0,
        &limits,
    )
    .unwrap();
    let uri_node = validate_node_spec(node_spec_from_uri(&uri).unwrap(), &matrix)
        .unwrap()
        .node;
    let expected = validate_node_spec(spec("vless"), &matrix).unwrap().node;
    assert_eq!(uri_node.protocol(), expected.protocol());
    assert_eq!(uri_node.endpoint(), expected.endpoint());
    assert_eq!(uri_node.credentials(), expected.credentials());
}

#[test]
fn canonical_seeds_are_semantically_equivalent_across_format_boundaries() {
    let matrix = CapabilityMatrix::default();
    for protocol in [
        "vless",
        "vmess",
        "shadowsocks",
        "trojan",
        "hysteria2",
        "tuic",
        "anytls",
    ] {
        let mut outcomes = Vec::new();
        for format in [
            FormatHint::UriList,
            FormatHint::ClashYaml,
            FormatHint::SingboxJson,
        ] {
            let mut candidate = spec(protocol);
            candidate.source_ref = Some(SourceRef {
                source_id: SourceId::new("equivalence-source").unwrap(),
                item_index: 0,
                format,
                line: Some(1),
            });
            outcomes.push(validate_node_spec(candidate, &matrix).unwrap().node);
        }
        assert!(outcomes.windows(2).all(|pair| {
            pair[0].protocol() == pair[1].protocol()
                && pair[0].endpoint() == pair[1].endpoint()
                && pair[0].credentials() == pair[1].credentials()
        }));
    }
}
