use std::collections::BTreeSet;

use nethop_protocol::{ControlResponse, PROTOCOL_VERSION};
use nethopctl::{CliCommand, parse_command};
use serde_json::Value;

const FIXTURE: &str = include_str!("../../../tests/webui/fixtures/cli-v1-before.json");

fn command_name(command: CliCommand) -> &'static str {
    match command {
        CliCommand::Status => "Status",
        CliCommand::Start => "Start",
        CliCommand::Stop => "Stop",
        CliCommand::ServiceCheck => "ServiceCheck",
        CliCommand::ServiceRestart => "ServiceRestart",
        CliCommand::ServiceReload => "ServiceReload",
        CliCommand::CaptureEnable => "CaptureEnable",
        CliCommand::CaptureDisable => "CaptureDisable",
        CliCommand::CaptureStatus => "CaptureStatus",
        CliCommand::CoreStart => "CoreStart",
        CliCommand::CoreStop => "CoreStop",
        CliCommand::CoreStatus => "CoreStatus",
        CliCommand::ResourceStatus => "ResourceStatus",
        CliCommand::Probe => "Probe",
        CliCommand::Update => "Update",
        CliCommand::ConfigReload => "ConfigReload",
        CliCommand::ConfigCheck => "ConfigCheck",
        CliCommand::ProtocolHello => "ProtocolHello",
        CliCommand::ConfigGet => "ConfigGet",
        CliCommand::ConfigValidate => "ConfigValidate",
        CliCommand::ConfigApply => "ConfigApply",
        CliCommand::ConfigSchema => "ConfigSchema",
        CliCommand::ConfigMutate => "ConfigMutate",
        CliCommand::CapabilityGet => "CapabilityGet",
        CliCommand::Events => "Events",
        CliCommand::NodeList => "NodeList",
        CliCommand::NodeTest => "NodeTest",
        CliCommand::NodeTestAll => "NodeTestAll",
        CliCommand::NodeSelection => "NodeSelection",
        CliCommand::NodeCurrent => "NodeCurrent",
        CliCommand::NodeShow => "NodeShow",
        CliCommand::NodeSelectAuto => "NodeSelectAuto",
        CliCommand::NodeSelectManual => "NodeSelectManual",
        CliCommand::NodeUseAuto => "NodeUseAuto",
        CliCommand::NodeUseManual => "NodeUseManual",
        CliCommand::NodeDelay => "NodeDelay",
        CliCommand::NodeImport => "NodeImport",
        CliCommand::NodeEdit => "NodeEdit",
        CliCommand::NodeRemove => "NodeRemove",
        CliCommand::NodeExport => "NodeExport",
        CliCommand::NodeOverrideGet => "NodeOverrideGet",
        CliCommand::NodeOverrideApply => "NodeOverrideApply",
        CliCommand::NodeOverrideRemove => "NodeOverrideRemove",
        CliCommand::ConnectionsGet => "ConnectionsGet",
        CliCommand::ConnectionClose => "ConnectionClose",
        CliCommand::ConnectionShow => "ConnectionShow",
        CliCommand::ConnectionsCloseAll => "ConnectionsCloseAll",
        CliCommand::LogsGet => "LogsGet",
        CliCommand::LogsTail => "LogsTail",
        CliCommand::LogsClear => "LogsClear",
        CliCommand::SubscriptionList => "SubscriptionList",
        CliCommand::SubscriptionShow => "SubscriptionShow",
        CliCommand::SubscriptionInspect => "SubscriptionInspect",
        CliCommand::SubscriptionDiagnose => "SubscriptionDiagnose",
        CliCommand::SubscriptionHistory => "SubscriptionHistory",
        CliCommand::SubscriptionUpdateAll => "SubscriptionUpdateAll",
        CliCommand::SubscriptionEdit => "SubscriptionEdit",
        CliCommand::SubscriptionMode => "SubscriptionMode",
        CliCommand::SubscriptionModeSetSingle => "SubscriptionModeSetSingle",
        CliCommand::SubscriptionModeSetMerge => "SubscriptionModeSetMerge",
        CliCommand::SubscriptionSelect => "SubscriptionSelect",
        CliCommand::SubscriptionAdd => "SubscriptionAdd",
        CliCommand::SubscriptionRemove => "SubscriptionRemove",
        CliCommand::SubscriptionMove => "SubscriptionMove",
        CliCommand::SubscriptionEnable => "SubscriptionEnable",
        CliCommand::SubscriptionDisable => "SubscriptionDisable",
        CliCommand::SubscriptionImportPreview => "SubscriptionImportPreview",
        CliCommand::SubscriptionImportApply => "SubscriptionImportApply",
        CliCommand::ApplicationAddPackage => "ApplicationAddPackage",
        CliCommand::ApplicationRemovePackage => "ApplicationRemovePackage",
        CliCommand::ApplicationAddUid => "ApplicationAddUid",
        CliCommand::ApplicationRemoveUid => "ApplicationRemoveUid",
        CliCommand::ApplicationList => "ApplicationList",
        CliCommand::ApplicationUsers => "ApplicationUsers",
        CliCommand::ApplicationPolicySet => "ApplicationPolicySet",
        CliCommand::ApplicationMode => "ApplicationMode",
        CliCommand::NetworkSet => "NetworkSet",
        CliCommand::NetworkStatus => "NetworkStatus",
        CliCommand::NetworkEvaluate => "NetworkEvaluate",
        CliCommand::WifiStatus => "WifiStatus",
        CliCommand::HotspotStatus => "HotspotStatus",
        CliCommand::CaptureCheck => "CaptureCheck",
        CliCommand::DiagnosticsBundle => "DiagnosticsBundle",
        CliCommand::LogsExport => "LogsExport",
        CliCommand::SupportBundle => "SupportBundle",
        CliCommand::TopologyGet => "TopologyGet",
        CliCommand::TrafficGet => "TrafficGet",
        CliCommand::TrafficLive => "TrafficLive",
        CliCommand::MetricsGet => "MetricsGet",
        CliCommand::BackupExport => "BackupExport",
        CliCommand::BackupRestore => "BackupRestore",
        CliCommand::CoreVersionCheck => "CoreVersionCheck",
        CliCommand::RuleSetStatus => "RuleSetStatus",
        CliCommand::RuleSetList => "RuleSetList",
        CliCommand::RuleSetShow => "RuleSetShow",
        CliCommand::RuleSetUpdate => "RuleSetUpdate",
        CliCommand::WebUiPayloadCreate => "WebUiPayloadCreate",
        CliCommand::WebUiPayloadAppend => "WebUiPayloadAppend",
        CliCommand::WebUiPayloadCommit => "WebUiPayloadCommit",
        CliCommand::WebUiPayloadRemove => "WebUiPayloadRemove",
        CliCommand::Help => "Help",
        CliCommand::Version => "Version",
    }
}

