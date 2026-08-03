use std::{collections::VecDeque, ffi::OsString, time::Duration};

use nethopd::{
    DaemonArguments, DaemonMode, ProcessIdentity, RestartPolicy, RuntimeRoot, SupervisorError,
    SupervisorLoopDriver, SupervisorLoopSignal, WorkerExit, WorkerProcess, WorkerProcessBackend,
    WorkerSignal, WorkerSupervisor, run_supervisor_loop,
};
use tempfile::tempdir;

#[test]
fn daemon_arguments_are_exact_and_mode_does_not_come_from_the_root_path() {
    let supervise = DaemonArguments::parse([
        OsString::from("--supervise"),
        OsString::from("--root"),
        OsString::from("/data/adb/nethop"),
    ])
    .unwrap();
    assert_eq!(supervise.mode(), DaemonMode::Supervise);
    assert_eq!(supervise.root().to_string_lossy(), "/data/adb/nethop");

    let worker = DaemonArguments::parse([
        OsString::from("--worker"),
        OsString::from("--root"),
        OsString::from("/data/adb/nethop"),
    ])
    .unwrap();
    assert_eq!(worker.mode(), DaemonMode::Worker);

    for invalid in [
        vec!["--supervise", "/data/adb/nethop"],
        vec!["--worker", "--root"],
        vec!["--worker", "--root", "/data/adb/nethop", "extra"],
        vec!["--root", "/data/adb/nethop", "--worker"],
    ] {
        assert!(DaemonArguments::parse(invalid).is_err());
    }
}

#[test]
fn runtime_root_requires_existing_absolute_real_root_and_run_directories() {
    let directory = tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    assert!(RuntimeRoot::open(&root).is_err());
    std::fs::create_dir(root.join("run")).unwrap();
    let runtime = RuntimeRoot::open(&root).unwrap();
    assert_eq!(runtime.root(), root);
    assert_eq!(runtime.run(), root.join("run"));
    assert_eq!(
        runtime.worker_arguments(),
        vec![
            OsString::from("--worker"),
            OsString::from("--root"),
            root.into_os_string(),
        ]
    );
    assert!(RuntimeRoot::open("relative/root").is_err());
}

#[derive(Debug)]
struct FakeProcess {
    stopped: bool,
}

impl WorkerProcess for FakeProcess {
    fn identity(&self) -> ProcessIdentity {
        ProcessIdentity::new(42, Some(1)).unwrap()
    }

    fn try_exit(&mut self) -> Result<Option<WorkerExit>, SupervisorError> {
        Ok(None)
    }

    fn signal(&mut self, _signal: WorkerSignal) -> Result<(), SupervisorError> {
        Ok(())
    }

    fn stop(&mut self, _timeout: Duration) -> Result<(), SupervisorError> {
        self.stopped = true;
        Ok(())
    }
}

#[derive(Debug)]
struct FakeBackend {
    starts: usize,
}

impl WorkerProcessBackend for FakeBackend {
    type Process = FakeProcess;

    fn start(&mut self) -> Result<Self::Process, SupervisorError> {
        self.starts += 1;
        Ok(FakeProcess { stopped: false })
    }
}

#[derive(Debug)]
struct FakeDriver {
    now: Duration,
    signals: VecDeque<SupervisorLoopSignal>,
    waits: Vec<Duration>,
}

impl SupervisorLoopDriver for FakeDriver {
    fn now(&self) -> Duration {
        self.now
    }

    fn wait(&mut self, timeout: Duration) -> SupervisorLoopSignal {
        self.waits.push(timeout);
        self.now = self.now.saturating_add(timeout);
        self.signals
            .pop_front()
            .unwrap_or(SupervisorLoopSignal::Stop)
    }
}

#[test]
fn supervisor_application_loop_starts_once_and_stops_owned_worker() {
    let backend = FakeBackend { starts: 0 };
    let mut supervisor = WorkerSupervisor::new(backend, RestartPolicy::default());
    let mut driver = FakeDriver {
        now: Duration::ZERO,
        signals: VecDeque::from([SupervisorLoopSignal::Wake, SupervisorLoopSignal::Stop]),
        waits: Vec::new(),
    };

    run_supervisor_loop(&mut supervisor, &mut driver).unwrap();
    assert_eq!(driver.waits, vec![Duration::from_millis(250); 2]);
    assert!(supervisor.active_identity().is_none());
}
