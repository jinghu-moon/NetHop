#![doc = "Pure domain and configuration transaction core for NetHop."]

pub mod capture;
pub mod composer;
pub mod diagnostics;
pub mod generation;
pub mod state;

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
pub use state::{RuntimeState, StateTransitionError};
