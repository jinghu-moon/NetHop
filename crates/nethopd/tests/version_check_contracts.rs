use nethopd::{
    CoreReleaseBodyFetcher, CoreUpdateAvailability, CoreVersion, CoreVersionCheckError,
    CoreVersionChecker, CoreVersionStateSink, JsonCoreVersionStateStore, ReleaseMetadata,
};

struct FixedRelease(&'static [u8]);

impl CoreReleaseBodyFetcher for FixedRelease {
    fn fetch_release_body(&mut self) -> Result<Vec<u8>, CoreVersionCheckError> {
        Ok(self.0.to_vec())
    }
}

#[test]
fn stable_versions_are_strict_and_ordered_without_a_semver_runtime_dependency() {
    assert!(CoreVersion::parse("1.13.15").unwrap() < CoreVersion::parse("v1.13.16").unwrap());
    assert_eq!(CoreVersion::parse("v1.14.0").unwrap().to_string(), "1.14.0");

    for invalid in ["", "v", "1.13", "1.13.15.1", "1.13.16-beta.1", "1.x.0"] {
        assert_eq!(
            CoreVersion::parse(invalid).unwrap_err(),
            CoreVersionCheckError::InvalidVersion
        );
    }
}

#[test]
fn release_parser_accepts_only_bounded_official_stable_shape() {
    let release = ReleaseMetadata::parse(
        br#"{"tag_name":"v1.13.16","draft":false,"prerelease":false,"ignored":true}"#,
    )
    .unwrap();
    assert_eq!(release.version().to_string(), "1.13.16");

    for unstable in [
        br#"{"tag_name":"v1.14.0-beta.1","draft":false,"prerelease":true}"#.as_slice(),
        br#"{"tag_name":"v1.14.0","draft":true,"prerelease":false}"#.as_slice(),
    ] {
        assert_eq!(
            ReleaseMetadata::parse(unstable).unwrap_err(),
            CoreVersionCheckError::UnstableRelease
        );
    }
    assert_eq!(
        ReleaseMetadata::parse(&vec![b'x'; 256 * 1024 + 1]).unwrap_err(),
        CoreVersionCheckError::ResponseSize
    );
}

#[test]
fn checker_reports_only_strictly_newer_stable_versions() {
    let current = CoreVersion::parse("1.13.15").unwrap();
    let mut newer = CoreVersionChecker::new(
        FixedRelease(br#"{"tag_name":"v1.13.16","draft":false,"prerelease":false}"#),
        current,
    );
    let status = newer.check().unwrap();
    assert_eq!(status.current(), current);
    assert_eq!(status.latest().to_string(), "1.13.16");
    assert_eq!(status.availability(), CoreUpdateAvailability::Available);

    let mut same = CoreVersionChecker::new(
        FixedRelease(br#"{"tag_name":"v1.13.15","draft":false,"prerelease":false}"#),
        current,
    );
    assert_eq!(
        same.check().unwrap().availability(),
        CoreUpdateAvailability::UpToDate
    );
}

#[test]
fn runtime_state_store_atomically_preserves_unrelated_fields() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("runtime.json");
    std::fs::write(&path, br#"{"runtime":{"state":"running"}}"#).unwrap();
    let mut checker = CoreVersionChecker::new(
        FixedRelease(br#"{"tag_name":"v1.13.16","draft":false,"prerelease":false}"#),
        CoreVersion::parse("1.13.15").unwrap(),
    );
    let status = checker.check().unwrap();
    let mut store = JsonCoreVersionStateStore::new(&path).unwrap();
    store.persist(&status, "posted").unwrap();

    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(document["schema"], "nethop.runtime.v1");
    assert_eq!(document["runtime"]["state"], "running");
    assert_eq!(
        document["core_update"]["status"]["availability"],
        "available"
    );
    assert_eq!(document["core_update"]["notification"], "posted");

    let mut reopened = JsonCoreVersionStateStore::new(&path).unwrap();
    let (restored, last_notified) = reopened.restore().unwrap().unwrap();
    assert_eq!(restored.latest().to_string(), "1.13.16");
    assert_eq!(last_notified.unwrap().to_string(), "1.13.16");
}
