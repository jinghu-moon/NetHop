#![doc = "Thin, file-independent client for the NetHop local control protocol."]

use nethop_protocol::{
    ConfigMutation, ControlMethod, ControlParams, ControlRequest, ControlResponse, EventKind,
    RequestId,
};
use serde_json::Value;
use thiserror::Error;

pub const DEFAULT_SOCKET_PATH: &str = "/data/adb/nethop/run/nethopd.sock";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliCommand {
    Status,
    Start,
    Stop,
    Probe,
    Update,
    ConfigReload,
    ProtocolHello,
    ConfigGet,
    ConfigValidate,
    ConfigApply,
    ConfigSchema,
    ConfigMutate,
    CapabilityGet,
    Events,
    NodeList,
    NodeTest,
    NodeSelect,
    NodeRemove,
    NodeExport,
    ConnectionsGet,
    ConnectionClose,
    ConnectionsCloseAll,
    LogsGet,
    LogsTail,
    LogsClear,
    SubscriptionList,
    SubscriptionAdd,
    SubscriptionRemove,
    SubscriptionMove,
    SubscriptionEnable,
    SubscriptionDisable,
    SubscriptionImportPreview,
    SubscriptionImportApply,
    ApplicationAddPackage,
    ApplicationRemovePackage,
    ApplicationAddUid,
    ApplicationRemoveUid,
    ApplicationList,
    ApplicationMode,
    NetworkSet,
    DiagnosticsBundle,
    TopologyGet,
    TrafficGet,
    BackupExport,
    BackupRestore,
    CoreVersionCheck,
    RuleSetStatus,
    RuleSetUpdate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliInvocation {
    command: CliCommand,
    wait: bool,
    if_needed: bool,
    expected_digest: Option<String>,
    manager_version: Option<String>,
    protocol_min: Option<u8>,
    protocol_max: Option<u8>,
    event_kinds: Vec<EventKind>,
    target: Option<String>,
    query: Option<String>,
    limit: Option<u8>,
    before: Option<String>,
    positional: Vec<String>,
    candidate_digest: Option<String>,
    input_file: Option<String>,
    text_input: bool,
    import_format: Option<String>,
    source_id: Option<String>,
    human: bool,
}

impl CliInvocation {
    pub const fn command(&self) -> CliCommand {
        self.command
    }

    pub const fn wait(&self) -> bool {
        self.wait
    }

    pub const fn if_needed(&self) -> bool {
        self.if_needed
    }

    pub fn input_file(&self) -> Option<&str> {
        self.input_file.as_deref()
    }

    pub const fn text_input(&self) -> bool {
        self.text_input
    }

    pub fn import_format(&self) -> Option<&str> {
        self.import_format.as_deref()
    }

    pub fn source_id(&self) -> Option<&str> {
        self.source_id.as_deref()
    }

    pub const fn human(&self) -> bool {
        self.human
    }
}

impl CliCommand {
    pub const fn method(self) -> ControlMethod {
        match self {
            Self::Status => ControlMethod::StatusGet,
            Self::Start => ControlMethod::ServiceStart,
            Self::Stop => ControlMethod::ServiceStop,
            Self::Probe => ControlMethod::CapabilityProbe,
            Self::Update => ControlMethod::SubscriptionUpdate,
            Self::ConfigReload => ControlMethod::ConfigReload,
            Self::ProtocolHello => ControlMethod::ProtocolHello,
            Self::ConfigGet => ControlMethod::ConfigGet,
            Self::ConfigValidate => ControlMethod::ConfigValidate,
            Self::ConfigApply => ControlMethod::ConfigApply,
            Self::ConfigSchema => ControlMethod::ConfigSchema,
            Self::ConfigMutate => ControlMethod::ConfigMutate,
            Self::CapabilityGet => ControlMethod::CapabilityGet,
            Self::Events => ControlMethod::EventsSubscribe,
            Self::NodeList => ControlMethod::NodeList,
            Self::NodeTest => ControlMethod::NodeTest,
            Self::NodeSelect => ControlMethod::NodeSelect,
            Self::NodeExport => ControlMethod::NodeExport,
            Self::NodeRemove => ControlMethod::ConfigMutate,
            Self::ConnectionsGet => ControlMethod::ConnectionsGet,
            Self::ConnectionClose => ControlMethod::ConnectionClose,
            Self::ConnectionsCloseAll => ControlMethod::ConnectionsCloseAll,
            Self::LogsGet => ControlMethod::LogsGet,
            Self::LogsTail => ControlMethod::EventsSubscribe,
            Self::LogsClear => ControlMethod::LogsClear,
            Self::SubscriptionList => ControlMethod::ConfigGet,
            Self::ApplicationList => ControlMethod::ConfigGet,
            Self::SubscriptionImportPreview => ControlMethod::SubscriptionImportPreview,
            Self::SubscriptionImportApply => ControlMethod::SubscriptionImportApply,
            Self::SubscriptionAdd
            | Self::SubscriptionRemove
            | Self::SubscriptionMove
            | Self::SubscriptionEnable
            | Self::SubscriptionDisable
            | Self::ApplicationAddPackage
            | Self::ApplicationRemovePackage
            | Self::ApplicationAddUid
            | Self::ApplicationRemoveUid
            | Self::ApplicationMode
            | Self::NetworkSet => ControlMethod::ConfigMutate,
            Self::DiagnosticsBundle => ControlMethod::DiagnosticsBundle,
            Self::TopologyGet => ControlMethod::TopologyGet,
            Self::TrafficGet => ControlMethod::TrafficGet,
            Self::BackupExport => ControlMethod::ConfigExport,
            Self::BackupRestore => ControlMethod::ConfigApply,
            Self::CoreVersionCheck => ControlMethod::CoreVersionCheck,
            Self::RuleSetStatus => ControlMethod::RuleSetStatus,
            Self::RuleSetUpdate => ControlMethod::RuleSetUpdate,
        }
    }
}

pub fn parse_command<I, S>(arguments: I) -> Result<CliCommand, CliError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut arguments = arguments.into_iter();
    let command = match arguments.next().as_ref().map(AsRef::as_ref) {
        Some("status") => CliCommand::Status,
        Some("start") => CliCommand::Start,
        Some("stop") => CliCommand::Stop,
        Some("probe") => CliCommand::Probe,
        Some("update") => CliCommand::Update,
        Some("config") => match arguments.next().as_ref().map(AsRef::as_ref) {
            Some("reload") => CliCommand::ConfigReload,
            Some("get") => CliCommand::ConfigGet,
            Some("validate") => CliCommand::ConfigValidate,
            Some("apply") => CliCommand::ConfigApply,
            Some("schema") => CliCommand::ConfigSchema,
            Some("mutate") => CliCommand::ConfigMutate,
            _ => return Err(CliError::Usage),
        },
        Some("hello") => CliCommand::ProtocolHello,
        Some("capability") if arguments.next().as_ref().map(AsRef::as_ref) == Some("get") => {
            CliCommand::CapabilityGet
        }
        Some("events") => CliCommand::Events,
        Some("node") => match arguments.next().as_ref().map(AsRef::as_ref) {
            Some("list") => CliCommand::NodeList,
            Some("test") => CliCommand::NodeTest,
            Some("select") => CliCommand::NodeSelect,
            Some("remove") => CliCommand::NodeRemove,
            Some("export") => CliCommand::NodeExport,
            _ => return Err(CliError::Usage),
        },
        Some("connections") => match arguments.next().as_ref().map(AsRef::as_ref) {
            None => CliCommand::ConnectionsGet,
            Some("close-all") => CliCommand::ConnectionsCloseAll,
            _ => return Err(CliError::Usage),
        },
        Some("connection") if arguments.next().as_ref().map(AsRef::as_ref) == Some("close") => {
            CliCommand::ConnectionClose
        }
        Some("diagnose") => CliCommand::DiagnosticsBundle,
        Some("topology") => CliCommand::TopologyGet,
        Some("traffic") => CliCommand::TrafficGet,
        Some("backup") => match arguments.next().as_ref().map(AsRef::as_ref) {
            Some("export") => CliCommand::BackupExport,
            Some("restore") => CliCommand::BackupRestore,
            _ => return Err(CliError::Usage),
        },
        Some("core") if arguments.next().as_ref().map(AsRef::as_ref) == Some("version-check") => {
            CliCommand::CoreVersionCheck
        }
        Some("ruleset") => match arguments.next().as_ref().map(AsRef::as_ref) {
            Some("status") => CliCommand::RuleSetStatus,
            Some("update") => CliCommand::RuleSetUpdate,
            _ => return Err(CliError::Usage),
        },
        Some("logs") => match arguments.next().as_ref().map(AsRef::as_ref) {
            Some("get") => CliCommand::LogsGet,
            Some("tail") => CliCommand::LogsTail,
            Some("clear") => CliCommand::LogsClear,
            _ => return Err(CliError::Usage),
        },
        Some("subscription") => match arguments.next().as_ref().map(AsRef::as_ref) {
            Some("list") => CliCommand::SubscriptionList,
            Some("add") => CliCommand::SubscriptionAdd,
            Some("remove") => CliCommand::SubscriptionRemove,
            Some("move") => CliCommand::SubscriptionMove,
            Some("enable") => CliCommand::SubscriptionEnable,
            Some("disable") => CliCommand::SubscriptionDisable,
            Some("update") => CliCommand::Update,
            Some("import") => match arguments.next().as_ref().map(AsRef::as_ref) {
                Some("preview") => CliCommand::SubscriptionImportPreview,
                Some("apply") => CliCommand::SubscriptionImportApply,
                _ => return Err(CliError::Usage),
            },
            _ => return Err(CliError::Usage),
        },
        Some("application") => match arguments.next().as_ref().map(AsRef::as_ref) {
            Some("list") => CliCommand::ApplicationList,
            Some("mode") => CliCommand::ApplicationMode,
            Some("add-package") => CliCommand::ApplicationAddPackage,
            Some("remove-package") => CliCommand::ApplicationRemovePackage,
            Some("add-uid") => CliCommand::ApplicationAddUid,
            Some("remove-uid") => CliCommand::ApplicationRemoveUid,
            _ => return Err(CliError::Usage),
        },
        Some("network") if arguments.next().as_ref().map(AsRef::as_ref) == Some("set") => {
            CliCommand::NetworkSet
        }
        _ => return Err(CliError::Usage),
    };
    if arguments.next().is_some() {
        return Err(CliError::Usage);
    }
    Ok(command)
}

