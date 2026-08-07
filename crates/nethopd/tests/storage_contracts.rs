use nethopd::{
    CounterBatch, CounterDeltaTracker, CounterName, CounterReading, StatsStore, StatsStoreError,
};
use tempfile::tempdir;

fn delta() -> nethopd::CounterDeltaBatch {
    let mut tracker = CounterDeltaTracker::default();
    tracker
        .apply(
            CounterBatch::new(
                "core-1",
                0,
                vec![CounterReading::new(
                    CounterName::terminal("node-a").unwrap(),
                    10,
                    20,
                )],
            )
            .unwrap(),
        )
        .unwrap();
    tracker
        .apply(
            CounterBatch::new(
                "core-1",
                1,
                vec![CounterReading::new(
                    CounterName::terminal("node-a").unwrap(),
                    25,
                    45,
                )],
            )
            .unwrap(),
        )
        .unwrap()
}

#[test]
fn store_uses_wal_private_file_and_accumulates_buckets() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("stats.sqlite");
    let mut store = StatsStore::open(&path).unwrap();
    let delta = delta();
    store.apply_delta(1_700_000_000, &delta).unwrap();
    store.apply_delta(1_700_000_000, &delta).unwrap();
    let bucket = store
        .bucket(1_700_000_000, "core-1", "terminal:node-a")
        .unwrap()
        .unwrap();
    assert_eq!(bucket.upload_bytes, 30);
    assert_eq!(bucket.download_bytes, 50);
    let totals = store.traffic_totals_since(1_699_999_999, 16).unwrap();
    assert_eq!(totals.len(), 1);
    assert_eq!(totals[0].counter_name, "terminal:node-a");
    assert_eq!((totals[0].upload_bytes, totals[0].download_bytes), (30, 50));
    assert!(matches!(
        store.traffic_totals_since(-1, 16),
        Err(StatsStoreError::InvalidBucket)
    ));
    #[cfg(unix)]
    {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    assert!(path.exists());
}

#[test]
fn invalid_bucket_and_overflow_fail_without_partial_rows() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("stats.sqlite");
    let mut store = StatsStore::open(&path).unwrap();
    let delta = delta();
    assert!(matches!(
        store.apply_delta(-1, &delta),
        Err(StatsStoreError::InvalidBucket)
    ));
    let mut tracker = CounterDeltaTracker::default();
    tracker
        .apply(CounterBatch::new("core-1", 0, Vec::new()).unwrap())
        .unwrap();
    let huge = tracker
        .apply(
            CounterBatch::new(
                "core-1",
                0,
                vec![CounterReading::new(
                    CounterName::terminal("huge").unwrap(),
                    u64::MAX,
                    0,
                )],
            )
            .unwrap(),
        )
        .unwrap();
    assert!(matches!(
        store.apply_delta(10, &huge),
        Err(StatsStoreError::BytesOutOfRange)
    ));
    assert!(
        store
            .bucket(10, "core-1", "terminal:huge")
            .unwrap()
            .is_none()
    );
}

#[test]
fn empty_baseline_delta_is_a_transactional_noop() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("stats.sqlite");
    let mut store = StatsStore::open(&path).unwrap();
    let mut tracker = CounterDeltaTracker::default();
    let baseline = tracker
        .apply(CounterBatch::new("core", 0, Vec::new()).unwrap())
        .unwrap();
    store.apply_delta(10, &baseline).unwrap();
    assert!(store.bucket(10, "core", "terminal:none").unwrap().is_none());
}
