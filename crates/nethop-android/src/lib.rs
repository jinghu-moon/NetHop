#![doc = "Android capability probing and controlled network mutation for NetHop."]

pub mod apps;
pub mod capability;
pub mod executor;
pub mod health;
pub mod netlink;
pub mod plan;
pub mod tun;

pub use apps::{
    AppCatalog, AppCatalogError, AppClass, AppIdentity, AppSelectionMode, CompiledAppSelection,
    PackageSnapshot, SharedUidExpansion, UidGroup,
};
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
#[cfg(any(target_os = "android", target_os = "linux"))]
pub use netlink::NetlinkRouteSocket;
pub use netlink::{
    NetlinkDebouncer, NetlinkError, NetlinkEventReader, NetlinkEventSource, NetworkAction,
    NetworkChange, NetworkEvent, NetworkEventBatch,
};
pub use plan::{
    NetworkOperationKind, NetworkPlan, NetworkPlanError, NetworkPlanner, PlanDiagnosticCode,
    PlanSlot,
};
pub use tun::{
    TunCandidate, TunFallbackError, TunFallbackPlan, TunFallbackPlanner, TunHealthError,
    TunHealthVerifier, default_tun_interface,
};