pub fn parse_invocation<I, S>(arguments: I) -> Result<CliInvocation, CliError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let arguments: Vec<String> = arguments
        .into_iter()
        .map(|value| value.as_ref().to_owned())
        .collect();
    let command_length = match arguments.first().map(String::as_str) {
        Some("subscription") if arguments.get(1).map(String::as_str) == Some("import") => 3,
        Some(
            "config" | "capability" | "node" | "connection" | "logs" | "subscription"
            | "application" | "network" | "backup" | "core" | "ruleset",
        ) => 2,
        Some("connections") if arguments.get(1).map(String::as_str) == Some("close-all") => 2,
        Some(_) => 1,
        None => return Err(CliError::Usage),
    };
    if arguments.len() < command_length {
        return Err(CliError::Usage);
    }
    let command = parse_command(arguments[..command_length].iter().map(String::as_str))?;
    let mut wait = false;
    let mut if_needed = false;
    let mut expected_digest = None;
    let mut candidate_digest = None;
    let mut input_file = None;
    let mut text_input = false;
    let mut import_format = None;
    let mut source_id = None;
    let mut human = false;
    let mut structured_output = false;
    let mut manager_version = None;
    let mut protocol_min = None;
    let mut protocol_max = None;
    let mut event_kinds = Vec::new();
    let mut target = None;
    let mut query = None;
    let mut limit = None;
    let mut before = None;
    let mut positional = Vec::new();
    let mut options = arguments[command_length..].iter();
    while let Some(option) = options.next() {
        match option.as_str() {
            "--wait" if !wait => wait = true,
            "--if-needed" if !if_needed => if_needed = true,
            "--source" if source_id.is_none() && command == CliCommand::Update => {
                let value = options.next().cloned().ok_or(CliError::Usage)?;
                if value.len() != 36
                    || !value.starts_with("src_")
                    || !value[4..]
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                {
                    return Err(CliError::Usage);
                }
                source_id = Some(value);
            }
            "--json" | "--jsonl" => structured_output = true,
            "--human" if command == CliCommand::Status && !human => human = true,
            "--expected-digest" if expected_digest.is_none() => {
                expected_digest = options.next().cloned();
                if expected_digest.is_none() {
                    return Err(CliError::Usage);
                }
            }
            "--candidate-digest" if candidate_digest.is_none() => {
                candidate_digest = Some(options.next().cloned().ok_or(CliError::Usage)?);
            }
            "--file" if input_file.is_none() && !text_input => {
                input_file = Some(options.next().cloned().ok_or(CliError::Usage)?);
            }
            "--text" if input_file.is_none() && !text_input => text_input = true,
            "--format" if import_format.is_none() => {
                let format = options.next().cloned().ok_or(CliError::Usage)?;
                if !matches!(
                    format.as_str(),
                    "auto"
                        | "uri_list"
                        | "base64_list"
                        | "clash_yaml"
                        | "singbox_json"
                        | "ini_profile"
                        | "surfboard_ini"
                ) {
                    return Err(CliError::Usage);
                }
                import_format = Some(format);
            }
            "--manager-version" if manager_version.is_none() => {
                manager_version = options.next().cloned();
            }
            "--protocol-min" if protocol_min.is_none() => {
                protocol_min = options.next().and_then(|value| value.parse().ok());
            }
            "--protocol-max" if protocol_max.is_none() => {
                protocol_max = options.next().and_then(|value| value.parse().ok());
            }
            "--kinds" if event_kinds.is_empty() => {
                let value = options.next().ok_or(CliError::Usage)?;
                event_kinds = parse_event_kinds(value)?;
            }
            "--limit" if limit.is_none() => {
                limit = Some(
                    options
                        .next()
                        .and_then(|value| value.parse::<u8>().ok())
                        .filter(|value| (1..=128).contains(value))
                        .ok_or(CliError::Usage)?,
                );
            }
            "--before" if before.is_none() => {
                before = Some(options.next().cloned().ok_or(CliError::Usage)?);
            }
            value
                if !value.starts_with('-')
                    && matches!(
                        command,
                        CliCommand::NodeTest
                            | CliCommand::NodeSelect
                            | CliCommand::NodeRemove
                            | CliCommand::NodeExport
                            | CliCommand::ConnectionClose
                    )
                    && target.is_none() =>
            {
                target = Some(value.to_owned());
            }
            value
                if !value.starts_with('-')
                    && matches!(command, CliCommand::NodeList | CliCommand::ConnectionsGet)
                    && query.is_none() =>
            {
                query = Some(value.to_owned());
            }
            value
                if !value.starts_with('-')
                    && matches!(
                        command,
                        CliCommand::SubscriptionAdd
                            | CliCommand::SubscriptionRemove
                            | CliCommand::SubscriptionMove
                            | CliCommand::SubscriptionEnable
                            | CliCommand::SubscriptionDisable
                            | CliCommand::ApplicationAddPackage
                            | CliCommand::ApplicationRemovePackage
                            | CliCommand::ApplicationAddUid
                            | CliCommand::ApplicationRemoveUid
                            | CliCommand::ApplicationMode
                            | CliCommand::NetworkSet
                    ) =>
            {
                positional.push(value.to_owned());
            }
            _ => return Err(CliError::Usage),
        }
    }
    let wait_allowed = matches!(
        command,
        CliCommand::Start
            | CliCommand::Stop
            | CliCommand::Update
            | CliCommand::RuleSetUpdate
            | CliCommand::ConfigReload
    );
    if (wait && !wait_allowed) || (if_needed && command != CliCommand::Update) {
        return Err(CliError::Usage);
    }
    if human && structured_output {
        return Err(CliError::Usage);
    }
    let needs_digest = matches!(
        command,
        CliCommand::ConfigValidate
            | CliCommand::ConfigApply
            | CliCommand::ConfigMutate
            | CliCommand::SubscriptionAdd
            | CliCommand::SubscriptionRemove
            | CliCommand::SubscriptionMove
            | CliCommand::SubscriptionEnable
            | CliCommand::SubscriptionDisable
            | CliCommand::ApplicationAddPackage
            | CliCommand::ApplicationRemovePackage
            | CliCommand::ApplicationAddUid
            | CliCommand::ApplicationRemoveUid
            | CliCommand::ApplicationMode
            | CliCommand::NetworkSet
            | CliCommand::NodeRemove
            | CliCommand::SubscriptionImportPreview
            | CliCommand::SubscriptionImportApply
            | CliCommand::BackupRestore
    );
    if expected_digest.is_some() != needs_digest {
        return Err(CliError::Usage);
    }
    if candidate_digest.is_some() != (command == CliCommand::SubscriptionImportApply) {
        return Err(CliError::Usage);
    }
    let import_command = matches!(
        command,
        CliCommand::SubscriptionImportPreview | CliCommand::SubscriptionImportApply
    );
    let backup_command = matches!(
        command,
        CliCommand::BackupExport | CliCommand::BackupRestore
    );
    let has_input = input_file.is_some() || text_input;
    let invalid_import_input = !backup_command && import_command != has_input;
    let invalid_backup_input = backup_command && (input_file.is_none() || text_input);
    let invalid_format_hint = !import_command && import_format.is_some();
    if invalid_import_input || invalid_backup_input || invalid_format_hint {
        return Err(CliError::Usage);
    }
    let hello = command == CliCommand::ProtocolHello;
    if manager_version.is_some() != hello
        || protocol_min.is_some() != hello
        || protocol_max.is_some() != hello
    {
        return Err(CliError::Usage);
    }
    if !event_kinds.is_empty() && !matches!(command, CliCommand::Events | CliCommand::LogsTail) {
        return Err(CliError::Usage);
    }
    let target_command = matches!(
        command,
        CliCommand::NodeTest
            | CliCommand::NodeSelect
            | CliCommand::NodeRemove
            | CliCommand::NodeExport
            | CliCommand::ConnectionClose
    );
    if target.is_some() != target_command {
        return Err(CliError::Usage);
    }
    let expected_positionals = match command {
        CliCommand::SubscriptionAdd | CliCommand::NetworkSet => 2,
        CliCommand::SubscriptionRemove
        | CliCommand::SubscriptionMove
        | CliCommand::SubscriptionEnable
        | CliCommand::SubscriptionDisable
        | CliCommand::ApplicationAddPackage
        | CliCommand::ApplicationRemovePackage
        | CliCommand::ApplicationAddUid
        | CliCommand::ApplicationRemoveUid => 1,
        CliCommand::ApplicationMode => 1,
        _ => 0,
    };
    if positional.len() != expected_positionals
        || (command != CliCommand::SubscriptionMove && before.is_some())
        || (command == CliCommand::SubscriptionMove && before.is_none())
    {
        return Err(CliError::Usage);
    }
    let query_command = matches!(command, CliCommand::NodeList | CliCommand::ConnectionsGet);
    let limit_command = query_command || command == CliCommand::LogsGet;
    if (!query_command && query.is_some()) || (!limit_command && limit.is_some()) {
        return Err(CliError::Usage);
    }
    Ok(CliInvocation {
        command,
        wait,
        if_needed,
        expected_digest,
        manager_version,
        protocol_min,
        protocol_max,
        event_kinds,
        target,
        query,
        limit,
        before,
        positional,
        candidate_digest,
        input_file,
        text_input,
        import_format,
        source_id,
        human,
    })
}

