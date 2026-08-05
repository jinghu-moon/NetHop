use std::{
    ffi::OsStr,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use nethop_core::CoreError;
use thiserror::Error;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_OUTPUT_LIMIT: usize = 64 * 1024;
const MAX_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_OUTPUT_LIMIT: usize = 1024 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerDiagnosticCode {
    InvalidPolicy,
    InvalidBinary,
    InvalidCandidatePath,
    SpawnFailed,
    WaitFailed,
    OutputReadFailed,
    CheckTimedOut,
    CheckFailed,
}

impl RunnerDiagnosticCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidPolicy => "runner_invalid_policy",
            Self::InvalidBinary => "runner_invalid_binary",
            Self::InvalidCandidatePath => "runner_invalid_candidate_path",
            Self::SpawnFailed => "sing_box_check_spawn_failed",
            Self::WaitFailed => "sing_box_check_wait_failed",
            Self::OutputReadFailed => "sing_box_check_output_read_failed",
            Self::CheckTimedOut => "sing_box_check_timed_out",
            Self::CheckFailed => "sing_box_check_failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunnerLimits {
    timeout: Duration,
    output_bytes_per_stream: usize,
}

impl RunnerLimits {
    pub fn new(timeout: Duration, output_bytes_per_stream: usize) -> Result<Self, RunnerError> {
        if timeout.is_zero()
            || timeout > MAX_TIMEOUT
            || output_bytes_per_stream == 0
            || output_bytes_per_stream > MAX_OUTPUT_LIMIT
        {
            return Err(RunnerError::InvalidPolicy);
        }
        Ok(Self {
            timeout,
            output_bytes_per_stream,
        })
    }

    pub const fn timeout(self) -> Duration {
        self.timeout
    }

    pub const fn output_bytes_per_stream(self) -> usize {
        self.output_bytes_per_stream
    }
}

impl Default for RunnerLimits {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            output_bytes_per_stream: DEFAULT_OUTPUT_LIMIT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckOutputSummary {
    total_bytes: usize,
    retained_bytes: usize,
    truncated: bool,
}

impl CheckOutputSummary {
    pub const fn total_bytes(self) -> usize {
        self.total_bytes
    }

    pub const fn retained_bytes(self) -> usize {
        self.retained_bytes
    }

