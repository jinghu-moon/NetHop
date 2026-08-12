use std::{collections::VecDeque, time::Duration};

use nethop_android::{IpFamily, NetworkAction, NetworkChange, NetworkEvent};
#[cfg(feature = "subscription-update")]
use nethop_core::{CaptureMode, CapturePolicy, ClashApi, ManagedOptions, TunStack};
use nethop_core::{GenerationId, RuntimeState};
use nethop_protocol::{ControlMethod, ControlRequest, RequestId};
#[cfg(feature = "subscription-update")]
use nethop_subscription::{
    CapabilityMatrix, FormatHint, ParserLimits, SourceId, SourceInput, convert_stable_sources,
};
#[cfg(feature = "subscription-update")]
use nethopd::build_candidate;
use nethopd::{
    ControlCommand, ControlRequestHandler, ControlSnapshot, CounterBatch, CounterName,
    CounterReading, CounterTransport, EventReconcileGate, ScheduleKey, SchedulePolicy,
    ScheduleStore, SchedulerEngine, StatsCollector, StatsError, StatsStore, UpdateStatus,
    WorkerControlHandler,
};
use tempfile::tempdir;

#[cfg(feature = "subscription-update")]
fn capture() -> CapturePolicy {
    CapturePolicy::new(
        CaptureMode::Tproxy,
        true,
        Some(12_345),
        Some(0x20_000),
        Vec::new(),
        Vec::new(),
    )
    .unwrap()
}

#[test]
#[cfg(feature = "subscription-update")]
fn stable_parser_output_builds_a_managed_generation_candidate() {
    let source_id = SourceId::new("src_11111111111111111111111111111111").unwrap();
    let conversion = convert_stable_sources(
        vec![SourceInput {
            source_id: source_id.clone(),
            format_hint: FormatHint::UriList,
            bytes: b"trojan://secret@example.com:443#node-a".to_vec(),
        }],
        &ParserLimits::default(),
        &CapabilityMatrix::default(),
    );
    let candidate = build_candidate(
        GenerationId::new(7).unwrap(),
        &conversion,
        nethopd::CandidateBuildProfile::new(
            capture(),
            ClashApi::new("127.0.0.1:9090", "x".repeat(32)).unwrap(),
            TunStack::System,
            ManagedOptions::default(),
        ),
        nethopd::SubscriptionMode::Single,
        &[source_id],
    )
    .unwrap();

    assert_eq!(candidate.generation().get(), 7);
    let config: serde_json::Value = serde_json::from_slice(candidate.config().bytes()).unwrap();
    assert_eq!(config["inbounds"][0]["type"], "tproxy");
    assert!(
        config["outbounds"]
            .as_array()
            .unwrap()
            .iter()
            .any(|outbound| outbound["type"] == "trojan")
    );
    assert_eq!(config["route"]["final"], "nethop-select");
}

#[test]
fn netlink_events_are_coalesced_into_one_reconcile_intent() {
    let mut gate = EventReconcileGate::default();
    gate.observe(
        Duration::ZERO,
        NetworkEvent::new(NetworkAction::Upsert, NetworkChange::Link),
    )
    .unwrap();
    gate.observe(
        Duration::from_millis(100),
        NetworkEvent::new(
            NetworkAction::Upsert,
            NetworkChange::Address(IpFamily::Ipv6),
        ),
    )
    .unwrap();
    assert_eq!(gate.take_ready(Duration::from_millis(349)).unwrap(), None);
    assert_eq!(
        gate.take_ready(Duration::from_millis(350)).unwrap(),
        Some(2)
    );
    assert_eq!(gate.take_ready(Duration::from_millis(351)).unwrap(), None);
}

#[derive(Debug)]
struct FakeCounterTransport(VecDeque<CounterBatch>);

impl CounterTransport for FakeCounterTransport {
    fn read_counters(&mut self) -> Result<CounterBatch, StatsError> {
        self.0.pop_front().ok_or(StatsError::TransportUnavailable)
    }
}

