#![cfg(feature = "subscription-update")]

use std::{fs, path::PathBuf};

use nethop_core::{
    CaptureMode, CapturePolicy, ClashApi, GenerationId, GenerationStore, ManagedOptions, TunStack,
};
use nethop_subscription::{
    CapabilityMatrix, FormatHint, ParserLimits, SourceId, SourceInput, convert_stable_sources,
};
use nethopd::{RunnerLimits, SingBoxCheckRunner, build_candidate};

#[test]
#[ignore = "requires an explicitly authorized local subscription body and platform sing-box binary"]
fn authorized_sfa_body_builds_and_checks_a_managed_generation() {
    let body_path = absolute_env_path("NETHOP_TEST_SFA_BODY_PATH");
    let sing_box = absolute_env_path("NETHOP_TEST_SING_BOX_BINARY");
    let body = fs::read(&body_path).expect("authorized subscription body must be readable");
    let conversion = convert_stable_sources(
        vec![SourceInput {
            source_id: SourceId::new("authorized-sfa-smoke").unwrap(),
            format_hint: FormatHint::SingboxJson,
            bytes: body,
        }],
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    );
    assert!(conversion.report.summary.source_success);
    assert!(conversion.report.summary.accepted > 0);
    assert_eq!(conversion.report.summary.rejected, 0);

    let capture = CapturePolicy::new(
        CaptureMode::Tproxy,
        true,
        Some(7893),
        Some(0x20_000),
        Vec::new(),
        vec![0],
    )
    .unwrap();
    let candidate = build_candidate(
        GenerationId::new(1).unwrap(),
        &conversion,
        capture,
        ClashApi::new("127.0.0.1:9090", "sfa-smoke-secret-32-bytes-long-00").unwrap(),
        TunStack::System,
        ManagedOptions::default(),
    )
    .unwrap();
    assert_eq!(candidate.config().node_count(), 18);

    let directory = tempfile::tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    let runner =
        SingBoxCheckRunner::new(sing_box, store.generations_root(), RunnerLimits::default())
            .unwrap();
    store
        .publish_with_path(&candidate, |path, _| runner.validate_for_publish(path, &[]))
        .unwrap();
    assert_eq!(
        store.current_generation().unwrap(),
        Some(candidate.generation())
    );
}

fn absolute_env_path(name: &str) -> PathBuf {
    let path = PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("missing {name}")));
    assert!(path.is_absolute(), "{name} must be an absolute path");
    path
}
