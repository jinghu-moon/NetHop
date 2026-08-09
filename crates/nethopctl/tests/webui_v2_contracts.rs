use nethop_protocol::{
    ControlMethod, EventKind, RequestId, WebUiPayloadNamespace, WebUiPayloadOperation,
};
use nethopctl::{
    CliCommand, build_request, matches_event_session, parse_event_termination, parse_invocation,
};

const HANDLE: &str = "p_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[test]
fn traffic_is_accepted_by_the_single_event_kind_parser() {
    let invocation = parse_invocation(["events", "--kinds", "runtime,traffic"]).unwrap();
    let request = build_request(&invocation, RequestId::new("events-v2").unwrap(), None).unwrap();
    assert_eq!(request.method(), ControlMethod::EventsSubscribe);
    assert_eq!(
        request.params().event_kinds().unwrap(),
        [EventKind::Runtime, EventKind::Traffic]
    );
}

#[test]
fn webui_event_session_is_bounded_and_termination_is_exact() {
    let session = "evt_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let invocation = parse_invocation([
        "events",
        "--jsonl",
        "--kinds",
        "runtime,traffic",
        "--session-id",
        session,
        "--max-runtime-seconds",
        "300",
    ])
    .unwrap();
    assert_eq!(invocation.event_session_id(), Some(session));
    assert_eq!(invocation.event_max_runtime_seconds(), Some(300));
    assert!(parse_invocation(["events", "--session-id", "../bad"]).is_err());
    assert!(parse_invocation(["events", "--max-runtime-seconds", "1"]).is_err());

    assert_eq!(
        parse_event_termination(["webui", "events", "terminate", session, "--json"])
            .unwrap()
            .as_deref(),
        Some(session)
    );
    assert!(
        parse_event_termination([
            "webui",
            "events",
            "terminate",
            "evt_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "--json"
        ])
        .is_err()
    );
    assert!(matches_event_session(
        &[
            "/data/adb/modules/nethop/bin/nethopctl",
            "events",
            "--session-id",
            session
        ],
        session
    ));
    assert!(!matches_event_session(
        &[
            "/data/adb/modules/nethop/bin/nethopctl",
            "events",
            "--session-id",
            "evt_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        ],
        session
    ));
    assert!(!matches_event_session(
        &[
            "/data/adb/modules/nethop/bin/nethopctl",
            "status",
            "--session-id",
            session
        ],
        session
    ));
}

#[test]
fn webui_payload_commands_map_to_typed_protocol_params() {
    let cases = [
        (
            vec!["webui", "payload", "create", "config"],
            CliCommand::WebUiPayloadCreate,
            ControlMethod::WebUiPayloadCreate,
        ),
        (
            vec!["webui", "payload", "append", "subscription", HANDLE, "e30="],
            CliCommand::WebUiPayloadAppend,
            ControlMethod::WebUiPayloadAppend,
        ),
        (
            vec![
                "webui",
                "payload",
                "commit",
                "backup",
                HANDLE,
                "backup-restore",
            ],
            CliCommand::WebUiPayloadCommit,
            ControlMethod::WebUiPayloadCommit,
        ),
        (
            vec!["webui", "payload", "remove", "config", HANDLE],
            CliCommand::WebUiPayloadRemove,
            ControlMethod::WebUiPayloadRemove,
        ),
    ];
    for (args, command, method) in cases {
        let invocation = parse_invocation(args).unwrap();
        assert_eq!(invocation.command(), command);
        let request =
            build_request(&invocation, RequestId::new("payload-v2").unwrap(), None).unwrap();
        assert_eq!(request.method(), method);
    }

    let append =
        parse_invocation(["webui", "payload", "append", "subscription", HANDLE, "e30="]).unwrap();
    let append = build_request(&append, RequestId::new("append-v2").unwrap(), None).unwrap();
    assert_eq!(
        append.params().payload_namespace(),
        Some(WebUiPayloadNamespace::Subscription)
    );
    assert_eq!(append.params().payload_handle(), Some(HANDLE));
    assert_eq!(append.params().payload_chunk(), Some("e30="));

    let commit = parse_invocation([
        "webui",
        "payload",
        "commit",
        "backup",
        HANDLE,
        "backup-restore",
    ])
    .unwrap();
    let commit = build_request(&commit, RequestId::new("commit-v2").unwrap(), None).unwrap();
    assert_eq!(
        commit.params().payload_operation(),
        Some(WebUiPayloadOperation::BackupRestore)
    );
}

#[test]
fn webui_payload_cli_accepts_only_the_config_mutate_allowlist_name() {
    let invocation = parse_invocation([
        "webui",
        "payload",
        "commit",
        "subscription",
        "p_0123456789abcdef0123456789abcdef",
        "config-mutate",
        "--json",
    ])
    .unwrap();
    let request =
        build_request(&invocation, RequestId::new("config-mutate").unwrap(), None).unwrap();
    assert_eq!(
        request.params().payload_operation(),
        Some(WebUiPayloadOperation::ConfigMutate)
    );
}

#[test]
fn webui_payload_cli_rejects_arbitrary_namespaces_handles_and_operations() {
    for (index, args) in [
        vec!["webui", "payload", "create", "../config"],
        vec!["webui", "payload", "remove", "config", "../outside"],
        vec!["webui", "payload", "commit", "config", HANDLE, "shell"],
    ]
    .into_iter()
    .enumerate()
    {
        let invocation = parse_invocation(args).unwrap();
        assert!(
            build_request(
                &invocation,
                RequestId::new(format!("invalid-{index}")).unwrap(),
                None,
            )
            .is_err()
        );
    }
    assert!(parse_invocation(["webui", "payload", "append", "config", HANDLE]).is_err());
}
