use std::fs;

use nethopd::{WorkerConfig, WorkerConfigError};
use tempfile::tempdir;

fn valid_config() -> &'static [u8] {
    br#"{
      "schema":"nethop-worker-v1",
      "inbound_port":7893,
      "bypass_mark":131072,
      "ipv6_guard":true,
      "include_uids":[],
      "exclude_uids":[0],
      "allocations":[
        {"mark":256,"mask":65280,"route_table":100,"rule_priority":10000},
        {"mark":512,"mask":65280,"route_table":101,"rule_priority":10001}
      ]
    }"#
}

#[test]
fn strict_worker_config_builds_bounded_capture_and_candidates() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nethop.json");
    fs::write(&path, valid_config()).unwrap();
    let config = WorkerConfig::load(&path).unwrap();
    assert_eq!(config.capture().inbound_port(), Some(7893));
    assert_eq!(config.capture().bypass_mark(), Some(131072));
    assert!(config.capture().ipv6_guard());
    assert_eq!(config.capture().exclude_uids(), [0]);
    assert_eq!(config.allocations().len(), 2);
    assert_eq!(config.allocations()[0].route_table(), 100);
}

#[test]
fn unknown_missing_and_duplicate_allocation_fields_fail_closed() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nethop.json");
    for (bytes, expected) in [
        (
            valid_config().replace(b"\n    }", b",\"extra\":true\n    }"),
            WorkerConfigError::UnknownField,
        ),
        (
            valid_config().replace(b"\"nethop-worker-v1\"", b"\"other\""),
            WorkerConfigError::UnsupportedSchema,
        ),
        (
            valid_config().replace(
                b"{\"mark\":512,\"mask\":65280,\"route_table\":101,\"rule_priority\":10001}",
                b"{\"mark\":256,\"mask\":65280,\"route_table\":100,\"rule_priority\":10000}",
            ),
            WorkerConfigError::InvalidAllocation,
        ),
    ] {
        fs::write(&path, bytes).unwrap();
        assert_eq!(WorkerConfig::load(&path).unwrap_err(), expected);
    }
}

trait ByteReplace {
    fn replace(&self, from: &[u8], to: &[u8]) -> Vec<u8>;
}

impl ByteReplace for [u8] {
    fn replace(&self, from: &[u8], to: &[u8]) -> Vec<u8> {
        let position = self
            .windows(from.len())
            .position(|window| window == from)
            .expect("fixture fragment exists");
        let mut output = Vec::with_capacity(self.len() - from.len() + to.len());
        output.extend_from_slice(&self[..position]);
        output.extend_from_slice(to);
        output.extend_from_slice(&self[position + from.len()..]);
        output
    }
}

#[test]
fn relative_empty_and_oversized_files_are_rejected_before_parsing() {
    assert_eq!(
        WorkerConfig::load(std::path::Path::new("relative.json")).unwrap_err(),
        WorkerConfigError::InvalidPath
    );
    let directory = tempdir().unwrap();
    let path = directory.path().join("nethop.json");
    fs::write(&path, []).unwrap();
    assert_eq!(
        WorkerConfig::load(&path).unwrap_err(),
        WorkerConfigError::InvalidPath
    );
    fs::write(&path, vec![b' '; 64 * 1024 + 1]).unwrap();
    assert_eq!(
        WorkerConfig::load(&path).unwrap_err(),
        WorkerConfigError::InvalidPath
    );
}
