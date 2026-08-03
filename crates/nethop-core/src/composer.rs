use std::{collections::BTreeMap, fmt};

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{CaptureMode, CapturePolicy};

const MAX_TAG_BYTES: usize = 128;
const MAX_PROTOCOL_BYTES: usize = 32;
const MAX_FIELD_COUNT: usize = 128;
const MAX_FIELD_NAME_BYTES: usize = 64;
const MAX_CONFIG_BYTES: usize = 5 * 1024 * 1024;
const MAX_MANAGED_NODES: usize = 2_000;
const MAX_AUTO_NODES: usize = 64;
const MIN_API_SECRET_BYTES: usize = 32;
const MAX_API_SECRET_BYTES: usize = 128;

const DIRECT_TAG: &str = "direct";
const BLOCK_TAG: &str = "block";
const AUTO_TAG: &str = "nethop-auto";
const SELECT_TAG: &str = "nethop-select";
const INBOUND_TAG: &str = "nethop-in";
const RESERVED_TAGS: &[&str] = &[DIRECT_TAG, BLOCK_TAG, AUTO_TAG, SELECT_TAG, INBOUND_TAG];

const RESERVED_FIELDS: &[&str] = &[
    "inbounds",
    "outbounds",
    "route",
    "dns",
    "experimental",
    "services",
    "log",
    "endpoints",
];

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ComposerError {
    #[error("outbound tag is empty or too long")]
    InvalidTag,
    #[error("outbound protocol is empty or too long")]
    InvalidProtocol,
    #[error("outbound contains reserved field: {0}")]
    ReservedField(String),
    #[error("outbound has too many fields")]
    TooManyFields,
    #[error("outbound field name is too long")]
    FieldNameTooLong,
    #[error("outbound tags must be unique")]
    DuplicateTag,
    #[error("at least one terminal outbound is required")]
    EmptyOutbounds,
    #[error("managed config exceeds the size limit")]
    ConfigTooLarge,
    #[error("managed config serialization failed: {0}")]
    Serialization(String),
    #[error("managed profile exceeds the active outbound limit")]
    TooManyOutbounds,
    #[error("terminal outbound tag is reserved by NetHop")]
    ReservedTag,
    #[error("Clash API must use an IPv4 loopback endpoint with a non-zero port")]
    InvalidApiEndpoint,
    #[error("Clash API secret does not meet the bounded policy")]
    InvalidApiSecret,
}

#[derive(Clone, PartialEq)]
pub struct TerminalOutbound {
    tag: String,
    protocol: String,
    fields: BTreeMap<String, Value>,
}

impl fmt::Debug for TerminalOutbound {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TerminalOutbound")
            .field("tag", &self.tag)
            .field("protocol", &self.protocol)
            .field("fields", &"[REDACTED]")
            .finish()
    }
}

impl TerminalOutbound {
    pub fn new(
        tag: impl Into<String>,
        protocol: impl Into<String>,
        fields: BTreeMap<String, Value>,
    ) -> Result<Self, ComposerError> {
        let tag = tag.into();
        let protocol = protocol.into();
        if tag.is_empty() || tag.len() > MAX_TAG_BYTES {
            return Err(ComposerError::InvalidTag);
        }
        if protocol.is_empty() || protocol.len() > MAX_PROTOCOL_BYTES {
            return Err(ComposerError::InvalidProtocol);
        }
        if fields.len() > MAX_FIELD_COUNT {
            return Err(ComposerError::TooManyFields);
        }
        for key in fields.keys() {
            if key.is_empty() || key.len() > MAX_FIELD_NAME_BYTES {
                return Err(ComposerError::FieldNameTooLong);
            }
            if RESERVED_FIELDS.contains(&key.as_str()) {
                return Err(ComposerError::ReservedField(key.clone()));
            }
        }
        Ok(Self {
            tag,
            protocol,
            fields,
        })
    }

    pub fn tag(&self) -> &str {
        &self.tag
    }

    pub fn protocol(&self) -> &str {
        &self.protocol
    }

