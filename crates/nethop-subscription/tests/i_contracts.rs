#![cfg(all(
    feature = "format-uri",
    feature = "format-base64",
    feature = "format-clash-yaml",
    feature = "format-singbox-json"
))]

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use nethop_subscription::{
    AdapterNodeResult, AdapterOutput, CapabilityMatrix, DiagnosticCode, FormatHint, NodeDiagnostic,
    ParserLimits, Severity, SourceId, SourceInput, canonical_node_bytes, compose_outbounds_json,
    convert_stable_sources, decode_base64_and_detect, detect_bytes, parse_clash_yaml,
    parse_singbox_json, parse_uri_list, report_from_adapter,
};
use proptest::prelude::*;

fn limits() -> ParserLimits {
    ParserLimits::default()
}

fn source_id(value: &str) -> SourceId {
    SourceId::new(value).expect("test source id")
}

fn trojan_node(name: String, host: String, port: u16) -> nethop_subscription::ProxyNode {
    let mut spec = nethop_subscription::NodeSpec::minimal("trojan", host, port);
    spec.display_name = Some(name);
    spec.password = Some("property-secret".into());
    spec.tls = true;
    nethop_subscription::validate_node_spec(spec, &CapabilityMatrix::default())
        .expect("trojan property input is valid")
        .node
}

fn canary_free(value: &str, canary: &str) -> bool {
    !value.contains(canary)
}

fn completes_within<F>(timeout: Duration, operation: F) -> bool
where
    F: FnOnce() + Send + 'static,
{
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        operation();
        let _ = sender.send(());
    });
    receiver.recv_timeout(timeout).is_ok()
}

#[test]
fn timeout_harness_detects_slow_operation_and_body_limit_is_enforced() {
    assert!(completes_within(Duration::from_millis(100), || {}));
    assert!(!completes_within(Duration::from_millis(1), || {
        thread::sleep(Duration::from_millis(20));
    }));

    let oversized = vec![b'x'; ParserLimits::default().max_body_bytes() + 1];
    assert!(detect_bytes(&oversized, FormatHint::Auto, &limits()).is_err());
}

#[test]
fn secret_canary_scanner_rejects_injected_leak_sample() {
    let canary = "I_PHASE0_SECRET_CANARY";
    assert!(canary_free("redacted diagnostic", canary));
    assert!(!canary_free(&format!("leaked={canary}"), canary));

    let source = SourceInput {
        source_id: source_id("i-canary"),
        format_hint: FormatHint::UriList,
        bytes: format!("trojan://{canary}@example.com:443#safe").into_bytes(),
    };
    let conversion = convert_stable_sources(vec![source], &limits(), &CapabilityMatrix::default());
    let report = conversion.report.bounded_json(&limits());
    assert!(canary_free(&report, canary));
    assert!(canary_free(&format!("{:?}", conversion.report), canary));
    assert!(!compose_outbounds_json(&conversion.nodes).is_empty());
}

#[test]
fn short_fuzz_corpus_is_panic_free_across_stable_boundaries() {
    let mut corpus = vec![
        Vec::new(),
        b"not a subscription".to_vec(),
        vec![0xff; 64],
        b"trojan://secret@example.com:443".to_vec(),
        b"proxies: []".to_vec(),
        br#"[{"type":"trojan","server":"example.com","server_port":443,"password":"x"}]"#.to_vec(),
    ];
    for seed in 0..128u8 {
        corpus.push((0..=seed).map(|index| index.wrapping_mul(31)).collect());
    }

    for bytes in corpus {
        let limits = limits();
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                let _ = detect_bytes(&bytes, FormatHint::Auto, &limits);
                let _ = parse_uri_list(&bytes, None, &limits);
                let _ = decode_base64_and_detect(&bytes, &limits);
                let _ = parse_clash_yaml(&bytes, None, &limits, &CapabilityMatrix::default());
                let _ = parse_singbox_json(&bytes, None, &limits, &CapabilityMatrix::default());
            }))
            .is_ok()
        );
    }
}

