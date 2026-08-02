#![allow(dead_code)]

use std::path::PathBuf;

use serde::Deserialize;
use serde::de::DeserializeOwned;

#[derive(Debug, Deserialize)]
pub struct PhaseEvidence {
    pub command: String,
    pub exit_code: i32,
    pub summary: String,
}

#[derive(Debug, Deserialize)]
pub struct TddEvidenceManifest {
    pub schema_version: u32,
    pub task_id: String,
    pub spec_refs: Vec<String>,
    pub tests: Vec<String>,
    pub red: PhaseEvidence,
    pub green: PhaseEvidence,
    pub refactor: PhaseEvidence,
    pub fixture_sha256: String,
    pub rust_toolchain: String,
    pub features: Vec<String>,
    pub implementation_files: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct FixtureManifest {
    pub fixture_id: String,
    pub format: String,
    pub protocol_counts: std::collections::BTreeMap<String, u32>,
    pub seed: u64,
    pub bytes: u64,
    pub nodes: u32,
    pub sha256: String,
}

pub fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn workspace_root() -> PathBuf {
    crate_root()
        .parent()
        .and_then(|path| path.parent())
        .expect("crate must live under workspace crates/")
        .to_path_buf()
}

pub fn read_workspace_file(relative: &str) -> String {
    let path = workspace_root().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read workspace file {}: {error}", path.display()))
}

pub fn read_fixture(relative: &str) -> serde_json::Value {
    let path = crate_root().join("tests").join("fixtures").join(relative);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read fixture {}: {error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("invalid JSON fixture {}: {error}", path.display()))
}

pub fn read_fixture_as<T: DeserializeOwned>(relative: &str) -> T {
    let path = crate_root().join("tests").join("fixtures").join(relative);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read fixture {}: {error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("invalid typed fixture {}: {error}", path.display()))
}
