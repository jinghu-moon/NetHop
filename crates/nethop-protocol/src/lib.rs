#![doc = "Bounded IPC v1 types and framing without socket ownership."]

use std::io::{Read, Write};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use serde_json::Value;
use thiserror::Error;

pub const PROTOCOL_VERSION: u8 = 3;
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
pub const MAX_WEBUI_STDOUT_BYTES: usize = MAX_FRAME_BYTES;
pub const MAX_WEBUI_STDERR_BYTES: usize = 64 * 1024;
pub const MAX_WEBUI_ARRAY_ITEMS: usize = 10_000;
pub const MAX_WEBUI_STRING_BYTES: usize = 64 * 1024;
pub const MAX_WEBUI_DIAGNOSTIC_BYTES: usize = 256 * 1024;
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
    #[serde(rename = "subscription.import_preview")]
    SubscriptionImportPreview,
    #[serde(rename = "subscription.import_apply")]
    SubscriptionImportApply,
    #[serde(rename = "subscription.mode_get")]
    SubscriptionModeGet,
    #[serde(rename = "subscription.mode_set")]
    SubscriptionModeSet,
    #[serde(rename = "subscription.select")]
    SubscriptionSelect,
    #[serde(rename = "subscription.set_enabled")]
    SubscriptionSetEnabled,
    #[serde(rename = "config.reload")]
    ConfigReload,
    #[serde(rename = "config.get")]
    ConfigGet,
    #[serde(rename = "config.export")]
    ConfigExport,
    #[serde(rename = "core.version_check")]
    CoreVersionCheck,
    #[serde(rename = "ruleset.status")]
    RuleSetStatus,
    #[serde(rename = "ruleset.update")]
    RuleSetUpdate,
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
    #[serde(rename = "node.list")]
    NodeList,
    #[serde(rename = "node.test")]
    NodeTest,
    #[serde(rename = "node.test_all")]
    NodeTestAll,
    #[serde(rename = "node.selection_get")]
    NodeSelectionGet,
    #[serde(rename = "node.select_auto")]
    NodeSelectAuto,
    #[serde(rename = "node.select_manual")]
    NodeSelectManual,
    #[serde(rename = "node.export")]
    NodeExport,
    #[serde(rename = "connections.get")]
    ConnectionsGet,
    #[serde(rename = "connection.close")]
    ConnectionClose,
    #[serde(rename = "connections.close_all")]
    ConnectionsCloseAll,
    #[serde(rename = "logs.get")]
    LogsGet,
    #[serde(rename = "logs.clear")]
    LogsClear,
    #[serde(rename = "diagnostics.bundle")]
    DiagnosticsBundle,
    #[serde(rename = "topology.get")]
    TopologyGet,
    #[serde(rename = "traffic.get")]
    TrafficGet,
    #[serde(rename = "metrics.get")]
    MetricsGet,
    #[serde(rename = "webui.payload.create")]
    WebUiPayloadCreate,
    #[serde(rename = "webui.payload.append")]
    WebUiPayloadAppend,
    #[serde(rename = "webui.payload.commit")]
    WebUiPayloadCommit,
    #[serde(rename = "webui.payload.remove")]
    WebUiPayloadRemove,
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
    AddApplicationTarget {
        target: ApplicationTarget,
    },
    RemoveApplicationTarget {
        target: ApplicationTarget,
    },
    ReplaceApplicationTargets {
        targets: Vec<ApplicationTarget>,
    },
    SetApplicationPolicy {
        mode: ApplicationPolicyMode,
        targets: Vec<ApplicationTarget>,
    },
    AddRoutingCidr {
        list: RoutingCidrList,
        cidr: String,
    },
    RemoveRoutingCidr {
        list: RoutingCidrList,
        cidr: String,
    },
    RemoveNode {
        node_id: String,
    },
    SetScalarField {
        field_id: String,
        value: Value,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ApplicationTarget {
    Package {
        android_user_id: u32,
        package: String,
    },
    Uid {
        uid: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplicationPolicyMode {
    All,
    Blacklist,
    Whitelist,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingCidrList {
    ForceProxy,
    Bypass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionMode {
    Single,
    Merge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Config,
    Runtime,
    Subscription,
    Generation,
    Network,
    Traffic,
    SubscriptionMode,
    SubscriptionActiveSet,
    NodeSelection,
    NodeActive,
    NodeTest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogChannel {
    Service,
    Subscription,
    Core,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebUiPayloadNamespace {
    Config,
    Subscription,
    Backup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebUiPayloadOperation {
    ConfigValidate,
    ConfigApply,
    ConfigMutate,
    SubscriptionImportPreview,
    SubscriptionImportApply,
    BackupRestore,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WebUiPayloadParams {
    namespace: WebUiPayloadNamespace,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    handle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    chunk: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    operation: Option<WebUiPayloadOperation>,
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
    candidate_digest: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    limit: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    log_channel: Option<LogChannel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    subscription_mode: Option<SubscriptionMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    payload: Option<Box<WebUiPayloadParams>>,
}

impl ControlParams {
    pub const fn new(wait: bool, if_needed: bool) -> Self {
        Self {
            wait,
            if_needed,
            expected_config_digest: None,
            candidate_digest: None,
            document: None,
            manager_version: None,
            manager_protocol_min: None,
            manager_protocol_max: None,
            mutation: None,
            event_kinds: None,
            target: None,
            query: None,
            limit: None,
            log_channel: None,
            source_id: None,
            subscription_mode: None,
            enabled: None,
            payload: None,
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

    pub fn import_document(
        expected_config_digest: String,
        candidate_digest: Option<String>,
        document: Value,
    ) -> Self {
        Self {
            expected_config_digest: Some(expected_config_digest),
            candidate_digest,
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

    pub fn target(target: String) -> Self {
        Self {
            target: Some(target),
            ..Self::default()
        }
    }

    pub fn list(query: Option<String>, limit: Option<u8>) -> Self {
        Self {
            query,
            limit,
            ..Self::default()
        }
    }

    pub fn logs(channel: Option<LogChannel>, limit: Option<u8>) -> Self {
        Self {
            limit,
            log_channel: channel,
            ..Self::default()
        }
    }

    pub fn subscription_update(wait: bool, if_needed: bool, source_id: Option<String>) -> Self {
        Self {
            wait,
            if_needed,
            source_id,
            ..Self::default()
        }
    }

    pub fn subscription_mode_set(
        expected_config_digest: String,
        mode: SubscriptionMode,
        target_source_id: Option<String>,
    ) -> Self {
        Self {
            expected_config_digest: Some(expected_config_digest),
            source_id: target_source_id,
            subscription_mode: Some(mode),
            ..Self::default()
        }
    }

    pub fn subscription_select(expected_config_digest: String, source_id: String) -> Self {
        Self {
            expected_config_digest: Some(expected_config_digest),
            source_id: Some(source_id),
            ..Self::default()
        }
    }

    pub fn subscription_set_enabled(
        expected_config_digest: String,
        source_id: String,
        enabled: bool,
    ) -> Self {
        Self {
            expected_config_digest: Some(expected_config_digest),
            source_id: Some(source_id),
            enabled: Some(enabled),
            ..Self::default()
        }
    }

    pub fn payload_create(namespace: WebUiPayloadNamespace) -> Self {
        Self {
            payload: Some(Box::new(WebUiPayloadParams {
                namespace,
                handle: None,
                chunk: None,
                operation: None,
            })),
            ..Self::default()
        }
    }

    pub fn payload_append(namespace: WebUiPayloadNamespace, handle: String, chunk: String) -> Self {
        Self {
            payload: Some(Box::new(WebUiPayloadParams {
                namespace,
                handle: Some(handle),
                chunk: Some(chunk),
                operation: None,
            })),
            ..Self::default()
        }
    }

    pub fn payload_commit(
        namespace: WebUiPayloadNamespace,
        handle: String,
        operation: WebUiPayloadOperation,
    ) -> Self {
        Self {
            payload: Some(Box::new(WebUiPayloadParams {
                namespace,
                handle: Some(handle),
                chunk: None,
                operation: Some(operation),
            })),
            ..Self::default()
        }
    }

    pub fn payload_remove(namespace: WebUiPayloadNamespace, handle: String) -> Self {
        Self {
            payload: Some(Box::new(WebUiPayloadParams {
                namespace,
                handle: Some(handle),
                chunk: None,
                operation: None,
            })),
            ..Self::default()
        }
    }

    pub fn expected_config_digest(&self) -> Option<&str> {
        self.expected_config_digest.as_deref()
    }

    pub fn candidate_digest(&self) -> Option<&str> {
        self.candidate_digest.as_deref()
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

    pub fn target_value(&self) -> Option<&str> {
        self.target.as_deref()
    }

    pub fn query_value(&self) -> Option<&str> {
        self.query.as_deref()
    }

    pub const fn limit(&self) -> Option<u8> {
        self.limit
    }

    pub const fn log_channel(&self) -> Option<LogChannel> {
        self.log_channel
    }

    pub fn source_id(&self) -> Option<&str> {
        self.source_id.as_deref()
    }

    pub const fn subscription_mode(&self) -> Option<SubscriptionMode> {
        self.subscription_mode
    }

    pub const fn enabled(&self) -> Option<bool> {
        self.enabled
    }

    pub fn payload_namespace(&self) -> Option<WebUiPayloadNamespace> {
        self.payload.as_deref().map(|payload| payload.namespace)
    }

    pub fn payload_handle(&self) -> Option<&str> {
        self.payload
            .as_deref()
            .and_then(|payload| payload.handle.as_deref())
    }

    pub fn payload_chunk(&self) -> Option<&str> {
        self.payload
            .as_deref()
            .and_then(|payload| payload.chunk.as_deref())
    }

    pub fn payload_operation(&self) -> Option<WebUiPayloadOperation> {
        self.payload
            .as_deref()
            .and_then(|payload| payload.operation)
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
                | ControlMethod::RuleSetUpdate
                | ControlMethod::ConfigReload
        );
        if (self.params.wait && !wait_allowed)
            || (self.params.if_needed && self.method != ControlMethod::SubscriptionUpdate)
        {
            return Err(ProtocolError::InvalidEnvelope);
        }
        if self
            .params
            .source_id
            .as_ref()
            .is_some_and(|source_id| !valid_source_id(source_id))
        {
            return Err(ProtocolError::InvalidEnvelope);
        }
        let import_method = matches!(
            self.method,
            ControlMethod::SubscriptionImportPreview | ControlMethod::SubscriptionImportApply
        );
        let document_method = matches!(
            self.method,
            ControlMethod::ConfigValidate | ControlMethod::ConfigApply
        ) || import_method;
        let candidate_method = self.method == ControlMethod::SubscriptionImportApply;
        let mutation_method = self.method == ControlMethod::ConfigMutate;
        let subscription_transaction = matches!(
            self.method,
            ControlMethod::SubscriptionModeSet
                | ControlMethod::SubscriptionSelect
                | ControlMethod::SubscriptionSetEnabled
        );
        if self.params.document.is_some() != document_method
            || self.params.mutation.is_some() != mutation_method
            || self.params.expected_config_digest.is_some()
                != (document_method || mutation_method || subscription_transaction)
            || self.params.candidate_digest.is_some() != candidate_method
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
        if let Some(digest) = &self.params.candidate_digest
            && (digest.len() != 64
                || !digest
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
        {
            return Err(ProtocolError::InvalidEnvelope);
        }
        if let Some(mutation) = &self.params.mutation {
            validate_mutation(mutation)?;
        }
        let mode_method = self.method == ControlMethod::SubscriptionModeSet;
        let source_presence_valid = match self.method {
            ControlMethod::SubscriptionUpdate => true,
            ControlMethod::SubscriptionSelect | ControlMethod::SubscriptionSetEnabled => {
                self.params.source_id.is_some()
            }
            ControlMethod::SubscriptionModeSet => match self.params.subscription_mode {
                Some(SubscriptionMode::Single) => self.params.source_id.is_some(),
                Some(SubscriptionMode::Merge) => self.params.source_id.is_none(),
                None => false,
            },
            _ => self.params.source_id.is_none(),
        };
        if self.params.subscription_mode.is_some() != mode_method
            || self.params.enabled.is_some()
                != (self.method == ControlMethod::SubscriptionSetEnabled)
            || !source_presence_valid
            || self.params.source_id.as_ref().is_some_and(|source_id| {
                source_id.len() != 36
                    || !source_id.starts_with("src_")
                    || !source_id[4..]
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
            })
        {
            return Err(ProtocolError::InvalidEnvelope);
        }
        let events_method = self.method == ControlMethod::EventsSubscribe;
        if self.params.event_kinds.is_some() != events_method
            || self.params.event_kinds.as_ref().is_some_and(|kinds| {
                kinds.len() > 11 || {
                    let mut unique = kinds.clone();
                    unique.sort_by_key(|kind| *kind as u8);
                    unique.dedup();
                    unique.len() != kinds.len()
                }
            })
        {
            return Err(ProtocolError::InvalidEnvelope);
        }
        let target_method = matches!(
            self.method,
            ControlMethod::NodeTest
                | ControlMethod::NodeSelectManual
                | ControlMethod::NodeExport
                | ControlMethod::ConnectionClose
        );
        if self.params.target.is_some() != target_method
            || self.params.target.as_ref().is_some_and(|target| {
                target.is_empty() || target.len() > 128 || target.chars().any(char::is_control)
            })
        {
            return Err(ProtocolError::InvalidEnvelope);
        }
        if self.method == ControlMethod::NodeSelectManual
            && self.params.target.as_ref().is_none_or(|target| {
                target.len() != 21
                    || !target.starts_with("nh1s-")
                    || !target[5..]
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
            })
        {
            return Err(ProtocolError::InvalidEnvelope);
        }
        let query_method = matches!(
            self.method,
            ControlMethod::NodeList | ControlMethod::ConnectionsGet
        );
        let limit_method = query_method || self.method == ControlMethod::LogsGet;
        if (!query_method && self.params.query.is_some())
            || (!limit_method && self.params.limit.is_some())
            || self
                .params
                .query
                .as_ref()
                .is_some_and(|query| query.len() > 128 || query.chars().any(char::is_control))
            || self
                .params
                .limit
                .is_some_and(|limit| !(1..=128).contains(&limit))
        {
            return Err(ProtocolError::InvalidEnvelope);
        }
        if self.method != ControlMethod::LogsGet && self.params.log_channel.is_some() {
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
        let payload_method = matches!(
            self.method,
            ControlMethod::WebUiPayloadCreate
                | ControlMethod::WebUiPayloadAppend
                | ControlMethod::WebUiPayloadCommit
                | ControlMethod::WebUiPayloadRemove
        );
        if self.params.payload.is_some() != payload_method {
            return Err(ProtocolError::InvalidEnvelope);
        }
        let valid_payload_shape = match self.method {
            ControlMethod::WebUiPayloadCreate => {
                self.params.payload_handle().is_none()
                    && self.params.payload_chunk().is_none()
                    && self.params.payload_operation().is_none()
            }
            ControlMethod::WebUiPayloadAppend => {
                self.params
                    .payload_handle()
                    .is_some_and(valid_payload_handle)
                    && self.params.payload_chunk().is_some_and(valid_payload_chunk)
                    && self.params.payload_operation().is_none()
            }
            ControlMethod::WebUiPayloadCommit => {
                self.params
                    .payload_handle()
                    .is_some_and(valid_payload_handle)
                    && self.params.payload_chunk().is_none()
                    && self.params.payload_operation().is_some()
            }
            ControlMethod::WebUiPayloadRemove => {
                self.params
                    .payload_handle()
                    .is_some_and(valid_payload_handle)
                    && self.params.payload_chunk().is_none()
                    && self.params.payload_operation().is_none()
            }
            _ => self.params.payload.is_none(),
        };
        if !valid_payload_shape {
            return Err(ProtocolError::InvalidEnvelope);
        }
        Ok(())
    }
}

fn valid_payload_handle(value: &str) -> bool {
    value.len() == 34
        && value.starts_with("p_")
        && value[2..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_payload_chunk(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 16 * 1024
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=' | b'-' | b'_')
        })
}

fn validate_mutation(mutation: &ConfigMutation) -> Result<(), ProtocolError> {
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
        ConfigMutation::AddApplicationTarget { target }
        | ConfigMutation::RemoveApplicationTarget { target } => application_target(target),
        ConfigMutation::ReplaceApplicationTargets { targets } => {
            targets.len() <= 2_000 && targets.iter().all(application_target)
        }
        ConfigMutation::SetApplicationPolicy { mode, targets } => {
            targets.len() <= 2_000
                && targets.iter().all(application_target)
                && (matches!(mode, ApplicationPolicyMode::All) == targets.is_empty())
        }
        ConfigMutation::AddRoutingCidr { cidr, .. }
        | ConfigMutation::RemoveRoutingCidr { cidr, .. } => bounded(cidr, 64),
        ConfigMutation::RemoveNode { node_id } => {
            node_id.len() == 21
                && node_id.starts_with("nh1s-")
                && node_id[5..]
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        }
        ConfigMutation::SetScalarField { field_id, value } => {
            bounded(field_id, 128)
                && matches!(value, Value::Bool(_) | Value::Number(_) | Value::String(_))
        }
    };
    valid.then_some(()).ok_or(ProtocolError::InvalidEnvelope)
}

fn bounded(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

fn valid_source_id(value: &str) -> bool {
    value.len() == 36
        && value.starts_with("src_")
        && value[4..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn application_target(target: &ApplicationTarget) -> bool {
    match target {
        ApplicationTarget::Package {
            android_user_id,
            package,
        } => *android_user_id <= 21_474 && bounded(package, 255),
        ApplicationTarget::Uid { uid } => *uid > 0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorDomain {
    Config,
    Source,
    Subscription,
    Capability,
    Network,
    Core,
    Node,
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
            Self::Node => "NODE",
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
            "NODE" => ErrorDomain::Node,
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
        match (self.ok, self.result.as_ref(), self.error.as_ref()) {
            (true, Some(result), None) => validate_webui_value(result, 0),
            (false, None, Some(error)) => error.validate(),
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
            (StreamKind::Item, true, None) => {
                validate_webui_value(self.payload.as_ref().expect("checked payload"), 0)
            }
            (StreamKind::End, false, None) => Ok(()),
            (StreamKind::Error, false, Some(error)) => error.validate(),
            _ => Err(ProtocolError::InvalidEnvelope),
        }
    }
}

fn validate_webui_value(value: &Value, depth: usize) -> Result<(), ProtocolError> {
    if depth > 32 {
        return Err(ProtocolError::InvalidEnvelope);
    }
    match value {
        Value::String(value) if value.len() > MAX_WEBUI_STRING_BYTES => {
            Err(ProtocolError::InvalidEnvelope)
        }
        Value::Array(values) if values.len() > MAX_WEBUI_ARRAY_ITEMS => {
            Err(ProtocolError::InvalidEnvelope)
        }
        Value::Array(values) => values
            .iter()
            .try_for_each(|value| validate_webui_value(value, depth + 1)),
        Value::Object(values) if values.len() > 2_048 => Err(ProtocolError::InvalidEnvelope),
        Value::Object(values) => values
            .values()
            .try_for_each(|value| validate_webui_value(value, depth + 1)),
        _ => Ok(()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebUiErrorKind {
    Incompatible,
    Timeout,
    Conflict,
    InvalidPayload,
    LimitExceeded,
    Unavailable,
}

impl WebUiErrorKind {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Incompatible => "NH-CORE-INCOMPATIBLE",
            Self::Timeout => "NH-CORE-TIMEOUT",
            Self::Conflict => "NH-CONFIG-CONFLICT",
            Self::InvalidPayload => "NH-CONFIG-INVALID-PAYLOAD",
            Self::LimitExceeded => "NH-CORE-LIMIT",
            Self::Unavailable => "NH-CORE-UNAVAILABLE",
        }
    }

    pub fn error_code(self) -> ErrorCode {
        ErrorCode::parse(self.code().to_owned()).expect("WebUI error codes are frozen and valid")
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
        let envelope: Value =
            serde_json::from_slice(payload).map_err(|_| ProtocolError::InvalidEnvelope)?;
        let version = envelope
            .get("version")
            .and_then(Value::as_u64)
            .ok_or(ProtocolError::InvalidEnvelope)?;
        if version != u64::from(PROTOCOL_VERSION) {
            return Err(ProtocolError::UnsupportedVersion);
        }
        let frame: WireFrame =
            serde_json::from_value(envelope).map_err(|_| ProtocolError::InvalidEnvelope)?;
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
