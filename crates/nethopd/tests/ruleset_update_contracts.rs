#![cfg(feature = "subscription-update")]

use std::{cell::Cell, collections::BTreeMap, fs, path::Path};

use nethopd::{
    CandidateChecker, RuleSetBodyFetcher, RuleSetFetchError, RuleSetLimits, RuleSetProvider,
    RuleSetProviderManifest, RuleSetReplaceOutcome, RuleSetStore, RuleSetUpdateError,
    RuleSetUpdatePreparation, RuleSetUpdateService, RunnerError, RuntimeRuleSetUpdateSource,
};

struct FixtureFetcher {
    bodies: BTreeMap<String, Result<Vec<u8>, RuleSetFetchError>>,
    calls: Vec<String>,
}

impl RuleSetBodyFetcher for FixtureFetcher {
    fn fetch(&mut self, provider: &RuleSetProvider) -> Result<Vec<u8>, RuleSetFetchError> {
        self.calls.push(provider.id().to_owned());
        self.bodies.remove(provider.id()).unwrap()
    }
}

struct FixtureChecker(Cell<usize>);

impl CandidateChecker for FixtureChecker {
    fn check(&self, _config_path: &Path) -> Result<(), RunnerError> {
        self.0.set(self.0.get() + 1);
        Ok(())
    }
}

fn store_fixture() -> (tempfile::TempDir, RuleSetStore) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("rulesets");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("cn-domain.srs"), b"SRS\x01old-domain").unwrap();
    fs::write(root.join("cn-ip.srs"), b"SRS\x02old-ip").unwrap();
    let store = RuleSetStore::open(root, RuleSetLimits::default()).unwrap();
    (directory, store)
}

#[test]
fn updater_fetches_the_closed_pair_then_runs_one_admission_transaction() {
    let (_directory, store) = store_fixture();
    let root = store.root().to_path_buf();
    let fetcher = FixtureFetcher {
        bodies: BTreeMap::from([
            ("cn-domain".into(), Ok(b"SRS\x01new-domain".to_vec())),
            ("cn-ip".into(), Ok(b"SRS\x02new-ip".to_vec())),
        ]),
        calls: Vec::new(),
    };
    let mut service = RuleSetUpdateService::new(
        store,
        fetcher,
        FixtureChecker(Cell::new(0)),
        RuleSetProviderManifest::bundled().unwrap().clone(),
    );

    assert_eq!(service.update().unwrap(), RuleSetReplaceOutcome::Updated);
    assert_eq!(service.fetcher().calls, ["cn-domain", "cn-ip"]);
    assert_eq!(service.checker().0.get(), 1);
    assert_eq!(
        fs::read(root.join("cn-domain.srs")).unwrap(),
        b"SRS\x01new-domain"
    );
    assert_eq!(fs::read(root.join("cn-ip.srs")).unwrap(), b"SRS\x02new-ip");
}

#[test]
fn second_fetch_failure_never_publishes_a_partial_pair() {
    let (_directory, store) = store_fixture();
    let root = store.root().to_path_buf();
    let fetcher = FixtureFetcher {
        bodies: BTreeMap::from([
            ("cn-domain".into(), Ok(b"SRS\x01new-domain".to_vec())),
            ("cn-ip".into(), Err(RuleSetFetchError::Fetch)),
        ]),
        calls: Vec::new(),
    };
    let mut service = RuleSetUpdateService::new(
        store,
        fetcher,
        FixtureChecker(Cell::new(0)),
        RuleSetProviderManifest::bundled().unwrap().clone(),
    );

    assert_eq!(service.update(), Err(RuleSetUpdateError::Fetch));
    assert_eq!(
        fs::read(root.join("cn-domain.srs")).unwrap(),
        b"SRS\x01old-domain"
    );
    assert_eq!(fs::read(root.join("cn-ip.srs")).unwrap(), b"SRS\x02old-ip");
}

#[test]
fn unchanged_downloaded_pair_does_not_run_sing_box_check() {
    let (_directory, store) = store_fixture();
    let fetcher = FixtureFetcher {
        bodies: BTreeMap::from([
            ("cn-domain".into(), Ok(b"SRS\x01old-domain".to_vec())),
            ("cn-ip".into(), Ok(b"SRS\x02old-ip".to_vec())),
        ]),
        calls: Vec::new(),
    };
    let mut service = RuleSetUpdateService::new(
        store,
        fetcher,
        FixtureChecker(Cell::new(0)),
        RuleSetProviderManifest::bundled().unwrap().clone(),
    );

    assert_eq!(service.update().unwrap(), RuleSetReplaceOutcome::Unchanged);
    assert_eq!(service.checker().0.get(), 0);
}

#[test]
fn published_update_remains_rollbackable_until_commit() {
    let (_directory, store) = store_fixture();
    let root = store.root().to_path_buf();
    let fetcher = FixtureFetcher {
        bodies: BTreeMap::from([
            ("cn-domain".into(), Ok(b"SRS\x01new-domain".to_vec())),
            ("cn-ip".into(), Ok(b"SRS\x02new-ip".to_vec())),
        ]),
        calls: Vec::new(),
    };
    let mut service = RuleSetUpdateService::new(
        store,
        fetcher,
        FixtureChecker(Cell::new(0)),
        RuleSetProviderManifest::bundled().unwrap().clone(),
    );

    assert_eq!(
        service.prepare_update().unwrap(),
        RuleSetUpdatePreparation::Prepared
    );
    service.publish_update().unwrap();
    assert_eq!(
        fs::read(root.join("cn-domain.srs")).unwrap(),
        b"SRS\x01new-domain"
    );

    service.rollback_update().unwrap();
    assert_eq!(
        fs::read(root.join("cn-domain.srs")).unwrap(),
        b"SRS\x01old-domain"
    );
    assert_eq!(fs::read(root.join("cn-ip.srs")).unwrap(), b"SRS\x02old-ip");
}

#[test]
fn transaction_methods_reject_out_of_order_calls() {
    let (_directory, store) = store_fixture();
    let mut service = RuleSetUpdateService::new(
        store,
        FixtureFetcher {
            bodies: BTreeMap::new(),
            calls: Vec::new(),
        },
        FixtureChecker(Cell::new(0)),
        RuleSetProviderManifest::bundled().unwrap().clone(),
    );

    assert_eq!(
        service.publish_update(),
        Err(RuleSetUpdateError::InvalidState)
    );
    assert_eq!(
        service.commit_update(),
        Err(RuleSetUpdateError::InvalidState)
    );
    assert_eq!(
        service.rollback_update(),
        Err(RuleSetUpdateError::InvalidState)
    );
}
