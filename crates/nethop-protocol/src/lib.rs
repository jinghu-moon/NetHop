#![doc = "Bounded IPC v1 types and framing without socket ownership."]

use std::io::{Read, Write};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use serde_json::Value;
use thiserror::Error;

pub const PROTOCOL_VERSION: u8 = 1;
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
const MAX_REQUEST_ID_BYTES: usize = 64;
const MAX_ERROR_DETAIL_BYTES: usize = 48;
const MAX_MESSAGE_BYTES: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RequestId(String);

impl RequestId {
    pub fn new(value: impl Into<String>) -> Result<Self, ProtocolError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_REQUEST_ID_BYTES
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err(ProtocolError::InvalidRequestId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for RequestId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for RequestId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlMethod {
    #[serde(rename = "status.get")]
    StatusGet,
    #[serde(rename = "service.start")]
    ServiceStart,
    #[serde(rename = "service.stop")]
    ServiceStop,
    #[serde(rename = "capability.probe")]
    CapabilityProbe,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmptyParams {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlRequest {
    version: u8,
    request_id: RequestId,
    method: ControlMethod,
    params: EmptyParams,
}

impl ControlRequest {
    pub fn new(request_id: RequestId, method: ControlMethod) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            request_id,
            method,
            params: EmptyParams {},
        }
    }

    pub const fn version(&self) -> u8 {
        self.version
    }

    pub fn request_id(&self) -> &RequestId {
        &self.request_id
    }

    pub const fn method(&self) -> ControlMethod {
        self.method
    }

    fn validate(&self) -> Result<(), ProtocolError> {
        (self.version == PROTOCOL_VERSION)
            .then_some(())
            .ok_or(ProtocolError::UnsupportedVersion)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorDomain {
    Config,
    Subscription,
    Capability,
    Network,
    Core,
    Stats,
    Auth,
}

impl ErrorDomain {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Config => "CONFIG",
            Self::Subscription => "SUB",
            Self::Capability => "CAP",
            Self::Network => "NET",
            Self::Core => "CORE",
            Self::Stats => "STATS",
            Self::Auth => "AUTH",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ErrorCode(String);

impl ErrorCode {
    pub fn new(domain: ErrorDomain, detail: &str) -> Result<Self, ProtocolError> {
        if detail.is_empty()
            || detail.len() > MAX_ERROR_DETAIL_BYTES
            || !detail
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'-')
            || detail.starts_with('-')
            || detail.ends_with('-')
            || detail.contains("--")
        {
            return Err(ProtocolError::InvalidErrorCode);
        }
        Ok(Self(format!("NH-{}-{detail}", domain.as_str())))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn parse(value: String) -> Result<Self, ProtocolError> {
        let rest = value
            .strip_prefix("NH-")
            .ok_or(ProtocolError::InvalidErrorCode)?;
        let (domain, detail) = rest
            .split_once('-')
            .ok_or(ProtocolError::InvalidErrorCode)?;
        let domain = match domain {
            "CONFIG" => ErrorDomain::Config,
            "SUB" => ErrorDomain::Subscription,
            "CAP" => ErrorDomain::Capability,
            "NET" => ErrorDomain::Network,
            "CORE" => ErrorDomain::Core,
            "STATS" => ErrorDomain::Stats,
            "AUTH" => ErrorDomain::Auth,
            _ => return Err(ProtocolError::InvalidErrorCode),
        };
        let parsed = Self::new(domain, detail)?;
        (parsed.0 == value)
            .then_some(parsed)
            .ok_or(ProtocolError::InvalidErrorCode)
    }
}

impl Serialize for ErrorCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ErrorCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlError {
    code: ErrorCode,
    message: String,
}

impl ControlError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Result<Self, ProtocolError> {
        let message = message.into();
        validate_message(&message)?;
        Ok(Self { code, message })
    }

    pub fn code(&self) -> &ErrorCode {
        &self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    fn validate(&self) -> Result<(), ProtocolError> {
        validate_message(&self.message)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlResponse {
    version: u8,
    request_id: RequestId,
    ok: bool,
    generation: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ControlError>,
}

impl ControlResponse {
    pub fn success(request_id: RequestId, generation: Option<u64>, result: Value) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            request_id,
            ok: true,
            generation,
            result: Some(result),
            error: None,
        }
    }

    pub fn failure(request_id: RequestId, generation: Option<u64>, error: ControlError) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            request_id,
            ok: false,
            generation,
            result: None,
            error: Some(error),
        }
    }

    pub fn request_id(&self) -> &RequestId {
        &self.request_id
    }

    pub const fn ok(&self) -> bool {
        self.ok
    }

    pub const fn generation(&self) -> Option<u64> {
        self.generation
    }

    pub fn result(&self) -> Option<&Value> {
        self.result.as_ref()
    }

    pub fn error(&self) -> Option<&ControlError> {
        self.error.as_ref()
    }

    fn validate(&self) -> Result<(), ProtocolError> {
        if self.version != PROTOCOL_VERSION {
            return Err(ProtocolError::UnsupportedVersion);
        }
        if self.generation == Some(0) {
            return Err(ProtocolError::InvalidEnvelope);
        }
        match (self.ok, self.result.is_some(), self.error.as_ref()) {
            (true, true, None) => Ok(()),
            (false, false, Some(error)) => error.validate(),
            _ => Err(ProtocolError::InvalidEnvelope),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamKind {
    Item,
    End,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamFrame {
    version: u8,
    request_id: RequestId,
    sequence: u64,
    kind: StreamKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ControlError>,
}

impl StreamFrame {
    pub fn item(request_id: RequestId, sequence: u64, payload: Value) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            request_id,
            sequence,
            kind: StreamKind::Item,
            payload: Some(payload),
            error: None,
        }
    }

    pub fn end(request_id: RequestId, sequence: u64) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            request_id,
            sequence,
            kind: StreamKind::End,
            payload: None,
            error: None,
        }
    }

    pub fn error(request_id: RequestId, sequence: u64, error: ControlError) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            request_id,
            sequence,
            kind: StreamKind::Error,
            payload: None,
            error: Some(error),
        }
    }

    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    pub const fn kind(&self) -> StreamKind {
        self.kind
    }

    fn validate(&self) -> Result<(), ProtocolError> {
        if self.version != PROTOCOL_VERSION || self.sequence == 0 {
            return Err(ProtocolError::InvalidEnvelope);
        }
        match (self.kind, self.payload.is_some(), self.error.as_ref()) {
            (StreamKind::Item, true, None) | (StreamKind::End, false, None) => Ok(()),
            (StreamKind::Error, false, Some(error)) => error.validate(),
            _ => Err(ProtocolError::InvalidEnvelope),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WireFrame {
    Request(ControlRequest),
    Response(ControlResponse),
    Stream(StreamFrame),
}

impl WireFrame {
    fn validate(&self) -> Result<(), ProtocolError> {
        match self {
            Self::Request(frame) => frame.validate(),
            Self::Response(frame) => frame.validate(),
            Self::Stream(frame) => frame.validate(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ProtocolError {
    #[error("frame payload exceeds one MiB")]
    FrameTooLarge,
    #[error("frame length prefix or payload is malformed")]
    InvalidFrameLength,
    #[error("frame payload is not valid UTF-8")]
    InvalidUtf8,
    #[error("frame payload is not a valid protocol envelope")]
    InvalidEnvelope,
    #[error("protocol version is unsupported")]
    UnsupportedVersion,
    #[error("request ID is invalid")]
    InvalidRequestId,
    #[error("stable error code is invalid")]
    InvalidErrorCode,
    #[error("control message is invalid or too long")]
    InvalidMessage,
    #[error("frame I/O failed")]
    Io,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct FrameCodec;

impl FrameCodec {
    pub fn encode(frame: &WireFrame) -> Result<Vec<u8>, ProtocolError> {
        frame.validate()?;
        let payload = serde_json::to_vec(frame).map_err(|_| ProtocolError::InvalidEnvelope)?;
        if payload.is_empty() || payload.len() > MAX_FRAME_BYTES {
            return Err(ProtocolError::FrameTooLarge);
        }
        let length = u32::try_from(payload.len()).map_err(|_| ProtocolError::FrameTooLarge)?;
        let mut encoded = Vec::with_capacity(4 + payload.len());
        encoded.extend_from_slice(&length.to_be_bytes());
        encoded.extend_from_slice(&payload);
        Ok(encoded)
    }

    pub fn decode(encoded: &[u8]) -> Result<WireFrame, ProtocolError> {
        if encoded.len() < 4 {
            return Err(ProtocolError::InvalidFrameLength);
        }
        let length = u32::from_be_bytes(
            encoded[..4]
                .try_into()
                .map_err(|_| ProtocolError::InvalidFrameLength)?,
        ) as usize;
        if length == 0 || length > MAX_FRAME_BYTES {
            return Err(ProtocolError::FrameTooLarge);
        }
        if encoded.len() != 4 + length {
            return Err(ProtocolError::InvalidFrameLength);
        }
        Self::decode_payload(&encoded[4..])
    }

    pub fn read_from(reader: &mut impl Read) -> Result<WireFrame, ProtocolError> {
        let mut prefix = [0u8; 4];
        reader
            .read_exact(&mut prefix)
            .map_err(|_| ProtocolError::Io)?;
        let length = u32::from_be_bytes(prefix) as usize;
        if length == 0 || length > MAX_FRAME_BYTES {
            return Err(ProtocolError::FrameTooLarge);
        }
        let mut payload = vec![0; length];
        reader
            .read_exact(&mut payload)
            .map_err(|_| ProtocolError::Io)?;
        Self::decode_payload(&payload)
    }

    pub fn write_to(writer: &mut impl Write, frame: &WireFrame) -> Result<(), ProtocolError> {
        let encoded = Self::encode(frame)?;
        writer.write_all(&encoded).map_err(|_| ProtocolError::Io)
    }

    fn decode_payload(payload: &[u8]) -> Result<WireFrame, ProtocolError> {
        std::str::from_utf8(payload).map_err(|_| ProtocolError::InvalidUtf8)?;
        let frame: WireFrame =
            serde_json::from_slice(payload).map_err(|_| ProtocolError::InvalidEnvelope)?;
        frame.validate()?;
        Ok(frame)
    }
}

fn validate_message(message: &str) -> Result<(), ProtocolError> {
    if message.is_empty()
        || message.len() > MAX_MESSAGE_BYTES
        || message
            .chars()
            .any(|character| character.is_control() && character != '\n' && character != '\t')
    {
        return Err(ProtocolError::InvalidMessage);
    }
    Ok(())
}
