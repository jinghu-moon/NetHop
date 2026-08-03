#![doc = "Thin, file-independent client for the NetHop local control protocol."]

use nethop_protocol::{ControlMethod, ControlRequest, ControlResponse, RequestId};
use thiserror::Error;

pub const DEFAULT_SOCKET_PATH: &str = "/data/adb/nethop/run/nethopd.sock";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliCommand {
    Status,
    Start,
    Stop,
    Probe,
}

impl CliCommand {
    pub const fn method(self) -> ControlMethod {
        match self {
            Self::Status => ControlMethod::StatusGet,
            Self::Start => ControlMethod::ServiceStart,
            Self::Stop => ControlMethod::ServiceStop,
            Self::Probe => ControlMethod::CapabilityProbe,
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
        _ => return Err(CliError::Usage),
    };
    if arguments.next().is_some() {
        return Err(CliError::Usage);
    }
    Ok(command)
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

pub fn render_response(response: &ControlResponse) -> Result<String, CliError> {
    serde_json::to_string(response).map_err(|_| CliError::InvalidResponse)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum CliError {
    #[error("usage: nethopctl <status|start|stop|probe>")]
    Usage,
    #[error("control socket is unavailable")]
    ConnectionFailed,
    #[error("control protocol request could not be sent")]
    RequestFailed,
    #[error("control protocol response is invalid")]
    InvalidResponse,
    #[error("control response request ID does not match")]
    ResponseRequestMismatch,
    #[error("this platform does not provide Unix domain sockets")]
    UnsupportedPlatform,
}

#[cfg(unix)]
mod unix {
    use std::{os::unix::net::UnixStream, path::PathBuf, time::Duration};

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
