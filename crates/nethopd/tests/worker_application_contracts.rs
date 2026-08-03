use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

use nethop_android::{
    ExecutionError, NetworkHealthError, NetworkHealthVerifier, NetworkPlan, PlanSlot,
};
use nethop_core::{CaptureMode, CapturePolicy, RuntimeState};
use nethop_protocol::{ControlMethod, ControlRequest, RequestId};
use nethopd::{
    ActiveRuntime, CandidateProcess, ControlRequestHandler, NetworkController, ProcessError,
    ProcessIdentity, RuntimeRecoverySource, WorkerApplication, WorkerClock, WorkerRecoveryError,
    WorkerRuntimeLimits, WorkerServiceTasks,
};

#[derive(Clone)]
struct TestClock(Rc<Cell<Duration>>);

impl WorkerClock for TestClock {
    fn now(&self) -> Duration {
        self.0.get()
    }
}

struct TestProcess;

impl CandidateProcess for TestProcess {
    fn identity(&self) -> ProcessIdentity {
        ProcessIdentity::new(1, Some(1)).unwrap()
    }

    fn is_running(&mut self) -> Result<bool, ProcessError> {
        Ok(true)
    }

    fn stop(self) -> Result<(), ProcessError> {
        Ok(())
    }
}

#[derive(Default)]
struct TestNetwork;

impl NetworkController for TestNetwork {
    type Receipt = ();

    fn apply(&mut self, _plan: &NetworkPlan) -> Result<Self::Receipt, ExecutionError> {
        Ok(())
    }

    fn rollback(
        &mut self,
        _plan: &NetworkPlan,
        _receipt: &mut Self::Receipt,
    ) -> Result<(), ExecutionError> {
        Ok(())
    }
}

#[derive(Default)]
struct TestRecovery {
    attempts: Rc<RefCell<Vec<&'static str>>>,
    fail: bool,
}

impl RuntimeRecoverySource<TestNetwork> for TestRecovery {
    type Process = TestProcess;

    fn recover(
        &mut self,
        _network: &mut TestNetwork,
        _policy: &CapturePolicy,
        _slot: PlanSlot,
    ) -> Result<Option<ActiveRuntime<Self::Process, ()>>, WorkerRecoveryError> {
        self.attempts.borrow_mut().push("recover");
        if self.fail {
            Err(WorkerRecoveryError::CapabilityProbeFailed)
        } else {
            Ok(None)
        }
    }

    fn probe(&mut self) -> bool {
        self.attempts.borrow_mut().push("probe");
        true
    }
}

#[derive(Default)]
struct TestVerifier;

impl NetworkHealthVerifier for TestVerifier {
    fn verify(&mut self, _plan: &NetworkPlan) -> Result<(), NetworkHealthError> {
        Ok(())
    }
}

fn policy() -> CapturePolicy {
    CapturePolicy::new(
        CaptureMode::Tproxy,
        true,
        Some(7893),
        Some(0x20_000),
        Vec::new(),
        vec![0],
    )
    .unwrap()
}

fn request(id: &str, method: ControlMethod) -> ControlRequest {
    ControlRequest::new(RequestId::new(id).unwrap(), method)
}

#[test]
fn missing_current_generation_stays_available_in_fail_open_direct() {
    let clock = TestClock(Rc::new(Cell::new(Duration::ZERO)));
    let attempts = Rc::new(RefCell::new(Vec::new()));
    let recovery = TestRecovery {
        attempts: Rc::clone(&attempts),
        fail: false,
    };
    let mut application = WorkerApplication::new(
        recovery,
        TestNetwork,
        TestVerifier,
        clock,
        policy(),
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    );

    assert_eq!(application.next_wakeup_in(), Duration::ZERO);
    application.run_ready().unwrap();
    assert_eq!(application.snapshot().state, RuntimeState::FailOpenDirect);
    assert_eq!(attempts.borrow().as_slice(), ["recover"]);
    assert_eq!(application.next_wakeup_in(), Duration::from_secs(1));

    let status = application.handle(request("status", ControlMethod::StatusGet));
    assert_eq!(status.result().unwrap()["state"], "fail_open_direct");
}

#[test]
fn typed_start_stop_and_probe_commands_are_consumed_on_the_worker_loop() {
    let clock = TestClock(Rc::new(Cell::new(Duration::ZERO)));
    let attempts = Rc::new(RefCell::new(Vec::new()));
    let recovery = TestRecovery {
        attempts: Rc::clone(&attempts),
        fail: false,
    };
    let mut application = WorkerApplication::new(
        recovery,
        TestNetwork,
        TestVerifier,
        clock,
        policy(),
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    );
    application.run_ready().unwrap();

    application.handle(request("probe", ControlMethod::CapabilityProbe));
    application.handle(request("start", ControlMethod::ServiceStart));
    application.run_ready().unwrap();
    assert_eq!(
        attempts.borrow().as_slice(),
        ["recover", "probe", "recover"]
    );

    application.handle(request("stop", ControlMethod::ServiceStop));
    application.run_ready().unwrap();
    assert_eq!(application.snapshot().state, RuntimeState::FailOpenDirect);
}

#[test]
fn transient_recovery_failure_uses_bounded_restart_deadline() {
    let now = Rc::new(Cell::new(Duration::ZERO));
    let clock = TestClock(Rc::clone(&now));
    let attempts = Rc::new(RefCell::new(Vec::new()));
    let recovery = TestRecovery {
        attempts: Rc::clone(&attempts),
        fail: true,
    };
    let mut application = WorkerApplication::new(
        recovery,
        TestNetwork,
        TestVerifier,
        clock,
        policy(),
        PlanSlot::A,
        WorkerRuntimeLimits::default(),
    );

    application.run_ready().unwrap();
    assert_eq!(application.snapshot().state, RuntimeState::Backoff);
    assert_eq!(application.next_wakeup_in(), Duration::from_secs(1));

    now.set(Duration::from_secs(1));
    application.run_ready().unwrap();
    assert_eq!(attempts.borrow().as_slice(), ["recover", "recover"]);
    assert_eq!(application.snapshot().state, RuntimeState::Backoff);
    assert_eq!(application.next_wakeup_in(), Duration::from_secs(2));
}
