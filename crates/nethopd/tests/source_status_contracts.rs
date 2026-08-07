#![cfg(feature = "subscription-update")]

use nethop_core::GenerationId;
use nethop_subscription::SourceId;
use nethopd::{
    InMemoryScheduleStore, ScheduleKey, SchedulePolicy, ScheduleStore, SchedulerEngine,
    SourceBodyOrigin, SourceHealth, SourceStatusStore, SourceUpdateDetail, SourceUpdateReport,
    StatsStore,
};
use tempfile::tempdir;

fn detail(source_id: &SourceId, origin: SourceBodyOrigin) -> SourceUpdateDetail {
    SourceUpdateDetail {
        source_id: source_id.clone(),
        origin: Some(origin),
        accepted: 2,
        duplicate: 1,
        rejected: 3,
        warnings: 4,
        diagnostic_code: None,
    }
}

fn report(source_id: &SourceId, origin: SourceBodyOrigin) -> SourceUpdateReport {
    SourceUpdateReport {
        generation: GenerationId::new(7).unwrap(),
        source_count: 1,
        accepted: 2,
        duplicate: 1,
        node_count: 2,
        sources: vec![detail(source_id, origin)],
    }
}

#[test]
fn source_status_persists_counts_and_joins_the_next_schedule() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nethop.db");
    let source_id = SourceId::new("src_01010101010101010101010101010101").unwrap();
    let key = ScheduleKey::new(source_id.as_str()).unwrap();
    let mut engine =
        SchedulerEngine::load(InMemoryScheduleStore::default(), SchedulePolicy::default()).unwrap();
    engine.ensure(key.clone(), 1_000).unwrap();
    let record = engine.record(&key).unwrap().clone();
    let expected_next_update = record.next_run_wall_seconds();
    let mut schedule = StatsStore::open(&path).unwrap();
    ScheduleStore::save(&mut schedule, &record).unwrap();

    let mut store = SourceStatusStore::open(&path).unwrap();
    store
        .record_report(1_500, &report(&source_id, SourceBodyOrigin::Fresh))
        .unwrap();
    let statuses = store.statuses([source_id.as_str()]).unwrap();
    let status = &statuses[0];
    assert_eq!(status.health, SourceHealth::Healthy);
    assert_eq!(status.last_attempt_wall_seconds, Some(1_500));
    assert_eq!(status.last_success_wall_seconds, Some(1_500));
    assert_eq!(status.next_update_wall_seconds, Some(expected_next_update));
    assert_eq!((status.accepted, status.duplicate), (2, 1));
    assert_eq!(status.generation, Some(7));
}

#[test]
fn last_known_good_and_failures_do_not_advance_last_success() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nethop.db");
    let source_id = SourceId::new("src_02020202020202020202020202020202").unwrap();
    let mut store = SourceStatusStore::open(&path).unwrap();
    store
        .record_report(100, &report(&source_id, SourceBodyOrigin::Fresh))
        .unwrap();
    store
        .record_report(200, &report(&source_id, SourceBodyOrigin::LastKnownGood))
        .unwrap();
    let degraded = store.statuses([source_id.as_str()]).unwrap().remove(0);
    assert_eq!(degraded.health, SourceHealth::Degraded);
    assert_eq!(degraded.last_success_wall_seconds, Some(100));
    assert!(degraded.using_last_known_good);

    store
        .record_failure(300, [source_id.as_str()], "fetch_failed")
        .unwrap();
    let failed = store.statuses([source_id.as_str()]).unwrap().remove(0);
    assert_eq!(failed.health, SourceHealth::Failed);
    assert_eq!(failed.last_success_wall_seconds, Some(100));
    assert_eq!(failed.diagnostic_code.as_deref(), Some("fetch_failed"));
}

#[test]
fn unknown_source_has_an_explicit_never_state_without_fake_timestamps() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nethop.db");
    let source_id = "src_03030303030303030303030303030303";
    let store = SourceStatusStore::open(&path).unwrap();

    let status = store.statuses([source_id]).unwrap().remove(0);
    assert_eq!(status.health, SourceHealth::Never);
    assert_eq!(status.last_attempt_wall_seconds, None);
    assert_eq!(status.last_success_wall_seconds, None);
}
