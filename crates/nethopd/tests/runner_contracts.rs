use std::{collections::BTreeMap, fs, time::Duration};

use nethop_core::{Candidate, GenerationId, GenerationStore, ManagedConfig, TerminalOutbound};
use nethopd::{RunnerLimits, SingBoxCheckRunner};
use serde_json::json;

#[test]
fn generation_validator_receives_the_controlled_candidate_config_path() {
    let directory = tempfile::tempdir().unwrap();
    let store = GenerationStore::new(directory.path()).unwrap();
    let config = ManagedConfig::from_outbounds(vec![
        TerminalOutbound::new(
            "node",
            "trojan",
            BTreeMap::from([
                ("server".into(), json!("example.com")),
                ("server_port".into(), json!(443)),
                ("password".into(), json!("fixture")),
            ]),
        )
        .unwrap(),
    ])
    .unwrap();
    let candidate = Candidate::new(GenerationId::new(1).unwrap(), config.clone());

    store
        .publish_with_path(&candidate, |path, bytes| {
            assert!(path.is_absolute());
            assert_eq!(path.file_name().unwrap(), "config.json");
            assert!(
                path.parent()
                    .unwrap()
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".candidate-")
            );
            assert_eq!(fs::read(path).unwrap(), bytes);
            Ok(())
        })
        .unwrap();
    assert_eq!(
        store.current_generation().unwrap(),
        Some(candidate.generation())
    );
}

#[test]
fn runner_policy_is_bounded_for_the_daemon_boundary() {
    let limits = RunnerLimits::new(Duration::from_secs(3), 16 * 1024).unwrap();
    assert_eq!(limits.timeout(), Duration::from_secs(3));
    assert_eq!(limits.output_bytes_per_stream(), 16 * 1024);

    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("generations");
    fs::create_dir(&root).unwrap();
    let runner = SingBoxCheckRunner::new(std::env::current_exe().unwrap(), root, limits);
    assert!(runner.is_ok());
}
