#[cfg(unix)]
use std::io::Read;
use std::{process::ExitCode, time::Duration};

#[cfg(unix)]
use nethop_protocol::MAX_FRAME_BYTES;
use nethop_protocol::RequestId;
#[cfg(unix)]
use nethopctl::{
    CliCommand, build_request, control_timeout, execute_with_input, render_response,
    render_status_human,
};
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
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments
        .iter()
        .any(|argument| matches!(argument.as_str(), "--help" | "-h"))
    {
        print_help(&arguments);
        return Ok(true);
    }
    if arguments
        .iter()
        .any(|argument| matches!(argument.as_str(), "--version" | "-V"))
    {
        println!("nethopctl {}", env!("CARGO_PKG_VERSION"));
        return Ok(true);
    }
    #[cfg(unix)]
    if let Some(session_id) = nethopctl::parse_event_termination(&arguments)? {
        let terminated = nethopctl::terminate_event_session(&session_id)?;
        println!(
            "{}",
            serde_json::json!({ "version": 2, "ok": true, "result": { "terminated": terminated } })
        );
        return Ok(true);
    }
    let invocation = parse_invocation(&arguments)?;
    let request_id = RequestId::new(format!("ctl-{}", std::process::id()))
        .map_err(|_| CliError::RequestFailed)?;
    #[cfg(unix)]
    {
        let input = if matches!(
            invocation.command(),
            CliCommand::SubscriptionImportPreview
                | CliCommand::SubscriptionImportApply
                | CliCommand::NodeImport
        ) {
            Some(read_import_input(&invocation)?)
        } else if matches!(invocation.command(), CliCommand::BackupRestore) {
            Some(read_backup_input(&invocation)?)
        } else if matches!(
            invocation.command(),
            CliCommand::ConfigValidate
                | CliCommand::ConfigApply
                | CliCommand::ConfigMutate
                | CliCommand::SubscriptionEdit
                | CliCommand::NodeImport
                | CliCommand::NodeEdit
                | CliCommand::NodeOverrideApply
        ) {
            Some(read_json_input(&invocation)?)
        } else {
            None
        };
        let timeout = control_timeout(invocation.command(), invocation.wait());
        let mut transport = nethopctl::UnixControlTransport::new(DEFAULT_SOCKET_PATH, timeout)?;
        if matches!(
            invocation.command(),
            CliCommand::Events | CliCommand::LogsTail | CliCommand::TrafficLive
        ) {
            let request = build_request(&invocation, request_id, None)?;
            let max_runtime = invocation
                .event_max_runtime_seconds()
                .map(Duration::from_secs);
            transport.stream_jsonl(&request, &mut std::io::stdout().lock(), max_runtime)?;
            return Ok(true);
        }
        let response = execute_with_input(&mut transport, &invocation, request_id, input)?;
        if invocation.command() == CliCommand::BackupExport && response.ok() {
            write_backup_output(
                &invocation,
                response.result().ok_or(CliError::InvalidResponse)?,
            )?;
            println!("backup exported");
        } else if invocation.command() == CliCommand::Status && invocation.human() {
            println!("{}", render_status_human(&response)?);
        } else {
            println!("{}", render_response(&response)?);
        }
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

fn print_help(arguments: &[String]) {
    let topic = arguments
        .iter()
        .find(|argument| !argument.starts_with('-'))
        .map(String::as_str);
    match topic {
        Some("service") => println!("nethopctl service <check|status|restart|reload> [--wait]"),
        Some("config") => {
            println!("nethopctl config <check|get|schema|validate|apply|mutate|reload>")
        }
        Some("subscription") => println!(
            "nethopctl subscription <list|show|inspect|edit|diagnose|history|update|update-all|mode|select|add|remove|move|enable|disable|import>"
        ),
        Some("node") => println!(
            "nethopctl node <list|current|show|test|test-all|selection|select|use|delay|import|edit|remove|export|override>"
        ),
        Some("application") => println!(
            "nethopctl application <list|users|policy set|mode|add-package|remove-package|add-uid|remove-uid>"
        ),
        Some("network") => println!("nethopctl network <status|evaluate|set>"),
        Some("capture") => println!("nethopctl capture <status|check|enable|disable>"),
        Some("ruleset") => println!("nethopctl ruleset <list|show|status|update>"),
        Some("logs") => println!("nethopctl logs <get|tail|clear|export>"),
        Some("traffic") => println!("nethopctl traffic [live]"),
        _ => println!(
            "nethopctl commands:\n  service  config  subscription  node  application\n  network  capture  ruleset  logs  traffic\n  status  diagnose  topology  metrics  backup  events\n\nUse `nethopctl <command> --help` for command details."
        ),
    }
}

#[cfg(unix)]
fn read_import_input(invocation: &nethopctl::CliInvocation) -> Result<serde_json::Value, CliError> {
    const MAX_IMPORT_BYTES: usize = 768 * 1024;
    let bytes = if let Some(path) = invocation.input_file() {
        let path = std::path::Path::new(path);
        let metadata = std::fs::symlink_metadata(path).map_err(|_| CliError::InvalidInput)?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() as usize > MAX_IMPORT_BYTES
        {
            return Err(CliError::InvalidInput);
        }
        std::fs::read(path).map_err(|_| CliError::InvalidInput)?
    } else if invocation.text_input() {
        let mut bytes = Vec::new();
        std::io::stdin()
            .take((MAX_IMPORT_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| CliError::InvalidInput)?;
        bytes
    } else {
        return Err(CliError::InputRequired);
    };
    if bytes.is_empty() || bytes.len() > MAX_IMPORT_BYTES {
        return Err(CliError::InvalidInput);
    }
    let content = String::from_utf8(bytes).map_err(|_| CliError::InvalidInput)?;
    Ok(serde_json::json!({
        "content": content,
        "format_hint": invocation.import_format().unwrap_or("auto"),
    }))
}

#[cfg(unix)]
fn read_json_input(invocation: &nethopctl::CliInvocation) -> Result<serde_json::Value, CliError> {
    let bytes = if let Some(path) = invocation.input_file() {
        let path = std::path::Path::new(path);
        let metadata = std::fs::symlink_metadata(path).map_err(|_| CliError::InvalidInput)?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() as usize > MAX_FRAME_BYTES
        {
            return Err(CliError::InvalidInput);
        }
        std::fs::read(path).map_err(|_| CliError::InvalidInput)?
    } else {
        let mut bytes = Vec::new();
        std::io::stdin()
            .take((MAX_FRAME_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| CliError::InvalidInput)?;
        bytes
    };
    if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
        return Err(CliError::InvalidInput);
    }
    serde_json::from_slice(&bytes).map_err(|_| CliError::InvalidInput)
}

#[cfg(unix)]
fn read_backup_input(invocation: &nethopctl::CliInvocation) -> Result<serde_json::Value, CliError> {
    let path = invocation.input_file().ok_or(CliError::InputRequired)?;
    let path = std::path::Path::new(path);
    let metadata = std::fs::symlink_metadata(path).map_err(|_| CliError::InvalidInput)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() as usize > MAX_FRAME_BYTES
    {
        return Err(CliError::InvalidInput);
    }
    let bytes = std::fs::read(path).map_err(|_| CliError::InvalidInput)?;
    let backup: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| CliError::InvalidInput)?;
    if backup.get("format") != Some(&serde_json::json!("nethop-config-backup-v1"))
        || !backup
            .get("document")
            .is_some_and(serde_json::Value::is_object)
    {
        return Err(CliError::InvalidInput);
    }
    Ok(backup["document"].clone())
}

#[cfg(unix)]
fn write_backup_output(
    invocation: &nethopctl::CliInvocation,
    result: &serde_json::Value,
) -> Result<(), CliError> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let path = invocation.input_file().ok_or(CliError::Usage)?;
    let path = std::path::Path::new(path);
    let metadata = std::fs::symlink_metadata(path);
    if metadata.is_ok() {
        return Err(CliError::InvalidInput);
    }
    if let Some(parent) = path.parent() {
        if !parent.is_dir() {
            return Err(CliError::InvalidInput);
        }
    }
    let bytes = serde_json::to_vec_pretty(result).map_err(|_| CliError::InvalidResponse)?;
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(CliError::InvalidResponse);
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| CliError::InvalidInput)?;
    file.write_all(&bytes).map_err(|_| CliError::InvalidInput)?;
    file.sync_all().map_err(|_| CliError::InvalidInput)
}
