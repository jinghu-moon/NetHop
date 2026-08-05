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
    let command_length = usize::from(
        arguments
            .first()
            .is_some_and(|value| matches!(value.as_str(), "config" | "capability")),
    ) + 1;
    if arguments.len() < command_length {
        return Err(CliError::Usage);
    }
    let command = parse_command(arguments[..command_length].iter().map(String::as_str))?;
    let mut wait = false;
    let mut if_needed = false;
    let mut expected_digest = None;
    let mut manager_version = None;
    let mut protocol_min = None;
    let mut protocol_max = None;
    let mut event_kinds = Vec::new();
    let mut options = arguments[command_length..].iter();
    while let Some(option) = options.next() {
        match option.as_str() {
            "--wait" if !wait => wait = true,
            "--if-needed" if !if_needed => if_needed = true,
            "--json" | "--jsonl" => {}
            "--expected-digest" if expected_digest.is_none() => {
                expected_digest = options.next().cloned();
                if expected_digest.is_none() {
                    return Err(CliError::Usage);
                }
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
            _ => return Err(CliError::Usage),
        }
    }
    let wait_allowed = matches!(
        command,
        CliCommand::Start | CliCommand::Stop | CliCommand::Update | CliCommand::ConfigReload
    );
    if (wait && !wait_allowed) || (if_needed && command != CliCommand::Update) {
        return Err(CliError::Usage);
    }
    let needs_digest = matches!(
        command,
        CliCommand::ConfigValidate | CliCommand::ConfigApply | CliCommand::ConfigMutate
    );
    if expected_digest.is_some() != needs_digest {
        return Err(CliError::Usage);
    }
    let hello = command == CliCommand::ProtocolHello;
    if manager_version.is_some() != hello
        || protocol_min.is_some() != hello
        || protocol_max.is_some() != hello
    {
        return Err(CliError::Usage);
    }
    if !event_kinds.is_empty() && command != CliCommand::Events {
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
    invocation: CliInvocation,
    request_id: RequestId,
    input: Option<Value>,
) -> Result<ControlResponse, CliError> {
    let request = build_request(&invocation, request_id.clone(), input)?;
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
        CliCommand::ConfigValidate | CliCommand::ConfigApply => ControlParams::config_document(
            invocation.expected_digest.clone().ok_or(CliError::Usage)?,
            input.ok_or(CliError::InputRequired)?,
        ),
        CliCommand::ConfigMutate => ControlParams::mutation(
            invocation.expected_digest.clone().ok_or(CliError::Usage)?,
            serde_json::from_value::<ConfigMutation>(input.ok_or(CliError::InputRequired)?)
                .map_err(|_| CliError::InvalidInput)?,
        ),
        CliCommand::ProtocolHello => ControlParams::hello(
            invocation.manager_version.clone().ok_or(CliError::Usage)?,
            invocation.protocol_min.ok_or(CliError::Usage)?,
            invocation.protocol_max.ok_or(CliError::Usage)?,
        ),
        CliCommand::Events => ControlParams::event_subscription(invocation.event_kinds.clone()),
        _ => ControlParams::new(invocation.wait, invocation.if_needed),
    };
    ControlRequest::new(request_id, invocation.command.method())
        .with_params(params)
        .map_err(|_| CliError::RequestFailed)
}

pub fn render_response(response: &ControlResponse) -> Result<String, CliError> {
    serde_json::to_string(response).map_err(|_| CliError::InvalidResponse)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum CliError {
    #[error(
        "usage: nethopctl <status|start|stop|probe|update [--if-needed] [--wait]|config reload [--wait]>"
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
