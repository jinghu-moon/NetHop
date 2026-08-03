use std::{
    fs,
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::Duration,
};

use thiserror::Error;

use crate::process::ProcessIdentity;

const MAX_FAILURE_WINDOW: Duration = Duration::from_secs(5 * 60);
const MAX_RECOVERY_DELAY: Duration = Duration::from_secs(60);
const MAX_STOP_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const RESTART_DELAYS: [Duration; 3] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestartPolicy {
    failure_window: Duration,
    recovery_delay: Duration,
    stop_timeout: Duration,
}

impl RestartPolicy {
    pub fn new(
        failure_window: Duration,
        recovery_delay: Duration,
        stop_timeout: Duration,
    ) -> Result<Self, SupervisorError> {
        if failure_window.is_zero()
            || failure_window > MAX_FAILURE_WINDOW
            || recovery_delay.is_zero()
            || recovery_delay > MAX_RECOVERY_DELAY
            || stop_timeout.is_zero()
            || stop_timeout > MAX_STOP_TIMEOUT
        {
            return Err(SupervisorError::InvalidPolicy);
        }
        Ok(Self {
            failure_window,
            recovery_delay,
            stop_timeout,
        })
    }

    pub const fn failure_window(self) -> Duration {
        self.failure_window
    }

    pub const fn recovery_delay(self) -> Duration {
        self.recovery_delay
    }

