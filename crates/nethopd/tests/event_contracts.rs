use nethop_protocol::{EventKind, RequestId};
use nethopd::EventHub;
use serde_json::json;
use tempfile::tempdir;

#[test]
fn subscription_starts_with_snapshot_and_filters_monotonic_events() {
    let hub = EventHub::new(json!({"kind":"snapshot","state":"direct"}), 4).unwrap();
    let mut subscription = hub
        .subscribe(
            RequestId::new("events-filter").unwrap(),
            &[EventKind::Config],
        )
        .unwrap();
    let snapshot = subscription.next_frame().unwrap();
    assert_eq!(snapshot.payload().unwrap()["kind"], "snapshot");

    hub.publish(EventKind::Runtime, json!({"kind":"runtime"}));
    hub.publish(
        EventKind::Config,
        json!({"kind":"config","state":"accepted"}),
    );
    let event = subscription.next_frame().unwrap();
    assert!(event.sequence() > snapshot.sequence());
    assert_eq!(event.payload().unwrap()["kind"], "config");
}

#[test]
fn slow_consumer_is_told_to_resync_then_receives_a_new_snapshot() {
    let hub = EventHub::new(json!({"kind":"snapshot","revision":1}), 2).unwrap();
    let mut subscription = hub
        .subscribe(RequestId::new("events-lag").unwrap(), &[])
        .unwrap();
    let first = subscription.next_frame().unwrap();
    for revision in 2..=5 {
        hub.replace_snapshot(json!({"kind":"snapshot","revision":revision}));
        hub.publish(
            EventKind::Config,
            json!({"kind":"config","revision":revision}),
        );
    }

    let resync = subscription.next_frame().unwrap();
    assert_eq!(resync.payload().unwrap()["kind"], "resync_required");
    let snapshot = subscription.next_frame().unwrap();
    assert!(snapshot.sequence() > resync.sequence());
    assert!(resync.sequence() > first.sequence());
    assert_eq!(snapshot.payload().unwrap()["revision"], 5);
}

#[test]
fn structured_history_is_bounded_newest_first_and_clear_reopens_the_log() {
    let directory = tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let hub = EventHub::new(json!({"kind":"snapshot"}), 8).unwrap();
    hub.install_file_log(&root).unwrap();
    hub.publish(EventKind::Runtime, json!({"kind":"first","token":"secret"}));
    hub.publish(EventKind::Network, json!({"kind":"second"}));

    let history = hub.structured_log_history(1).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0]["payload"]["kind"], "second");

    assert_eq!(hub.clear_structured_logs().unwrap(), 1);
    assert!(hub.structured_log_history(8).unwrap().is_empty());
    hub.publish(EventKind::Config, json!({"kind":"after-clear"}));
    assert_eq!(
        hub.structured_log_history(8).unwrap()[0]["payload"]["kind"],
        "after-clear"
    );
}

#[cfg(unix)]
#[test]
fn structured_log_clear_never_follows_symlinks() {
    use std::os::unix::fs::symlink;

    let directory = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let target = outside.path().join("outside.log");
    std::fs::write(&target, "keep").unwrap();
    symlink(&target, root.join("linked.log")).unwrap();
    let hub = EventHub::new(json!({"kind":"snapshot"}), 8).unwrap();
    hub.install_file_log(&root).unwrap();
    hub.clear_structured_logs().unwrap();
    assert_eq!(std::fs::read_to_string(target).unwrap(), "keep");
}

#[test]
fn subscriber_count_and_ring_capacity_are_bounded() {
    assert!(EventHub::new(json!({}), 0).is_err());
    let hub = EventHub::new(json!({"kind":"snapshot"}), 1).unwrap();
    let mut subscriptions = Vec::new();
    for index in 0..4 {
        subscriptions.push(
            hub.subscribe(RequestId::new(format!("event-{index}")).unwrap(), &[])
                .unwrap(),
        );
    }
    assert!(
        hub.subscribe(RequestId::new("event-overflow").unwrap(), &[])
            .is_err()
    );
    drop(subscriptions.pop());
    assert!(
        hub.subscribe(RequestId::new("event-recovered").unwrap(), &[])
            .is_ok()
    );
}

#[test]
fn file_event_log_is_private_bounded_jsonl_without_raw_config_values() {
    let directory = tempdir().unwrap();
    let hub = EventHub::new(json!({"kind":"snapshot"}), 4).unwrap();
    hub.install_file_log(directory.path()).unwrap();
    hub.publish(
        EventKind::Config,
        json!({"kind":"config","state":"accepted","active_config_digest":"abc"}),
    );

    let entries = std::fs::read_dir(directory.path())
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(entries.len(), 1);
    let path = entries[0].path();
    assert_eq!(
        path.extension().and_then(|value| value.to_str()),
        Some("log")
    );
    let line = std::fs::read_to_string(&path).unwrap();
    let record: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
    assert_eq!(record["seq"], 1);
    assert_eq!(record["kind"], "config");
    assert_eq!(record["payload"]["state"], "accepted");
    assert!(!line.contains("url"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o077,
            0
        );
    }
}
