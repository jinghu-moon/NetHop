mod common;

use serde_json::Value;

const EVIDENCE_ROOT: &str = "artifacts/subscription-parser/m010";

#[test]
fn m010_cyclonedx_covers_each_dependency_profile_without_unknown_licenses() {
    for profile in ["parser-only", "fetch", "dev-test"] {
        let bom = read_evidence(&format!("{profile}.cdx.json"));
        assert_eq!(bom["bomFormat"], "CycloneDX");
        assert_eq!(bom["specVersion"], "1.6");
        assert_eq!(bom["metadata"]["component"]["name"], "nethop-subscription");

        let components = bom["components"].as_array().expect("components array");
        assert!(
            !components.is_empty(),
            "{profile} closure must not be empty"
        );
        for component in components {
            let name = component["name"].as_str().expect("component name");
            let expression = component["licenses"][0]["expression"]
                .as_str()
                .unwrap_or_default();
            assert!(
                !expression.is_empty() && expression != "UNKNOWN",
                "{profile} contains unknown license for {name}"
            );
            assert!(
                component["purl"]
                    .as_str()
                    .is_some_and(|purl| purl.starts_with("pkg:cargo/"))
            );
        }
    }
}

#[test]
fn m010_license_inventory_and_provenance_bind_current_inputs() {
    let licenses = read_evidence("licenses.json");
    assert_eq!(licenses["schema_version"], 1);
    assert_eq!(licenses["unknown_licenses"], 0);
    assert!(
        licenses["packages"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );

    let provenance = read_evidence("provenance.json");
    assert_eq!(provenance["schema_version"], 1);
    assert_eq!(provenance["status"], "passed_with_tooling_disclosure");
    assert_eq!(
        provenance["inputs"]["cargo_lock_sha256"],
        sha256_workspace_file("Cargo.lock")
    );
    assert_eq!(
        provenance["inputs"]["workspace_source_sha256"],
        workspace_source_digest()
    );
    assert_eq!(
        provenance["dependency_profiles"]["parser-only"]["feature_leakage"],
        false
    );
    assert_eq!(
        provenance["dependency_profiles"]["fetch"]["feature_leakage"],
        false
    );
    assert_eq!(
        provenance["dependency_profiles"]["dev-test"]["feature_leakage"],
        false
    );
    assert_eq!(provenance["tools"]["cargo_deny"]["status"], "passed");
    let deny = read_evidence("cargo-deny-report.json");
    assert_eq!(deny["status"], "passed");
    assert_eq!(deny["checks"].as_array().map(Vec::len), Some(4));
    assert_eq!(
        provenance["tools"]["cargo_cyclonedx"]["status"],
        "not_available"
    );
}

#[test]
fn m010_generator_is_repository_local_and_does_not_enter_runtime_dependencies() {
    let script =
        common::read_workspace_file("scripts/generate-subscription-parser-release-evidence.ps1");
    for required in ["cargo metadata", "--locked", "CycloneDX", "Cargo.lock"] {
        assert!(script.contains(required), "generator missing {required}");
    }

    let manifest = common::read_workspace_file("crates/nethop-subscription/Cargo.toml");
    for forbidden in ["cyclonedx", "spdx", "cargo-deny", "cargo_metadata"] {
        assert!(
            !manifest.contains(forbidden),
            "runtime manifest contains {forbidden}"
        );
    }
}

#[test]
fn m011_nightly_fuzz_workflow_separates_long_runs_from_pr_smoke() {
    let workflow = common::read_workspace_file(".github/workflows/subscription-parser-nightly.yml");
    for required in [
        "schedule:",
        "workflow_dispatch:",
        "cargo-fuzz",
        "detect_inputs",
        "uri_base64_inputs",
        "clash_yaml_inputs",
        "singbox_json_inputs",
        "surfboard_inputs",
        "scripts/run-subscription-parser-fuzz.ps1",
        "if-no-files-found: error",
    ] {
        assert!(
            workflow.contains(required),
            "nightly workflow missing {required}"
        );
    }
    let pr_workflow = common::read_workspace_file(".github/workflows/subscription-parser.yml");
    assert!(!pr_workflow.contains("max_total_time=1800"));
    assert!(!pr_workflow.contains("subscription-parser-nightly"));
}

#[test]
fn m011_fuzz_manifest_and_corpus_are_bounded_and_traceable() {
    let manifest = common::read_workspace_file("crates/nethop-subscription/fuzz/Cargo.toml");
    assert!(manifest.contains("cargo-fuzz = true"));
    assert!(manifest.contains("libfuzzer-sys"));

    let report = read_evidence_from("artifacts/subscription-parser/m011/schedule-report.json");
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["status"], "dry_run_passed");
    assert_eq!(report["runner"]["pr_smoke_seconds"], 60);
    assert_eq!(report["runner"]["nightly_seconds_per_target"], 1800);
    assert_eq!(
        report["runner"]["release_candidate_seconds_per_target"],
        3600
    );
    assert_eq!(report["runner"]["rss_limit_mb"], 512);
    assert_eq!(report["runner"]["parser_release_rss_budget_mb"], 110);

    let corpora = report["corpora"].as_array().expect("corpora array");
    assert_eq!(corpora.len(), 5);
    for corpus in corpora {
        let path = corpus["path"].as_str().expect("corpus path");
        let digest = corpus["sha256"].as_str().expect("corpus digest");
        assert_eq!(digest.len(), 64);
        assert!(common::workspace_root().join(path).is_dir());
        assert!(corpus["files"].as_u64().is_some_and(|count| count > 0));
    }
}

