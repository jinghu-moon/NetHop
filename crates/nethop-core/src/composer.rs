use std::{
    collections::{BTreeMap, HashSet},
    fmt,
};

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
const MAX_ROUTING_CIDRS: usize = 512;
const MAX_CIDR_BYTES: usize = 64;
const MAX_ROUTING_DOMAINS: usize = 512;
const MAX_DOMAIN_BYTES: usize = 253;
const MIN_API_SECRET_BYTES: usize = 32;
const MAX_API_SECRET_BYTES: usize = 128;

const DIRECT_TAG: &str = "direct";
const BLOCK_TAG: &str = "block";
const SELECT_TAG: &str = "nethop-select";
const INBOUND_TAG: &str = "nethop-in";
const FETCH_INBOUND_TAG: &str = "nethop-fetch";
pub const MANAGED_FETCH_PROXY_ENDPOINT: &str = "127.0.0.1:7894";
pub const MANAGED_FETCH_PROXY_USERNAME: &str = "nethop";
const CN_DOMAIN_RULE_SET_TAG: &str = "nethop-cn-domain";
const CN_IP_RULE_SET_TAG: &str = "nethop-cn-ip";
const CN_DOMAIN_RULE_SET_PATH: &str = "/data/adb/nethop/rulesets/cn-domain.srs";
const CN_IP_RULE_SET_PATH: &str = "/data/adb/nethop/rulesets/cn-ip.srs";
const RESERVED_TAGS: &[&str] = &[
    DIRECT_TAG,
    BLOCK_TAG,
    SELECT_TAG,
    INBOUND_TAG,
    FETCH_INBOUND_TAG,
];

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
    #[error("managed options are outside the bounded policy")]
    InvalidManagedOptions,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedOutboundMode {
    Rule,
    Global,
    Direct,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedLogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl ManagedLogLevel {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedOptions {
    outbound_mode: ManagedOutboundMode,
    urltest_interval_minutes: u16,
    urltest_tolerance_ms: u16,
    urltest_max_candidates: usize,
    log_level: ManagedLogLevel,
    bypass_private: bool,
    bypass_cn: bool,
    force_proxy_cidrs: Vec<String>,
    bypass_cidrs: Vec<String>,
    force_proxy_domains: Vec<String>,
    bypass_domains: Vec<String>,
    block_domains: Vec<String>,
}

impl Default for ManagedOptions {
    fn default() -> Self {
        Self {
            outbound_mode: ManagedOutboundMode::Rule,
            urltest_interval_minutes: 10,
            urltest_tolerance_ms: 50,
            urltest_max_candidates: MAX_AUTO_NODES,
            log_level: ManagedLogLevel::Warn,
            bypass_private: true,
            bypass_cn: true,
            force_proxy_cidrs: Vec::new(),
            bypass_cidrs: Vec::new(),
            force_proxy_domains: Vec::new(),
            bypass_domains: Vec::new(),
            block_domains: Vec::new(),
        }
    }
}

impl ManagedOptions {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        outbound_mode: ManagedOutboundMode,
        urltest_interval_minutes: u16,
        urltest_tolerance_ms: u16,
        urltest_max_candidates: usize,
        log_level: ManagedLogLevel,
        bypass_private: bool,
        bypass_cn: bool,
        force_proxy_cidrs: Vec<String>,
        bypass_cidrs: Vec<String>,
    ) -> Result<Self, ComposerError> {
        if !(5..=1440).contains(&urltest_interval_minutes)
            || urltest_tolerance_ms > 1000
            || !(1..=256).contains(&urltest_max_candidates)
            || force_proxy_cidrs.len() > MAX_ROUTING_CIDRS
            || bypass_cidrs.len() > MAX_ROUTING_CIDRS
            || force_proxy_cidrs
                .iter()
                .chain(&bypass_cidrs)
                .any(|cidr| cidr.is_empty() || cidr.len() > MAX_CIDR_BYTES || !cidr.contains('/'))
        {
            return Err(ComposerError::InvalidManagedOptions);
        }
        Ok(Self {
            outbound_mode,
            urltest_interval_minutes,
            urltest_tolerance_ms,
            urltest_max_candidates,
            log_level,
            bypass_private,
            bypass_cn,
            force_proxy_cidrs,
            bypass_cidrs,
            force_proxy_domains: Vec::new(),
            bypass_domains: Vec::new(),
            block_domains: Vec::new(),
        })
    }

    pub fn with_domain_rules(
        mut self,
        force_proxy_domains: Vec<String>,
        bypass_domains: Vec<String>,
        block_domains: Vec<String>,
    ) -> Result<Self, ComposerError> {
        if force_proxy_domains.len() > MAX_ROUTING_DOMAINS
            || bypass_domains.len() > MAX_ROUTING_DOMAINS
            || block_domains.len() > MAX_ROUTING_DOMAINS
            || force_proxy_domains
                .iter()
                .chain(&bypass_domains)
                .chain(&block_domains)
                .any(|domain| !valid_domain_suffix(domain))
        {
            return Err(ComposerError::InvalidManagedOptions);
        }
        self.force_proxy_domains = force_proxy_domains;
        self.bypass_domains = bypass_domains;
        self.block_domains = block_domains;
        Ok(self)
    }

    pub const fn urltest_max_candidates(&self) -> usize {
        self.urltest_max_candidates
    }
}

fn valid_domain_suffix(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_DOMAIN_BYTES
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        })
        && value.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .bytes()
                    .next()
                    .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
                && label
                    .bytes()
                    .last()
                    .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
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
    auto_pool: Vec<String>,
    clash_api: ClashApi,
    tun_stack: TunStack,
    options: ManagedOptions,
}

