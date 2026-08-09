use std::{collections::VecDeque, time::Duration};

use nethop_android::{TunHealthError, TunHealthProbe};
use nethopd::{
    CandidateProcess, ProcessError, ProcessIdentity, TunRunner, TunRunnerError, TunRunnerLimits,
    TunRuntime,
};

#[derive(Debug)]
struct TestProcess {
    running: bool,
}

impl CandidateProcess for TestProcess {
    fn identity(&self) -> ProcessIdentity {
        ProcessIdentity::new(42, Some(7)).unwrap()
    }

    fn is_running(&mut self) -> Result<bool, ProcessError> {
        Ok(self.running)
    }

    fn stop(self) -> Result<(), ProcessError> {
        Ok(())
    }
}

#[derive(Debug)]
struct ScriptedTunHealth {
    active: VecDeque<bool>,
    absent: VecDeque<bool>,
}

impl TunHealthProbe for ScriptedTunHealth {
    fn verify(&mut self) -> Result<(), TunHealthError> {
        if self.active.pop_front().unwrap_or(false) {
            Ok(())
        } else {
            Err(TunHealthError::InterfaceMissing)
        }
    }

    fn verify_absent(&mut self) -> Result<(), TunHealthError> {
        if self.absent.pop_front().unwrap_or(false) {
            Ok(())
        } else {
            Err(TunHealthError::InterfaceStillPresent)
        }
    }
}

fn limits() -> TunRunnerLimits {
    TunRunnerLimits::new(
        Duration::from_millis(20),
        Duration::from_millis(20),
        Duration::from_millis(1),
    )
    .unwrap()
}

#[test]
fn tun_runner_waits_for_interface_health_while_the_core_is_alive() {
    let verifier = ScriptedTunHealth {
        active: VecDeque::from([false, false, true, true]),
        absent: VecDeque::new(),
    };
    let mut runner = TunRunner::new(verifier, limits());
    let mut process = TestProcess { running: true };

    runner.wait_healthy(&mut process).unwrap();
    runner.verify_active().unwrap();
}

#[test]
fn tun_runner_rejects_an_early_core_exit() {
    let verifier = ScriptedTunHealth {
        active: VecDeque::from([true]),
        absent: VecDeque::new(),
    };
    let mut runner = TunRunner::new(verifier, limits());
    let mut process = TestProcess { running: false };

    assert_eq!(
        runner.wait_healthy(&mut process).unwrap_err(),
        TunRunnerError::CoreExited
    );
}

#[test]
fn tun_runner_waits_until_the_owned_interface_disappears() {
    let verifier = ScriptedTunHealth {
        active: VecDeque::new(),
        absent: VecDeque::from([false, false, true]),
    };
    let mut runner = TunRunner::new(verifier, limits());

    runner.wait_stopped().unwrap();
}