#[test]
fn counter_transport_delta_and_sqlite_store_form_one_collection_pipeline() {
    let counter = CounterName::terminal("node-a").unwrap();
    let transport = FakeCounterTransport(VecDeque::from([
        CounterBatch::new(
            "core-1",
            0,
            vec![CounterReading::new(counter.clone(), 100, 200)],
        )
        .unwrap(),
        CounterBatch::new("core-1", 1, vec![CounterReading::new(counter, 140, 260)]).unwrap(),
    ]));
    let directory = tempdir().unwrap();
    let mut store = StatsStore::open(directory.path().join("stats.sqlite")).unwrap();
    let mut collector = StatsCollector::new(transport);
    assert!(!collector.collect(60, &mut store).unwrap());
    assert!(collector.collect(120, &mut store).unwrap());
    let bucket = store
        .bucket(120, "core-1", "terminal:node-a")
        .unwrap()
        .unwrap();
    assert_eq!((bucket.upload_bytes, bucket.download_bytes), (40, 60));
}

#[test]
fn scheduler_state_survives_store_reopen() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("stats.sqlite");
    let key = ScheduleKey::new("subscription:main").unwrap();
    {
        let store = StatsStore::open(&path).unwrap();
        let mut scheduler = SchedulerEngine::load(store, SchedulePolicy::default()).unwrap();
        scheduler.ensure(key.clone(), 1_700_000_000).unwrap();
        scheduler.mark_success(&key, 1_700_000_000).unwrap();
    }
    let mut reopened = StatsStore::open(&path).unwrap();
    let records = reopened.load().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].key(), &key);
    assert!(records[0].next_run_wall_seconds() > 1_700_000_000);
}

#[test]
fn control_handler_exposes_snapshot_and_queues_only_typed_commands() {
    let mut handler = WorkerControlHandler::new(ControlSnapshot {
        state: RuntimeState::RunningTproxy,
        generation: Some(GenerationId::new(8).unwrap()),
        last_update: UpdateStatus::Never,
    });
    let status = handler.handle(ControlRequest::new(
        RequestId::new("status").unwrap(),
        ControlMethod::StatusGet,
    ));
    assert!(status.ok());
    assert_eq!(status.generation(), Some(8));
    assert_eq!(status.result().unwrap()["state"], "running_tproxy");
    assert_eq!(status.result().unwrap()["last_update"], "never");
    assert!(handler.take_command().is_none());

    let start = handler.handle(ControlRequest::new(
        RequestId::new("start").unwrap(),
        ControlMethod::ServiceStart,
    ));
    assert!(start.ok());
    assert_eq!(handler.take_command(), Some(ControlCommand::Start));
}

#[test]
fn stop_preempts_pending_start_and_update_without_dropping_probe() {
    let snapshot = ControlSnapshot {
        state: RuntimeState::FailOpenDirect,
        generation: None,
        last_update: UpdateStatus::Never,
    };
    let mut handler = WorkerControlHandler::new(snapshot).with_update_available();
    handler.queue_command(ControlCommand::Start);
    handler.queue_command(ControlCommand::Update);
    handler.queue_command(ControlCommand::Probe);
    handler.queue_command(ControlCommand::Stop);

    assert_eq!(handler.take_command(), Some(ControlCommand::Stop));
    assert_eq!(handler.take_command(), Some(ControlCommand::Probe));
    assert_eq!(handler.take_command(), None);
}

#[test]
fn update_command_is_rejected_until_an_updater_is_injected() {
    let snapshot = ControlSnapshot {
        state: RuntimeState::FailOpenDirect,
        generation: None,
        last_update: UpdateStatus::Never,
    };
    let request = || {
        ControlRequest::new(
            RequestId::new("update").unwrap(),
            ControlMethod::SubscriptionUpdate,
        )
    };
    let mut unavailable = WorkerControlHandler::new(snapshot);
    let rejected = unavailable.handle(request());
    assert!(!rejected.ok());
    assert_eq!(
        rejected.error().unwrap().code().as_str(),
        "NH-SUB-UPDATE-UNAVAILABLE"
    );
    assert!(unavailable.take_command().is_none());

    let mut available = WorkerControlHandler::new(snapshot).with_update_available();
    let accepted = available.handle(request());
    assert!(accepted.ok());
    assert_eq!(available.take_command(), Some(ControlCommand::Update));
}
