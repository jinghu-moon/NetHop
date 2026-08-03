use std::{process::ExitCode, time::Duration};

use nethop_protocol::RequestId;
use nethopctl::{CliError, DEFAULT_SOCKET_PATH, parse_command};
#[cfg(unix)]
use nethopctl::{execute, render_response};

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
    let command = parse_command(std::env::args().skip(1))?;
    let request_id = RequestId::new(format!("ctl-{}", std::process::id()))
        .map_err(|_| CliError::RequestFailed)?;
    #[cfg(unix)]
    {
        let mut transport =
            nethopctl::UnixControlTransport::new(DEFAULT_SOCKET_PATH, Duration::from_secs(5))?;
        let response = execute(&mut transport, command, request_id)?;
        println!("{}", render_response(&response)?);
        Ok(response.ok())
    }
    #[cfg(not(unix))]
    {
        let _ = (
            command,
            request_id,
            DEFAULT_SOCKET_PATH,
            Duration::from_secs(5),
        );
        Err(CliError::UnsupportedPlatform)
    }
}
