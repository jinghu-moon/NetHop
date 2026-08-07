use std::{
    cell::Cell,
    fs,
    path::{Path, PathBuf},
};

use nethopd::{
    CandidateChecker, RuleSetError, RuleSetLimits, RuleSetPreparation, RuleSetReplaceOutcome,
    RuleSetStore, RunnerError,
};

struct FixtureChecker {
    accept: bool,
    calls: Cell<usize>,
}

impl FixtureChecker {
    fn new(accept: bool) -> Self {
        Self {
            accept,
            calls: Cell::new(0),
        }
    }
}

impl CandidateChecker for FixtureChecker {
    fn check(&self, config_path: &Path) -> Result<(), RunnerError> {
        self.calls.set(self.calls.get() + 1);
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(config_path).unwrap()).unwrap();
        let sets = value["route"]["rule_set"].as_array().unwrap();
        assert_eq!(sets.len(), 2);
        for set in sets {
            let path = PathBuf::from(set["path"].as_str().unwrap());
            assert!(path.is_absolute());
            assert_eq!(path.parent(), config_path.parent());
            assert!(fs::read(path).unwrap().starts_with(b"SRS"));
        }
        if self.accept {
            Ok(())
        } else {
            Err(RunnerError::SpawnFailed)
        }
    }
}

fn fixture() -> (tempfile::TempDir, RuleSetStore) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("rulesets");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("cn-domain.srs"), b"SRS\x01old-domain").unwrap();
    fs::write(root.join("cn-ip.srs"), b"SRS\x02old-ip").unwrap();
    let store = RuleSetStore::open(root, RuleSetLimits::default()).unwrap();
    (directory, store)
}

#[test]
fn admitted_pair_replaces_both_files_and_cleans_staging() {
    let (_directory, store) = fixture();
    let checker = FixtureChecker::new(true);

    assert_eq!(
        store
            .replace(b"SRS\x01new-domain", b"SRS\x02new-ip", &checker)
            .unwrap(),
        RuleSetReplaceOutcome::Updated
    );

    assert_eq!(checker.calls.get(), 1);
    assert_eq!(
        fs::read(store.root().join("cn-domain.srs")).unwrap(),
        b"SRS\x01new-domain"
    );
    assert_eq!(
        fs::read(store.root().join("cn-ip.srs")).unwrap(),
        b"SRS\x02new-ip"
    );
    assert!(fs::read_dir(store.root()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with('.')
    }));
}

#[test]
fn identical_pair_is_a_noop_before_external_check() {
    let (_directory, store) = fixture();
    let checker = FixtureChecker::new(true);

    assert_eq!(
        store
            .replace(b"SRS\x01old-domain", b"SRS\x02old-ip", &checker)
            .unwrap(),
        RuleSetReplaceOutcome::Unchanged
    );
    assert_eq!(checker.calls.get(), 0);
}

#[test]
fn published_pair_can_be_rolled_back_before_commit() {
    let (_directory, store) = fixture();
    let prepared = match store
        .prepare(
            b"SRS\x01candidate-domain",
            b"SRS\x02candidate-ip",
            &FixtureChecker::new(true),
        )
        .unwrap()
    {
        RuleSetPreparation::Prepared(prepared) => prepared,
        RuleSetPreparation::Unchanged => panic!("fixture must produce a candidate"),
    };

    let published = store.publish(prepared).unwrap();
    assert_eq!(
        fs::read(store.root().join("cn-domain.srs")).unwrap(),
        b"SRS\x01candidate-domain"
    );
    store.rollback(&published).unwrap();

    assert_eq!(
        fs::read(store.root().join("cn-domain.srs")).unwrap(),
        b"SRS\x01old-domain"
    );
    assert_eq!(
        fs::read(store.root().join("cn-ip.srs")).unwrap(),
        b"SRS\x02old-ip"
    );
    assert!(fs::read_dir(store.root()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with('.')
    }));
}

#[test]
fn committed_pair_removes_rollback_material() {
    let (_directory, store) = fixture();
    let prepared = match store
        .prepare(
            b"SRS\x01candidate-domain",
            b"SRS\x02candidate-ip",
            &FixtureChecker::new(true),
        )
        .unwrap()
    {
        RuleSetPreparation::Prepared(prepared) => prepared,
        RuleSetPreparation::Unchanged => panic!("fixture must produce a candidate"),
    };
    let published = store.publish(prepared).unwrap();

    store.commit(&published).unwrap();

    assert_eq!(
        fs::read(store.root().join("cn-domain.srs")).unwrap(),
        b"SRS\x01candidate-domain"
    );
    assert!(fs::read_dir(store.root()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with('.')
    }));
}