#[test]
fn m011_failure_artifact_schema_preserves_reproducer_context_without_secrets() {
    let schema =
        read_evidence_from("artifacts/subscription-parser/m011/failure-artifact-schema.json");
    assert_eq!(schema["schema_version"], 1);
    for required in [
        "target",
        "exit_code",
        "corpus_sha256",
        "artifact_sha256",
        "max_total_time_seconds",
        "rss_limit_mb",
        "toolchain",
    ] {
        assert!(
            schema["required"]
                .as_array()
                .is_some_and(|items| items.iter().any(|item| item == required)),
            "failure schema missing {required}"
        );
    }
    let serialized = serde_json::to_string(&schema).unwrap();
    for forbidden in ["subscription_url", "credential", "password", "token"] {
        assert!(
            !serialized.contains(forbidden),
            "failure schema contains {forbidden}"
        );
    }
}

#[test]
fn m012_support_matrix_is_evidence_derived_and_android_scoped() {
    let matrix = read_evidence_from("artifacts/subscription-parser/m012/support-matrix.json");
    assert_eq!(matrix["schema_version"], 1);
    assert_eq!(matrix["scope"], "android_subscription_parser");
    assert_eq!(matrix["runtime_core_claims"], false);

    let formats = matrix["formats"].as_array().expect("format matrix");
    for stable in ["uri_list", "base64_list", "clash_yaml", "singbox_json"] {
        let entry = find_named(formats, "format", stable);
        assert_eq!(entry["support_level"], "reference_verified");
        assert_eq!(entry["default_enabled"], true);
    }
    let surfboard = find_named(formats, "format", "surfboard_ini");
    assert_eq!(surfboard["support_level"], "experimental");
    assert_eq!(surfboard["default_enabled"], false);

    let protocols = matrix["protocols"].as_array().expect("protocol matrix");
    assert_eq!(protocols.len(), 9);
    for name in [
        "vless",
        "vmess",
        "shadowsocks",
        "trojan",
        "hysteria2",
        "tuic",
        "anytls",
        "http",
        "socks",
    ] {
        let entry = find_named(protocols, "protocol", name);
        assert_eq!(entry["parser_support"], "reference_verified");
        assert_eq!(entry["android_data_plane"], "best_effort");
    }

    let unsupported = matrix["unsupported_protocols"]
        .as_array()
        .expect("unsupported protocol matrix");
    let wireguard = find_named(unsupported, "protocol", "wireguard");
    assert_eq!(wireguard["support_level"], "unsupported");
    assert_eq!(
        wireguard["reason"],
        "sing_box_1_13_15_endpoint_outside_terminal_outbound_contract"
    );
    let naive = find_named(unsupported, "protocol", "naive");
    assert_eq!(naive["support_level"], "unsupported");
    assert_eq!(
        naive["reason"],
        "android_sing_box_1_13_15_missing_with_naive_outbound"
    );
    let mieru = find_named(unsupported, "protocol", "mieru");
    assert_eq!(mieru["support_level"], "unsupported");
    assert_eq!(mieru["reason"], "not_implemented_by_sing_box_1_13_15");

    assert_eq!(matrix["environments"].as_array().map(Vec::len), Some(3));
    let serialized = serde_json::to_string(&matrix).unwrap();
    assert!(!serialized.contains("all_android_devices"));
    assert!(!serialized.contains("full_client_compatibility"));
}