fn parse_event_kinds(value: &str) -> Result<Vec<EventKind>, CliError> {
    if value.is_empty() {
        return Err(CliError::Usage);
    }
    value
        .split(',')
        .map(|kind| match kind {
            "config" => Ok(EventKind::Config),
            "runtime" => Ok(EventKind::Runtime),
            "subscription" => Ok(EventKind::Subscription),
            "generation" => Ok(EventKind::Generation),
            "network" => Ok(EventKind::Network),
            _ => Err(CliError::Usage),
        })
        .collect()
}

fn parse_scalar(value: &str) -> Value {
    if value == "true" {
        Value::Bool(true)
    } else if value == "false" {
        Value::Bool(false)
    } else if let Ok(number) = value.parse::<i64>() {
        Value::Number(number.into())
    } else {
        Value::String(value.to_owned())
    }
}

pub trait ControlTransport {
    fn exchange(&mut self, request: &ControlRequest) -> Result<ControlResponse, CliError>;
}

pub fn execute(
    transport: &mut impl ControlTransport,
    command: CliCommand,
    request_id: RequestId,
) -> Result<ControlResponse, CliError> {
    let request = ControlRequest::new(request_id.clone(), command.method());
    let response = transport.exchange(&request)?;
    if response.request_id() != &request_id {
        return Err(CliError::ResponseRequestMismatch);
    }
    Ok(response)
}

