use std::{collections::VecDeque, time::Duration};

use nethopd::{
    ProcessIdentity, RestartPolicy, SupervisorError, SupervisorEvent, SupervisorState, WorkerExit,
    WorkerProcess, WorkerProcessBackend, WorkerSignal, WorkerSupervisor,
};

#[derive(Debug)]
struct FakeProcess {
    identity: ProcessIdentity,
    exits: VecDeque<Option<WorkerExit>>,
    signals: Vec<WorkerSignal>,
    stopped: bool,
}

impl WorkerProcess for FakeProcess {
    fn identity(&self) -> ProcessIdentity {
        self.identity
    }

    fn try_exit(&mut self) -> Result<Option<WorkerExit>, SupervisorError> {
        Ok(self.exits.pop_front().flatten())
    }

    fn signal(&mut self, signal: WorkerSignal) -> Result<(), SupervisorError> {
        self.signals.push(signal);
        Ok(())
    }

    fn stop(&mut self, _timeout: Duration) -> Result<(), SupervisorError> {
        self.stopped = true;
        Ok(())
    }
}

#[derive(Debug)]
struct FakeBackend {
    starts: VecDeque<Result<FakeProcess, SupervisorError>>,
}

impl WorkerProcessBackend for FakeBackend {
    type Process = FakeProcess;

    fn start(&mut self) -> Result<Self::Process, SupervisorError> {
        self.starts
            .pop_front()
            .unwrap_or(Err(SupervisorError::StartFailed))
    }
}

fn identity(pid: u32) -> ProcessIdentity {
    ProcessIdentity::new(pid, Some(pid as u64)).unwrap()
}

fn supervisor(backend: FakeBackend) -> WorkerSupervisor<FakeBackend> {
    WorkerSupervisor::new(backend, RestartPolicy::default())
}

#[test]
fn supervisor_restarts_after_worker_exit_with_bounded_backoff() {
    let backend = FakeBackend {
        starts: VecDeque::from([
            Ok(FakeProcess {
                identity: identity(101),
                exits: VecDeque::from([Some(WorkerExit::new(Some(1)))]),
                signals: Vec::new(),
                stopped: false,
            }),
            Ok(FakeProcess {
                identity: identity(102),
                exits: VecDeque::from([None]),
                signals: Vec::new(),
                stopped: false,
            }),
        ]),
    };
    let mut supervisor = supervisor(backend);
    assert!(matches!(
        supervisor.tick(Duration::ZERO).unwrap(),
        SupervisorEvent::Started(_)
    ));
    assert!(matches!(
        supervisor.tick(Duration::from_millis(1)).unwrap(),
        SupervisorEvent::WorkerExited(WorkerExit { .. })
    ));
    assert_eq!(supervisor.state(), SupervisorState::BackingOff);
    assert_eq!(
        supervisor.next_action(),
        Some(Duration::from_secs(1) + Duration::from_millis(1))
    );
    assert!(matches!(
        supervisor.tick(Duration::from_millis(500)).unwrap(),
        SupervisorEvent::RestartScheduled(_)
    ));
    assert!(matches!(
        supervisor.tick(Duration::from_millis(1_001)).unwrap(),
        SupervisorEvent::Started(_)
    ));
}

#[test]
fn fourth_failure_opens_circuit_and_recovery_is_low_frequency() {
    let backend = FakeBackend {
        starts: VecDeque::from([
            Err(SupervisorError::StartFailed),
            Err(SupervisorError::StartFailed),
            Err(SupervisorError::StartFailed),
            Err(SupervisorError::StartFailed),
            Ok(FakeProcess {
                identity: identity(200),
                exits: VecDeque::from([None]),
                signals: Vec::new(),
                stopped: false,
            }),
        ]),
    };
    let mut supervisor = supervisor(backend);
    assert!(matches!(
        supervisor.tick(Duration::ZERO).unwrap(),
        SupervisorEvent::StartFailed
    ));
    assert!(matches!(
        supervisor.tick(Duration::from_secs(1)).unwrap(),
        SupervisorEvent::StartFailed
    ));
    assert!(matches!(
        supervisor.tick(Duration::from_secs(3)).unwrap(),
        SupervisorEvent::StartFailed
    ));
    assert!(matches!(
        supervisor.tick(Duration::from_secs(7)).unwrap(),
        SupervisorEvent::CircuitOpened
    ));
    assert_eq!(supervisor.state(), SupervisorState::CircuitOpen);
    assert!(matches!(
        supervisor.tick(Duration::from_secs(66)).unwrap(),
        SupervisorEvent::RecoveryProbeScheduled(_)
    ));
    assert!(matches!(
        supervisor.tick(Duration::from_secs(67)).unwrap(),
        SupervisorEvent::Started(_)
    ));
}

#[test]
fn signal_and_stop_are_forwarded_and_stop_is_idempotent() {
    let backend = FakeBackend {
        starts: VecDeque::from([Ok(FakeProcess {
            identity: identity(300),
            exits: VecDeque::from([None]),
            signals: Vec::new(),
            stopped: false,
        })]),
    };
    let mut supervisor = supervisor(backend);
    supervisor.tick(Duration::ZERO).unwrap();
    assert_eq!(
        supervisor.forward_signal(WorkerSignal::Interrupt).unwrap(),
        SupervisorEvent::SignalForwarded(WorkerSignal::Interrupt)
    );
    assert_eq!(supervisor.stop().unwrap(), SupervisorEvent::Stopped);
    assert_eq!(supervisor.stop().unwrap(), SupervisorEvent::Stopped);
    assert_eq!(supervisor.state(), SupervisorState::Stopped);
    assert_eq!(
        supervisor
            .forward_signal(WorkerSignal::Terminate)
            .unwrap_err(),
        SupervisorError::NoWorker
    );
}