impl ManagedProfile {
    pub fn new(
        capture: CapturePolicy,
        outbounds: Vec<TerminalOutbound>,
        auto_pool: Vec<String>,
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
        let profile = Self {
            capture,
            outbounds,
            auto_pool: Vec::new(),
            clash_api,
            tun_stack: TunStack::Gvisor,
            options: ManagedOptions::default(),
        };
        profile.with_auto_pool(auto_pool)
    }

    pub fn with_tun_stack(mut self, tun_stack: TunStack) -> Self {
        self.tun_stack = tun_stack;
        self
    }

    pub fn with_auto_pool(mut self, auto_pool: Vec<String>) -> Result<Self, ComposerError> {
        let unique = auto_pool.iter().collect::<HashSet<_>>();
        if auto_pool.len() > MAX_AUTO_NODES
            || auto_pool.is_empty() && !self.outbounds.is_empty()
            || auto_pool.iter().any(|tag| {
                tag.is_empty()
                    || tag.len() > MAX_TAG_BYTES
                    || tag.chars().any(char::is_control)
                    || !self.outbounds.iter().any(|outbound| outbound.tag() == tag)
            })
            || unique.len() != auto_pool.len()
        {
            return Err(ComposerError::InvalidManagedOptions);
        }
        self.auto_pool = auto_pool;
        Ok(self)
    }