pub fn execute_invocation(
    transport: &mut impl ControlTransport,
    invocation: CliInvocation,
    request_id: RequestId,
) -> Result<ControlResponse, CliError> {
    let request = build_request(&invocation, request_id.clone(), None)?;
    let response = transport.exchange(&request)?;
    if response.request_id() != &request_id {
        return Err(CliError::ResponseRequestMismatch);
    }
    Ok(response)
}

pub fn execute_with_input(
    transport: &mut impl ControlTransport,
    invocation: &CliInvocation,
    request_id: RequestId,
    input: Option<Value>,
) -> Result<ControlResponse, CliError> {
    let request = build_request(invocation, request_id.clone(), input)?;
    let response = transport.exchange(&request)?;
    if response.request_id() != &request_id {
        return Err(CliError::ResponseRequestMismatch);
    }
    Ok(response)
}

pub fn build_request(
    invocation: &CliInvocation,
    request_id: RequestId,
    input: Option<Value>,
) -> Result<ControlRequest, CliError> {
    let params = match invocation.command {
        CliCommand::ConfigValidate | CliCommand::ConfigApply | CliCommand::BackupRestore => {
            ControlParams::config_document(
                invocation.expected_digest.clone().ok_or(CliError::Usage)?,
                input.ok_or(CliError::InputRequired)?,
            )
        }
        CliCommand::ConfigMutate => ControlParams::mutation(
            invocation.expected_digest.clone().ok_or(CliError::Usage)?,
            serde_json::from_value::<ConfigMutation>(input.ok_or(CliError::InputRequired)?)
                .map_err(|_| CliError::InvalidInput)?,
        ),
        CliCommand::SubscriptionAdd => ControlParams::mutation(
            invocation.expected_digest.clone().ok_or(CliError::Usage)?,
            ConfigMutation::AddSource {
                name: invocation.positional[0].clone(),
                url: invocation.positional[1].clone(),
            },
        ),
        CliCommand::SubscriptionImportPreview | CliCommand::SubscriptionImportApply => {
            ControlParams::import_document(
                invocation.expected_digest.clone().ok_or(CliError::Usage)?,
                invocation.candidate_digest.clone(),
                input.ok_or(CliError::InputRequired)?,
            )
        }
        CliCommand::SubscriptionRemove => ControlParams::mutation(
            invocation.expected_digest.clone().ok_or(CliError::Usage)?,
            ConfigMutation::RemoveSource {
                source_id: invocation.positional[0].clone(),
            },
        ),
        CliCommand::SubscriptionMove => ControlParams::mutation(
            invocation.expected_digest.clone().ok_or(CliError::Usage)?,
            ConfigMutation::MoveSource {
                source_id: invocation.positional[0].clone(),
                before_source_id: invocation.before.clone(),
            },
        ),
        CliCommand::SubscriptionEnable | CliCommand::SubscriptionDisable => {
            ControlParams::mutation(
                invocation.expected_digest.clone().ok_or(CliError::Usage)?,
                ConfigMutation::UpdateSource {
                    source_id: invocation.positional[0].clone(),
                    name: None,
                    url: None,
                    enabled: Some(invocation.command == CliCommand::SubscriptionEnable),
                },
            )
        }
        CliCommand::ApplicationAddPackage => ControlParams::mutation(
            invocation.expected_digest.clone().ok_or(CliError::Usage)?,
            ConfigMutation::AddApplicationTarget {
                target: nethop_protocol::ApplicationTarget::Package {
                    android_user_id: 0,
                    package: invocation.positional[0].clone(),
                },
            },
        ),
        CliCommand::ApplicationRemovePackage => ControlParams::mutation(
            invocation.expected_digest.clone().ok_or(CliError::Usage)?,
            ConfigMutation::RemoveApplicationTarget {
                target: nethop_protocol::ApplicationTarget::Package {
                    android_user_id: 0,
                    package: invocation.positional[0].clone(),
                },
            },
        ),
        CliCommand::ApplicationAddUid | CliCommand::ApplicationRemoveUid => {
            let uid = invocation.positional[0]
                .parse::<u32>()
                .map_err(|_| CliError::InvalidInput)?;
            let target = nethop_protocol::ApplicationTarget::Uid { uid };
            let mutation = if invocation.command == CliCommand::ApplicationAddUid {
                ConfigMutation::AddApplicationTarget { target }
            } else {
                ConfigMutation::RemoveApplicationTarget { target }
            };
            ControlParams::mutation(
                invocation.expected_digest.clone().ok_or(CliError::Usage)?,
                mutation,
            )
        }
        CliCommand::NetworkSet => ControlParams::mutation(
            invocation.expected_digest.clone().ok_or(CliError::Usage)?,
            ConfigMutation::SetScalarField {
                field_id: invocation.positional[0].clone(),
                value: parse_scalar(&invocation.positional[1]),
            },
        ),
        CliCommand::ApplicationMode => ControlParams::mutation(
            invocation.expected_digest.clone().ok_or(CliError::Usage)?,
            ConfigMutation::SetScalarField {
                field_id: "applications.mode".into(),
                value: Value::String(invocation.positional[0].clone()),
            },
        ),
        CliCommand::ProtocolHello => ControlParams::hello(
            invocation.manager_version.clone().ok_or(CliError::Usage)?,
            invocation.protocol_min.ok_or(CliError::Usage)?,
            invocation.protocol_max.ok_or(CliError::Usage)?,
        ),
        CliCommand::Events | CliCommand::LogsTail => {
            ControlParams::event_subscription(invocation.event_kinds.clone())
        }
        CliCommand::NodeTest
        | CliCommand::NodeSelect
        | CliCommand::NodeExport
        | CliCommand::ConnectionClose => {
            ControlParams::target(invocation.target.clone().ok_or(CliError::Usage)?)
        }
        CliCommand::NodeRemove => ControlParams::mutation(
            invocation.expected_digest.clone().ok_or(CliError::Usage)?,
            ConfigMutation::RemoveNode {
                node_id: invocation.target.clone().ok_or(CliError::Usage)?,
            },
        ),
        CliCommand::NodeList | CliCommand::ConnectionsGet | CliCommand::LogsGet => {
            ControlParams::list(invocation.query.clone(), invocation.limit)
        }
        CliCommand::Update => ControlParams::subscription_update(
            invocation.wait,
            invocation.if_needed,
            invocation.source_id.clone(),
        ),
        _ => ControlParams::new(invocation.wait, invocation.if_needed),
    };
    ControlRequest::new(request_id, invocation.command.method())
        .with_params(params)
        .map_err(|_| CliError::RequestFailed)
}