    pub const fn truncated(self) -> bool {
        self.truncated
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckReport {
    elapsed: Duration,
    stdout: CheckOutputSummary,
    stderr: CheckOutputSummary,
}

impl CheckReport {
    pub const fn elapsed(&self) -> Duration {
        self.elapsed
    }

    pub const fn stdout(&self) -> CheckOutputSummary {
        self.stdout
    }

    pub const fn stderr(&self) -> CheckOutputSummary {
        self.stderr
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RunnerError {
    #[error("runner limits are outside the allowed bounds")]
    InvalidPolicy,
    #[error("sing-box binary must be an absolute regular non-symlink file")]
    InvalidBinary,
    #[error("config must be a regular config.json inside a managed candidate or sealed generation")]
    InvalidCandidatePath,
    #[error("sing-box check could not be started")]
    SpawnFailed,
    #[error("sing-box check process could not be observed")]
    WaitFailed,
    #[error("sing-box check output could not be drained")]
    OutputReadFailed,
    #[error("sing-box check exceeded its timeout")]
    TimedOut {
        stdout: CheckOutputSummary,
        stderr: CheckOutputSummary,
    },
    #[error("sing-box check rejected the candidate")]
    CheckFailed {
        exit_code: Option<i32>,
        stdout: CheckOutputSummary,
        stderr: CheckOutputSummary,
    },
}

impl RunnerError {
    pub const fn code(&self) -> RunnerDiagnosticCode {
        match self {
            Self::InvalidPolicy => RunnerDiagnosticCode::InvalidPolicy,
            Self::InvalidBinary => RunnerDiagnosticCode::InvalidBinary,
            Self::InvalidCandidatePath => RunnerDiagnosticCode::InvalidCandidatePath,
            Self::SpawnFailed => RunnerDiagnosticCode::SpawnFailed,
            Self::WaitFailed => RunnerDiagnosticCode::WaitFailed,
            Self::OutputReadFailed => RunnerDiagnosticCode::OutputReadFailed,
            Self::TimedOut { .. } => RunnerDiagnosticCode::CheckTimedOut,
            Self::CheckFailed { .. } => RunnerDiagnosticCode::CheckFailed,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SingBoxCheckRunner {
    binary: PathBuf,
    candidate_root: PathBuf,
    limits: RunnerLimits,
}

impl SingBoxCheckRunner {
    pub fn new(
        binary: impl Into<PathBuf>,
        candidate_root: impl Into<PathBuf>,
        limits: RunnerLimits,
    ) -> Result<Self, RunnerError> {
        let binary = checked_regular_file(binary.into(), RunnerError::InvalidBinary)?;
        let candidate_root = candidate_root.into();
        let metadata =
            fs::symlink_metadata(&candidate_root).map_err(|_| RunnerError::InvalidCandidatePath)?;
        if !candidate_root.is_absolute() || !metadata.is_dir() || metadata.file_type().is_symlink()
        {
            return Err(RunnerError::InvalidCandidatePath);
        }
        let candidate_root = candidate_root
            .canonicalize()
            .map_err(|_| RunnerError::InvalidCandidatePath)?;
        Ok(Self {
            binary,
            candidate_root,
            limits,
        })
    }

    pub fn check_candidate(&self, config_path: &Path) -> Result<CheckReport, RunnerError> {
        let config_path = self.validate_candidate_path(config_path)?;
        let started = Instant::now();
        let mut command = Command::new(&self.binary);
        command
            .arg("check")
            .arg("-c")
            .arg(&config_path)
            .current_dir(config_path.parent().expect("validated config has parent"))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|_| RunnerError::SpawnFailed)?;
        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                terminate_child(&mut child);
                return Err(RunnerError::SpawnFailed);
            }
        };
        let stderr = match child.stderr.take() {
            Some(stderr) => stderr,
            None => {
                terminate_child(&mut child);
                return Err(RunnerError::SpawnFailed);
            }
        };
        let stdout_reader = match spawn_reader(stdout, self.limits.output_bytes_per_stream) {
            Ok(reader) => reader,
            Err(error) => {
                terminate_child(&mut child);
                return Err(error);
            }
        };
        let stderr_reader = match spawn_reader(stderr, self.limits.output_bytes_per_stream) {
            Ok(reader) => reader,
            Err(error) => {
                terminate_child(&mut child);
                let _ = join_reader(stdout_reader);
                return Err(error);
            }
        };

        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started.elapsed() < self.limits.timeout => {
                    thread::sleep(POLL_INTERVAL);
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let stdout = join_reader(stdout_reader)?;
                    let stderr = join_reader(stderr_reader)?;
                    return Err(RunnerError::TimedOut { stdout, stderr });
                }
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = join_reader(stdout_reader);
                    let _ = join_reader(stderr_reader);
                    return Err(RunnerError::WaitFailed);
                }
            }
        };
        let stdout = join_reader(stdout_reader)?;
        let stderr = join_reader(stderr_reader)?;
        result_from_status(status, started.elapsed(), stdout, stderr)
    }

    pub fn validate_for_publish(&self, config_path: &Path, _bytes: &[u8]) -> Result<(), CoreError> {
        self.check_candidate(config_path)
            .map(|_| ())
            .map_err(|_| CoreError::ValidationFailed)
    }

    fn validate_candidate_path(&self, config_path: &Path) -> Result<PathBuf, RunnerError> {
        if !config_path.is_absolute() || config_path.file_name() != Some(OsStr::new("config.json"))
        {
            return Err(RunnerError::InvalidCandidatePath);
        }
        let parent = config_path
            .parent()
            .ok_or(RunnerError::InvalidCandidatePath)?;
        let parent_name = parent
            .file_name()
            .and_then(OsStr::to_str)
            .ok_or(RunnerError::InvalidCandidatePath)?;
        if !is_managed_generation_directory(parent_name) {
            return Err(RunnerError::InvalidCandidatePath);
        }
        let parent_metadata =
            fs::symlink_metadata(parent).map_err(|_| RunnerError::InvalidCandidatePath)?;
        if !parent_metadata.is_dir() || parent_metadata.file_type().is_symlink() {
            return Err(RunnerError::InvalidCandidatePath);
        }
        let canonical =
            checked_regular_file(config_path.to_path_buf(), RunnerError::InvalidCandidatePath)?;
        if canonical
            .parent()
            .and_then(Path::parent)
            .is_none_or(|root| root != self.candidate_root)
        {
            return Err(RunnerError::InvalidCandidatePath);
        }
        Ok(canonical)
    }

    #[cfg(test)]
    fn command_arguments(
        &self,
        config_path: &Path,
    ) -> Result<Vec<std::ffi::OsString>, RunnerError> {
        let config_path = self.validate_candidate_path(config_path)?;
        Ok(vec![
            std::ffi::OsString::from("check"),
            std::ffi::OsString::from("-c"),
            config_path.into_os_string(),
        ])
    }
}

fn is_managed_generation_directory(name: &str) -> bool {
    if name
        .strip_prefix(".candidate-")
        .is_some_and(|suffix| !suffix.is_empty())
    {
        return true;
    }
    name.parse::<u64>()
        .ok()
        .filter(|generation| *generation != 0)
        .is_some_and(|generation| generation.to_string() == name)
}

fn terminate_child(child: &mut std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

pub(crate) fn checked_regular_file(
    path: PathBuf,
    error: RunnerError,
) -> Result<PathBuf, RunnerError> {
    let metadata = fs::symlink_metadata(&path).map_err(|_| error.clone())?;
    if !path.is_absolute() || !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(error);
    }
    path.canonicalize().map_err(|_| error)
}

pub(crate) fn spawn_reader<R: Read + Send + 'static>(
    reader: R,
    limit: usize,
) -> Result<thread::JoinHandle<io::Result<CheckOutputSummary>>, RunnerError> {
    thread::Builder::new()
        .name("nethop-check-output".into())
        .spawn(move || drain_bounded(reader, limit))
        .map_err(|_| RunnerError::OutputReadFailed)
}

pub(crate) fn join_reader(
    reader: thread::JoinHandle<io::Result<CheckOutputSummary>>,
) -> Result<CheckOutputSummary, RunnerError> {
    reader
        .join()
        .map_err(|_| RunnerError::OutputReadFailed)?
        .map_err(|_| RunnerError::OutputReadFailed)
}

fn drain_bounded(mut reader: impl Read, limit: usize) -> io::Result<CheckOutputSummary> {
    let mut buffer = [0_u8; 8192];
    let mut retained = Vec::with_capacity(limit.min(buffer.len()));
    let mut total_bytes = 0_usize;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total_bytes = total_bytes.saturating_add(read);
        let remaining = limit.saturating_sub(retained.len());
        retained.extend_from_slice(&buffer[..read.min(remaining)]);
    }
    Ok(CheckOutputSummary {
        total_bytes,
        retained_bytes: retained.len(),
        truncated: total_bytes > limit,
    })
}

