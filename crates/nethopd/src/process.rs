use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use thiserror::Error;

use crate::runner::{
    CheckOutputSummary, RunnerError, checked_regular_file, join_reader, spawn_reader,
};

const DEFAULT_STOP_TIMEOUT: Duration = Duration::from_secs(3);
const DEFAULT_OUTPUT_LIMIT: usize = 64 * 1024;
const MAX_STOP_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_OUTPUT_LIMIT: usize = 1024 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessDiagnosticCode {
    InvalidPolicy,
    InvalidBinary,
    InvalidGenerationPath,
    SpawnFailed,
    ObserveFailed,
    OutputReadFailed,
    StopFailed,
}

impl ProcessDiagnosticCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidPolicy => "core_process_invalid_policy",
            Self::InvalidBinary => "core_process_invalid_binary",
            Self::InvalidGenerationPath => "core_process_invalid_generation_path",
            Self::SpawnFailed => "core_process_spawn_failed",
            Self::ObserveFailed => "core_process_observe_failed",
            Self::OutputReadFailed => "core_process_output_read_failed",
            Self::StopFailed => "core_process_stop_failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoreProcessLimits {
    stop_timeout: Duration,
    output_bytes_per_stream: usize,
}

impl CoreProcessLimits {
    pub fn new(
        stop_timeout: Duration,
        output_bytes_per_stream: usize,
    ) -> Result<Self, ProcessError> {
        if stop_timeout.is_zero()
            || stop_timeout > MAX_STOP_TIMEOUT
            || output_bytes_per_stream == 0
            || output_bytes_per_stream > MAX_OUTPUT_LIMIT
        {
            return Err(ProcessError::InvalidPolicy);
        }
        Ok(Self {
            stop_timeout,
            output_bytes_per_stream,
        })
    }

    pub const fn stop_timeout(self) -> Duration {
        self.stop_timeout
    }

    pub const fn output_bytes_per_stream(self) -> usize {
        self.output_bytes_per_stream
    }
}