pub fn render_response(response: &ControlResponse) -> Result<String, CliError> {
    serde_json::to_string(response).map_err(|_| CliError::InvalidResponse)
}

pub fn render_status_human(response: &ControlResponse) -> Result<String, CliError> {
    let result = response.result().ok_or(CliError::InvalidResponse)?;
    let state = result["state"].as_str().ok_or(CliError::InvalidResponse)?;
    if !matches!(
        state,
        "init"
            | "probing"
            | "starting_core"
            | "running_tproxy"
            | "starting_tun"
            | "running_tun"
            | "degraded"
            | "fail_open_direct"
            | "backoff"
            | "circuit_open"
            | "stopping"
    ) {
        return Err(CliError::InvalidResponse);
    }
    let generation = result["generation"]
        .as_u64()
        .map_or_else(|| "none".to_owned(), |value| value.to_string());
    let last_update = result["last_update"]
        .as_str()
        .ok_or(CliError::InvalidResponse)?;
    if !matches!(last_update, "never" | "succeeded" | "failed") {
        return Err(CliError::InvalidResponse);
    }
    let core_update = &result["core_update"];
    let current = core_update["current"]
        .as_str()
        .filter(|version| valid_display_version(version))
        .ok_or(CliError::InvalidResponse)?;
    let update = match core_update.get("availability").and_then(Value::as_str) {
        Some("up_to_date") => "up to date".to_owned(),
        Some("available") => {
            let latest = core_update["latest"]
                .as_str()
                .filter(|version| valid_display_version(version))
                .ok_or(CliError::InvalidResponse)?;
            format!("{latest} available; update the NetHop module")
        }
        None if core_update["state"] == "never_checked" => "not checked".to_owned(),
        _ => return Err(CliError::InvalidResponse),
    };
    let dns = match (
        result["dns_split"]["mode"].as_str(),
        result["dns_split"]["dns_split"].as_str(),
    ) {
        (Some("off"), Some("healthy")) => "healthy".to_owned(),
        (Some(mode @ ("opportunistic" | "strict")), Some("degraded_private_dns")) => {
            format!("degraded ({mode} Private DNS); disable Private DNS for split DNS")
        }
        (Some("unknown"), Some("unknown")) => "unknown".to_owned(),
        _ => return Err(CliError::InvalidResponse),
    };
    Ok(format!(
        "NetHop status\nState: {state}\nGeneration: {generation}\nSubscription: {last_update}\nDNS split: {dns}\nCore: {current}\nCore update: {update}"
    ))
}

