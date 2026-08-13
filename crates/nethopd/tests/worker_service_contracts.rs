use std::{cell::RefCell, collections::VecDeque, time::Duration};

use nethop_protocol::{ControlRequest, ControlResponse};
use nethopd::{
    ControlRequestHandler, ControlServerError, WorkerControlService, WorkerServiceDriver,
    WorkerServiceError, WorkerServiceSignal, WorkerServiceTasks, run_worker_service,
};
use serde_json::json;

impl ControlRequestHandler for Tasks {
    fn handle(&mut self, request: ControlRequest) -> ControlResponse {
        ControlResponse::success(request.request_id().clone(), None, json!({"state":"init"}))
    }
}

struct ControlService;

impl WorkerControlService for ControlService {
    fn prepare(&self) -> Result<(), ControlServerError> {
        Ok(())
    }

    fn serve_ready(
        &self,
        _handler: &mut impl ControlRequestHandler,
    ) -> Result<bool, ControlServerError> {
        Ok(false)
    }
}

struct SequencedControlService {
    results: RefCell<VecDeque<Result<bool, ControlServerError>>>,
}

impl WorkerControlService for SequencedControlService {
    fn prepare(&self) -> Result<(), ControlServerError> {
        Ok(())
    }

    fn serve_ready(
        &self,
        _handler: &mut impl ControlRequestHandler,
    ) -> Result<bool, ControlServerError> {
        self.results.borrow_mut().pop_front().unwrap_or(Ok(false))
    }
}

#[derive(Default)]
struct Tasks {
    runs: usize,
    shutdowns: usize,
    fail_run: bool,
}

impl WorkerServiceTasks for Tasks {
    fn next_wakeup_in(&self) -> Duration {
        Duration::from_secs(60)
    }

    fn run_ready(&mut self) -> Result<(), WorkerServiceError> {
        self.runs += 1;
        if self.fail_run {
            Err(WorkerServiceError::TaskFailed)
        } else {
            Ok(())
        }
    }

    fn shutdown(&mut self) -> Result<(), WorkerServiceError> {
        self.shutdowns += 1;
        Ok(())
    }
}

struct Driver {
    signals: VecDeque<WorkerServiceSignal>,
    waits: Vec<Duration>,
}

impl WorkerServiceDriver for Driver {
    fn wait(&mut self, timeout: Duration) -> WorkerServiceSignal {
        self.waits.push(timeout);
        self.signals
            .pop_front()
            .unwrap_or(WorkerServiceSignal::Stop)
    }
}

#[test]
fn idle_worker_loop_is_bounded_and_runs_shutdown_once() {
    let server = ControlService;
    let mut tasks = Tasks::default();
    let mut driver = Driver {
        signals: VecDeque::from([WorkerServiceSignal::Wake, WorkerServiceSignal::Stop]),
        waits: Vec::new(),
    };

    run_worker_service(&server, &mut tasks, &mut driver).unwrap();
    assert_eq!(tasks.runs, 2);
    assert_eq!(tasks.shutdowns, 1);
    assert_eq!(driver.waits, vec![Duration::from_secs(1); 2]);
}

#[test]
fn task_failure_still_runs_shutdown_cleanup() {
    let server = ControlService;
    let mut tasks = Tasks {
        fail_run: true,
        ..Tasks::default()
    };
    let mut driver = Driver {
        signals: VecDeque::new(),
        waits: Vec::new(),
    };

    assert!(matches!(
        run_worker_service(&server, &mut tasks, &mut driver),
        Err(WorkerServiceError::TaskFailed)
    ));
    assert_eq!(tasks.shutdowns, 1);
    assert!(driver.waits.is_empty());
}

#[test]
fn disconnected_control_client_does_not_restart_the_worker() {
    let server = SequencedControlService {
        results: RefCell::new(VecDeque::from([
            Err(ControlServerError::ResponseFailed),
            Ok(false),
        ])),
    };
    let mut tasks = Tasks::default();
    let mut driver = Driver {
        signals: VecDeque::from([WorkerServiceSignal::Stop]),
        waits: Vec::new(),
    };

    run_worker_service(&server, &mut tasks, &mut driver).unwrap();
    assert_eq!(tasks.runs, 1);
    assert_eq!(tasks.shutdowns, 1);
}

#[test]
fn malformed_or_unauthorized_client_is_isolated_to_its_connection() {
    let server = SequencedControlService {
        results: RefCell::new(VecDeque::from([
            Err(ControlServerError::PeerCredentialFailed),
            Err(ControlServerError::AuthorizationDenied),
            Err(ControlServerError::InvalidRequest),
            Ok(false),
        ])),
    };
    let mut tasks = Tasks::default();
    let mut driver = Driver {
        signals: VecDeque::from([WorkerServiceSignal::Stop]),
        waits: Vec::new(),
    };

    run_worker_service(&server, &mut tasks, &mut driver).unwrap();
    assert_eq!(tasks.runs, 1);
    assert_eq!(tasks.shutdowns, 1);
}

#[test]
fn accept_failure_remains_a_service_level_error() {
    let server = SequencedControlService {
        results: RefCell::new(VecDeque::from([Err(ControlServerError::AcceptFailed)])),
    };
    let mut tasks = Tasks::default();
    let mut driver = Driver {
        signals: VecDeque::new(),
        waits: Vec::new(),
    };

    assert!(matches!(
        run_worker_service(&server, &mut tasks, &mut driver),
        Err(WorkerServiceError::Control(
            ControlServerError::AcceptFailed
        ))
    ));
    assert_eq!(tasks.runs, 0);
    assert_eq!(tasks.shutdowns, 1);
}