proptest! {
    #[test]
    fn canonical_bytes_are_deterministic_for_arbitrary_valid_specs(
        name in "[a-zA-Z0-9_-]{1,32}",
        host in "[a-z]{1,16}\\.example",
        port in 1u16..65535u16,
    ) {
        let outcome = trojan_node(name, host, port);
        prop_assert_eq!(
            canonical_node_bytes(&outcome),
            canonical_node_bytes(&outcome),
        );
    }

    #[test]
    fn stable_conversion_output_is_independent_of_repeated_execution(
        host in "[a-z]{1,16}\\.example",
        port in 1u16..65535u16,
    ) {
        let source = SourceInput {
            source_id: source_id("property"),
            format_hint: FormatHint::UriList,
            bytes: format!("trojan://property-secret@{host}:{port}").into_bytes(),
        };
        let first = convert_stable_sources(
            vec![source.clone()],
            &limits(),
            &CapabilityMatrix::default(),
        );
        let second = convert_stable_sources(
            vec![source],
            &limits(),
            &CapabilityMatrix::default(),
        );
        prop_assert_eq!(first.outbounds_json, second.outbounds_json);
        prop_assert_eq!(first.report.summary, second.report.summary);
    }

    #[test]
    fn display_name_changes_do_not_affect_canonical_bytes_but_endpoint_changes_do(
        host in "[a-z]{1,16}\\.example",
        port in 1u16..65534u16,
        first_name in "[a-zA-Z0-9_-]{1,32}",
        second_name in "[a-zA-Z0-9_-]{1,32}",
    ) {
        let first = trojan_node(first_name, host.clone(), port);
        let second = trojan_node(second_name, host.clone(), port);
        let changed = trojan_node("changed".into(), host, port + 1);
        prop_assert_eq!(canonical_node_bytes(&first), canonical_node_bytes(&second));
        prop_assert_ne!(canonical_node_bytes(&first), canonical_node_bytes(&changed));
    }

    #[test]
    fn duplicate_uri_permutations_keep_the_same_composed_node_set(
        names in prop::collection::vec("[a-zA-Z0-9_-]{1,16}", 1..16),
    ) {
        let forward = names
            .iter()
            .map(|name| format!("trojan://property-secret@example.com:443#{name}"))
            .collect::<Vec<_>>()
            .join("\n");
        let reverse = names
            .iter()
            .rev()
            .map(|name| format!("trojan://property-secret@example.com:443#{name}"))
            .collect::<Vec<_>>()
            .join("\n");
        let forward = convert_stable_sources(
            vec![SourceInput {
                source_id: source_id("permutation"),
                format_hint: FormatHint::UriList,
                bytes: forward.into_bytes(),
            }],
            &limits(),
            &CapabilityMatrix::default(),
        );
        let reverse = convert_stable_sources(
            vec![SourceInput {
                source_id: source_id("permutation"),
                format_hint: FormatHint::UriList,
                bytes: reverse.into_bytes(),
            }],
            &limits(),
            &CapabilityMatrix::default(),
        );
        prop_assert_eq!(forward.outbounds_json, reverse.outbounds_json);
        prop_assert_eq!(forward.nodes.len(), 1);
        prop_assert_eq!(reverse.nodes.len(), 1);
    }

    #[test]
    fn report_diagnostic_cap_preserves_summary_and_code_count(
        count in 1usize..2000usize,
    ) {
        let diagnostic = NodeDiagnostic::new(DiagnosticCode::InvalidUri, Severity::Error);
        let output = AdapterOutput {
            nodes: (0..count)
                .map(|index| AdapterNodeResult::rejected(index as u32, diagnostic.clone()))
                .collect(),
            diagnostics: Vec::new(),
        };
        let report = report_from_adapter(
            FormatHint::UriList,
            &output,
            &[],
            0,
            &limits(),
        );
        prop_assert_eq!(report.summary.rejected, count);
        prop_assert_eq!(report.diagnostic_counts[&DiagnosticCode::InvalidUri], count);
        prop_assert!(report.diagnostics.len() <= limits().max_detailed_diagnostics());
        prop_assert_eq!(
            report.summary.truncated,
            count > limits().max_detailed_diagnostics(),
        );
    }
}
