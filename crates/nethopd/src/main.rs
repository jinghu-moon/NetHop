use std::process::ExitCode;

use nethopd::{ApplicationError, DaemonArguments, DaemonMode, RuntimeRoot, run_system_supervisor};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), ApplicationError> {
    let arguments = DaemonArguments::parse(std::env::args_os().skip(1))?;
    let runtime = RuntimeRoot::open(arguments.root())?;
    match arguments.mode() {
        DaemonMode::Supervise => run_system_supervisor(&runtime),
        DaemonMode::Worker => Err(ApplicationError::UnsupportedPlatform),
    }
}