fn valid_display_version(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum CliError {
    #[error(
        "usage: nethopctl <status|start|stop|probe|update|ruleset|config|node|connections|connection|logs|diagnose|topology>"
    )]
    Usage,
    #[error("control socket is unavailable")]
    ConnectionFailed,
    #[error("control protocol request could not be sent")]
    RequestFailed,
    #[error("control protocol response is invalid")]
    InvalidResponse,
    #[error("control response request ID does not match")]
    ResponseRequestMismatch,
    #[error("command requires a bounded JSON document on stdin")]
    InputRequired,
    #[error("stdin is not a valid command JSON document")]
    InvalidInput,
    #[error("this platform does not provide Unix domain sockets")]
    UnsupportedPlatform,
}

#[cfg(unix)]
mod unix {
    use std::{io::Write, os::unix::net::UnixStream, path::PathBuf, time::Duration};

    use nethop_protocol::{FrameCodec, WireFrame};

    use super::*;

    #[derive(Debug, Clone)]
    pub struct UnixControlTransport {
        socket_path: PathBuf,
        timeout: Duration,
    }

    impl UnixControlTransport {
        pub fn new(socket_path: impl Into<PathBuf>, timeout: Duration) -> Result<Self, CliError> {
            let socket_path = socket_path.into();
            if !socket_path.is_absolute() || timeout.is_zero() || timeout > Duration::from_secs(30)
            {
                return Err(CliError::ConnectionFailed);
            }
            Ok(Self {
                socket_path,
                timeout,
            })
        }

