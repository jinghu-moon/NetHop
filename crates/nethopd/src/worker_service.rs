use std::time::Duration;

use thiserror::Error;

use crate::{ControlRequestHandler, ControlServerError};

const MAX_IDLE_POLL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerServiceSignal {
    Wake,
    Stop,
}

pub trait WorkerServiceDriver {
    fn wait(&mut self, timeout: Duration) -> WorkerServiceSignal;
}

pub trait WorkerServiceTasks {
    fn next_wakeup_in(&self) -> Duration;
    fn run_ready(&mut self) -> Result<(), WorkerServiceError>;
    fn shutdown(&mut self) -> Result<(), WorkerServiceError>;
}

pub trait WorkerControlService {
    fn prepare(&self) -> Result<(), ControlServerError>;

    fn serve_ready(
        &self,
        handler: &mut impl ControlRequestHandler,
    ) -> Result<bool, ControlServerError>;
}

#[cfg(unix)]
impl WorkerControlService for crate::UnixControlServer {
    fn prepare(&self) -> Result<(), ControlServerError> {
        self.set_nonblocking(true)
    }

    fn serve_ready(
        &self,
        handler: &mut impl ControlRequestHandler,
    ) -> Result<bool, ControlServerError> {
        self.try_serve_once(handler).map(|peer| peer.is_some())
    }
}

pub fn run_worker_service<C, D, A>(
    server: &C,
    application: &mut A,
    driver: &mut D,
) -> Result<(), WorkerServiceError>
where
    C: WorkerControlService,
    D: WorkerServiceDriver,
    A: WorkerServiceTasks + ControlRequestHandler,
{
    server.prepare()?;
    loop {
        let iteration = (|| {
            while server.serve_ready(application)? {}
            application.run_ready()
        })();
        if let Err(error) = iteration {
            return match application.shutdown() {
                Ok(()) => Err(error),
                Err(_) => Err(WorkerServiceError::ShutdownFailed),
            };
        }
        let timeout = application.next_wakeup_in().min(MAX_IDLE_POLL);
        if driver.wait(timeout) == WorkerServiceSignal::Stop {
            return application.shutdown();
        }
    }
}

#[derive(Debug, Error)]
pub enum WorkerServiceError {
    #[error("worker control service failed")]
    Control(#[from] ControlServerError),
    #[error("worker periodic task failed")]
    TaskFailed,
    #[error("worker shutdown cleanup failed")]
    ShutdownFailed,
}
