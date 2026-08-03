use nethopd::{
    CounterBatch, CounterDeltaTracker, CounterName, CounterReading, CounterTransport, StatsError,
};

fn batch(core: &str, degraded: u64, upload: u64, download: u64) -> CounterBatch {
    CounterBatch::new(
        core,
        degraded,
        vec![CounterReading::new(
            CounterName::terminal("node-a").unwrap(),
            upload,
            download,
        )],
    )
    .unwrap()
}

#[test]
fn first_snapshot_and_core_restart_only_establish_baselines() {
    let mut tracker = CounterDeltaTracker::default();
    let first = tracker.apply(batch("core-1", 2, 100, 200)).unwrap();
    assert!(first.baseline_only());
    assert!(first.counters().is_empty());

    let delta = tracker.apply(batch("core-1", 3, 140, 260)).unwrap();
    assert!(!delta.baseline_only());
    assert_eq!(delta.attribution_degraded_delta(), 1);
    assert_eq!(delta.counters()[0].upload_bytes(), 40);
    assert_eq!(delta.counters()[0].download_bytes(), 60);

    let restarted = tracker.apply(batch("core-2", 0, 5, 7)).unwrap();
    assert!(restarted.baseline_only());
    assert!(restarted.counters().is_empty());
}

#[test]
fn same_core_counter_regression_does_not_replace_last_good_baseline() {
    let mut tracker = CounterDeltaTracker::default();
    tracker.apply(batch("core", 0, 100, 100)).unwrap();
    assert_eq!(
        tracker.apply(batch("core", 0, 90, 110)).unwrap_err(),
        StatsError::CounterRegressed
    );
    let delta = tracker.apply(batch("core", 0, 120, 130)).unwrap();
    assert_eq!(delta.counters()[0].upload_bytes(), 20);
    assert_eq!(delta.counters()[0].download_bytes(), 30);
}

#[test]
fn duplicate_and_disappearing_counters_fail_closed() {
    let terminal = CounterName::terminal("node").unwrap();
    assert_eq!(
        CounterBatch::new(
            "core",
            0,
            vec![
                CounterReading::new(terminal.clone(), 1, 1),
                CounterReading::new(terminal, 2, 2),
            ],
        )
        .unwrap_err(),
        StatsError::DuplicateCounter
    );

    let mut tracker = CounterDeltaTracker::default();
    tracker.apply(batch("core", 0, 1, 1)).unwrap();
    let empty = CounterBatch::new("core", 0, Vec::new()).unwrap();
    assert_eq!(
        tracker.apply(empty).unwrap_err(),
        StatsError::CounterMissing
    );
}

#[test]
fn counter_names_and_instance_ids_are_bounded_and_transport_is_abstract() {
    assert_eq!(
        CounterName::terminal("x".repeat(129)).unwrap_err(),
        StatsError::InvalidCounter
    );
    assert_eq!(
        CounterBatch::new("!", 0, Vec::new()).unwrap_err(),
        StatsError::InvalidCoreInstance
    );

    struct FakeTransport(Option<CounterBatch>);
    impl CounterTransport for FakeTransport {
        fn read_counters(&mut self) -> Result<CounterBatch, StatsError> {
            self.0.take().ok_or(StatsError::TransportUnavailable)
        }
    }
    let mut transport = FakeTransport(Some(batch("core", 0, 1, 2)));
    assert_eq!(
        transport.read_counters().unwrap().core_instance_id(),
        "core"
    );
}