#[test]
fn reopening_store_rolls_back_an_uncommitted_published_pair() {
    let (_directory, store) = fixture();
    let prepared = match store
        .prepare(
            b"SRS\x01candidate-domain",
            b"SRS\x02candidate-ip",
            &FixtureChecker::new(true),
        )
        .unwrap()
    {
        RuleSetPreparation::Prepared(prepared) => prepared,
        RuleSetPreparation::Unchanged => panic!("fixture must produce a candidate"),
    };
    let _interrupted = store.publish(prepared).unwrap();

    let reopened = RuleSetStore::open(store.root(), RuleSetLimits::default()).unwrap();

    assert_eq!(
        fs::read(reopened.root().join("cn-domain.srs")).unwrap(),
        b"SRS\x01old-domain"
    );
    assert_eq!(
        fs::read(reopened.root().join("cn-ip.srs")).unwrap(),
        b"SRS\x02old-ip"
    );
    assert!(fs::read_dir(reopened.root()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with('.')
    }));
}

#[test]
fn opening_store_removes_only_known_stale_transaction_artifacts() {
    let (_directory, store) = fixture();
    fs::write(
        store.root().join(".previous-domain-1-1.srs"),
        b"SRS\x01stale",
    )
    .unwrap();
    fs::write(store.root().join(".previous-ip-1-1.srs"), b"SRS\x02stale").unwrap();
    let candidate = store.root().join(".candidate-ruleset-1-1");
    fs::create_dir(&candidate).unwrap();
    fs::write(candidate.join("unused"), b"stale").unwrap();

    let reopened = RuleSetStore::open(store.root(), RuleSetLimits::default()).unwrap();

    assert!(fs::read_dir(reopened.root()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with('.')
    }));
    assert_eq!(
        fs::read(reopened.root().join("cn-domain.srs")).unwrap(),
        b"SRS\x01old-domain"
    );
}

#[test]
fn prepared_transaction_cannot_be_published_by_another_store() {
    let (_directory, store) = fixture();
    let prepared = match store
        .prepare(
            b"SRS\x01candidate-domain",
            b"SRS\x02candidate-ip",
            &FixtureChecker::new(true),
        )
        .unwrap()
    {
        RuleSetPreparation::Prepared(prepared) => prepared,
        RuleSetPreparation::Unchanged => panic!("fixture must produce a candidate"),
    };
    let (_other_directory, other_store) = fixture();

    assert_eq!(
        other_store.publish(prepared).unwrap_err(),
        RuleSetError::ForeignTransaction
    );
    assert_eq!(
        fs::read(store.root().join("cn-domain.srs")).unwrap(),
        b"SRS\x01old-domain"
    );
}

#[test]
fn rejected_candidate_preserves_the_complete_previous_pair() {
    let (_directory, store) = fixture();
    let checker = FixtureChecker::new(false);

    assert_eq!(
        store
            .replace(b"SRS\x01rejected-domain", b"SRS\x02rejected-ip", &checker,)
            .unwrap_err(),
        RuleSetError::CheckFailed
    );
    assert_eq!(
        fs::read(store.root().join("cn-domain.srs")).unwrap(),
        b"SRS\x01old-domain"
    );
    assert_eq!(
        fs::read(store.root().join("cn-ip.srs")).unwrap(),
        b"SRS\x02old-ip"
    );
}

#[test]
fn malformed_or_oversized_input_is_rejected_before_external_check() {
    let (_directory, store) = fixture();
    let checker = FixtureChecker::new(true);
    assert_eq!(
        store.replace(b"not-srs", b"SRS\x02valid", &checker),
        Err(RuleSetError::InvalidCandidate)
    );

    let tiny_store = RuleSetStore::open(store.root(), RuleSetLimits::new(8).unwrap()).unwrap();
    assert_eq!(
        tiny_store.replace(b"SRS\x01too-large", b"SRS\x02valid", &checker),
        Err(RuleSetError::CandidateTooLarge)
    );
    assert_eq!(checker.calls.get(), 0);
}

#[cfg(unix)]
#[test]
fn symlink_current_target_is_rejected_without_touching_its_referent() {
    use std::os::unix::fs::symlink;

    let (_directory, store) = fixture();
    let external = store.root().parent().unwrap().join("external.srs");
    fs::write(&external, b"SRS\x01external").unwrap();
    fs::remove_file(store.root().join("cn-domain.srs")).unwrap();
    symlink(&external, store.root().join("cn-domain.srs")).unwrap();

    assert_eq!(
        store.replace(
            b"SRS\x01new-domain",
            b"SRS\x02new-ip",
            &FixtureChecker::new(true),
        ),
        Err(RuleSetError::InvalidCurrent)
    );
    assert_eq!(fs::read(external).unwrap(), b"SRS\x01external");
}
