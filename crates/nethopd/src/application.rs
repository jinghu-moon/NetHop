use std::{
    ffi::{OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

use thiserror::Error;

use crate::{
    RestartPolicy, SupervisorError, SystemWorkerBackend, WorkerProcessBackend, WorkerSupervisor,
};

const SUPERVISOR_POLL_INTERVAL: Duration = Duration::from_millis(250);
static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonMode {
    Supervise,
    Worker,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonArguments {
    mode: DaemonMode,
    root: PathBuf,
}

impl DaemonArguments {
    pub fn parse<I, S>(arguments: I) -> Result<Self, ApplicationError>
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        let mut arguments = arguments.into_iter().map(Into::into);
        let mode = match arguments.next().as_deref() {
            Some(value) if value == OsStr::new("--supervise") => DaemonMode::Supervise,
            Some(value) if value == OsStr::new("--worker") => DaemonMode::Worker,
            _ => return Err(ApplicationError::Usage),
        };
        if arguments.next().as_deref() != Some(OsStr::new("--root")) {
            return Err(ApplicationError::Usage);
        }
        let root = arguments.next().ok_or(ApplicationError::Usage)?;
        if arguments.next().is_some() {
            return Err(ApplicationError::Usage);
        }
        Ok(Self {
            mode,
            root: PathBuf::from(root),
        })
    }

    pub const fn mode(&self) -> DaemonMode {
        self.mode
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeRoot {
    root: PathBuf,
    run: PathBuf,
}

impl RuntimeRoot {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, ApplicationError> {
        let root = checked_directory(root.into())?;
        let run = checked_directory(root.join("run"))?;
        Ok(Self { root, run })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn run(&self) -> &Path {
        &self.run
    }

    pub fn supervisor_pid_path(&self) -> PathBuf {
        self.run.join("supervisor.pid")
    }

    pub fn worker_arguments(&self) -> Vec<OsString> {
        vec![
            OsString::from("--worker"),
            OsString::from("--root"),
            self.root.as_os_str().to_owned(),
        ]
    }
}

fn checked_directory(path: PathBuf) -> Result<PathBuf, ApplicationError> {
    if !path.is_absolute() {
        return Err(ApplicationError::InvalidRuntimeRoot);
    }
    let metadata = fs::symlink_metadata(&path).map_err(|_| ApplicationError::InvalidRuntimeRoot)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(ApplicationError::InvalidRuntimeRoot);
    }
    let canonical = path
        .canonicalize()
        .map_err(|_| ApplicationError::InvalidRuntimeRoot)?;
    if canonical != path {
        return Err(ApplicationError::InvalidRuntimeRoot);
    }
    Ok(path)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupervisorLoopSignal {
    Wake,
    Stop,
}

pub trait SupervisorLoopDriver {
    fn now(&self) -> Duration;
    fn wait(&mut self, timeout: Duration) -> SupervisorLoopSignal;
}

pub fn run_supervisor_loop<B, D>(
    supervisor: &mut WorkerSupervisor<B>,
    driver: &mut D,
) -> Result<(), ApplicationError>
where
    B: WorkerProcessBackend,
    D: SupervisorLoopDriver,
{
    loop {
        let now = driver.now();
        if let Err(error) = supervisor.tick(now) {
            let _ = supervisor.stop();
            return Err(error.into());
        }
        let timeout = supervisor
            .next_action()
            .map_or(SUPERVISOR_POLL_INTERVAL, |deadline| {
                deadline.saturating_sub(now).min(SUPERVISOR_POLL_INTERVAL)
            });
        if driver.wait(timeout) == SupervisorLoopSignal::Stop {
            supervisor.stop()?;
            return Ok(());
        }
    }
}

#[derive(Debug)]
pub struct SystemSupervisorDriver {
    started: Instant,
}

impl SystemSupervisorDriver {
    pub fn install() -> Result<Self, ApplicationError> {
        STOP_REQUESTED.store(false, Ordering::Release);
        install_signal_handlers()?;
        Ok(Self {
            started: Instant::now(),
        })
    }
}

impl SupervisorLoopDriver for SystemSupervisorDriver {
    fn now(&self) -> Duration {
        self.started.elapsed()
    }

    fn wait(&mut self, timeout: Duration) -> SupervisorLoopSignal {
        if STOP_REQUESTED.load(Ordering::Acquire) {
            return SupervisorLoopSignal::Stop;
        }
        thread::sleep(timeout);
        if STOP_REQUESTED.load(Ordering::Acquire) {
            SupervisorLoopSignal::Stop
        } else {
            SupervisorLoopSignal::Wake
        }
    }
}

pub fn run_system_supervisor(runtime: &RuntimeRoot) -> Result<(), ApplicationError> {
    ensure_root()?;
    let _pid = PidFile::acquire(runtime.supervisor_pid_path())?;
    let executable = std::env::current_exe().map_err(|_| ApplicationError::InvalidExecutable)?;
    let backend = SystemWorkerBackend::new(executable, runtime.worker_arguments())?;
    let mut supervisor = WorkerSupervisor::new(backend, RestartPolicy::default());
    let mut driver = SystemSupervisorDriver::install()?;
    run_supervisor_loop(&mut supervisor, &mut driver)
}

#[derive(Debug)]
struct PidFile {
    path: PathBuf,
}

impl PidFile {
    fn acquire(path: PathBuf) -> Result<Self, ApplicationError> {
        if path.file_name().is_none() {
            return Err(ApplicationError::PidFileFailed);
        }
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                remove_stale_pid_file(&path)?;
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .map_err(|_| ApplicationError::AlreadyRunning)?
            }
            Err(_) => return Err(ApplicationError::PidFileFailed),
        };
        set_private_file(&file).map_err(|_| ApplicationError::PidFileFailed)?;
        let start_time = process_start_time_ticks(std::process::id()).unwrap_or(0);
        if writeln!(file, "{} {start_time}", std::process::id()).is_err()
            || file.sync_all().is_err()
        {
            drop(file);
            let _ = fs::remove_file(&path);
            return Err(ApplicationError::PidFileFailed);
        }
        Ok(Self { path })
    }
}

fn remove_stale_pid_file(path: &Path) -> Result<(), ApplicationError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| ApplicationError::AlreadyRunning)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(ApplicationError::AlreadyRunning);
    }
    let contents = fs::read_to_string(path).map_err(|_| ApplicationError::AlreadyRunning)?;
    let mut fields = contents.split_whitespace();
    let pid = fields
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or(ApplicationError::AlreadyRunning)?;
    let expected = fields
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or(ApplicationError::AlreadyRunning)?;
    if fields.next().is_some()
        || process_start_time_ticks(pid).is_some_and(|actual| actual == expected)
    {
        return Err(ApplicationError::AlreadyRunning);
    }
    fs::remove_file(path).map_err(|_| ApplicationError::AlreadyRunning)
}

