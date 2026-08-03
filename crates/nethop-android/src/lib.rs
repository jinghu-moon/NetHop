#![doc = "Android capability probing and controlled network mutation for NetHop."]

pub mod capability;
pub mod executor;
pub mod health;
pub mod plan;
pub mod tun;

pub use capability::{
    AllocationCapability, AndroidToolPaths, CapabilityDiagnosticCode, CapabilityError,
    CapabilityProbe, CapabilityReport, CapabilityStatus, CommandProbeBackend, FamilyCapability,
    IpFamily, NetfilterBackend, ProbeBackend, ProbeCommand, ProbeLimits, ProbeOutput,
    ResourceCandidate,
};
pub use executor::{
    ApplyReceipt, CommandFailure, CommandInvocation, CommandOutput, ExecutionDiagnosticCode,
    ExecutionError, NetworkCommandBackend, NetworkExecutor, NetworkProgram, SystemCommandBackend,
    SystemCommandLimits,
};
pub use health::{
    NetworkHealthDiagnosticCode, NetworkHealthError, NetworkHealthVerifier, NetworkPlanVerifier,
};
pub use plan::{
    NetworkOperationKind, NetworkPlan, NetworkPlanError, NetworkPlanner, PlanDiagnosticCode,
    PlanSlot,
};
pub use tun::{
    TunCandidate, TunFallbackError, TunFallbackPlan, TunFallbackPlanner, TunHealthError,
    TunHealthVerifier, default_tun_interface,
};