fn result_from_status(
    status: ExitStatus,
    elapsed: Duration,
    stdout: CheckOutputSummary,
    stderr: CheckOutputSummary,
) -> Result<CheckReport, RunnerError> {
    if status.success() {
        Ok(CheckReport {
            elapsed,
            stdout,
            stderr,
        })
    } else {
        Err(RunnerError::CheckFailed {
            exit_code: status.code(),
            stdout,
            stderr,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Cursor, time::Duration};

    use tempfile::tempdir;

    use super::{
        RunnerDiagnosticCode, RunnerError, RunnerLimits, SingBoxCheckRunner, drain_bounded,
    };

    fn runner_fixture() -> (tempfile::TempDir, SingBoxCheckRunner, std::path::PathBuf) {
        let directory = tempdir().unwrap();
        let generations = directory.path().join("generations");
        let candidate = generations.join(".candidate-1-1");
        fs::create_dir_all(&candidate).unwrap();
        let config = candidate.join("config.json");
        fs::write(&config, b"{}").unwrap();
        let binary = std::env::current_exe().unwrap();
        let runner = SingBoxCheckRunner::new(binary, generations, RunnerLimits::default()).unwrap();
        (directory, runner, config)
    }

    #[test]
    fn command_arguments_are_fixed_to_sing_box_check() {
        let (_directory, runner, config) = runner_fixture();
        let args = runner.command_arguments(&config).unwrap();
        assert_eq!(args[0], "check");
        assert_eq!(args[1], "-c");
        assert_eq!(args[2], config.canonicalize().unwrap());
    }

    #[test]
    fn sealed_generation_is_accepted_but_unmanaged_paths_are_rejected() {
        let (directory, runner, _config) = runner_fixture();
        let stable = directory.path().join("generations/1/config.json");
        fs::create_dir_all(stable.parent().unwrap()).unwrap();
        fs::write(&stable, b"{}").unwrap();
        let args = runner.command_arguments(&stable).unwrap();
        assert_eq!(args[2], stable.canonicalize().unwrap());

        let malformed = directory.path().join("generations/01/config.json");
        fs::create_dir_all(malformed.parent().unwrap()).unwrap();
        fs::write(&malformed, b"{}").unwrap();
        assert_eq!(
            runner.check_candidate(&malformed).unwrap_err(),
            RunnerError::InvalidCandidatePath
        );

        let outside = directory.path().join(".candidate-2-2/config.json");
        fs::create_dir_all(outside.parent().unwrap()).unwrap();
        fs::write(&outside, b"{}").unwrap();
        assert_eq!(
            runner.check_candidate(&outside).unwrap_err(),
            RunnerError::InvalidCandidatePath
        );
    }

    #[test]
    fn real_process_nonzero_exit_is_reported_without_output_contents() {
        let (_directory, runner, config) = runner_fixture();
        let error = runner.check_candidate(&config).unwrap_err();
        let RunnerError::CheckFailed {
            exit_code,
            stdout,
            stderr,
        } = error
        else {
            panic!("test harness must reject sing-box arguments");
        };
        assert!(exit_code.is_some());
        assert!(stdout.retained_bytes() <= RunnerLimits::default().output_bytes_per_stream());
        assert!(stderr.retained_bytes() <= RunnerLimits::default().output_bytes_per_stream());
    }

    #[test]
    fn output_is_drained_but_retention_is_bounded() {
        let output = drain_bounded(Cursor::new(vec![b'x'; 4096]), 64).unwrap();
        assert_eq!(output.total_bytes(), 4096);
        assert_eq!(output.retained_bytes(), 64);
        assert!(output.truncated());
    }

    #[test]
    fn limits_and_diagnostic_codes_are_stable() {
        assert_eq!(
            RunnerLimits::new(Duration::ZERO, 1).unwrap_err(),
            RunnerError::InvalidPolicy
        );
        assert_eq!(
            RunnerLimits::new(Duration::from_secs(1), 0).unwrap_err(),
            RunnerError::InvalidPolicy
        );
        assert_eq!(
            RunnerDiagnosticCode::CheckTimedOut.as_str(),
            "sing_box_check_timed_out"
        );
    }

    #[cfg(all(unix, not(target_os = "android")))]
    fn unix_runner(
        script: &str,
        limits: RunnerLimits,
    ) -> (tempfile::TempDir, SingBoxCheckRunner, std::path::PathBuf) {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().unwrap();
        let binary = directory.path().join("sing-box");
        fs::write(&binary, script).unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        let generations = directory.path().join("generations");
        let candidate = generations.join(".candidate-1-1");
        fs::create_dir_all(&candidate).unwrap();
        let config = candidate.join("config.json");
        fs::write(&config, b"{}").unwrap();
        let runner = SingBoxCheckRunner::new(binary, generations, limits).unwrap();
        (directory, runner, config)
    }

    #[cfg(all(unix, not(target_os = "android")))]
    #[test]
    fn unix_process_contract_covers_success_and_output_truncation() {
        let script = "#!/bin/sh\n[ \"$1\" = check ] && [ \"$2\" = -c ] || exit 9\ni=0; while [ $i -lt 100 ]; do printf 0123456789; i=$((i+1)); done\n";
        let (_directory, runner, config) = unix_runner(
            script,
            RunnerLimits::new(Duration::from_secs(1), 64).unwrap(),
        );
        let report = runner.check_candidate(&config).unwrap();
        assert_eq!(report.stdout().total_bytes(), 1000);
        assert_eq!(report.stdout().retained_bytes(), 64);
        assert!(report.stdout().truncated());
    }

    #[cfg(all(unix, not(target_os = "android")))]
    #[test]
    fn unix_process_contract_kills_a_timed_out_check() {
        let script = "#!/bin/sh\nexec sleep 5\n";
        let (_directory, runner, config) = unix_runner(
            script,
            RunnerLimits::new(Duration::from_millis(30), 64).unwrap(),
        );
        let error = runner.check_candidate(&config).unwrap_err();
        assert!(matches!(error, RunnerError::TimedOut { .. }));
    }
}
