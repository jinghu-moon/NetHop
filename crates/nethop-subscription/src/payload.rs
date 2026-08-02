use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest as ShaDigest, Sha256};
use thiserror::Error;

use crate::limits::{MAX_BODY_BYTES, MAX_STRING_BYTES, ParserLimits};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayloadOriginKind {
    QrRawValue,
    LocalFile,
    PastedText,
    HttpResponse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpScheme {
    Http,
    Https,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormatHint {
    Auto,
    UriList,
    Base64List,
    ClashYaml,
    SingboxJson,
    IniProfile,
    SurfboardIni,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SourceId(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SourceIdError {
    #[error("source id is empty")]
    Empty,
    #[error("source id exceeds 128 bytes")]
    TooLong,
    #[error("source id contains an invalid character")]
    InvalidCharacter,
}

impl SourceId {
    pub fn new(value: impl Into<String>) -> Result<Self, SourceIdError> {
        let value = value.into();
        if value.is_empty() {
            return Err(SourceIdError::Empty);
        }
        if value.len() > 128 {
            return Err(SourceIdError::TooLong);
        }
        if !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
        {
            return Err(SourceIdError::InvalidCharacter);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SourceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("SourceId").field(&self.0).finish()
    }
}

impl fmt::Display for SourceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Digest([u8; 32]);

impl Digest {
    pub fn sha256(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }

    pub fn from_hex(value: &str) -> Option<Self> {
        if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let mut bytes = [0_u8; 32];
        for (index, slot) in bytes.iter_mut().enumerate() {
            *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
        }
        Some(Self(bytes))
    }

    pub const fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
    pub fn hex(&self) -> String {
        self.0.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}

impl fmt::Debug for Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.hex())
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.hex())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceivedAt {
    pub wall_clock_unix_ms: u64,
    pub monotonic_nanos: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FetchMetadata {
    pub status_code: u16,
    pub declared_content_type: Option<String>,
    pub response_bytes: usize,
    pub final_scheme: HttpScheme,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PayloadOrigin {
    QrRawValue,
    LocalFile { display_name: Option<String> },
    PastedText,
    HttpResponse { metadata: FetchMetadata },
}

impl PayloadOrigin {
    pub fn kind(&self) -> PayloadOriginKind {
        match self {
            Self::QrRawValue => PayloadOriginKind::QrRawValue,
            Self::LocalFile { .. } => PayloadOriginKind::LocalFile,
            Self::PastedText => PayloadOriginKind::PastedText,
            Self::HttpResponse { .. } => PayloadOriginKind::HttpResponse,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PayloadError {
    #[error("payload exceeds {MAX_BODY_BYTES} bytes")]
    TooLarge,
    #[error("payload text is not valid UTF-8")]
    InvalidUtf8,
    #[error("payload metadata string exceeds {MAX_STRING_BYTES} bytes")]
    MetadataTooLong,
    #[error("local file display name must not contain a path or control character")]
    InvalidDisplayName,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceMetadata {
    pub source_id: Option<SourceId>,
    pub origin_kind: PayloadOriginKind,
    pub content_digest: Digest,
    pub source_url_digest: Option<Digest>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ImportPayload {
    origin: PayloadOrigin,
    bytes: Vec<u8>,
    declared_content_type: Option<String>,
    expected_format: FormatHint,
    source_id: Option<SourceId>,
    source_url_digest: Option<Digest>,
    received_at: ReceivedAt,
}

impl ImportPayload {
    pub fn new(
        origin: PayloadOrigin,
        bytes: Vec<u8>,
        expected_format: FormatHint,
        source_id: Option<SourceId>,
        source_url_digest: Option<Digest>,
        received_at: ReceivedAt,
        limits: &ParserLimits,
    ) -> Result<Self, PayloadError> {
        if bytes.len() > limits.max_body_bytes() {
            return Err(PayloadError::TooLarge);
        }
        if let PayloadOrigin::LocalFile {
            display_name: Some(name),
        } = &origin
        {
            if name.len() > MAX_STRING_BYTES
                || name.chars().any(char::is_control)
                || name.contains(['/', '\\'])
            {
                return Err(PayloadError::InvalidDisplayName);
            }
        }
        let declared_content_type = match &origin {
            PayloadOrigin::HttpResponse { metadata } => metadata.declared_content_type.clone(),
            _ => None,
        };
        if declared_content_type
            .as_ref()
            .is_some_and(|value| value.len() > MAX_STRING_BYTES)
        {
            return Err(PayloadError::MetadataTooLong);
        }
        Ok(Self {
            origin,
            bytes,
            declared_content_type,
            expected_format,
            source_id,
            source_url_digest,
            received_at,
        })
    }

    pub fn from_text(
        origin: PayloadOrigin,
        text: String,
        expected_format: FormatHint,
        source_id: Option<SourceId>,
        received_at: ReceivedAt,
        limits: &ParserLimits,
    ) -> Result<Self, PayloadError> {
        if text.len() > limits.max_body_bytes() {
            return Err(PayloadError::TooLarge);
        }
        Self::new(
            origin,
            text.into_bytes(),
            expected_format,
            source_id,
            None,
            received_at,
            limits,
        )
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn origin(&self) -> &PayloadOrigin {
        &self.origin
    }
    pub fn expected_format(&self) -> FormatHint {
        self.expected_format
    }
    pub fn source_id(&self) -> Option<&SourceId> {
        self.source_id.as_ref()
    }
    pub fn received_at(&self) -> &ReceivedAt {
        &self.received_at
    }
    pub fn declared_content_type(&self) -> Option<&str> {
        self.declared_content_type.as_deref()
    }

    pub fn metadata(&self) -> SourceMetadata {
        SourceMetadata {
            source_id: self.source_id.clone(),
            origin_kind: self.origin.kind(),
            content_digest: Digest::sha256(&self.bytes),
            source_url_digest: self.source_url_digest,
        }
    }
}

impl fmt::Debug for ImportPayload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImportPayload")
            .field("origin_kind", &self.origin.kind())
            .field("bytes_len", &self.bytes.len())
            .field("expected_format", &self.expected_format)
            .field("source_id", &self.source_id)
            .field("content_digest", &Digest::sha256(&self.bytes))
            .finish()
    }
}
