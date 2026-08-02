use std::fmt;

use base64::Engine;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::payload::{PayloadError, SourceIdError};
use crate::{
    ConversionReport, DiagnosticCode, Digest, FetchMetadata, FormatHint, HttpScheme, ImportPayload,
    ParserLimits, PayloadOrigin, ReceivedAt, SourceId, StableConversion,
};

pub const PARSER_IPC_SCHEMA_VERSION: u32 = 1;
pub const MAX_PARSER_IPC_FRAME_BYTES: usize = 7 * 1024 * 1024;
pub const ACTIVE_OUTBOUND_BASELINE: usize = 500;
pub const MANAGED_ACTIVE_OUTBOUND_LIMIT: usize = 2_000;
pub const CONVERSION_NODE_LIMIT: usize = crate::limits::MAX_NODE_COUNT;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestProfile {
    NetHopGeneric,
    Mihomo,
    ClashStandard,
    Surfboard,
    SingBox,
    SingBoxAndroid,
}

impl RequestProfile {
    pub const fn user_agent(self) -> &'static str {
        match self {
            Self::NetHopGeneric => "NetHop/0.1",
            Self::Mihomo => "clash.meta",
            Self::ClashStandard => "clash",
            Self::Surfboard => "Surfboard",
            Self::SingBox => "sing-box",
            Self::SingBoxAndroid => "SFA",
        }
    }

    pub const fn accept(self) -> &'static str {
        match self {
            Self::NetHopGeneric => "*/*",
            Self::Mihomo => "application/yaml, text/yaml, */*",
            Self::ClashStandard => "application/yaml, text/yaml, */*",
            Self::Surfboard => "text/plain, */*",
            Self::SingBox => "application/json, */*",
            Self::SingBoxAndroid => "application/json, */*",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum IpcPayloadOrigin {
    QrRawValue {
        user_confirmed: bool,
    },
    LocalFile {
        display_name: Option<String>,
    },
    PastedText,
    HttpResponse {
        status_code: u16,
        declared_content_type: Option<String>,
        final_scheme: HttpScheme,
    },
}

#[derive(Clone, PartialEq, Eq)]
pub struct ParserIpcRequest {
    request_id: SourceId,
    source_id: SourceId,
    origin: IpcPayloadOrigin,
    expected_format: FormatHint,
    request_profile: RequestProfile,
    source_url_digest: Option<Digest>,
    payload: Vec<u8>,
}

impl ParserIpcRequest {
    pub fn from_json(input: &[u8], limits: &ParserLimits) -> Result<Self, ParserIpcRequestError> {
        if input.len() > MAX_PARSER_IPC_FRAME_BYTES {
            return Err(ParserIpcRequestError::FrameTooLarge);
        }
        let wire = serde_json::from_slice::<ParserIpcRequestWire>(input)
            .map_err(|_| ParserIpcRequestError::InvalidRequest)?;
        if wire.schema_version != PARSER_IPC_SCHEMA_VERSION {
            return Err(ParserIpcRequestError::UnsupportedSchema);
        }
        let request_id = SourceId::new(wire.request_id).map_err(map_source_id_error)?;
        let source_id = SourceId::new(wire.source_id).map_err(map_source_id_error)?;
        let source_url_digest = wire
            .source_url_digest
            .map(|value| Digest::from_hex(&value).ok_or(ParserIpcRequestError::InvalidRequest))
            .transpose()?;
        let payload = decode_payload(&wire.payload_base64, limits)?;
        validate_origin(&wire.origin, &payload)?;
        Ok(Self {
            request_id,
            source_id,
            origin: wire.origin,
            expected_format: wire.expected_format,
            request_profile: wire.request_profile,
            source_url_digest,
            payload,
        })
    }

    pub const fn schema_version(&self) -> u32 {
        PARSER_IPC_SCHEMA_VERSION
    }

    pub fn request_id(&self) -> &SourceId {
        &self.request_id
    }

    pub fn source_id(&self) -> &SourceId {
        &self.source_id
    }

    pub fn origin(&self) -> &IpcPayloadOrigin {
        &self.origin
    }

    pub const fn expected_format(&self) -> FormatHint {
        self.expected_format
    }

    pub const fn request_profile(&self) -> RequestProfile {
        self.request_profile
    }

    pub fn to_import_payload(
        &self,
        received_at: ReceivedAt,
        limits: &ParserLimits,
    ) -> Result<ImportPayload, ParserIpcRequestError> {
        if self.payload.len() > limits.max_body_bytes() {
            return Err(ParserIpcRequestError::PayloadTooLarge);
        }
        let origin = match &self.origin {
            IpcPayloadOrigin::QrRawValue { .. } => PayloadOrigin::QrRawValue,
            IpcPayloadOrigin::LocalFile { display_name } => PayloadOrigin::LocalFile {
                display_name: display_name.clone(),
            },
            IpcPayloadOrigin::PastedText => PayloadOrigin::PastedText,
            IpcPayloadOrigin::HttpResponse {
                status_code,
                declared_content_type,
                final_scheme,
            } => PayloadOrigin::HttpResponse {
                metadata: FetchMetadata {
                    status_code: *status_code,
                    declared_content_type: declared_content_type.clone(),
                    response_bytes: self.payload.len(),
                    final_scheme: *final_scheme,
                },
            },
        };
        ImportPayload::new(
            origin,
            self.payload.clone(),
            self.expected_format,
            Some(self.source_id.clone()),
            self.source_url_digest,
            received_at,
            limits,
        )
        .map_err(ParserIpcRequestError::from)
    }
}

impl fmt::Debug for ParserIpcRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ParserIpcRequest")
            .field("schema_version", &PARSER_IPC_SCHEMA_VERSION)
            .field("request_id", &self.request_id)
            .field("source_id", &self.source_id)
            .field("origin", &self.origin)
            .field("expected_format", &self.expected_format)
            .field("request_profile", &self.request_profile)
            .field("payload_bytes", &self.payload.len())
            .field("content_digest", &Digest::sha256(&self.payload))
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParserIpcRequestError {
    #[error("parser IPC frame exceeds the configured maximum")]
    FrameTooLarge,
    #[error("parser IPC request is invalid")]
    InvalidRequest,
    #[error("parser IPC schema version is unsupported")]
    UnsupportedSchema,
    #[error("parser IPC request identifier is invalid")]
    InvalidRequestId,
    #[error("parser IPC source identifier is invalid")]
    InvalidSourceId,
    #[error("parser IPC payload exceeds the parser body limit")]
    PayloadTooLarge,
    #[error("parser IPC HTTP response must be HTTPS and successful")]
    InsecureHttpResponse,
    #[error("parser IPC payload metadata is invalid")]
    InvalidPayloadMetadata,
    #[error("parser IPC QR raw value must be valid UTF-8 text")]
    InvalidQrRawValue,
    #[error("parser IPC QR URL requires user confirmation")]
    UnconfirmedUrl,
}

impl From<PayloadError> for ParserIpcRequestError {
    fn from(value: PayloadError) -> Self {
        match value {
            PayloadError::TooLarge => Self::PayloadTooLarge,
            PayloadError::InvalidUtf8
            | PayloadError::MetadataTooLong
            | PayloadError::InvalidDisplayName => Self::InvalidPayloadMetadata,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ParserIpcRequestWire {
    schema_version: u32,
    request_id: String,
    source_id: String,
    origin: IpcPayloadOrigin,
    expected_format: FormatHint,
    request_profile: RequestProfile,
    source_url_digest: Option<String>,
    payload_base64: String,
}

fn map_source_id_error(error: SourceIdError) -> ParserIpcRequestError {
    match error {
        SourceIdError::Empty | SourceIdError::TooLong | SourceIdError::InvalidCharacter => {
            ParserIpcRequestError::InvalidSourceId
        }
    }
}

fn decode_payload(
    payload_base64: &str,
    limits: &ParserLimits,
) -> Result<Vec<u8>, ParserIpcRequestError> {
    let max_encoded_bytes = limits.max_body_bytes().div_ceil(3) * 4;
    if payload_base64.len() > max_encoded_bytes {
        return Err(ParserIpcRequestError::PayloadTooLarge);
    }
    let payload = base64::engine::general_purpose::STANDARD
        .decode(payload_base64)
        .map_err(|_| ParserIpcRequestError::InvalidRequest)?;
    if payload.len() > limits.max_body_bytes() {
        return Err(ParserIpcRequestError::PayloadTooLarge);
    }
    Ok(payload)
}

fn validate_origin(origin: &IpcPayloadOrigin, payload: &[u8]) -> Result<(), ParserIpcRequestError> {
    match origin {
        IpcPayloadOrigin::QrRawValue { user_confirmed } => {
            let raw_value = std::str::from_utf8(payload)
                .map_err(|_| ParserIpcRequestError::InvalidQrRawValue)?;
            if raw_value.contains('\0') {
                return Err(ParserIpcRequestError::InvalidQrRawValue);
            }
            if !user_confirmed && looks_like_url(raw_value) {
                return Err(ParserIpcRequestError::UnconfirmedUrl);
            }
        }
        IpcPayloadOrigin::HttpResponse {
            status_code,
            final_scheme,
            ..
        } => {
            if *final_scheme != HttpScheme::Https || !(200..300).contains(status_code) {
                return Err(ParserIpcRequestError::InsecureHttpResponse);
            }
        }
        IpcPayloadOrigin::LocalFile { .. } | IpcPayloadOrigin::PastedText => {}
    }
    Ok(())
}

fn looks_like_url(value: &str) -> bool {
    let scheme_end = value.find(':').unwrap_or(0);
    scheme_end > 0
        && value.as_bytes()[..scheme_end]
            .iter()
            .copied()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CandidateStatus {
    Ready {
        node_count: usize,
        candidate_digest: String,
    },
    AcceptedZero,
    Rejected {
        code: DiagnosticCode,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ParserIpcResponse {
    schema_version: u32,
    request_id: String,
    candidate: CandidateStatus,
    report: ConversionReport,
}

impl ParserIpcResponse {
    pub fn from_conversion(
        request_id: SourceId,
        conversion: &StableConversion,
        limits: &ParserLimits,
    ) -> Result<Self, ParserIpcResponseError> {
        let report = bounded_report(&conversion.report, limits)?;
        let candidate = candidate_status(conversion);
        Ok(Self {
            schema_version: PARSER_IPC_SCHEMA_VERSION,
            request_id: request_id.as_str().to_owned(),
            candidate,
            report,
        })
    }

    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    pub fn candidate(&self) -> &CandidateStatus {
        &self.candidate
    }

    pub fn report(&self) -> &ConversionReport {
        &self.report
    }

    pub fn to_json(&self, limits: &ParserLimits) -> Result<String, ParserIpcResponseError> {
        let json =
            serde_json::to_string(self).map_err(|_| ParserIpcResponseError::Serialization)?;
        if json.len() > limits.max_report_bytes() {
            return Err(ParserIpcResponseError::ReportTooLarge);
        }
        Ok(json)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ParserIpcResponseError {
    #[error("parser IPC response report exceeds the configured maximum")]
    ReportTooLarge,
    #[error("parser IPC response cannot be serialized")]
    Serialization,
}

fn bounded_report(
    report: &ConversionReport,
    limits: &ParserLimits,
) -> Result<ConversionReport, ParserIpcResponseError> {
    let json = report.bounded_json(limits);
    if json.len() > limits.max_report_bytes() {
        return Err(ParserIpcResponseError::ReportTooLarge);
    }
    serde_json::from_str(&json).map_err(|_| ParserIpcResponseError::Serialization)
}

fn candidate_status(conversion: &StableConversion) -> CandidateStatus {
    if conversion.nodes.len() > MANAGED_ACTIVE_OUTBOUND_LIMIT {
        return CandidateStatus::Rejected {
            code: DiagnosticCode::ActiveLimitExceeded,
        };
    }
    if conversion.report.summary.source_success {
        return CandidateStatus::Ready {
            node_count: conversion.nodes.len(),
            candidate_digest: Digest::sha256(conversion.outbounds_json.as_bytes()).hex(),
        };
    }
    if conversion.report.summary.rejected == 0 {
        CandidateStatus::AcceptedZero
    } else {
        CandidateStatus::Rejected {
            code: conversion
                .report
                .diagnostics
                .first()
                .map(|diagnostic| diagnostic.code.clone())
                .unwrap_or(DiagnosticCode::SourceAllFailed),
        }
    }
}
