use std::{
    thread,
    time::{Duration, Instant},
};

use nethop_android::TunHealthProbe;
use thiserror::Error;

use crate::CandidateProcess;

const MAX_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TunRunnerLimits {
    startup_timeout: Duration,
    shutdown_timeout: Duration,
    poll_interval: Duration,
}

impl TunRunnerLimits {
    pub fn new(
        startup_timeout: Duration,
        shutdown_timeout: Duration,
        poll_interval: Duration,
    ) -> Result<Self, TunRunnerError> {
        if startup_timeout.is_zero()
            || shutdown_timeout.is_zero()
            || poll_interval.is_zero()
            || startup_timeout > MAX_TIMEOUT
            || shutdown_timeout > MAX_TIMEOUT
            || poll_interval > startup_timeout.min(shutdown_timeout)
        {
            return Err(TunRunnerError::InvalidLimits);
        }
        Ok(Self {
            startup_timeout,
            shutdown_timeout,
            poll_interval,
        })
    }
}

impl Default for TunRunnerLimits {
    fn default() -> Self {
        Self {
            startup_timeout: Duration::from_secs(3),
            shutdown_timeout: Duration::from_secs(3),
            poll_interval: Duration::from_millis(50),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TunRunnerError {
    #[error("TUN runner limits are invalid")]
    InvalidLimits,
    #[error("sing-box exited before the TUN interface became healthy")]
    CoreExited,
    #[error("sing-box state could not be observed while waiting for TUN")]
    CoreObserveFailed,
    #[error("TUN interface did not become healthy before the deadline")]
    StartupTimedOut,
    #[error("TUN interface is unhealthy")]
    Unhealthy,
    #[error("TUN interface remained after sing-box stopped")]
    CleanupTimedOut,
}

impl TunRunnerError {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidLimits => "tun_runner_invalid_limits",
            Self::CoreExited => "tun_runner_core_exited",
            Self::CoreObserveFailed => "tun_runner_core_observe_failed",
            Self::StartupTimedOut => "tun_runner_startup_timed_out",
            Self::Unhealthy => "tun_runner_unhealthy",
            Self::CleanupTimedOut => "tun_runner_cleanup_timed_out",
        }
    }
}

pub trait TunRuntime {
    fn wait_healthy<P: CandidateProcess>(&mut self, process: &mut P) -> Result<(), TunRunnerError>;
    fn verify_active(&mut self) -> Result<(), TunRunnerError>;
    fn wait_stopped(&mut self) -> Result<(), TunRunnerError>;

    fn replace_timeout(&mut self, _timeout: Duration) -> Result<(), TunRunnerError> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct TunRunner<V> {
    verifier: V,
    limits: TunRunnerLimits,
}

impl<V> TunRunner<V> {
    pub const fn new(verifier: V, limits: TunRunnerLimits) -> Self {
        Self { verifier, limits }
    }

    pub fn into_verifier(self) -> V {
        self.verifier
    }
}

impl<V> TunRuntime for TunRunner<V>
where
    V: TunHealthProbe,
{
    fn wait_healthy<P: CandidateProcess>(&mut self, process: &mut P) -> Result<(), TunRunnerError> {
        let started = Instant::now();
        loop {
            match process.is_running() {
                Ok(true) => {}
                Ok(false) => return Err(TunRunnerError::CoreExited),
                Err(_) => return Err(TunRunnerError::CoreObserveFailed),
            }
            if self.verifier.verify().is_ok() {
                return Ok(());
            }
            if started.elapsed() >= self.limits.startup_timeout {
                return Err(TunRunnerError::StartupTimedOut);
            }
            thread::sleep(self.limits.poll_interval);
        }
    }

    fn verify_active(&mut self) -> Result<(), TunRunnerError> {
        self.verifier
            .verify()
            .map_err(|_| TunRunnerError::Unhealthy)
    }

    fn wait_stopped(&mut self) -> Result<(), TunRunnerError> {
        let started = Instant::now();
        loop {
            if self.verifier.verify_absent().is_ok() {
                return Ok(());
            }
            if started.elapsed() >= self.limits.shutdown_timeout {
                return Err(TunRunnerError::CleanupTimedOut);
            }
            thread::sleep(self.limits.poll_interval);
        }
    }

    fn replace_timeout(&mut self, timeout: Duration) -> Result<(), TunRunnerError> {
        self.limits = TunRunnerLimits::new(timeout, timeout, self.limits.poll_interval)?;
        Ok(())
    }
}
