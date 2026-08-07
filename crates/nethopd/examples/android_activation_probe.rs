#[cfg(unix)]
use std::{env, path::PathBuf, process};

#[cfg(unix)]
use nethop_android::{
    AndroidToolPaths, CapabilityProbe, CommandProbeBackend, NetworkExecutor, NetworkHealthVerifier,
    NetworkPlanVerifier, NetworkPlanner, PlanSlot, ProbeLimits, SystemCommandBackend,
    SystemCommandLimits,
};
#[cfg(unix)]
use nethop_core::{GenerationId, GenerationStore};
#[cfg(unix)]
use nethopd::{
    CandidateChecker, ConfigStore, CoreProcessLimits, CoreProcessRunner, HealthProbe, RunnerLimits,
    SingBoxCheckRunner, StartupLivenessProbe,
};

#[cfg(unix)]
fn main() {
    if let Err(code) = run() {
        println!("{}", serde_json::json!({ "ok": false, "code": code }));
        process::exit(2);
    }
}

#[cfg(not(unix))]
fn main() {
    eprintln!("android_activation_probe is only available on Unix targets");
    std::process::exit(1);
}

#[cfg(unix)]
fn run() -> Result<(), &'static str> {
    let mut arguments = env::args_os().skip(1).map(PathBuf::from);
    let root = arguments.next().ok_or("missing_root")?;
    let worker_config = arguments.next().ok_or("missing_worker_config")?;
    let sing_box = arguments.next().ok_or("missing_sing_box")?;
    let generation = arguments
        .next()
        .and_then(|value| value.to_string_lossy().parse::<u64>().ok())
        .and_then(|value| GenerationId::new(value).ok())
        .ok_or("invalid_generation")?;
    if arguments.next().is_some() {
        return Err("unexpected_argument");
    }

    let config = ConfigStore::new(&worker_config)
        .map_err(|_| "worker_config_failed")?
        .load()
        .map_err(|_| "worker_config_failed")?
        .effective()
        .clone();
    let inbound_port = config.capture().inbound_port().ok_or("inbound_missing")?;
    let store = GenerationStore::new(&root).map_err(|_| "store_failed")?;
    let checker =
        SingBoxCheckRunner::new(&sing_box, store.generations_root(), RunnerLimits::default())
            .map_err(|error| error.code().as_str())?;
    let launcher = CoreProcessRunner::new(
        &sing_box,
        store.generations_root(),
        CoreProcessLimits::default(),
    )
    .map_err(|error| error.code().as_str())?;
    let tools = AndroidToolPaths::from_system().map_err(|error| error.code().as_str())?;
    let mut capabilities = CapabilityProbe::new(
        CommandProbeBackend::new(tools.clone(), ProbeLimits::default()),
        config.allocations().to_vec(),
        inbound_port,
    )
    .map_err(|error| error.code().as_str())?;
    let mut network = NetworkExecutor::new(
        SystemCommandBackend::from_system(SystemCommandLimits::default())
            .map_err(|error| error.code().as_str())?,
    );
    let mut verifier = NetworkPlanVerifier::new(
        CommandProbeBackend::new(tools, ProbeLimits::default()),
        inbound_port,
    )
    .map_err(|error| error.code().as_str())?;
    let liveness = StartupLivenessProbe::default();
    let sealed = store
        .sealed_generation(generation)
        .map_err(|_| "invalid_current_generation")?;
    checker
        .check(&sealed.config_path())
        .map_err(|_| "core_check_failed")?;
    let report = capabilities
        .probe()
        .map_err(|error| error.code().as_str())?;
    let plan = NetworkPlanner
        .build_tproxy(generation, PlanSlot::A, config.capture(), &report)
        .map_err(|error| error.code().as_str())?;
    let mut process = launcher
        .start(&sealed.config_path())
        .map_err(|error| error.code().as_str())?;
    if liveness.wait_healthy(&mut process).is_err() {
        let _ = process.stop();
        return Err("core_health_failed");
    }
    let mut receipt = match network.apply(&plan) {
        Ok(receipt) => receipt,
        Err(error) => {
            let _ = process.stop();
            return Err(error.code().as_str());
        }
    };
    let verification = verifier
        .verify(&plan)
        .map_err(|error| error.code().as_str());
    let network_cleanup_failed = network.rollback(&plan, &mut receipt).is_err();
    let core_cleanup_failed = process.stop().is_err();
    verification?;
    if network_cleanup_failed {
        return Err("rollback_network_failed");
    }
    if core_cleanup_failed {
        return Err("rollback_core_failed");
    }
    let allocation = plan.allocation();
    let result = serde_json::json!({
        "ok": true,
        "stage": "activated",
        "generation": generation.get(),
        "mark": allocation.mark(),
        "route_table": allocation.route_table(),
        "rule_priority": allocation.rule_priority(),
    });
    println!("{result}");
    Ok(())
}