impl Default for CoreProcessLimits {
    fn default() -> Self {
        Self {
            stop_timeout: DEFAULT_STOP_TIMEOUT,
            output_bytes_per_stream: DEFAULT_OUTPUT_LIMIT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessIdentity {
    pid: u32,
    start_time_ticks: Option<u64>,
}

impl ProcessIdentity {
    pub const fn new(pid: u32, start_time_ticks: Option<u64>) -> Option<Self> {
        if pid == 0 {
            None
        } else {
            Some(Self {
                pid,
                start_time_ticks,
            })
        }
    }

    pub const fn pid(self) -> u32 {
        self.pid
    }

    pub const fn start_time_ticks(self) -> Option<u64> {
        self.start_time_ticks
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessExitReport {
    exit_code: Option<i32>,
    stdout: CheckOutputSummary,
    stderr: CheckOutputSummary,
}

impl ProcessExitReport {
    pub const fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }

    pub const fn stdout(&self) -> CheckOutputSummary {
        self.stdout
    }

    pub const fn stderr(&self) -> CheckOutputSummary {
        self.stderr
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopReport {
    forced: bool,
    exit: ProcessExitReport,
}

impl StopReport {
    pub const fn forced(&self) -> bool {
        self.forced
    }

    pub const fn exit(&self) -> &ProcessExitReport {
        &self.exit
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProcessError {
    #[error("core process limits are outside the allowed bounds")]
    InvalidPolicy,
    #[error("sing-box binary must be an absolute regular non-symlink file")]
    InvalidBinary,
    #[error("config must be a regular config.json inside a sealed generation")]
    InvalidGenerationPath,
    #[error("sing-box core could not be started")]
    SpawnFailed,
    #[error("sing-box core process could not be observed")]
    ObserveFailed,
    #[error("sing-box core output could not be drained")]
    OutputReadFailed,
    #[error("sing-box core could not be stopped")]
    StopFailed,
}

impl ProcessError {
    pub const fn code(&self) -> ProcessDiagnosticCode {
        match self {
            Self::InvalidPolicy => ProcessDiagnosticCode::InvalidPolicy,
            Self::InvalidBinary => ProcessDiagnosticCode::InvalidBinary,
            Self::InvalidGenerationPath => ProcessDiagnosticCode::InvalidGenerationPath,
            Self::SpawnFailed => ProcessDiagnosticCode::SpawnFailed,
            Self::ObserveFailed => ProcessDiagnosticCode::ObserveFailed,
            Self::OutputReadFailed => ProcessDiagnosticCode::OutputReadFailed,
            Self::StopFailed => ProcessDiagnosticCode::StopFailed,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CoreProcessRunner {
    binary: PathBuf,
    generations_root: PathBuf,
    limits: CoreProcessLimits,
}

impl CoreProcessRunner {
    pub fn new(
        binary: impl Into<PathBuf>,
        generations_root: impl Into<PathBuf>,
        limits: CoreProcessLimits,
    ) -> Result<Self, ProcessError> {
        let binary = checked_regular_file(binary.into(), RunnerError::InvalidBinary)
            .map_err(|_| ProcessError::InvalidBinary)?;
        let generations_root = generations_root.into();
        let metadata = fs::symlink_metadata(&generations_root)
            .map_err(|_| ProcessError::InvalidGenerationPath)?;
        if !generations_root.is_absolute()
            || !metadata.is_dir()
            || metadata.file_type().is_symlink()
        {
            return Err(ProcessError::InvalidGenerationPath);
        }
        let generations_root = generations_root
            .canonicalize()
            .map_err(|_| ProcessError::InvalidGenerationPath)?;
        Ok(Self {
            binary,
            generations_root,
            limits,
        })
    }

    pub fn start(&self, config_path: &Path) -> Result<RunningCore, ProcessError> {
        let config_path = self.validate_generation_path(config_path)?;
        let mut command = Command::new(&self.binary);
        command
            .arg("run")
            .arg("-c")
            .arg(&config_path)
            .current_dir(config_path.parent().expect("validated config has parent"))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|_| ProcessError::SpawnFailed)?;
        let identity = ProcessIdentity::new(child.id(), process_start_time_ticks(child.id()))
            .expect("spawned child PID is non-zero");
        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                terminate_child(&mut child);
                return Err(ProcessError::SpawnFailed);
            }
        };
        let stderr = match child.stderr.take() {
            Some(stderr) => stderr,
            None => {
                terminate_child(&mut child);
                return Err(ProcessError::SpawnFailed);
            }
        };
        let stdout_reader = match spawn_reader(stdout, self.limits.output_bytes_per_stream) {
            Ok(reader) => reader,
            Err(_) => {
                terminate_child(&mut child);
                return Err(ProcessError::OutputReadFailed);
            }
        };
        let stderr_reader = match spawn_reader(stderr, self.limits.output_bytes_per_stream) {
            Ok(reader) => reader,
            Err(_) => {
                terminate_child(&mut child);
                let _ = join_reader(stdout_reader);
                return Err(ProcessError::OutputReadFailed);
            }
        };
        Ok(RunningCore {
            child: Some(child),
            identity,
            config_path,
            stdout_reader: Some(stdout_reader),
            stderr_reader: Some(stderr_reader),
            stop_timeout: self.limits.stop_timeout,
            exit_report: None,
        })
    }

    fn validate_generation_path(&self, config_path: &Path) -> Result<PathBuf, ProcessError> {
        if !config_path.is_absolute() || config_path.file_name() != Some(OsStr::new("config.json"))
        {
            return Err(ProcessError::InvalidGenerationPath);
        }
        let canonical =
            checked_regular_file(config_path.to_path_buf(), RunnerError::InvalidCandidatePath)
                .map_err(|_| ProcessError::InvalidGenerationPath)?;
        let generation_dir = canonical
            .parent()
            .ok_or(ProcessError::InvalidGenerationPath)?;
        let generation = generation_dir
            .file_name()
            .and_then(OsStr::to_str)
            .and_then(|value| value.parse::<u64>().ok());
        if generation.is_none_or(|value| value == 0)
            || generation_dir.parent() != Some(self.generations_root.as_path())
        {
            return Err(ProcessError::InvalidGenerationPath);
        }
        Ok(canonical)
    }

    #[cfg(test)]
    fn command_arguments(
        &self,
        config_path: &Path,
    ) -> Result<Vec<std::ffi::OsString>, ProcessError> {
        let config_path = self.validate_generation_path(config_path)?;
        Ok(vec![
            std::ffi::OsString::from("run"),
            std::ffi::OsString::from("-c"),
            config_path.into_os_string(),
        ])
    }
}

type OutputReader = thread::JoinHandle<std::io::Result<CheckOutputSummary>>;

#[derive(Debug)]
pub struct RunningCore {
    child: Option<Child>,
    identity: ProcessIdentity,
    config_path: PathBuf,
    stdout_reader: Option<OutputReader>,
    stderr_reader: Option<OutputReader>,
    stop_timeout: Duration,
    exit_report: Option<ProcessExitReport>,
}

impl RunningCore {
    pub const fn identity(&self) -> ProcessIdentity {
        self.identity
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub fn try_exit(&mut self) -> Result<Option<ProcessExitReport>, ProcessError> {
        if let Some(report) = &self.exit_report {
            return Ok(Some(report.clone()));
        }
        let child = self.child.as_mut().ok_or(ProcessError::ObserveFailed)?;
        match child.try_wait().map_err(|_| ProcessError::ObserveFailed)? {
            Some(status) => self.finish(status).map(Some),
            None => Ok(None),
        }
    }

    pub fn stop(mut self) -> Result<StopReport, ProcessError> {
        let result = self.stop_inner();
        if result.is_ok() {
            self.child = None;
        }
        result
    }

    fn stop_inner(&mut self) -> Result<StopReport, ProcessError> {
        if let Some(exit) = &self.exit_report {
            return Ok(StopReport {
                forced: false,
                exit: exit.clone(),
            });
        }
        if let Some(status) = self
            .child
            .as_mut()
            .ok_or(ProcessError::StopFailed)?
            .try_wait()
            .map_err(|_| ProcessError::ObserveFailed)?
        {
            return self.finish(status).map(|exit| StopReport {
                forced: false,
                exit,
            });
        }
        let forced_request =
            request_graceful_stop(self.child.as_mut().ok_or(ProcessError::StopFailed)?)?;
        let started = Instant::now();
        while started.elapsed() < self.stop_timeout {
            if let Some(status) = self
                .child
                .as_mut()
                .ok_or(ProcessError::StopFailed)?
                .try_wait()
                .map_err(|_| ProcessError::ObserveFailed)?
            {
                return self.finish(status).map(|exit| StopReport {
                    forced: forced_request,
                    exit,
                });
            }
            thread::sleep(POLL_INTERVAL);
        }
        let child = self.child.as_mut().ok_or(ProcessError::StopFailed)?;
        child.kill().map_err(|_| ProcessError::StopFailed)?;
        let status = child.wait().map_err(|_| ProcessError::StopFailed)?;
        self.finish(status)
            .map(|exit| StopReport { forced: true, exit })
    }

    fn finish(&mut self, status: ExitStatus) -> Result<ProcessExitReport, ProcessError> {
        if let Some(report) = &self.exit_report {
            return Ok(report.clone());
        }
        self.child.take();
        let stdout = self
            .stdout_reader
            .take()
            .ok_or(ProcessError::OutputReadFailed)
            .and_then(|reader| join_reader(reader).map_err(|_| ProcessError::OutputReadFailed))?;
        let stderr = self
            .stderr_reader
            .take()
            .ok_or(ProcessError::OutputReadFailed)
            .and_then(|reader| join_reader(reader).map_err(|_| ProcessError::OutputReadFailed))?;
        let report = ProcessExitReport {
            exit_code: status.code(),
            stdout,
            stderr,
        };
        self.exit_report = Some(report.clone());
        Ok(report)
    }
}

impl Drop for RunningCore {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(reader) = self.stdout_reader.take() {
            let _ = join_reader(reader);
        }
        if let Some(reader) = self.stderr_reader.take() {
            let _ = join_reader(reader);
        }
    }
}

#[cfg(unix)]
fn request_graceful_stop(child: &mut Child) -> Result<bool, ProcessError> {
    let pid = i32::try_from(child.id()).map_err(|_| ProcessError::StopFailed)?;
    // SAFETY: `pid` is obtained from the owned child and SIGTERM carries no pointer data.
    let result = unsafe { libc::kill(pid, libc::SIGTERM) };
    (result == 0)
        .then_some(false)
        .ok_or(ProcessError::StopFailed)
}

#[cfg(windows)]
fn request_graceful_stop(child: &mut Child) -> Result<bool, ProcessError> {
    child
        .kill()
        .map(|_| true)
        .map_err(|_| ProcessError::StopFailed)
}

fn terminate_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(any(target_os = "android", target_os = "linux"))]
fn process_start_time_ticks(pid: u32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after_name = stat.rsplit_once(") ")?.1;
    after_name.split_whitespace().nth(19)?.parse().ok()
}

#[cfg(not(any(target_os = "android", target_os = "linux")))]
const fn process_start_time_ticks(_pid: u32) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use std::{fs, thread, time::Duration};

    use tempfile::tempdir;

    use super::{CoreProcessLimits, CoreProcessRunner, ProcessError};

    fn runner_fixture() -> (tempfile::TempDir, CoreProcessRunner, std::path::PathBuf) {
        let directory = tempdir().unwrap();
        let generations = directory.path().join("generations");
        let generation = generations.join("1");
        fs::create_dir_all(&generation).unwrap();
        let config = generation.join("config.json");
        fs::write(&config, b"{}").unwrap();
        let runner = CoreProcessRunner::new(
            std::env::current_exe().unwrap(),
            generations,
            CoreProcessLimits::default(),
        )
        .unwrap();
        (directory, runner, config)
    }

    #[test]
    fn command_arguments_are_fixed_to_sing_box_run() {
        let (_directory, runner, config) = runner_fixture();
        let args = runner.command_arguments(&config).unwrap();
        assert_eq!(args[0], "run");
        assert_eq!(args[1], "-c");
        assert_eq!(args[2], config.canonicalize().unwrap());
    }

    #[test]
    fn candidate_and_outside_paths_are_rejected() {
        let (directory, runner, _config) = runner_fixture();
        let candidate = directory
            .path()
            .join("generations/.candidate-2-1/config.json");
        fs::create_dir_all(candidate.parent().unwrap()).unwrap();
        fs::write(&candidate, b"{}").unwrap();
        assert_eq!(
            runner.start(&candidate).unwrap_err(),
            ProcessError::InvalidGenerationPath
        );
    }

    #[test]
    fn limits_are_bounded() {
        assert_eq!(
            CoreProcessLimits::new(Duration::ZERO, 1).unwrap_err(),
            ProcessError::InvalidPolicy
        );
        assert_eq!(
            CoreProcessLimits::new(Duration::from_secs(1), 0).unwrap_err(),
            ProcessError::InvalidPolicy
        );
    }

    #[test]
    fn observed_early_exit_can_be_stopped_idempotently() {
        let (_directory, runner, config) = runner_fixture();
        let mut process = runner.start(&config).unwrap();
        let exit = loop {
            if let Some(exit) = process.try_exit().unwrap() {
                break exit;
            }
            thread::sleep(Duration::from_millis(5));
        };
        assert!(exit.exit_code().is_some());
        let stop = process.stop().unwrap();
        assert_eq!(stop.exit(), &exit);
        assert!(!stop.forced());
    }

    #[cfg(all(unix, not(target_os = "android")))]
    fn unix_runner(
        script: &str,
        stop_timeout: Duration,
    ) -> (tempfile::TempDir, CoreProcessRunner, std::path::PathBuf) {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().unwrap();
        let binary = directory.path().join("sing-box");
        fs::write(&binary, script).unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        let generations = directory.path().join("generations");
        let generation = generations.join("1");
        fs::create_dir_all(&generation).unwrap();
        let config = generation.join("config.json");
        fs::write(&config, b"{}").unwrap();
        let runner = CoreProcessRunner::new(
            binary,
            generations,
            CoreProcessLimits::new(stop_timeout, 64).unwrap(),
        )
        .unwrap();
        (directory, runner, config)
    }

    #[cfg(all(unix, not(target_os = "android")))]
    #[test]
    fn unix_process_contract_stops_gracefully_with_stable_identity() {
        let script = "#!/bin/sh\n[ \"$1\" = run ] && [ \"$2\" = -c ] || exit 9\ntrap 'exit 0' TERM\nwhile :; do :; done\n";
        let (_directory, runner, config) = unix_runner(script, Duration::from_secs(1));
        let mut process = runner.start(&config).unwrap();
        assert!(process.identity().pid() > 0);
        assert!(process.try_exit().unwrap().is_none());
        let report = process.stop().unwrap();
        assert!(!report.forced());
    }

    #[cfg(all(unix, not(target_os = "android")))]
    #[test]
    fn unix_process_contract_force_kills_after_grace_timeout() {
        let script = "#!/bin/sh\ntrap '' TERM\nwhile :; do :; done\n";
        let (_directory, runner, config) = unix_runner(script, Duration::from_millis(30));
        let process = runner.start(&config).unwrap();
        let report = process.stop().unwrap();
        assert!(report.forced());
    }
}