#[test]
fn m012_release_manifest_enables_only_verified_release_features() {
    let manifest = read_evidence_from("artifacts/subscription-parser/m012/release-manifest.json");
    assert_eq!(manifest["schema_version"], 1);
    assert_eq!(manifest["package"], "nethop-subscription");
    assert_eq!(manifest["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(manifest["target"], "aarch64-linux-android");
    assert_eq!(manifest["status"], "release_candidate");

    let enabled = manifest["features"]["enabled"]
        .as_array()
        .expect("enabled features");
    assert!(enabled.iter().any(|value| value == "fetch"));
    assert!(!enabled.iter().any(|value| value == "format-surfboard"));
    assert!(
        manifest["features"]["disabled"]
            .as_array()
            .is_some_and(|values| values.iter().any(|value| value == "format-surfboard"))
    );

    for artifact in manifest["artifacts"].as_array().expect("artifact index") {
        let path = artifact["path"].as_str().expect("artifact path");
        assert!(
            common::workspace_root().join(path).is_file(),
            "missing {path}"
        );
        assert_eq!(artifact["sha256"], sha256_workspace_file(path));
    }
}

#[test]
fn m012_generator_uses_test_manifests_instead_of_client_brand_claims() {
    let script =
        common::read_workspace_file("scripts/generate-subscription-parser-support-matrix.ps1");
    for evidence in [
        "alioth-parser-integration.json",
        "cross-environment-compatibility.json",
        "sing-box-1.13.15-mapping.json",
        "04-subscription-parser-phase0b-performance-report.md",
    ] {
        assert!(
            script.contains(evidence),
            "support generator missing {evidence}"
        );
    }
    for forbidden in ["NekoBox", "Karing", "Hiddify", "FlClash", "v2rayNG"] {
        assert!(
            !script.contains(forbidden),
            "support generator contains brand claim {forbidden}"
        );
    }
}

#[test]
fn m013_release_candidate_checklist_has_no_missing_or_failed_hard_gate() {
    let checklist =
        read_evidence_from("artifacts/subscription-parser/m013/release-candidate-checklist.json");
    assert_eq!(checklist["schema_version"], 1);
    assert_eq!(checklist["status"], "passed");
    let gates = checklist["gates"].as_array().expect("release gates");
    assert!(gates.len() >= 10);
    assert!(gates.iter().all(|gate| gate["status"] == "passed"));
    for required in [
        "locked-metadata",
        "workspace-tests",
        "all-features-tests",
        "release-tests",
        "clippy",
        "cargo-deny",
        "fuzz-smoke",
        "performance-evidence",
        "android-evidence",
        "support-matrix",
    ] {
        assert!(
            gates.iter().any(|gate| gate["id"] == required),
            "missing gate {required}"
        );
    }
}

#[test]
fn m013_artifact_index_is_complete_and_digest_bound() {
    let index = read_evidence_from("artifacts/subscription-parser/m013/artifact-index.json");
    assert_eq!(index["schema_version"], 1);
    assert_eq!(index["status"], "passed");
    let artifacts = index["artifacts"].as_array().expect("release artifacts");
    assert!(artifacts.len() >= 15);
    for artifact in artifacts {
        let path = artifact["path"].as_str().expect("artifact path");
        assert!(
            common::workspace_root().join(path).is_file(),
            "missing {path}"
        );
        assert_eq!(artifact["sha256"], sha256_workspace_file(path));
    }
}

#[test]
fn m013_release_gate_is_locked_offline_fixture_driven_and_fails_closed() {
    let script = common::read_workspace_file("scripts/subscription-parser-release-gate.ps1");
    for required in [
        "cargo metadata",
        "--locked",
        "cargo-deny",
        "run-subscription-parser-fuzz.ps1",
        "cargo clippy",
        "cargo fmt",
        "aarch64-linux-android",
        "release-candidate-checklist.json",
    ] {
        assert!(script.contains(required), "release gate missing {required}");
    }
    assert!(!script.contains("update.glados-config.com"));
    assert!(!script.contains("http://"));
}

#[test]
fn m014_release_freeze_manifest_is_auditable_and_all_invariants_pass() {
    let freeze = read_evidence_from("artifacts/subscription-parser/m014/release-freeze.json");
    assert_eq!(freeze["schema_version"], 1);
    assert_eq!(freeze["status"], "frozen");
    assert_eq!(freeze["release_candidate_status"], "passed");
    assert_eq!(freeze["target"], "aarch64-linux-android");
    assert_eq!(freeze["invariants"].as_array().map(Vec::len), Some(10));
    assert!(
        freeze["invariants"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["status"] == "passed")
    );
    assert_eq!(freeze["checks"]["workspace_tests"], "passed");
    assert_eq!(freeze["checks"]["all_features_tests"], "passed");
    assert_eq!(freeze["checks"]["fuzz_smoke"], "passed");

    for artifact in freeze["artifacts"].as_array().expect("freeze artifacts") {
        let path = artifact["path"].as_str().expect("freeze artifact path");
        assert!(
            common::workspace_root().join(path).is_file(),
            "missing {path}"
        );
        assert_eq!(artifact["sha256"], sha256_workspace_file(path));
    }
}

#[test]
fn m014_freeze_script_does_not_mutate_git_or_enable_unverified_features() {
    let script = common::read_workspace_file("scripts/subscription-parser-freeze-gate.ps1");
    for required in [
        "cargo test --workspace",
        "--all-features",
        "cargo clippy",
        "release-freeze.json",
        "invariants",
        "git diff --check",
    ] {
        assert!(script.contains(required), "freeze gate missing {required}");
    }
    for forbidden in [
        "git commit",
        "git push",
        "format-surfboard = true",
        "panic = \"abort\"",
    ] {
        assert!(
            !script.contains(forbidden),
            "freeze gate contains forbidden mutation/feature {forbidden}"
        );
    }
}

fn read_evidence(name: &str) -> Value {
    let path = format!("{EVIDENCE_ROOT}/{name}");
    read_evidence_from(&path)
}

fn read_evidence_from(path: &str) -> Value {
    serde_json::from_str(&common::read_workspace_file(path)).expect("valid evidence JSON")
}

fn sha256_workspace_file(path: &str) -> String {
    use sha2::{Digest, Sha256};

    let bytes = std::fs::read(common::workspace_root().join(path)).expect("workspace file");
    let text = String::from_utf8(bytes).expect("release evidence must be UTF-8 text");
    let canonical = text.replace("\r\n", "\n");
    hex_lower(&Sha256::digest(canonical.as_bytes()))
}

fn workspace_source_digest() -> String {
    use sha2::{Digest, Sha256};

    let manifest = read_evidence("source-files.json");
    let entries = manifest["files"].as_array().expect("source file array");
    let mut canonical = String::new();
    for entry in entries {
        let path = entry["path"].as_str().expect("source path");
        canonical.push_str(&sha256_workspace_file(path));
        canonical.push_str("  ");
        canonical.push_str(path);
        canonical.push('\n');
    }
    hex_lower(&Sha256::digest(canonical.as_bytes()))
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn find_named<'a>(items: &'a [Value], key: &str, value: &str) -> &'a Value {
    items
        .iter()
        .find(|item| item[key] == value)
        .unwrap_or_else(|| panic!("missing {key}={value}"))
}
