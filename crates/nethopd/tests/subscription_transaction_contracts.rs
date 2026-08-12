use std::{fs, sync::mpsc, thread, time::Duration};

use nethopd::{
    CommitJournal, CommitJournalStore, CommitPhase, MutationCoordinator, RecoveryAction,
    TransactionError,
};
use tempfile::tempdir;

fn journal() -> CommitJournal {
    CommitJournal::new(
        "a".repeat(64),
        "b".repeat(64),
        Some(7),
        8,
        ".candidate-8-worker",
        ".candidate-config-8",
    )
    .unwrap()
}

#[test]
fn journal_is_strict_private_bounded_and_secret_free() {
    let directory = tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let store = CommitJournalStore::new(&root).unwrap();
    let mut value = journal();
    value.advance(CommitPhase::Journaled).unwrap();
    store.write(&value).unwrap();
    assert_eq!(store.load().unwrap(), Some(value));
    let bytes = fs::read(store.path()).unwrap();
    assert!(bytes.len() < 4096);
    let text = String::from_utf8(bytes).unwrap();
    assert!(!text.contains("https://"));
    assert!(!text.contains("password"));
    assert!(!text.contains("uuid"));

    fs::write(store.path(), r#"{"schema":2}"#).unwrap();
    assert_eq!(store.load().unwrap_err(), TransactionError::InvalidJournal);
}

#[test]
fn journal_phase_is_monotonic_and_recovery_is_idempotent() {
    let directory = tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let store = CommitJournalStore::new(root).unwrap();
    let mut value = journal();
    value.advance(CommitPhase::Sealed).unwrap();
    assert_eq!(
        value.advance(CommitPhase::Checked).unwrap_err(),
        TransactionError::PhaseRegression
    );
    assert_eq!(
        store.recovery_action(&value, Some(7), None).unwrap(),
        RecoveryAction::DiscardStaged
    );
    value.advance(CommitPhase::GenerationPublished).unwrap();
    assert_eq!(
        store
            .recovery_action(&value, Some(8), Some(&"b".repeat(64)))
            .unwrap(),
        RecoveryAction::CompleteConfigPublish
    );
    assert_eq!(
        store
            .recovery_action(&value, Some(8), Some(&"b".repeat(64)))
            .unwrap(),
        RecoveryAction::CompleteConfigPublish
    );
}

#[test]
fn mutation_coordinator_allows_exactly_one_commit_section() {
    let coordinator = MutationCoordinator::default();
    let first = coordinator.acquire().unwrap();
    assert!(first.held());
    let second = coordinator.clone();
    let (started_tx, started_rx) = mpsc::channel();
    let (acquired_tx, acquired_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        started_tx.send(()).unwrap();
        let guard = second.acquire().unwrap();
        acquired_tx.send(guard.held()).unwrap();
    });
    started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(acquired_rx.recv_timeout(Duration::from_millis(50)).is_err());
    drop(first);
    assert!(acquired_rx.recv_timeout(Duration::from_secs(1)).unwrap());
    worker.join().unwrap();
}

#[test]
fn recovery_discards_unpublished_generation_and_keeps_old_config() {
    let directory = tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let canonical = root.join("nethop.toml");
    fs::write(&canonical, b"old-config").unwrap();
    let store = CommitJournalStore::new(&root).unwrap();
    let old_digest = nethop_subscription::Digest::sha256(b"old-config").hex();
    let new_digest = nethop_subscription::Digest::sha256(b"new-config").hex();
    let mut value = CommitJournal::new(
        old_digest,
        new_digest,
        Some(7),
        8,
        ".candidate-8-worker",
        ".candidate-config-8",
    )
    .unwrap();
    store.stage_config(&value, b"new-config").unwrap();
    store
        .advance_and_write(&mut value, CommitPhase::Journaled)
        .unwrap();

    assert_eq!(
        store.recover(&canonical, Some(7)).unwrap(),
        RecoveryAction::DiscardStaged
    );
    assert_eq!(fs::read(canonical).unwrap(), b"old-config");
    assert!(!store.path().exists());
}

#[test]
fn recovery_finishes_config_after_generation_publish_and_is_idempotent() {
    let directory = tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let canonical = root.join("nethop.toml");
    fs::write(&canonical, b"old-config").unwrap();
    let store = CommitJournalStore::new(&root).unwrap();
    let old_digest = nethop_subscription::Digest::sha256(b"old-config").hex();
    let new_digest = nethop_subscription::Digest::sha256(b"new-config").hex();
    let mut value = CommitJournal::new(
        old_digest,
        new_digest,
        Some(7),
        8,
        ".candidate-8-worker",
        ".candidate-config-8",
    )
    .unwrap();
    store.stage_config(&value, b"new-config").unwrap();
    store
        .advance_and_write(&mut value, CommitPhase::GenerationPublished)
        .unwrap();

    assert_eq!(
        store.recover(&canonical, Some(8)).unwrap(),
        RecoveryAction::CompleteConfigPublish
    );
    assert_eq!(fs::read(&canonical).unwrap(), b"new-config");
    assert_eq!(
        store.recover(&canonical, Some(8)).unwrap(),
        RecoveryAction::ClearJournal
    );
}