#[test]
fn every_stable_cli_command_has_a_v1_before_golden() {
    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(fixture["protocol_version"], 1);
    assert_eq!(PROTOCOL_VERSION, 6);

    let success: ControlResponse = serde_json::from_value(fixture["success"].clone()).unwrap();
    let failure: ControlResponse = serde_json::from_value(fixture["failure"].clone()).unwrap();
    assert!(success.ok());
    assert!(!failure.ok());
    assert_eq!(success.request_id(), failure.request_id());

    let mut observed = BTreeSet::new();
    for case in fixture["commands"].as_array().unwrap() {
        let args = case["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect::<Vec<_>>();
        if case["id"] == "NodeSelect" {
            assert!(parse_command(args).is_err());
            continue;
        }
        let command = parse_command(args).unwrap();
        assert_eq!(case["id"], command_name(command));
        if matches!(
            case["id"].as_str(),
            Some("SubscriptionEnable" | "SubscriptionDisable")
        ) {
            assert_eq!(
                serde_json::to_value(command.method()).unwrap(),
                "subscription.set_enabled"
            );
            assert_ne!(
                serde_json::to_value(command.method()).unwrap(),
                case["method"]
            );
        } else {
            assert_eq!(
                serde_json::to_value(command.method()).unwrap(),
                case["method"]
            );
        }
        assert!(observed.insert(command_name(command)));
    }

    let all = [
        CliCommand::Status,
        CliCommand::Start,
        CliCommand::Stop,
        CliCommand::Probe,
        CliCommand::Update,
        CliCommand::ConfigReload,
        CliCommand::ProtocolHello,
        CliCommand::ConfigGet,
        CliCommand::ConfigValidate,
        CliCommand::ConfigApply,
        CliCommand::ConfigSchema,
        CliCommand::ConfigMutate,
        CliCommand::CapabilityGet,
        CliCommand::Events,
        CliCommand::NodeList,
        CliCommand::NodeTest,
        CliCommand::NodeRemove,
        CliCommand::NodeExport,
        CliCommand::ConnectionsGet,
        CliCommand::ConnectionClose,
        CliCommand::ConnectionsCloseAll,
        CliCommand::LogsGet,
        CliCommand::LogsTail,
        CliCommand::LogsClear,
        CliCommand::SubscriptionList,
        CliCommand::SubscriptionAdd,
        CliCommand::SubscriptionRemove,
        CliCommand::SubscriptionMove,
        CliCommand::SubscriptionEnable,
        CliCommand::SubscriptionDisable,
        CliCommand::SubscriptionImportPreview,
        CliCommand::SubscriptionImportApply,
        CliCommand::ApplicationAddPackage,
        CliCommand::ApplicationRemovePackage,
        CliCommand::ApplicationAddUid,
        CliCommand::ApplicationRemoveUid,
        CliCommand::ApplicationList,
        CliCommand::ApplicationMode,
        CliCommand::NetworkSet,
        CliCommand::DiagnosticsBundle,
        CliCommand::TopologyGet,
        CliCommand::TrafficGet,
        CliCommand::BackupExport,
        CliCommand::BackupRestore,
        CliCommand::CoreVersionCheck,
        CliCommand::RuleSetStatus,
        CliCommand::RuleSetUpdate,
    ];
    assert_eq!(observed.len(), all.len());
    for command in all {
        assert!(observed.contains(command_name(command)));
    }
}
