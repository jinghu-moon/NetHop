#![doc = "Android capability probing and controlled network mutation for NetHop."]

pub mod apps;
pub mod capability;
pub mod executor;
pub mod forwarding;
pub mod health;
pub mod netlink;
pub mod notification;
pub mod plan;
pub mod private_dns;
pub mod tun;
pub mod wifi_scene;

pub use apps::{
    AppCatalog, AppCatalogError, AppClass, AppIdentity, AppSelectionMode, CompiledAppSelection,
    PackageSnapshot, SharedUidExpansion, UidGroup,
};
pub use capability::{
    AllocationCapability, AndroidToolPaths, CapabilityDiagnosticCode, CapabilityError,
    CapabilityProbe, CapabilityReport, CapabilityStatus, CommandProbeBackend, FamilyCapability,
    IpFamily, NetfilterBackend, NetfilterTable, PackageListKind, ProbeBackend, ProbeCommand,
    ProbeLimits, ProbeOutput, ResourceCandidate,
};
pub use executor::{
    ApplyReceipt, CommandFailure, CommandInvocation, CommandOutput, ExecutionDiagnosticCode,
    ExecutionError, NetworkCommandBackend, NetworkExecutor, NetworkProgram, SystemCommandBackend,
    SystemCommandLimits,
};
pub use forwarding::{
    ForwardingHealthError, ForwardingPlan, ForwardingPlanError, ForwardingPlanVerifier,
    ForwardingPlanner,
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
pub use notification::{
    CommandUpdateNotifier, UpdateNotificationOutcome, UpdateNotificationSink,
    core_update_notification_arguments,
};
pub use plan::{
    NetworkOperationKind, NetworkPlan, NetworkPlanError, NetworkPlanner, PlanDiagnosticCode,
    PlanSlot,
};
pub use private_dns::{
    CommandPrivateDnsFactsSource, DnsSplitStatus, PrivateDnsError, PrivateDnsFactsSource,
    PrivateDnsMode, PrivateDnsStatus,
};
pub use tun::{
    TunCandidate, TunFallbackError, TunFallbackPlan, TunFallbackPlanner, TunHealthError,
    TunHealthProbe, TunHealthVerifier, default_tun_interface,
};
pub use wifi_scene::{
    CommandWifiFactsSource, WifiFactsSource, WifiNetworkFacts, WifiSceneAction, WifiSceneDecision,
    WifiSceneError, WifiSceneMatcher, WifiSceneRule,
};
