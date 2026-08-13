use std::{fs, path::PathBuf};

#[test]
fn evidence_feature_is_opt_in_and_release_runner_is_registered() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest = fs::read_to_string(root.join("Cargo.toml")).unwrap();

    assert!(manifest.contains("benchmark-evidence = []"));
    assert!(manifest.contains("name = \"node_benchmark_evidence\""));
    assert!(manifest.contains("required-features = [\"benchmark-evidence\"]"));
    assert!(!manifest.contains("default = [\"subscription-update\", \"benchmark-evidence\"]"));
}

#[test]
fn host_gate_requires_raw_samples_limits_and_secret_scan() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let gate =
        fs::read_to_string(root.join("scripts/node-benchmark-host-release-gate.ps1")).unwrap();

    for contract in [
        "bootstrap_raw_micros",
        "wall_ms.p95",
        "peak_tasks",
        "peak_sockets",
        "residual_tasks",
        "residual_sockets",
        "peak_heap_delta_bytes",
        "node_benchmark_postprocess_evidence",
        "elapsed_ms.p95",
        "Bearer|terminal-|subscription|token=|https://",
    ] {
        assert!(gate.contains(contract), "missing gate contract: {contract}");
    }
}

#[test]
fn size_gate_fails_closed_when_whole_zip_inputs_differ() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let gate = fs::read_to_string(root.join("scripts/node-benchmark-size-evidence.ps1")).unwrap();

    assert!(gate.contains("$zipComparable = $coreSame -and $webrootSame"));
    assert!(gate.contains("passed = if ($zipComparable)"));
    assert!(gate.contains("nethopd size delta exceeds 750 KiB"));
}

#[test]
fn evidence_validator_checks_digests_secrets_and_panic_strategy() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let validator =
        fs::read_to_string(root.join("scripts/validate-node-benchmark-evidence.ps1")).unwrap();

    for contract in [
        "report_sha256",
        "postprocess_sha256",
        "requiredTasks",
        "forbidden sensitive material",
        "rustc --print cfg --target aarch64-linux-android",
        "panic=unwind",
    ] {
        assert!(
            validator.contains(contract),
            "missing validator contract: {contract}"
        );
    }
}
