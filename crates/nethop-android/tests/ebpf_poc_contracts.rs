use nethop_android::{EbpfPocDiagnostic, EbpfPocFacts, EbpfPocScope};

fn complete_facts() -> EbpfPocFacts {
    EbpfPocFacts::new(true, true, true, true, true)
}

#[test]
fn local_poc_requires_bpf_cgroup_v2_socket_attach_and_core_support() {
    assert_eq!(
        complete_facts().evaluate(EbpfPocScope::Local),
        EbpfPocDiagnostic::Eligible
    );
    for (facts, expected) in [
        (
            EbpfPocFacts::new(false, true, true, true, true),
            EbpfPocDiagnostic::BpfUnavailable,
        ),
        (
            EbpfPocFacts::new(true, false, true, true, true),
            EbpfPocDiagnostic::CgroupV2Unavailable,
        ),
        (
            EbpfPocFacts::new(true, true, false, true, true),
            EbpfPocDiagnostic::CgroupSocketAttachUnavailable,
        ),
        (
            EbpfPocFacts::new(true, true, true, true, false),
            EbpfPocDiagnostic::CoreUnsupported,
        ),
    ] {
        assert_eq!(facts.evaluate(EbpfPocScope::Local), expected);
    }
}

#[test]
fn shared_network_poc_additionally_requires_tc_attach() {
    let facts = EbpfPocFacts::new(true, true, true, false, true);
    assert_eq!(
        facts.evaluate(EbpfPocScope::SharedNetwork),
        EbpfPocDiagnostic::TcAttachUnavailable
    );
    assert_eq!(
        facts.evaluate(EbpfPocScope::Local),
        EbpfPocDiagnostic::Eligible
    );
}

#[test]
fn diagnostics_are_stable_and_do_not_claim_activation() {
    let cases = [
        (EbpfPocDiagnostic::Eligible, "ebpf_poc_eligible"),
        (EbpfPocDiagnostic::BpfUnavailable, "ebpf_bpf_unavailable"),
        (
            EbpfPocDiagnostic::CgroupV2Unavailable,
            "ebpf_cgroup_v2_unavailable",
        ),
        (
            EbpfPocDiagnostic::CgroupSocketAttachUnavailable,
            "ebpf_cgroup_socket_attach_unavailable",
        ),
        (
            EbpfPocDiagnostic::TcAttachUnavailable,
            "ebpf_tc_attach_unavailable",
        ),
        (EbpfPocDiagnostic::CoreUnsupported, "ebpf_core_unsupported"),
    ];
    for (diagnostic, code) in cases {
        assert_eq!(diagnostic.code(), code);
        assert!(!diagnostic.code().contains("active"));
        assert!(!diagnostic.code().contains("ready"));
    }
}
