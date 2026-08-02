#![cfg(feature = "format-surfboard")]

use nethop_subscription::{
    CapabilityMatrix, CapabilityQuery, Credentials, DiagnosticCode, FormatHint, ParserLimits,
    ProxyProtocol, SourceId, SourceInput, TransportKind, convert_stable_sources,
    parse_surfboard_ini,
};
use proptest::prelude::*;
use sha2::{Digest, Sha256};

#[test]
fn surfboard_fixture_manifest_is_bound_to_redacted_bytes() {
    let bytes = include_bytes!("fixtures/surfboard/basic.conf");
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/surfboard/manifest.json")).unwrap();
    assert_eq!(manifest["format"], "surfboard_ini");
    assert_eq!(manifest["bytes"].as_u64(), Some(bytes.len() as u64));
    let digest = Sha256::digest(bytes);
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(manifest["sha256"], hex);
}

#[test]
fn surfboard_fixture_is_nodes_only_and_partially_successful() {
    let bytes = include_bytes!("fixtures/surfboard/basic.conf");
    let source = SourceId::new("surfboard-fixture").unwrap();
    let output = parse_surfboard_ini(
        bytes,
        Some(&source),
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    )
    .unwrap();

    assert_eq!(output.accepted_count(), 3);
    assert_eq!(output.rejected_count(), 2);
    assert!(
        output
            .diagnostics
            .iter()
            .any(|item| item.code == DiagnosticCode::NonNodeSectionIgnored)
    );
    assert!(output.nodes.iter().any(|item| {
        item.diagnostic
            .as_ref()
            .is_some_and(|d| d.code == DiagnosticCode::UnsupportedProtocol)
    }));
    assert!(output.nodes.iter().any(|item| {
        item.diagnostic
            .as_ref()
            .is_some_and(|d| d.code == DiagnosticCode::UnsupportedSemantics)
    }));
}

#[test]
fn surfboard_tokenizer_preserves_quoted_commas_and_rejects_unbounded_lines() {
    let input = b"[Proxy]\nquoted = trojan, example.com, 443, password=\"a,b\", tls=true\n";
    let output = parse_surfboard_ini(
        input,
        None,
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    )
    .unwrap();
    assert_eq!(
        output.accepted_count(),
        1,
        "diagnostics={:?}",
        output
            .nodes
            .iter()
            .filter_map(|item| item.diagnostic.as_ref().map(|item| &item.code))
            .collect::<Vec<_>>()
    );

    let mut oversized = b"[Proxy]\nnode = trojan, example.com, 443, password=".to_vec();
    oversized.extend(std::iter::repeat_n(
        b'x',
        ParserLimits::default().max_line_bytes(),
    ));
    oversized.extend_from_slice(b"\n");
    let error = parse_surfboard_ini(
        &oversized,
        None,
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    )
    .unwrap_err();
    assert_eq!(error.code, DiagnosticCode::InvalidIni);
}

#[test]
fn surfboard_limits_nodes_and_classifies_unknown_fields() {
    let limits = ParserLimits::new(1024, 2, 512, 8, 512).unwrap();
    let three_nodes = b"[Proxy]\na = trojan, a.example, 443, a\nb = trojan, b.example, 443, b\nc = trojan, c.example, 443, c\n";
    let error =
        parse_surfboard_ini(three_nodes, None, &limits, &CapabilityMatrix::default()).unwrap_err();
    assert_eq!(error.code, DiagnosticCode::NodeLimitExceeded);

    let harmless = b"[Proxy]\na = trojan, a.example, 443, a, tls=true, icon=ignored\n";
    let output = parse_surfboard_ini(
        harmless,
        None,
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    )
    .unwrap();
    assert_eq!(output.accepted_count(), 1);
    assert!(
        output.nodes[0]
            .warnings
            .iter()
            .any(|item| item.code == DiagnosticCode::UnknownField)
    );
}

#[test]
fn surfboard_shadowsocks_obfs_maps_to_audited_sing_box_plugin() {
    let input = b"[Proxy]\na = ss, a.example, 443, encrypt-method=aes-128-gcm, password=fixture, obfs=tls, obfs-host=cdn.example\n";
    let matrix = CapabilityMatrix::default();
    assert!(matrix.supports(&CapabilityQuery {
        protocol: ProxyProtocol::Shadowsocks,
        transport: TransportKind::Tcp,
        tls: false,
        reality: false,
        utls: false,
        udp: false,
        flow: None,
        plugin: Some("obfs-local".into()),
    }));
    let output = parse_surfboard_ini(input, None, &ParserLimits::default(), &matrix).unwrap();
    assert_eq!(
        output.accepted_count(),
        1,
        "diagnostics={:?}",
        output
            .nodes
            .iter()
            .filter_map(|item| item.diagnostic.as_ref().map(|item| &item.code))
            .collect::<Vec<_>>()
    );
    let node = output.nodes[0].node.as_ref().unwrap();
    let Credentials::Shadowsocks { plugin, .. } = node.credentials() else {
        panic!("expected Shadowsocks credentials");
    };
    let plugin = plugin.as_ref().expect("obfs must create plugin");
    assert_eq!(plugin.name.as_str(), "obfs-local");
    assert_eq!(plugin.options["obfs"].as_str(), "tls");
    assert_eq!(plugin.options["obfs-host"].as_str(), "cdn.example");

    let conversion = convert_stable_sources(
        vec![SourceInput {
            source_id: SourceId::new("surfboard-obfs").unwrap(),
            format_hint: FormatHint::SurfboardIni,
            bytes: input.to_vec(),
        }],
        &ParserLimits::default(),
        &matrix,
    );
    let outbounds: serde_json::Value = serde_json::from_str(&conversion.outbounds_json).unwrap();
    assert_eq!(outbounds[0]["plugin"], "obfs-local");
    assert_eq!(
        outbounds[0]["plugin_opts"],
        "obfs=tls;obfs-host=cdn.example"
    );
}

#[test]
fn surfboard_malformed_seed_corpus_is_panic_free_and_rejected() {
    let seeds: [&[u8]; 5] = [
        b"[Proxy\nnode = trojan, example.com, 443, password\n",
        b"[Proxy]\nnode = trojan, example.com, 443, \"unterminated\n",
        b"[Proxy]\nnode = trojan, example.com, 443, \"dangling\\\n",
        b"[Proxy]]\nnode = trojan, example.com, 443, password\n",
        b"[]\nnode = trojan, example.com, 443, password\n",
    ];
    for seed in seeds {
        let outcome = std::panic::catch_unwind(|| {
            parse_surfboard_ini(
                seed,
                None,
                &ParserLimits::default(),
                &CapabilityMatrix::default(),
            )
        });
        let parsed = outcome.expect("malformed Surfboard seed must not panic");
        assert!(
            parsed.is_err() || parsed.is_ok_and(|output| output.accepted_count() == 0),
            "malformed Surfboard seed must not produce an active node"
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn surfboard_bounded_ascii_input_never_panics(bytes in prop::collection::vec(0_u8..=127, 0..2048)) {
        let _ = parse_surfboard_ini(
            &bytes,
            None,
            &ParserLimits::default(),
            &CapabilityMatrix::default(),
        );
    }
}