    fn to_json(&self) -> Map<String, Value> {
        let mut object: Map<String, Value> = self
            .fields
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        object.insert("tag".into(), Value::String(self.tag.clone()));
        object.insert("type".into(), Value::String(self.protocol.clone()));
        object
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunStack {
    System,
    Gvisor,
}

impl TunStack {
    const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Gvisor => "gvisor",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ClashApi {
    endpoint: String,
    secret: String,
}

impl ClashApi {
    pub fn new(
        endpoint: impl Into<String>,
        secret: impl Into<String>,
    ) -> Result<Self, ComposerError> {
        let endpoint = endpoint.into();
        let port = endpoint
            .strip_prefix("127.0.0.1:")
            .and_then(|value| value.parse::<u16>().ok())
            .filter(|port| *port != 0)
            .ok_or(ComposerError::InvalidApiEndpoint)?;
        debug_assert_ne!(port, 0);
        let secret = secret.into();
        if !(MIN_API_SECRET_BYTES..=MAX_API_SECRET_BYTES).contains(&secret.len())
            || secret.chars().any(char::is_control)
        {
            return Err(ComposerError::InvalidApiSecret);
        }
        Ok(Self { endpoint, secret })
    }
}

impl fmt::Debug for ClashApi {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClashApi")
            .field("endpoint", &self.endpoint)
            .field("secret", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, PartialEq)]
pub struct ManagedProfile {
    capture: CapturePolicy,
    outbounds: Vec<TerminalOutbound>,
    clash_api: ClashApi,
    tun_stack: TunStack,
}

impl ManagedProfile {
    pub fn new(
        capture: CapturePolicy,
        outbounds: Vec<TerminalOutbound>,
        clash_api: ClashApi,
    ) -> Result<Self, ComposerError> {
        if outbounds.len() > MAX_MANAGED_NODES {
            return Err(ComposerError::TooManyOutbounds);
        }
        if outbounds
            .iter()
            .any(|outbound| RESERVED_TAGS.contains(&outbound.tag()))
        {
            return Err(ComposerError::ReservedTag);
        }
        Ok(Self {
            capture,
            outbounds,
            clash_api,
            tun_stack: TunStack::System,
        })
    }

    pub fn with_tun_stack(mut self, tun_stack: TunStack) -> Self {
        self.tun_stack = tun_stack;
        self
    }
}

impl fmt::Debug for ManagedProfile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ManagedProfile")
            .field("capture", &self.capture)
            .field("node_count", &self.outbounds.len())
            .field("clash_api_endpoint", &self.clash_api.endpoint)
            .field("tun_stack", &self.tun_stack)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ManagedConfig {
    bytes: Vec<u8>,
    digest: String,
    node_count: usize,
}

impl ManagedConfig {
    pub fn from_outbounds(mut outbounds: Vec<TerminalOutbound>) -> Result<Self, ComposerError> {
        normalize_outbounds(&mut outbounds)?;
        let value = serde_json::json!({
            "outbounds": outbounds.iter().map(TerminalOutbound::to_json).collect::<Vec<_>>()
        });
        Self::from_value(value, outbounds.len())
    }

    pub fn from_profile(mut profile: ManagedProfile) -> Result<Self, ComposerError> {
        normalize_outbounds(&mut profile.outbounds)?;
        let node_tags = profile
            .outbounds
            .iter()
            .map(|outbound| outbound.tag.clone())
            .collect::<Vec<_>>();
        let auto_tags = node_tags
            .iter()
            .take(MAX_AUTO_NODES)
            .cloned()
            .collect::<Vec<_>>();
        let mut selector_tags = Vec::with_capacity(node_tags.len() + 1);
        selector_tags.push(AUTO_TAG.to_owned());
        selector_tags.extend(node_tags.iter().cloned());

        let mut outbounds = Vec::with_capacity(profile.outbounds.len() + 4);
        outbounds.push(serde_json::json!({ "type": "direct", "tag": DIRECT_TAG }));
        outbounds.push(serde_json::json!({ "type": "block", "tag": BLOCK_TAG }));
        outbounds.push(serde_json::json!({
            "type": "urltest",
            "tag": AUTO_TAG,
            "outbounds": auto_tags,
            "url": "https://www.gstatic.com/generate_204",
            "interval": "10m",
            "tolerance": 50,
            "idle_timeout": "30m",
            "interrupt_exist_connections": false
        }));
        outbounds.push(serde_json::json!({
            "type": "selector",
            "tag": SELECT_TAG,
            "outbounds": selector_tags,
            "default": AUTO_TAG,
            "interrupt_exist_connections": false
        }));
        outbounds.extend(
            profile
                .outbounds
                .iter()
                .map(TerminalOutbound::to_json)
                .map(Value::Object),
        );

        let value = serde_json::json!({
            "log": {
                "level": "warn",
                "timestamp": true
            },
            "dns": {
                "servers": [{ "type": "local", "tag": "dns-local" }],
                "final": "dns-local",
                "strategy": "prefer_ipv4",
                "disable_cache": false,
                "cache_capacity": 4096
            },
            "inbounds": compose_inbounds(&profile),
            "outbounds": outbounds,
            "route": {
                "auto_detect_interface": true,
                "rules": [
                    { "inbound": [INBOUND_TAG], "action": "sniff" },
                    { "ip_is_private": true, "outbound": DIRECT_TAG }
                ],
                "final": SELECT_TAG
            },
            "experimental": {
                "clash_api": {
                    "external_controller": profile.clash_api.endpoint,
                    "secret": profile.clash_api.secret
                }
            }
        });
        Self::from_value(value, profile.outbounds.len())
    }

    fn from_value(value: Value, node_count: usize) -> Result<Self, ComposerError> {
        let bytes = serde_json::to_vec(&value)
            .map_err(|error| ComposerError::Serialization(error.to_string()))?;
        if bytes.len() > MAX_CONFIG_BYTES {
            return Err(ComposerError::ConfigTooLarge);
        }
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let digest = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Ok(Self {
            bytes,
            digest,
            node_count,
        })
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn digest_sha256(&self) -> &str {
        &self.digest
    }

    pub const fn node_count(&self) -> usize {
        self.node_count
    }
}

impl fmt::Debug for ManagedConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ManagedConfig")
            .field("bytes", &"[REDACTED]")
            .field("digest", &self.digest)
            .field("node_count", &self.node_count)
            .finish()
    }
}

fn normalize_outbounds(outbounds: &mut [TerminalOutbound]) -> Result<(), ComposerError> {
    if outbounds.is_empty() {
        return Err(ComposerError::EmptyOutbounds);
    }
    outbounds.sort_by(|left, right| left.tag.cmp(&right.tag));
    if outbounds
        .windows(2)
        .any(|window| window[0].tag == window[1].tag)
    {
        return Err(ComposerError::DuplicateTag);
    }
    Ok(())
}

fn compose_inbounds(profile: &ManagedProfile) -> Vec<Value> {
    match profile.capture.mode() {
        CaptureMode::Tproxy => vec![serde_json::json!({
            "type": "tproxy",
            "tag": INBOUND_TAG,
            "listen": "::",
            "listen_port": profile.capture.inbound_port()
        })],
        CaptureMode::Tun => {
            let mut inbound = Map::from_iter([
                ("type".to_owned(), Value::String("tun".to_owned())),
                ("tag".to_owned(), Value::String(INBOUND_TAG.to_owned())),
                (
                    "interface_name".to_owned(),
                    Value::String("nethop0".to_owned()),
                ),
                (
                    "address".to_owned(),
                    serde_json::json!(["172.19.0.1/30", "fdfe:dcba:9876::1/126"]),
                ),
                (
                    "stack".to_owned(),
                    Value::String(profile.tun_stack.as_str().to_owned()),
                ),
                ("auto_route".to_owned(), Value::Bool(true)),
                ("strict_route".to_owned(), Value::Bool(true)),
            ]);
            if !profile.capture.include_uids().is_empty() {
                inbound.insert(
                    "include_uid".to_owned(),
                    serde_json::json!(profile.capture.include_uids()),
                );
            }
            if !profile.capture.exclude_uids().is_empty() {
                inbound.insert(
                    "exclude_uid".to_owned(),
                    serde_json::json!(profile.capture.exclude_uids()),
                );
            }
            vec![Value::Object(inbound)]
        }
        CaptureMode::Direct => Vec::new(),
    }
}
