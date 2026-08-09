use std::time::{Duration, SystemTime};

use base64::{Engine as _, engine::general_purpose};
use nethop_protocol::WebUiPayloadNamespace;
use nethopd::{MAX_PAYLOAD_BYTES, MAX_PAYLOAD_CHUNK_BYTES, WebUiPayloadError, WebUiPayloadStore};
use tempfile::tempdir;

fn store() -> (tempfile::TempDir, WebUiPayloadStore) {
    let temporary = tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap().join("payloads");
    let store = WebUiPayloadStore::open(root).unwrap();
    (temporary, store)
}

#[test]
fn create_returns_unique_server_handles_and_round_trips_all_base64_variants() {
    let (_temporary, store) = store();
    let first = store.create(WebUiPayloadNamespace::Config).unwrap();
    let second = store.create(WebUiPayloadNamespace::Config).unwrap();
    assert_ne!(first, second);
    assert!(first.starts_with("p_"));
    assert_eq!(first.len(), 34);

    let variants = ["YWJjZA==", "YWJjZA", "--__", "--__"];
    for (index, chunk) in variants.into_iter().enumerate() {
        let handle = store.create(WebUiPayloadNamespace::Subscription).unwrap();
        store
            .append(WebUiPayloadNamespace::Subscription, &handle, chunk)
            .unwrap();
        let decoded = store
            .consume(WebUiPayloadNamespace::Subscription, &handle)
            .unwrap();
        assert!(!decoded.is_empty(), "variant {index} must decode");
        assert_eq!(
            store.consume(WebUiPayloadNamespace::Subscription, &handle),
            Err(WebUiPayloadError::Unavailable)
        );
    }
}

#[test]
fn append_is_bounded_and_invalid_input_does_not_change_payload() {
    let (_temporary, store) = store();
    let handle = store.create(WebUiPayloadNamespace::Config).unwrap();
    assert_eq!(
        store.append(WebUiPayloadNamespace::Config, &handle, "not base64!"),
        Err(WebUiPayloadError::InvalidChunk)
    );
    let valid = general_purpose::STANDARD.encode([42_u8; MAX_PAYLOAD_CHUNK_BYTES]);
    assert_eq!(
        store
            .append(WebUiPayloadNamespace::Config, &handle, &valid)
            .unwrap(),
        MAX_PAYLOAD_CHUNK_BYTES
    );
    let bytes = store
        .consume(WebUiPayloadNamespace::Config, &handle)
        .unwrap();
    assert_eq!(bytes, vec![42; MAX_PAYLOAD_CHUNK_BYTES]);
}

#[test]
fn cumulative_limit_removes_staging_payload_and_remove_is_idempotent() {
    let (_temporary, store) = store();
    let handle = store.create(WebUiPayloadNamespace::Backup).unwrap();
    let chunk = general_purpose::STANDARD.encode([7_u8; MAX_PAYLOAD_CHUNK_BYTES]);
    let full_chunks = MAX_PAYLOAD_BYTES / MAX_PAYLOAD_CHUNK_BYTES;
    for _ in 0..full_chunks {
        store
            .append(WebUiPayloadNamespace::Backup, &handle, &chunk)
            .unwrap();
    }
    assert_eq!(
        store.append(WebUiPayloadNamespace::Backup, &handle, &chunk),
        Err(WebUiPayloadError::LimitExceeded)
    );
    assert_eq!(
        store.consume(WebUiPayloadNamespace::Backup, &handle),
        Err(WebUiPayloadError::Unavailable)
    );
    store
        .remove(WebUiPayloadNamespace::Backup, &handle)
        .unwrap();
    store
        .remove(WebUiPayloadNamespace::Backup, &handle)
        .unwrap();
}

#[test]
fn cleanup_is_ttl_driven_and_bounded_to_owned_handles() {
    let (_temporary, store) = store();
    let keep = store.create(WebUiPayloadNamespace::Config).unwrap();
    assert_eq!(
        store
            .cleanup_expired(SystemTime::now(), Duration::from_secs(24 * 60 * 60))
            .unwrap(),
        0
    );
    assert_eq!(
        store
            .cleanup_expired(SystemTime::now(), Duration::ZERO)
            .unwrap(),
        1
    );
    assert_eq!(
        store.consume(WebUiPayloadNamespace::Config, &keep),
        Err(WebUiPayloadError::Unavailable)
    );
}

#[cfg(unix)]
#[test]
fn directories_and_payloads_are_private_and_links_are_rejected() {
    use std::fs::hard_link;
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};

    let (temporary, store) = store();
    let root = temporary.path().canonicalize().unwrap().join("payloads");
    assert_eq!(
        std::fs::metadata(&root).unwrap().permissions().mode() & 0o077,
        0
    );
    let handle = store.create(WebUiPayloadNamespace::Config).unwrap();
    let path = root.join("config").join(&handle);
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o077,
        0
    );

    let linked = root.join("config").join(format!("p_{}", "b".repeat(32)));
    hard_link(&path, &linked).unwrap();
    assert!(std::fs::metadata(&path).unwrap().nlink() > 1);
    assert_eq!(
        store.remove(WebUiPayloadNamespace::Config, &handle),
        Err(WebUiPayloadError::UnsafeFile)
    );

    let outside = temporary.path().join("outside");
    std::fs::write(&outside, "keep").unwrap();
    let symlink_handle = format!("p_{}", "c".repeat(32));
    symlink(&outside, root.join("config").join(&symlink_handle)).unwrap();
    assert_eq!(
        store.remove(WebUiPayloadNamespace::Config, &symlink_handle),
        Err(WebUiPayloadError::UnsafeFile)
    );
    assert_eq!(std::fs::read_to_string(outside).unwrap(), "keep");
}
