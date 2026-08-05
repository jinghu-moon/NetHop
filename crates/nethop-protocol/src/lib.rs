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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ControlMethod {
    #[serde(rename = "protocol.hello")]
    ProtocolHello,
    #[serde(rename = "status.get")]
    StatusGet,
    #[serde(rename = "service.start")]
    ServiceStart,
    #[serde(rename = "service.stop")]
    ServiceStop,
    #[serde(rename = "capability.probe")]
    CapabilityProbe,
    #[serde(rename = "subscription.update")]
    SubscriptionUpdate,
    #[serde(rename = "config.reload")]
    ConfigReload,
    #[serde(rename = "config.get")]
    ConfigGet,
    #[serde(rename = "config.validate")]
    ConfigValidate,
    #[serde(rename = "config.apply")]
    ConfigApply,
    #[serde(rename = "config.schema")]
    ConfigSchema,
    #[serde(rename = "capability.get")]
    CapabilityGet,
    #[serde(rename = "config.mutate")]
    ConfigMutate,
    #[serde(rename = "events.subscribe")]
    EventsSubscribe,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConfigMutation {
    SetServiceEnabled {
        enabled: bool,
    },
    AddSource {
        name: String,
        url: String,
    },
    UpdateSource {
        source_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        enabled: Option<bool>,
    },
    RemoveSource {
        source_id: String,
    },
    MoveSource {
        source_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before_source_id: Option<String>,
    },
    AddPackage {
        package: String,
    },
    RemovePackage {
        package: String,
    },
    ReplacePackages {
        packages: Vec<String>,
    },
    AddRoutingCidr {
        list: RoutingCidrList,
        cidr: String,
    },
    RemoveRoutingCidr {
        list: RoutingCidrList,
        cidr: String,
    },
    SetScalarField {
        field_id: String,
        value: Value,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingCidrList {
    ForceProxy,
    Bypass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Config,
    Runtime,
    Subscription,
    Generation,
    Network,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlParams {
    #[serde(default, skip_serializing_if = "is_false")]
    wait: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    if_needed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expected_config_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    document: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    manager_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    manager_protocol_min: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    manager_protocol_max: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mutation: Option<ConfigMutation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    event_kinds: Option<Vec<EventKind>>,
}

impl ControlParams {
    pub const fn new(wait: bool, if_needed: bool) -> Self {
        Self {
            wait,
            if_needed,
            expected_config_digest: None,
            document: None,
            manager_version: None,
            manager_protocol_min: None,
            manager_protocol_max: None,
            mutation: None,
            event_kinds: None,
        }
    }

    pub const fn wait(&self) -> bool {
        self.wait
    }

    pub const fn if_needed(&self) -> bool {
        self.if_needed
    }

    pub fn config_document(expected_config_digest: String, document: Value) -> Self {
        Self {
            expected_config_digest: Some(expected_config_digest),
            document: Some(document),
            ..Self::default()
        }
    }

    pub fn hello(
        manager_version: String,
        manager_protocol_min: u8,
        manager_protocol_max: u8,
    ) -> Self {
        Self {
            manager_version: Some(manager_version),
            manager_protocol_min: Some(manager_protocol_min),
            manager_protocol_max: Some(manager_protocol_max),
            ..Self::default()
        }
    }

    pub fn mutation(expected_config_digest: String, mutation: ConfigMutation) -> Self {
        Self {
            expected_config_digest: Some(expected_config_digest),
            mutation: Some(mutation),
            ..Self::default()
        }
    }

    pub fn event_subscription(event_kinds: Vec<EventKind>) -> Self {
        Self {
            event_kinds: Some(event_kinds),
            ..Self::default()
        }
    }

    pub fn expected_config_digest(&self) -> Option<&str> {
        self.expected_config_digest.as_deref()
    }

    pub const fn document(&self) -> Option<&Value> {
        self.document.as_ref()
    }

    pub fn manager_version(&self) -> Option<&str> {
        self.manager_version.as_deref()
    }

    pub const fn manager_protocol_range(&self) -> Option<(u8, u8)> {
        match (self.manager_protocol_min, self.manager_protocol_max) {
            (Some(min), Some(max)) => Some((min, max)),
            _ => None,
        }
    }

    pub const fn mutation_value(&self) -> Option<&ConfigMutation> {
        self.mutation.as_ref()
    }

    pub fn event_kinds(&self) -> Option<&[EventKind]> {
        self.event_kinds.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlRequest {
    version: u8,
    request_id: RequestId,
    method: ControlMethod,
    params: ControlParams,
}

impl ControlRequest {
    pub fn new(request_id: RequestId, method: ControlMethod) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            request_id,
            method,
            params: ControlParams::default(),
        }
    }

    pub fn with_params(mut self, params: ControlParams) -> Result<Self, ProtocolError> {
        self.params = params;
        self.validate()?;
        Ok(self)
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

    pub const fn params(&self) -> &ControlParams {
        &self.params
    }

    fn validate(&self) -> Result<(), ProtocolError> {
        if self.version != PROTOCOL_VERSION {
            return Err(ProtocolError::UnsupportedVersion);
        }
        let wait_allowed = matches!(
            self.method,
            ControlMethod::ServiceStart
                | ControlMethod::ServiceStop
                | ControlMethod::SubscriptionUpdate
                | ControlMethod::ConfigReload
        );
        if (self.params.wait && !wait_allowed)
            || (self.params.if_needed && self.method != ControlMethod::SubscriptionUpdate)
        {
            return Err(ProtocolError::InvalidEnvelope);
        }
        let document_method = matches!(
            self.method,
            ControlMethod::ConfigValidate | ControlMethod::ConfigApply
        );
        let mutation_method = self.method == ControlMethod::ConfigMutate;
        if self.params.document.is_some() != document_method
            || self.params.mutation.is_some() != mutation_method
            || self.params.expected_config_digest.is_some() != (document_method || mutation_method)
        {
            return Err(ProtocolError::InvalidEnvelope);
        }
        if let Some(digest) = &self.params.expected_config_digest
            && (digest.len() != 64
                || !digest
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
        {
            return Err(ProtocolError::InvalidEnvelope);
        }
        if self
            .params
            .document
            .as_ref()
            .is_some_and(|document| !document.is_object())
        {
            return Err(ProtocolError::InvalidEnvelope);
        }
        if let Some(mutation) = &self.params.mutation {
            validate_mutation(mutation)?;
        }
        let events_method = self.method == ControlMethod::EventsSubscribe;
        if self.params.event_kinds.is_some() != events_method
            || self.params.event_kinds.as_ref().is_some_and(|kinds| {
                kinds.len() > 5 || {
                    let mut unique = kinds.clone();
                    unique.sort_by_key(|kind| *kind as u8);
                    unique.dedup();
                    unique.len() != kinds.len()
                }
            })
        {
            return Err(ProtocolError::InvalidEnvelope);
        }
        let hello = self.method == ControlMethod::ProtocolHello;
        if self.params.manager_version.is_some() != hello
            || self.params.manager_protocol_min.is_some() != hello
            || self.params.manager_protocol_max.is_some() != hello
        {
            return Err(ProtocolError::InvalidEnvelope);
        }
        if hello {
            let version = self.params.manager_version.as_deref().unwrap_or_default();
            let min = self.params.manager_protocol_min.unwrap_or_default();
            let max = self.params.manager_protocol_max.unwrap_or_default();
            if version.is_empty()
                || version.len() > 64
                || version.chars().any(char::is_control)
                || min == 0
                || min > max
            {
                return Err(ProtocolError::InvalidEnvelope);
            }
        }
        Ok(())
    }
}

fn validate_mutation(mutation: &ConfigMutation) -> Result<(), ProtocolError> {
    let bounded = |value: &str, max: usize| {
        !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
    };
    let source_id = |value: &str| {
        value.len() == 36
            && value.starts_with("src_")
            && value[4..]
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    };
    let valid = match mutation {
        ConfigMutation::SetServiceEnabled { .. } => true,
        ConfigMutation::AddSource { name, url } => bounded(name, 128) && url.len() <= 16 * 1024,
        ConfigMutation::UpdateSource {
            source_id: id,
            name,
            url,
            enabled,
        } => {
            source_id(id)
                && (name.is_some() || url.is_some() || enabled.is_some())
                && name.as_ref().is_none_or(|value| bounded(value, 128))
                && url.as_ref().is_none_or(|value| value.len() <= 16 * 1024)
        }
        ConfigMutation::RemoveSource { source_id: id } => source_id(id),
        ConfigMutation::MoveSource {
            source_id: id,
            before_source_id,
        } => {
            source_id(id)
                && before_source_id
                    .as_ref()
                    .is_none_or(|value| source_id(value) && value != id)
        }
        ConfigMutation::AddPackage { package } | ConfigMutation::RemovePackage { package } => {
            bounded(package, 255)
        }
        ConfigMutation::ReplacePackages { packages } => {
            packages.len() <= 2_000 && packages.iter().all(|value| bounded(value, 255))
        }
        ConfigMutation::AddRoutingCidr { cidr, .. }
        | ConfigMutation::RemoveRoutingCidr { cidr, .. } => bounded(cidr, 64),
        ConfigMutation::SetScalarField { field_id, value } => {
            bounded(field_id, 128)
                && matches!(value, Value::Bool(_) | Value::Number(_) | Value::String(_))
        }
    };
    valid.then_some(()).ok_or(ProtocolError::InvalidEnvelope)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorDomain {
    Config,
    Source,
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
            Self::Source => "SOURCE",
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
            "SOURCE" => ErrorDomain::Source,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<Value>,
}

impl ControlError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Result<Self, ProtocolError> {
        let message = message.into();
        validate_message(&message)?;
        Ok(Self {
            code,
            message,
            details: None,
        })
    }

    pub fn with_details(
        code: ErrorCode,
        message: impl Into<String>,
        details: Value,
    ) -> Result<Self, ProtocolError> {
        let message = message.into();
        validate_message(&message)?;
        Ok(Self {
            code,
            message,
            details: Some(details),
        })
    }

    pub fn code(&self) -> &ErrorCode {
        &self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub const fn details(&self) -> Option<&Value> {
        self.details.as_ref()
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

    pub const fn payload(&self) -> Option<&Value> {
        self.payload.as_ref()
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

const fn is_false(value: &bool) -> bool {
    !*value
}
