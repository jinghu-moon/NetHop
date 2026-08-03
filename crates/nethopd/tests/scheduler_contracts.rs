use nethopd::{
    InMemoryScheduleStore, ScheduleKey, SchedulePolicy, SchedulerEngine, SchedulerError,
};

#[test]
fn first_registration_is_due_and_success_schedules_next_day() {
    let mut scheduler =
        SchedulerEngine::load(InMemoryScheduleStore::default(), SchedulePolicy::default()).unwrap();
    let key = ScheduleKey::new("subscription:main").unwrap();
    scheduler.ensure(key.clone(), 1_700_000_000).unwrap();
    assert_eq!(
        scheduler.due(1_700_000_000).unwrap().as_slice(),
        std::slice::from_ref(&key)
    );
    scheduler.mark_success(&key, 1_700_000_000).unwrap();
    assert!(scheduler.due(1_700_000_001).unwrap().is_empty());
    let record = scheduler.record(&key).unwrap();
    assert_eq!(record.failure_count(), 0);
    assert!(record.next_run_wall_seconds() > 1_700_000_000 + 23 * 60 * 60);
}

#[test]
fn failures_use_exponential_backoff_and_cap_failure_count() {
    let policy = SchedulePolicy::new(86_400, 60, 300, 0).unwrap();
    let mut scheduler = SchedulerEngine::load(InMemoryScheduleStore::default(), policy).unwrap();
    let key = ScheduleKey::new("rules:cn").unwrap();
    scheduler.ensure(key.clone(), 100).unwrap();
    scheduler.mark_failure(&key, 100).unwrap();
    assert_eq!(scheduler.record(&key).unwrap().next_run_wall_seconds(), 160);
    scheduler.mark_failure(&key, 160).unwrap();
    assert_eq!(scheduler.record(&key).unwrap().next_run_wall_seconds(), 280);
    for now in [
        280, 580, 880, 1_180, 1_480, 1_780, 2_080, 2_380, 2_680, 2_980, 3_280, 3_580, 3_880, 4_180,
    ] {
        scheduler.mark_failure(&key, now).unwrap();
    }
    assert_eq!(scheduler.record(&key).unwrap().failure_count(), 16);
}

#[test]
fn jitter_is_stable_for_same_key_and_spreads_different_keys() {
    let policy = SchedulePolicy::default();
    let mut left = SchedulerEngine::load(InMemoryScheduleStore::default(), policy).unwrap();
    let mut right = SchedulerEngine::load(InMemoryScheduleStore::default(), policy).unwrap();
    let left_key = ScheduleKey::new("subscription:left").unwrap();
    let right_key = ScheduleKey::new("subscription:right").unwrap();
    left.ensure(left_key.clone(), 1_700_000_000).unwrap();
    right.ensure(right_key.clone(), 1_700_000_000).unwrap();
    left.mark_success(&left_key, 1_700_000_000).unwrap();
    right.mark_success(&right_key, 1_700_000_000).unwrap();
    let mut repeat = SchedulerEngine::load(InMemoryScheduleStore::default(), policy).unwrap();
    repeat.ensure(left_key.clone(), 1_700_000_000).unwrap();
    repeat.mark_success(&left_key, 1_700_000_000).unwrap();
    assert_eq!(
        left.record(&left_key).unwrap().next_run_wall_seconds(),
        repeat.record(&left_key).unwrap().next_run_wall_seconds()
    );
    assert_ne!(
        left.record(&left_key).unwrap().next_run_wall_seconds(),
        right.record(&right_key).unwrap().next_run_wall_seconds()
    );
}

#[test]
fn clock_regression_and_invalid_inputs_fail_closed() {
    let mut scheduler =
        SchedulerEngine::load(InMemoryScheduleStore::default(), SchedulePolicy::default()).unwrap();
    let key = ScheduleKey::new("version").unwrap();
    scheduler.ensure(key.clone(), 100).unwrap();
    scheduler.due(100).unwrap();
    assert_eq!(
        scheduler.due(99).unwrap_err(),
        SchedulerError::ClockRegressed
    );
    assert_eq!(
        ScheduleKey::new("\n").unwrap_err(),
        SchedulerError::InvalidKey
    );
    assert_eq!(
        scheduler.ensure(key, -1).unwrap_err(),
        SchedulerError::InvalidWallTime
    );
}