        pub fn stream_jsonl(
            &self,
            request: &ControlRequest,
            output: &mut impl Write,
        ) -> Result<(), CliError> {
            let mut stream =
                UnixStream::connect(&self.socket_path).map_err(|_| CliError::ConnectionFailed)?;
            stream
                .set_write_timeout(Some(self.timeout))
                .map_err(|_| CliError::ConnectionFailed)?;
            FrameCodec::write_to(&mut stream, &WireFrame::Request(request.clone()))
                .map_err(|_| CliError::RequestFailed)?;
            loop {
                match FrameCodec::read_from(&mut stream) {
                    Ok(WireFrame::Stream(frame)) => {
                        serde_json::to_writer(&mut *output, &frame)
                            .map_err(|_| CliError::InvalidResponse)?;
                        output
                            .write_all(b"\n")
                            .map_err(|_| CliError::InvalidResponse)?;
                        output.flush().map_err(|_| CliError::InvalidResponse)?;
                        if !matches!(frame.kind(), nethop_protocol::StreamKind::Item) {
                            return Ok(());
                        }
                    }
                    _ => return Err(CliError::InvalidResponse),
                }
            }
        }
    }

    impl ControlTransport for UnixControlTransport {
        fn exchange(&mut self, request: &ControlRequest) -> Result<ControlResponse, CliError> {
            let mut stream =
                UnixStream::connect(&self.socket_path).map_err(|_| CliError::ConnectionFailed)?;
            stream
                .set_read_timeout(Some(self.timeout))
                .map_err(|_| CliError::ConnectionFailed)?;
            stream
                .set_write_timeout(Some(self.timeout))
                .map_err(|_| CliError::ConnectionFailed)?;
            FrameCodec::write_to(&mut stream, &WireFrame::Request(request.clone()))
                .map_err(|_| CliError::RequestFailed)?;
            match FrameCodec::read_from(&mut stream) {
                Ok(WireFrame::Response(response)) => Ok(response),
                _ => Err(CliError::InvalidResponse),
            }
        }
    }
}

#[cfg(unix)]
pub use unix::UnixControlTransport;