impl Drop for PidFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(unix)]
fn set_private_file(file: &File) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;

    file.set_permissions(fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_private_file(_file: &File) -> Result<(), std::io::Error> {
    Ok(())
}

#[cfg(unix)]
fn ensure_root() -> Result<(), ApplicationError> {
    // SAFETY: geteuid has no preconditions and does not dereference pointers.
    (unsafe { libc::geteuid() } == 0)
        .then_some(())
        .ok_or(ApplicationError::RootRequired)
}

#[cfg(any(target_os = "android", target_os = "linux"))]
fn process_start_time_ticks(pid: u32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat.rsplit_once(") ")?
        .1
        .split_whitespace()
        .nth(19)?
        .parse()
        .ok()
}

#[cfg(not(any(target_os = "android", target_os = "linux")))]
const fn process_start_time_ticks(_pid: u32) -> Option<u64> {
    None
}

#[cfg(not(unix))]
fn ensure_root() -> Result<(), ApplicationError> {
    Err(ApplicationError::UnsupportedPlatform)
}

#[cfg(unix)]
extern "C" fn request_stop(_signal: libc::c_int) {
    STOP_REQUESTED.store(true, Ordering::Release);
}

#[cfg(unix)]
fn install_signal_handlers() -> Result<(), ApplicationError> {
    // SAFETY: signal installs a process-global handler with the C ABI. The
    // handler performs only an atomic store, which is signal-safe.
    unsafe {
        let handler = request_stop as *const () as libc::sighandler_t;
        if libc::signal(libc::SIGTERM, handler) == libc::SIG_ERR
            || libc::signal(libc::SIGINT, handler) == libc::SIG_ERR
        {
            return Err(ApplicationError::SignalHandlerFailed);
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn install_signal_handlers() -> Result<(), ApplicationError> {
    Err(ApplicationError::UnsupportedPlatform)
}

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error("usage: nethopd <--supervise|--worker> --root <absolute-path>")]
    Usage,
    #[error("runtime root and run directory must be absolute real directories")]
    InvalidRuntimeRoot,
    #[error("nethopd must run as root")]
    RootRequired,
    #[error("nethopd is unsupported on this platform")]
    UnsupportedPlatform,
    #[error("nethopd executable path is invalid")]
    InvalidExecutable,
    #[error("nethopd instance is already running")]
    AlreadyRunning,
    #[error("nethopd PID file could not be published")]
    PidFileFailed,
    #[error("nethopd signal handlers could not be installed")]
    SignalHandlerFailed,
    #[error("worker supervisor failed")]
    Supervisor(#[from] SupervisorError),
}
