use nethop_protocol::{
    BenchmarkControlTiming, BenchmarkEngineTiming, BenchmarkProbeSummary, BenchmarkReport,
    BenchmarkStatus, BenchmarkTerminalTiming, BenchmarkTrigger, ControlMethod, ControlParams,
    ControlRequest, EventKind, FastSelectionDeferredReason, NodeBenchmarkFastSelection,
    NodeBenchmarkOperationAck, NodeBenchmarkProgress, NodeBenchmarkSelection,
    NodeBenchmarkSelectionPhase, NodeBenchmarkTerminalReport, NodeProbeOutcome, NodeProbeState,
    PROTOCOL_VERSION, ProtocolError, RequestId, SubscriptionMode,
};
use serde_json::{Value, json};

const GOLDEN: &str =
    include_str!("../../../tests/subscription-selection/fixtures/protocol-v5-golden.json");

fn request(method: ControlMethod) -> ControlRequest {
    ControlRequest::new(RequestId::new("v3-contract").unwrap(), method)
}

#[test]
fn benchmark_ack_and_terminal_report_are_strict_bounded_dtos() {
    let ack: NodeBenchmarkOperationAck = serde_json::from_value(json!({
        "operation_id": format!("bench_{}", "1".repeat(29)),
        "phase": "running",
        "joined_existing": false,
        "trigger": "manual",
        "candidate_count": 2,
        "fast_selection_earliest_ms": 2000,
        "fast_selection_latest_ms": 2800,
        "fast_selection_deadline_ms": 3000,
        "probe_cutoff_ms": 4500,
        "deadline_ms": 4900,
        "fast_selection": { "state": "pending" }
    }))
    .unwrap();
    assert!(ack.validate().is_ok());
    assert!(
        serde_json::from_value::<NodeBenchmarkOperationAck>(json!({
            "operation_id": format!("bench_{}", "1".repeat(29)),
            "phase": "running",
            "joined_existing": false,
            "trigger": "manual",
            "candidate_count": 65,
            "fast_selection_earliest_ms": 2000,
            "fast_selection_latest_ms": 2800,
            "fast_selection_deadline_ms": 3000,
            "probe_cutoff_ms": 4500,
            "deadline_ms": 4900,
            "fast_selection": { "state": "pending" },
            "secret": "must-not-pass"
        }))
        .is_err()
    );

    let report = BenchmarkReport::from_outcomes(
        BenchmarkTrigger::Manual,
        7,
        1,
        100,
        vec![
            NodeProbeOutcome::success("nh1s-0123456789abcdef", 42).unwrap(),
            NodeProbeOutcome::failed("nh1s-fedcba9876543210", NodeProbeState::Timeout).unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(report.status, BenchmarkStatus::Partial);
    assert!(report.validate().is_ok());
    let mut inconsistent = report.clone();
    inconsistent.failed = 1;
    assert_eq!(inconsistent.validate(), Err(ProtocolError::InvalidEnvelope));
    assert!(NodeProbeOutcome::success("nh1s-0123456789abcdef", 65_536).is_err());
    assert!(NodeProbeOutcome::failed("nh1s-0123456789abcdef", NodeProbeState::Success).is_err());
}

#[test]
fn fast_selection_milestone_is_strict_and_bounded() {
    let selection = json!({
        "version": 2,
        "intent": { "mode": "auto" },
        "active_terminal": { "kind": "node", "node_id": "nh1s-0123456789abcdef" },
        "changed_at": 1
    });
    let milestone = NodeBenchmarkSelection {
        operation_id: format!("bench_{}", "1".repeat(29)),
        phase: NodeBenchmarkSelectionPhase::Selection,
        generation: 7,
        fast_selection: NodeBenchmarkFastSelection::Switched {
            completed: 43,
            candidate_count: 64,
            elapsed_us: 2_100_000,
            selection,
        },
    };
    milestone.validate().unwrap();

    let deferred = NodeBenchmarkFastSelection::Deferred {
        completed: 31,
        candidate_count: 64,
        elapsed_us: 2_800_000,
        reason: FastSelectionDeferredReason::InsufficientCoverage,
    };
    deferred.validate().unwrap();

    let mut late = milestone;
    if let NodeBenchmarkFastSelection::Switched { elapsed_us, .. } = &mut late.fast_selection {
        *elapsed_us = 3_000_001;
    }
    assert_eq!(late.validate(), Err(ProtocolError::InvalidEnvelope));
}

#[test]
fn benchmark_progress_is_strict_generation_bound_and_incremental() {
    let progress: NodeBenchmarkProgress = serde_json::from_value(json!({
        "operation_id": format!("bench_{}", "1".repeat(29)),
        "phase": "progress",
        "generation": 7,
        "completed": 1,
        "candidate_count": 2,
        "outcome": {
            "node_id": "nh1s-0123456789abcdef",
            "state": "success",
            "latency_ms": 42,
            "request_elapsed_us": 40_000,
            "completed_at_us": 41_000
        }
    }))
    .unwrap();
    progress.validate().unwrap();
    let mut inconsistent = progress;
    inconsistent.completed = 3;
    assert!(inconsistent.validate().is_err());
}

#[test]
fn benchmark_timings_are_microsecond_precise_bounded_and_conservative() {
    let report = BenchmarkReport::from_timed_outcomes(
        BenchmarkTrigger::Manual,
        7,
        0,
        100,
        BenchmarkEngineTiming {
            thread_spawn_us: 100,
            runtime_init_us: 200,
            candidate_dispatch_us: 300,
            probe_us: 98_900,
            result_assembly_us: 400,
            total_us: 100_000,
        },
        vec![NodeProbeOutcome::success("nh1s-0123456789abcdef", 42).unwrap()],
    )
    .unwrap();
    let terminal = NodeBenchmarkTerminalReport {
        operation_id: format!("bench_{}", "1".repeat(29)),
        phase: nethop_protocol::NodeBenchmarkCompletedPhase::Completed,
        report,
        selection: None,
        fast_selection: NodeBenchmarkFastSelection::Deferred {
            completed: 1,
            candidate_count: 1,
            elapsed_us: 2_800_000,
            reason: FastSelectionDeferredReason::CurrentPending,
        },
        timing: BenchmarkTerminalTiming {
            admission_us: 500,
            worker_reap_us: 250,
            fast_control: BenchmarkControlTiming::zero(),
            terminal_control: BenchmarkControlTiming {
                intent_load_us: 50,
                current_snapshot_us: 0,
                decision_us: 0,
                target_resolve_us: 0,
                selector_apply_us: 0,
                final_snapshot_us: 450,
                total_us: 600,
            },
            operation_total_us: 101_500,
        },
    };
    terminal.validate().unwrap();

    let mut inconsistent = terminal;
    inconsistent.timing.terminal_control.selector_apply_us = 1_000;
    assert_eq!(inconsistent.validate(), Err(ProtocolError::InvalidEnvelope));
}

#[test]
fn benchmark_probe_summary_explains_completed_results_and_cutoff_tail() {
    let outcomes = vec![
        NodeProbeOutcome::success("nh1s-0123456789abcdef", 42)
            .unwrap()
            .with_timing(80_000, 100_000)
            .unwrap(),
        NodeProbeOutcome::failed("nh1s-fedcba9876543210", NodeProbeState::Unavailable)
            .unwrap()
            .with_timing(700_000, 800_000)
            .unwrap(),
        NodeProbeOutcome::failed("nh1s-1111111111111111", NodeProbeState::Timeout)
            .unwrap()
            .with_timing(4_490_000, 4_500_000)
            .unwrap(),
    ];
    let summary = BenchmarkProbeSummary {
        first_result_us: Some(100_000),
        last_result_us: Some(800_000),
        last_success_us: Some(100_000),
        completed_within_500ms: 1,
        completed_within_1s: 2,
        completed_within_2s: 2,
        completed_within_3s: 2,
        completed_before_cutoff: 2,
        cutoff_pending: 1,
        cutoff_tail_us: 3_700_000,
    };
    let report = BenchmarkReport::from_timed_outcomes_with_probe_summary(
        BenchmarkTrigger::Manual,
        7,
        0,
        4_501,
        BenchmarkEngineTiming {
            thread_spawn_us: 100,
            runtime_init_us: 200,
            candidate_dispatch_us: 300,
            probe_us: 4_500_000,
            result_assembly_us: 100,
            total_us: 4_501_000,
        },
        summary,
        outcomes,
    )
    .unwrap();
    report.validate().unwrap();

    let mut inconsistent = report.clone();
    inconsistent.probe.completed_within_1s = 0;
    assert_eq!(inconsistent.validate(), Err(ProtocolError::InvalidEnvelope));

    let mut inconsistent = report;
    inconsistent.nodes[0].completed_at_us = 70_000;
    assert_eq!(inconsistent.validate(), Err(ProtocolError::InvalidEnvelope));
}

#[test]
fn benchmark_report_preserves_the_daemon_candidate_order() {
    let report = BenchmarkReport::from_outcomes(
        BenchmarkTrigger::Periodic,
        8,
        0,
        20,
        vec![
            NodeProbeOutcome::success("nh1s-fedcba9876543210", 40).unwrap(),
            NodeProbeOutcome::success("nh1s-0123456789abcdef", 20).unwrap(),
        ],
    )
    .unwrap();

    assert_eq!(report.nodes[0].node_id, "nh1s-fedcba9876543210");
    assert_eq!(report.nodes[1].node_id, "nh1s-0123456789abcdef");
}

#[test]
fn protocol_v5_selection_method_names_match_golden() {
    let golden: Value = serde_json::from_str(GOLDEN).unwrap();
    assert_eq!(PROTOCOL_VERSION, golden["version"]);
    let cases = [
        (ControlMethod::SubscriptionModeGet, "mode_get"),
        (ControlMethod::SubscriptionModeSet, "mode_set"),
        (ControlMethod::SubscriptionSelect, "subscription_select"),
        (
            ControlMethod::SubscriptionSetEnabled,
            "subscription_set_enabled",
        ),
        (ControlMethod::NodeSelectionGet, "node_selection_get"),
        (ControlMethod::NodeSelectAuto, "node_select_auto"),
        (ControlMethod::NodeSelectManual, "node_select_manual"),
        (ControlMethod::NodeTestAll, "node_test_all"),
        (
            ControlMethod::NodeTestOperationGet,
            "node_test_operation_get",
        ),
        (ControlMethod::NodeList, "node_list"),
    ];
    for (method, key) in cases {
        assert_eq!(
            serde_json::to_value(request(method)).unwrap()["method"],
            golden["methods"][key]
        );
    }
}

#[test]
fn subscription_transactions_require_exact_cas_and_mode_specific_params() {
    let digest = "a".repeat(64);
    let source = "src_0123456789abcdef0123456789abcdef".to_owned();
    assert!(
        request(ControlMethod::SubscriptionModeSet)
            .with_params(ControlParams::subscription_mode_set(
                digest.clone(),
                SubscriptionMode::Single,
                Some(source.clone()),
            ))
            .is_ok()
    );
    assert!(
        request(ControlMethod::SubscriptionModeSet)
            .with_params(ControlParams::subscription_mode_set(
                digest.clone(),
                SubscriptionMode::Merge,
                None,
            ))
            .is_ok()
    );
    assert_eq!(
        request(ControlMethod::SubscriptionModeSet)
            .with_params(ControlParams::subscription_mode_set(
                digest.clone(),
                SubscriptionMode::Single,
                None,
            ))
            .unwrap_err(),
        ProtocolError::InvalidEnvelope
    );
    assert!(
        request(ControlMethod::SubscriptionSetEnabled)
            .with_params(ControlParams::subscription_set_enabled(
                digest, source, true,
            ))
            .is_ok()
    );
}

#[test]
fn manual_selection_accepts_only_stable_ids_and_auto_has_no_target() {
    assert!(
        request(ControlMethod::NodeSelectManual)
            .with_params(ControlParams::target("nh1s-0123456789abcdef".to_owned()))
            .is_ok()
    );
    assert!(
        request(ControlMethod::NodeSelectManual)
            .with_params(ControlParams::target("internal-tag".to_owned()))
            .is_err()
    );
    assert!(
        request(ControlMethod::NodeSelectAuto)
            .with_params(ControlParams::default())
            .is_ok()
    );
    assert!(
        request(ControlMethod::NodeTestOperationGet)
            .with_params(ControlParams::target(format!("bench_{}", "1".repeat(29))))
            .is_ok()
    );
    assert!(
        request(ControlMethod::NodeTestOperationGet)
            .with_params(ControlParams::target("invalid-operation".to_owned()))
            .is_err()
    );
}

#[test]
fn selection_events_are_typed_and_secret_free() {
    let kinds = [
        EventKind::SubscriptionMode,
        EventKind::SubscriptionActiveSet,
        EventKind::NodeSelection,
        EventKind::NodeActive,
        EventKind::NodeTest,
    ];
    let value = json!({"kinds":kinds,"node_id":"nh1s-0123456789abcdef"});
    let text = serde_json::to_string(&value).unwrap();
    assert!(!text.contains("https://"));
    assert!(!text.contains("internal_tag"));
    assert!(!text.contains("password"));
}
