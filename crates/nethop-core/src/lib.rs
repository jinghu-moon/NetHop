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
    ClashApi, ComposerError, ManagedConfig, ManagedLogLevel, ManagedOptions, ManagedOutboundMode,
    ManagedProfile, ManagedSelectorMode, TerminalOutbound, TunStack,
};
pub use diagnostics::{CoreDiagnosticCode, CoreError};
pub use generation::{
    Candidate, GenerationId, GenerationManifest, GenerationStore, PreparedCandidate,
    SealedGeneration,
};
pub use state::{RuntimeState, StateTransitionError};