    pub const fn stop_timeout(self) -> Duration {
        self.stop_timeout
    }
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            failure_window: MAX_FAILURE_WINDOW,
            recovery_delay: MAX_RECOVERY_DELAY,
            stop_timeout: Duration::from_secs(3),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerSignal {
    Terminate,
    Interrupt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerExit {
    code: Option<i32>,
}

impl WorkerExit {
    pub const fn new(code: Option<i32>) -> Self {
        Self { code }
    }

    pub const fn code(self) -> Option<i32> {
        self.code
    }
}

pub trait WorkerProcess {
    fn identity(&self) -> ProcessIdentity;
    fn try_exit(&mut self) -> Result<Option<WorkerExit>, SupervisorError>;
    fn signal(&mut self, signal: WorkerSignal) -> Result<(), SupervisorError>;
    fn stop(&mut self, timeout: Duration) -> Result<(), SupervisorError>;
}

pub trait WorkerProcessBackend {
    type Process: WorkerProcess;

    fn start(&mut self) -> Result<Self::Process, SupervisorError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupervisorState {
    Idle,
    Running,
    BackingOff,
    CircuitOpen,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupervisorEvent {
    Started(ProcessIdentity),
    Running,
    WorkerExited(WorkerExit),
    StartFailed,
    RestartScheduled(Duration),
    CircuitOpened,
    RecoveryProbeScheduled(Duration),
    SignalForwarded(WorkerSignal),
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SupervisorError {
    #[error("supervisor policy is outside the bounded limits")]
    InvalidPolicy,
    #[error("worker executable is not an absolute regular non-symlink file")]
    InvalidWorkerBinary,
    #[error("worker process could not be started")]
    StartFailed,
    #[error("worker process could not be observed")]
    ObserveFailed,
    #[error("worker signal could not be forwarded")]
    SignalFailed,
    #[error("worker process could not be stopped")]
    StopFailed,
    #[error("worker identity no longer matches its recorded PID/start-time")]
    IdentityMismatch,
    #[error("no worker is currently running")]
    NoWorker,
}

#[derive(Debug)]
pub struct WorkerSupervisor<B>
where
    B: WorkerProcessBackend,
{
    backend: B,
    policy: RestartPolicy,
    state: SupervisorState,
    active: Option<B::Process>,
    failures: Vec<Duration>,
    next_action: Option<Duration>,
}

impl<B> WorkerSupervisor<B>
where
    B: WorkerProcessBackend,
{
    pub fn new(backend: B, policy: RestartPolicy) -> Self {
        Self {
            backend,
            policy,
            state: SupervisorState::Idle,
            active: None,
            failures: Vec::new(),
            next_action: Some(Duration::ZERO),
        }
    }

    pub const fn state(&self) -> SupervisorState {
        self.state
    }

    pub fn active_identity(&self) -> Option<ProcessIdentity> {
        self.active.as_ref().map(WorkerProcess::identity)
    }

    pub fn next_action(&self) -> Option<Duration> {
        self.next_action
    }

    pub fn failure_count(&self) -> usize {
        self.failures.len()
    }

    pub fn tick(&mut self, now: Duration) -> Result<SupervisorEvent, SupervisorError> {
        if self.state == SupervisorState::Stopped {
            return Ok(SupervisorEvent::Stopped);
        }
        if let Some(process) = &mut self.active {
            match process.try_exit()? {
                Some(exit) => {
                    self.active = None;
                    return Ok(self.record_failure(now, Some(exit)));
                }
                None => {
                    self.state = SupervisorState::Running;
                    return Ok(SupervisorEvent::Running);
                }
            }
        }
        if self
            .next_action
            .is_some_and(|next_action| now < next_action)
        {
            return Ok(match self.state {
                SupervisorState::CircuitOpen => SupervisorEvent::RecoveryProbeScheduled(
                    self.next_action.unwrap_or(now).saturating_sub(now),
                ),
                _ => SupervisorEvent::RestartScheduled(
                    self.next_action.unwrap_or(now).saturating_sub(now),
                ),
            });
        }
        match self.backend.start() {
            Ok(process) => {
                let identity = process.identity();
                self.active = Some(process);
                self.state = SupervisorState::Running;
                self.next_action = None;
                Ok(SupervisorEvent::Started(identity))
            }
            Err(SupervisorError::StartFailed) => Ok(self.record_failure(now, None)),
            Err(error) => Err(error),
        }
    }

    pub fn forward_signal(
        &mut self,
        signal: WorkerSignal,
    ) -> Result<SupervisorEvent, SupervisorError> {
        let process = self.active.as_mut().ok_or(SupervisorError::NoWorker)?;
        process.signal(signal)?;
        Ok(SupervisorEvent::SignalForwarded(signal))
    }

    pub fn stop(&mut self) -> Result<SupervisorEvent, SupervisorError> {
        if let Some(mut process) = self.active.take() {
            process.stop(self.policy.stop_timeout)?;
        }
        self.state = SupervisorState::Stopped;
        self.next_action = None;
        Ok(SupervisorEvent::Stopped)
    }

    fn record_failure(&mut self, now: Duration, exit: Option<WorkerExit>) -> SupervisorEvent {
        self.failures
            .retain(|failure| now.saturating_sub(*failure) <= self.policy.failure_window);
        self.failures.push(now);
        if self.failures.len() > RESTART_DELAYS.len() {
            self.state = SupervisorState::CircuitOpen;
            self.next_action = Some(now.saturating_add(self.policy.recovery_delay));
            exit.map_or(
                SupervisorEvent::CircuitOpened,
                SupervisorEvent::WorkerExited,
            )
        } else {
            self.state = SupervisorState::BackingOff;
            let delay = RESTART_DELAYS[self.failures.len() - 1];
            self.next_action = Some(now.saturating_add(delay));
            if let Some(exit) = exit {
                SupervisorEvent::WorkerExited(exit)
            } else {
                SupervisorEvent::StartFailed
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct SystemWorkerBackend {
    binary: PathBuf,
    arguments: Vec<std::ffi::OsString>,
}

impl SystemWorkerBackend {
    pub fn new(
        binary: impl Into<PathBuf>,
        arguments: Vec<std::ffi::OsString>,
    ) -> Result<Self, SupervisorError> {
        let binary = binary.into();
        let metadata =
            fs::symlink_metadata(&binary).map_err(|_| SupervisorError::InvalidWorkerBinary)?;
        if !binary.is_absolute() || !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(SupervisorError::InvalidWorkerBinary);
        }
        Ok(Self { binary, arguments })
    }
}

impl WorkerProcessBackend for SystemWorkerBackend {
    type Process = SystemWorkerProcess;

    fn start(&mut self) -> Result<Self::Process, SupervisorError> {
        let mut command = Command::new(&self.binary);
        command
            .args(&self.arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let child = command.spawn().map_err(|_| SupervisorError::StartFailed)?;
        SystemWorkerProcess::new(child)
    }
}

#[derive(Debug)]
pub struct SystemWorkerProcess {
    child: Child,
    identity: ProcessIdentity,
}

impl SystemWorkerProcess {
    fn new(child: Child) -> Result<Self, SupervisorError> {
        let identity = ProcessIdentity::new(child.id(), process_start_time_ticks(child.id()))
            .ok_or(SupervisorError::StartFailed)?;
        Ok(Self { child, identity })
    }
}

impl WorkerProcess for SystemWorkerProcess {
    fn identity(&self) -> ProcessIdentity {
        self.identity
    }

    fn try_exit(&mut self) -> Result<Option<WorkerExit>, SupervisorError> {
        self.child
            .try_wait()
            .map_err(|_| SupervisorError::ObserveFailed)
            .map(|status| status.map(|status| WorkerExit::new(status.code())))
    }

    fn signal(&mut self, signal: WorkerSignal) -> Result<(), SupervisorError> {
        ensure_identity(self.identity)?;
        send_signal(&mut self.child, self.identity, signal)
    }

    fn stop(&mut self, timeout: Duration) -> Result<(), SupervisorError> {
        if self.try_exit()?.is_some() {
            return Ok(());
        }
        self.signal(WorkerSignal::Terminate)?;
        let started = std::time::Instant::now();
        while started.elapsed() < timeout {
            if self.try_exit()?.is_some() {
                return Ok(());
            }
            thread::sleep(POLL_INTERVAL);
        }
        self.child.kill().map_err(|_| SupervisorError::StopFailed)?;
        self.child.wait().map_err(|_| SupervisorError::StopFailed)?;
        Ok(())
    }
}

fn ensure_identity(_identity: ProcessIdentity) -> Result<(), SupervisorError> {
    #[cfg(any(target_os = "android", target_os = "linux"))]
    if let Some(expected) = _identity.start_time_ticks() {
        let path = format!("/proc/{}/stat", _identity.pid());
        let stat = fs::read_to_string(path).map_err(|_| SupervisorError::IdentityMismatch)?;
        let actual = stat
            .rsplit_once(") ")
            .and_then(|(_, rest)| rest.split_whitespace().nth(19))
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or(SupervisorError::IdentityMismatch)?;
        if actual != expected {
            return Err(SupervisorError::IdentityMismatch);
        }
    }
    Ok(())
}

#[cfg(unix)]
fn send_signal(
    _child: &mut Child,
    identity: ProcessIdentity,
    signal: WorkerSignal,
) -> Result<(), SupervisorError> {
    let number = match signal {
        WorkerSignal::Terminate => libc::SIGTERM,
        WorkerSignal::Interrupt => libc::SIGINT,
    };
    let pid = i32::try_from(identity.pid()).map_err(|_| SupervisorError::SignalFailed)?;
    // SAFETY: pid is checked against the owned process identity and the signal
    // carries no pointer data.
    let result = unsafe { libc::kill(pid, number) };
    (result == 0)
        .then_some(())
        .ok_or(SupervisorError::SignalFailed)
}

#[cfg(not(unix))]
fn send_signal(
    child: &mut Child,
    _identity: ProcessIdentity,
    _signal: WorkerSignal,
) -> Result<(), SupervisorError> {
    child.kill().map_err(|_| SupervisorError::SignalFailed)
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
