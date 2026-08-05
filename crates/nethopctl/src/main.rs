#[cfg(unix)]
use std::io::Read;
use std::{process::ExitCode, time::Duration};

#[cfg(unix)]
use nethop_protocol::MAX_FRAME_BYTES;
use nethop_protocol::RequestId;
#[cfg(unix)]
use nethopctl::{CliCommand, build_request, execute_with_input, render_response};
use nethopctl::{CliError, DEFAULT_SOCKET_PATH, parse_invocation};

fn main() -> ExitCode {
    match run() {
        Ok(success) => {
            if success {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<bool, CliError> {
    let invocation = parse_invocation(std::env::args().skip(1))?;
    let request_id = RequestId::new(format!("ctl-{}", std::process::id()))
        .map_err(|_| CliError::RequestFailed)?;
    #[cfg(unix)]
    {
        let input = if matches!(
            invocation.command(),
            CliCommand::ConfigValidate | CliCommand::ConfigApply | CliCommand::ConfigMutate
        ) {
            Some(read_json_stdin()?)
        } else {
            None
        };
        let timeout = if invocation.wait() {
            Duration::from_secs(30)
        } else {
            Duration::from_secs(5)
        };
        let mut transport = nethopctl::UnixControlTransport::new(DEFAULT_SOCKET_PATH, timeout)?;
        if invocation.command() == CliCommand::Events {
            let request = build_request(&invocation, request_id, None)?;
            transport.stream_jsonl(&request, &mut std::io::stdout().lock())?;
            return Ok(true);
        }
        let response = execute_with_input(&mut transport, invocation, request_id, input)?;
        println!("{}", render_response(&response)?);
        Ok(response.ok())
    }
    #[cfg(not(unix))]
    {
        let _ = (
            invocation,
            request_id,
            DEFAULT_SOCKET_PATH,
            Duration::from_secs(5),
        );
        Err(CliError::UnsupportedPlatform)
    }
}

#[cfg(unix)]
fn read_json_stdin() -> Result<serde_json::Value, CliError> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .take((MAX_FRAME_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| CliError::InvalidInput)?;
    if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
        return Err(CliError::InvalidInput);
    }
    serde_json::from_slice(&bytes).map_err(|_| CliError::InvalidInput)
}
