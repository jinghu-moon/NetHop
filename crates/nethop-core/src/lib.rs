#![doc = "Pure domain and configuration transaction core for NetHop."]

pub mod capture;
pub mod composer;
pub mod diagnostics;
pub mod generation;
pub mod lifecycle;
pub mod resource;
pub mod state;
pub mod territory;

pub use capture::{
    CaptureMode, CapturePolicy, CapturePolicyError, ForwardingPolicy, InterfacePolicy,
};
pub use composer::{
    ClashApi, ComposerError, MANAGED_FETCH_PROXY_ENDPOINT, MANAGED_FETCH_PROXY_USERNAME,
    ManagedConfig, ManagedLogLevel, ManagedOptions, ManagedOutboundMode, ManagedProfile,
    TerminalOutbound, TunStack,
};
pub use diagnostics::{CoreDiagnosticCode, CoreError};
pub use generation::{
    Candidate, GenerationId, GenerationManifest, GenerationNodeRecord, GenerationNodeRegistry,
    GenerationStore, PreparedCandidate, SealedGeneration,
};
pub use lifecycle::{
    CaptureState, CoreState, LifecycleAxes, LifecycleError, LifecycleEvent, LifecycleReducer,
    ResourceState,
};
pub use resource::{
    DEFAULT_TEST_COLD_TIMEOUT, DEFAULT_TEST_WARM_TIMEOUT, IdleDecision, IdleInputs, IdlePolicy,
    ResourceAggregate, ResourceBudget, ResourcePressure, ResourceSample, ResumeCost,
};
pub use state::{RuntimeState, StateTransitionError};
pub use territory::{
    DisplayTerritoryCode, InvalidTerritoryCode, TerritoryRecord, territories, territory_by_alpha2,
    territory_by_alpha3,
};