    pub fn with_options(mut self, options: ManagedOptions) -> Self {
        self.options = options;
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
            .field("options", &self.options)
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
        if profile
            .auto_pool
            .iter()
            .any(|tag| !node_tags.iter().any(|node_tag| node_tag == tag))
        {
            return Err(ComposerError::InvalidManagedOptions);
        }
        let default_tag = profile
            .auto_pool
            .first()
            .ok_or(ComposerError::InvalidManagedOptions)?;

        let mut outbounds = Vec::with_capacity(profile.outbounds.len() + 3);
        outbounds.push(serde_json::json!({ "type": "direct", "tag": DIRECT_TAG }));
        outbounds.push(serde_json::json!({ "type": "block", "tag": BLOCK_TAG }));
        outbounds.push(serde_json::json!({
            "type": "selector",
            "tag": SELECT_TAG,
            "outbounds": node_tags,
            "default": default_tag,
            "interrupt_exist_connections": false
        }));
        outbounds.extend(
            profile
                .outbounds
                .iter()
                .map(TerminalOutbound::to_json)
                .map(Value::Object),
        );

        let rule_mode_with_cn =
            profile.options.outbound_mode == ManagedOutboundMode::Rule && profile.options.bypass_cn;
        let mut route_rules = vec![
            serde_json::json!({ "inbound": [INBOUND_TAG], "action": "sniff" }),
            serde_json::json!({
                "type": "logical",
                "mode": "or",
                "rules": [
                    { "protocol": "dns" },
                    { "port": 53 }
                ],
                "action": "hijack-dns"
            }),
            serde_json::json!({
                "inbound": [FETCH_INBOUND_TAG],
                "action": "resolve",
                "server": "dns-proxy",
                "strategy": "prefer_ipv4"
            }),
            serde_json::json!({
                "inbound": [FETCH_INBOUND_TAG],
                "ip_is_private": true,
                "outbound": BLOCK_TAG
            }),
            serde_json::json!({
                "inbound": [FETCH_INBOUND_TAG],
                "outbound": SELECT_TAG
            }),
        ];
        if !profile.options.block_domains.is_empty() {
            route_rules.push(serde_json::json!({
                "domain_suffix": profile.options.block_domains,
                "outbound": BLOCK_TAG
            }));
        }
        if !profile.options.force_proxy_domains.is_empty() {
            route_rules.push(serde_json::json!({
                "domain_suffix": profile.options.force_proxy_domains,
                "outbound": SELECT_TAG
            }));
        }
        if !profile.options.bypass_domains.is_empty() {
            route_rules.push(serde_json::json!({
                "domain_suffix": profile.options.bypass_domains,
                "outbound": DIRECT_TAG
            }));
        }
        if !profile.options.force_proxy_cidrs.is_empty() {
            route_rules.push(serde_json::json!({
                "ip_cidr": profile.options.force_proxy_cidrs,
                "outbound": SELECT_TAG
            }));
        }
        if !profile.options.bypass_cidrs.is_empty() {
            route_rules.push(serde_json::json!({
                "ip_cidr": profile.options.bypass_cidrs,
                "outbound": DIRECT_TAG
            }));
        }
        if profile.options.bypass_private {
            route_rules.push(serde_json::json!({ "ip_is_private": true, "outbound": DIRECT_TAG }));
        }
        if rule_mode_with_cn {
            route_rules.push(serde_json::json!({
                "rule_set": [CN_DOMAIN_RULE_SET_TAG],
                "outbound": DIRECT_TAG
            }));
            route_rules.push(serde_json::json!({
                "action": "resolve",
                "strategy": "prefer_ipv4",
                "server": "dns-proxy"
            }));
            route_rules.push(serde_json::json!({
                "rule_set": [CN_IP_RULE_SET_TAG],
                "outbound": DIRECT_TAG
            }));
        }
        let route_final = match profile.options.outbound_mode {
            ManagedOutboundMode::Rule | ManagedOutboundMode::Global => SELECT_TAG,
            ManagedOutboundMode::Direct => DIRECT_TAG,
        };
        let dns_final = match profile.options.outbound_mode {
            ManagedOutboundMode::Direct => "dns-direct",
            ManagedOutboundMode::Rule | ManagedOutboundMode::Global => "dns-proxy",
        };
        let mut dns_rules = Vec::new();
        if !profile.options.force_proxy_domains.is_empty() {
            dns_rules.push(serde_json::json!({
                "domain_suffix": profile.options.force_proxy_domains,
                "action": "route",
                "server": "dns-proxy"
            }));
        }
        if !profile.options.bypass_domains.is_empty() {
            dns_rules.push(serde_json::json!({
                "domain_suffix": profile.options.bypass_domains,
                "action": "route",
                "server": "dns-direct"
            }));
        }
        if rule_mode_with_cn {
            dns_rules.push(serde_json::json!({
                "rule_set": [CN_DOMAIN_RULE_SET_TAG],
                "action": "route",
                "server": "dns-direct"
            }));
        }
        let route_rule_sets = if rule_mode_with_cn {
            vec![
                serde_json::json!({
                    "type": "local",
                    "tag": CN_DOMAIN_RULE_SET_TAG,
                    "format": "binary",
                    "path": CN_DOMAIN_RULE_SET_PATH
                }),
                serde_json::json!({
                    "type": "local",
                    "tag": CN_IP_RULE_SET_TAG,
                    "format": "binary",
                    "path": CN_IP_RULE_SET_PATH
                }),
            ]
        } else {
            Vec::new()
        };
        let value = serde_json::json!({
            "log": {
                "level": profile.options.log_level.as_str(),
                "timestamp": true
            },
            "dns": {
                "servers": [
                    {
                        "type": "https",
                        "tag": "dns-direct",
                        "server": "223.5.5.5",
                        "server_port": 443,
                        "path": "/dns-query",
                        "headers": { "Host": "dns.alidns.com" },
                        "tls": { "server_name": "dns.alidns.com" }
                    },
                    {
                        "type": "https",
                        "tag": "dns-proxy",
                        "server": "1.1.1.1",
                        "server_port": 443,
                        "path": "/dns-query",
                        "headers": { "Host": "cloudflare-dns.com" },
                        "tls": { "server_name": "cloudflare-dns.com" },
                        "detour": SELECT_TAG
                    }
                ],
                "rules": dns_rules,
                "final": dns_final,
                "strategy": "prefer_ipv4",
                "disable_cache": false,
                "cache_capacity": 4096
            },
            "inbounds": compose_inbounds(&profile),
            "outbounds": outbounds,
            "route": {
                "auto_detect_interface": true,
                "default_domain_resolver": "dns-direct",
                "rule_set": route_rule_sets,
                "rules": route_rules,
                "final": route_final
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
    let capture = match profile.capture.mode() {
        CaptureMode::Tproxy => serde_json::json!({
            "type": "tproxy",
            "tag": INBOUND_TAG,
            "listen": "::",
            "listen_port": profile.capture.inbound_port()
        }),
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
            Value::Object(inbound)
        }
        CaptureMode::Direct => return Vec::new(),
    };
    let fetch_port = MANAGED_FETCH_PROXY_ENDPOINT
        .rsplit_once(':')
        .and_then(|(_, port)| port.parse::<u16>().ok())
        .expect("managed fetch proxy endpoint is static and valid");
    vec![
        capture,
        serde_json::json!({
            "type": "http",
            "tag": FETCH_INBOUND_TAG,
            "listen": "127.0.0.1",
            "listen_port": fetch_port,
            "users": [{
                "username": MANAGED_FETCH_PROXY_USERNAME,
                "password": profile.clash_api.secret
            }]
        }),
    ]
}
