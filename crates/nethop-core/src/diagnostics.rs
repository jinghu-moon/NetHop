use thiserror::Error;

use crate::{capture::CapturePolicyError, composer::ComposerError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreDiagnosticCode {
    InvalidStateTransition,
    InvalidGenerationId,
    GenerationPublishFailed,
    ValidationFailed,
    ComposerRejected,
    CaptureRejected,
    IoFailure,
    SerializationFailure,
}

impl CoreDiagnosticCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidStateTransition => "invalid_state_transition",
            Self::InvalidGenerationId => "invalid_generation_id",
            Self::GenerationPublishFailed => "generation_publish_failed",
            Self::ValidationFailed => "validation_failed",
            Self::ComposerRejected => "composer_rejected",
            Self::CaptureRejected => "capture_rejected",
            Self::IoFailure => "io_failure",
            Self::SerializationFailure => "serialization_failure",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CoreError {
    #[error("composer rejected candidate: {0}")]
    Composer(#[from] ComposerError),
    #[error("capture policy rejected: {0}")]
    Capture(#[from] CapturePolicyError),
    #[error("generation id must be greater than zero")]
    InvalidGenerationId,
    #[error("candidate validation failed")]
    ValidationFailed,
    #[error("generation publish failed during {operation}: {message}")]
    GenerationPublishFailed { operation: String, message: String },
    #[error("serialization failed: {0}")]
    SerializationFailure(String),
    #[error("current generation pointer is invalid")]
    InvalidCurrentPointer,
}

impl CoreError {
    pub const fn code(&self) -> CoreDiagnosticCode {
        match self {
            Self::Composer(_) => CoreDiagnosticCode::ComposerRejected,
            Self::Capture(_) => CoreDiagnosticCode::CaptureRejected,
            Self::InvalidGenerationId => CoreDiagnosticCode::InvalidGenerationId,
            Self::ValidationFailed => CoreDiagnosticCode::ValidationFailed,
            Self::GenerationPublishFailed { .. } => CoreDiagnosticCode::GenerationPublishFailed,
            Self::SerializationFailure(_) => CoreDiagnosticCode::SerializationFailure,
            Self::InvalidCurrentPointer => CoreDiagnosticCode::GenerationPublishFailed,
        }
    }
}

pub(crate) fn io_error(operation: &str, error: std::io::Error) -> CoreError {
    CoreError::GenerationPublishFailed {
        operation: operation.to_owned(),
        message: error.to_string(),
    }
}
