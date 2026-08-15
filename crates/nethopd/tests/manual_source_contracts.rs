#![cfg(feature = "subscription-update")]

use std::fs;

use nethop_subscription::FormatHint;
use nethopd::{ManualSource, ManualSourceStore};
use tempfile::tempdir;

fn private_directory(_path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(_path, fs::Permissions::from_mode(0o700)).unwrap();
    }
}

#[test]
fn manual_source_is_private_bounded_versioned_and_redacted() {
    let directory = tempdir().unwrap();
    private_directory(directory.path());
    let path = directory.path().join("manual-source.body");
    let store = ManualSourceStore::new(&path).unwrap();
    let secret = b"trojan://manual-password@example.com:443#manual\n";

    store.replace(FormatHint::UriList, secret).unwrap();
    let source = store.load().unwrap().unwrap();
    assert_eq!(source.format_hint(), FormatHint::UriList);
    assert_eq!(source.bytes(), secret);
    assert_eq!(source.digest().len(), 64);
    assert_eq!(
        ManualSource::source_id().as_str(),
        "src_00000000000000000000000000000000"
    );
    assert!(!format!("{source:?}").contains("manual-password"));
    assert!(
        fs::read(&path)
            .unwrap()
            .starts_with(b"nethop-manual-source-v1 uri_list\n")
    );
}

#[test]
fn failed_generation_can_restore_the_previous_manual_source_exactly() {
    let directory = tempdir().unwrap();
    private_directory(directory.path());
    let path = directory.path().join("manual-source.body");
    let store = ManualSourceStore::new(&path).unwrap();
    let first = b"trojan://first@example.com:443#first\n";
    let second = b"trojan://second@example.com:443#second\n";
    store.replace(FormatHint::UriList, first).unwrap();

    let checkpoint = store.replace(FormatHint::UriList, second).unwrap();
    assert_eq!(store.load().unwrap().unwrap().bytes(), second);
    store.restore(checkpoint).unwrap();
    assert_eq!(store.load().unwrap().unwrap().bytes(), first);
}

#[test]
#[cfg(unix)]
fn manual_source_rejects_a_public_parent_before_the_first_write() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempdir().unwrap();
    fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o755)).unwrap();

    assert!(ManualSourceStore::new(directory.path().join("manual-source.body")).is_err());
}
