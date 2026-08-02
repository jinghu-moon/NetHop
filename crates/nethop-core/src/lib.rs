#![doc = "Pure domain and configuration transaction core for NetHop."]

pub mod capture;
pub mod composer;
pub mod diagnostics;
pub mod generation;
pub mod state;

pub use capture::{CaptureMode, CapturePolicy, CapturePolicyError};
pub use composer::{ComposerError, ManagedConfig, TerminalOutbound};
pub use diagnostics::{CoreDiagnosticCode, CoreError};
pub use generation::{Candidate, GenerationId, GenerationManifest, GenerationStore};
pub use state::{RuntimeState, StateTransitionError};
