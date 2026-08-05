#[cfg(unix)]
use std::{env, path::PathBuf, process};

#[cfg(unix)]
use nethop_android::{
    AndroidToolPaths, CapabilityProbe, CommandProbeBackend, NetworkExecutor, NetworkPlanVerifier,
    PlanSlot, ProbeLimits, SystemCommandBackend, SystemCommandLimits,
};
#[cfg(unix)]
use nethop_core::{GenerationId, GenerationStore};
#[cfg(unix)]
use nethopd::{
    ConfigStore, CoreProcessLimits, CoreProcessRunner, CurrentGenerationActivator,
    NetworkDataPlaneHealthProbe, RunnerLimits, SingBoxCheckRunner, StartupLivenessProbe,
    WorkerRecoveryError,
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
    let verifier = NetworkPlanVerifier::new(
        CommandProbeBackend::new(tools, ProbeLimits::default()),
        inbound_port,
    )
    .map_err(|error| error.code().as_str())?;
    let mut data_plane = NetworkDataPlaneHealthProbe::new(verifier);
    let liveness = StartupLivenessProbe::default();
    let mut activator = CurrentGenerationActivator::new(
        &store,
        &checker,
        &launcher,
        &liveness,
        &mut capabilities,
        &mut network,
        &mut data_plane,
    );

    let active = activator
        .recover_generation(generation, config.capture(), PlanSlot::A)
        .map_err(recovery_code)?
        .ok_or("generation_missing")?;
    let allocation = active.plan().allocation();
    let result = serde_json::json!({
        "ok": true,
        "stage": "activated",
        "generation": generation.get(),
        "mark": allocation.mark(),
        "route_table": allocation.route_table(),
        "rule_priority": allocation.rule_priority(),
    });
    active.stop(&mut network).map_err(|error| {
        if error.network_failed() {
            "rollback_network_failed"
        } else {
            "rollback_core_failed"
        }
    })?;
    println!("{result}");
    Ok(())
}

#[cfg(unix)]
const fn recovery_code(error: WorkerRecoveryError) -> &'static str {
    match error {
        WorkerRecoveryError::InvalidCurrentGeneration => "invalid_current_generation",
        WorkerRecoveryError::CapabilityProbeFailed => "capability_probe_failed",
        WorkerRecoveryError::CoreCheckFailed => "core_check_failed",
        WorkerRecoveryError::NetworkPlanRejected => "network_plan_rejected",
        WorkerRecoveryError::CoreStartFailed => "core_start_failed",
        WorkerRecoveryError::CoreHealthFailed {
            cleanup_failed: false,
        } => "core_health_failed",
        WorkerRecoveryError::CoreHealthFailed {
            cleanup_failed: true,
        } => "core_health_cleanup_failed",
        WorkerRecoveryError::NetworkApplyFailed {
            cleanup_failed: false,
        } => "network_apply_failed",
        WorkerRecoveryError::NetworkApplyFailed {
            cleanup_failed: true,
        } => "network_apply_cleanup_failed",
        WorkerRecoveryError::DataPlaneHealthFailed {
            cleanup_failed: false,
        } => "data_plane_health_failed",
        WorkerRecoveryError::DataPlaneHealthFailed {
            cleanup_failed: true,
        } => "data_plane_health_cleanup_failed",
    }
}
