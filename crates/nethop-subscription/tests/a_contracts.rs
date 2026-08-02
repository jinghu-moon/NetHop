mod common;

use std::process::Command;

fn manifest_text() -> String {
    common::read_workspace_file("crates/nethop-subscription/Cargo.toml")
}

fn root_manifest_text() -> String {
    common::read_workspace_file("Cargo.toml")
}

#[test]
fn workspace_metadata_contract() {
    let output = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(common::workspace_root())
        .output()
        .expect("cargo metadata must be runnable");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("metadata must be JSON");
    let packages = metadata["packages"]
        .as_array()
        .expect("packages must be an array");
    let package = packages
        .iter()
        .find(|package| package["name"] == "nethop-subscription")
        .expect("nethop-subscription package must exist");
    assert_eq!(package["edition"], "2024");
    assert_eq!(package["rust_version"], "1.86");
    assert_eq!(package["license"], "AGPL-3.0-only");
    assert!(package["targets"].as_array().unwrap().iter().any(|target| {
        target["kind"]
            .as_array()
            .is_some_and(|kinds| kinds.iter().any(|kind| kind == "lib"))
    }));
}

#[test]
fn release_profile_contract() {
    let manifest = root_manifest_text();
    for expected in [
        "[profile.release]",
        "opt-level = 3",
        "lto = \"thin\"",
        "codegen-units = 1",
        "strip = \"symbols\"",
        "incremental = false",
    ] {
        assert!(
            manifest.contains(expected),
            "release profile missing {expected}"
        );
    }
    assert!(
        !manifest.contains("panic = \"abort\""),
        "initial profile must retain unwind"
    );
}

#[test]
fn stable_feature_graph_contract() {
    let manifest = manifest_text();
    for expected in [
        "default = [",
        "\"parser\"",
        "\"format-uri\"",
        "\"format-base64\"",
        "\"format-clash-yaml\"",
        "\"format-singbox-json\"",
        "format-clash-yaml = [\"parser\", \"dep:serde-saphyr\"]",
    ] {
        assert!(
            manifest.contains(expected),
            "feature graph missing {expected}"
        );
    }
    assert!(!manifest.contains("base64-simd"));
    assert!(!manifest.contains("simd-unsafe"));
}

#[test]
fn optional_feature_isolation_contract() {
    let manifest = manifest_text();
    assert!(manifest.contains("fetch = [\"parser\", \"dep:flate2\", \"dep:ureq\", \"dep:url\"]"));
    assert!(manifest.contains("ureq = { version = \"=3.3.0\", optional = true"));
    assert!(manifest.contains("url = { version = \"2\", optional = true"));
    for experimental in [
        "format-stash",
        "format-surge",
        "format-surfboard",
        "format-shadowrocket",
        "format-quantumultx",
    ] {
        assert!(
            manifest.contains(experimental),
            "missing experimental feature {experimental}"
        );
    }
    assert!(!manifest.contains("fetch = [\"parser\"]"));
}

#[test]
fn test_layout_contract() {
    let root = common::crate_root();
    assert!(root.join("src/lib.rs").is_file());
    assert!(root.join("tests/smoke.rs").is_file());
    assert!(root.join("tests/common/mod.rs").is_file());
    assert!(
        !root.join("tests/common.rs").is_file(),
        "common helper must not be a test target"
    );
}

#[test]
fn tdd_manifest_schema_contract() {
    let valid: common::TddEvidenceManifest = common::read_fixture_as("tdd-manifest-valid.json");
    assert_eq!(valid.schema_version, 1);
    assert!(valid.task_id.starts_with('A'));
    assert!(!valid.spec_refs.is_empty());
    assert!(!valid.tests.is_empty());
    assert!(!valid.red.command.is_empty());
    assert_eq!(valid.red.exit_code, 101);
    assert_eq!(valid.green.exit_code, 0);
    assert_eq!(valid.refactor.exit_code, 0);
    assert!(!valid.red.summary.is_empty());
    assert_eq!(valid.fixture_sha256.len(), 64);
    assert!(!valid.rust_toolchain.is_empty());
    assert!(!valid.features.is_empty());
    assert!(!valid.implementation_files.is_empty());

    let invalid = common::read_fixture("tdd-manifest-invalid.json");
    assert!(invalid.get("task_id").is_none());
    assert!(invalid.get("fixture_sha256").is_none());
}

#[test]
fn fixture_manifest_schema_contract() {
    let valid: common::FixtureManifest = common::read_fixture_as("fixture-manifest-valid.json");
    assert!(!valid.fixture_id.is_empty());
    assert_eq!(valid.format, "uri_list");
    assert_eq!(valid.protocol_counts.values().sum::<u32>(), valid.nodes);
    assert!(valid.seed > 0);
    assert!(valid.bytes > 0);
    assert_eq!(valid.sha256.len(), 64);

    let invalid = common::read_fixture("fixture-manifest-invalid.json");
    assert!(
        invalid["sha256"]
            .as_str()
            .is_some_and(|value| value.len() != 64)
    );
    assert!(invalid["nodes"].as_i64().is_some_and(|value| value < 0));
}

#[test]
fn ci_matrix_contract() {
    let workflow = common::read_workspace_file(".github/workflows/subscription-parser.yml");
    for feature_set in [
        "parser,format-uri,format-base64,format-clash-yaml,format-singbox-json",
        "parser,experimental-formats",
        "parser,format-uri,format-base64,format-clash-yaml,format-singbox-json,fetch",
    ] {
        assert!(
            workflow.contains(feature_set),
            "CI matrix missing {feature_set}"
        );
    }
    assert!(workflow.contains("cargo tree --locked -e normal,features"));
    assert!(workflow.contains("cargo test --locked"));
}

#[test]
fn gate_script_contract() {
    let script = common::read_workspace_file("scripts/a-gate.ps1");
    for fragment in [
        "@(\"metadata\", \"--locked\", \"--format-version\", \"1\")",
        "@(\"tree\", \"--locked\", \"-e\", \"normal,features\")",
        "@(\"test\", \"--locked\")",
        "@(\"test\", \"--locked\", \"--test\", \"b_contracts\")",
        "parser,format-uri,format-base64,format-clash-yaml,format-singbox-json",
        "parser,experimental-formats",
        "parser,format-uri,format-base64,format-clash-yaml,format-singbox-json,fetch",
    ] {
        assert!(script.contains(fragment), "gate script missing {fragment}");
    }
}
