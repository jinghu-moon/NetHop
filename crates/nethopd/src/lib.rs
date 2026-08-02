#![doc = "Controlled daemon process boundaries for NetHop."]

pub mod runner;

pub use runner::{
    CheckOutputSummary, CheckReport, RunnerDiagnosticCode, RunnerError, RunnerLimits,
    SingBoxCheckRunner,
};
