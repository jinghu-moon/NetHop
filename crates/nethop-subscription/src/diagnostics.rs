use std::{collections::BTreeMap, fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use crate::{payload::SourceId, protocol::ProxyProtocol};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("unknown diagnostic code: {0}")]
pub struct UnknownDiagnosticCode(pub String);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DiagnosticCode {
    EmptyInput,
    InputTooLarge,
    InvalidUtf8,
    NulByte,
    InvalidPercentEncoding,
    InvalidUri,
    QueryLimitExceeded,
    NodeLimitExceeded,
    DuplicateQueryParameter,
    FragmentTooLong,
    Base64NestingExceeded,
    VmessInnerJsonTooLarge,
    UnknownFormat,
    AmbiguousFormat,
    AmbiguousDialect,
    FormatHintMismatch,
    InvalidBase64,
    InvalidJson,
    InvalidYaml,
    InvalidIni,
    DuplicateKey,
    DuplicateCredentialKey,
    YamlAliasLimitExceeded,
    YamlNodeLimitExceeded,
    YamlMergeKeyUnsupported,
    MissingRequiredField,
    InvalidEndpoint,
    UnsupportedProtocol,
    UnsupportedTransport,
    UnsupportedSemantics,
    InvalidTlsCombination,
    InsecureTls,
    InvalidCredential,
    UnknownField,
    NonNodeSectionIgnored,
    DuplicateNode,
    SourceAllFailed,
    ActiveLimitExceeded,
    SsrfBlocked,
    SsrfPeerMismatch,
    ResponseTooLarge,
    UnsupportedContentEncoding,
    ProxyFetchSecurityUnsupported,
    NestedResourceBlocked,
    ClashInlineProxiesMissing,
    ClashProxyProvidersNotImported,
    LastKnownGoodUsed,
    Unknown(String),
}

impl DiagnosticCode {
    pub fn as_str(&self) -> &str {
        match self {
            Self::EmptyInput => "empty_input",
            Self::InputTooLarge => "input_too_large",
            Self::InvalidUtf8 => "invalid_utf8",
            Self::NulByte => "nul_byte",
            Self::InvalidPercentEncoding => "invalid_percent_encoding",
            Self::InvalidUri => "invalid_uri",
            Self::QueryLimitExceeded => "query_limit_exceeded",
            Self::NodeLimitExceeded => "node_limit_exceeded",
            Self::DuplicateQueryParameter => "duplicate_query_parameter",
            Self::FragmentTooLong => "fragment_too_long",
            Self::Base64NestingExceeded => "base64_nesting_exceeded",
            Self::VmessInnerJsonTooLarge => "vmess_inner_json_too_large",
            Self::UnknownFormat => "unknown_format",
            Self::AmbiguousFormat => "ambiguous_format",
            Self::AmbiguousDialect => "ambiguous_dialect",
            Self::FormatHintMismatch => "format_hint_mismatch",
            Self::InvalidBase64 => "invalid_base64",
            Self::InvalidJson => "invalid_json",
            Self::InvalidYaml => "invalid_yaml",
            Self::InvalidIni => "invalid_ini",
            Self::DuplicateKey => "duplicate_key",
            Self::DuplicateCredentialKey => "duplicate_credential_key",
            Self::YamlAliasLimitExceeded => "yaml_alias_limit_exceeded",
            Self::YamlNodeLimitExceeded => "yaml_node_limit_exceeded",
            Self::YamlMergeKeyUnsupported => "yaml_merge_key_unsupported",
            Self::MissingRequiredField => "missing_required_field",
            Self::InvalidEndpoint => "invalid_endpoint",
            Self::UnsupportedProtocol => "unsupported_protocol",
            Self::UnsupportedTransport => "unsupported_transport",
            Self::UnsupportedSemantics => "unsupported_semantics",
            Self::InvalidTlsCombination => "invalid_tls_combination",
            Self::InsecureTls => "insecure_tls",
            Self::InvalidCredential => "invalid_credential",
            Self::UnknownField => "unknown_field",
            Self::NonNodeSectionIgnored => "non_node_section_ignored",
            Self::DuplicateNode => "duplicate_node",
            Self::SourceAllFailed => "source_all_failed",
            Self::ActiveLimitExceeded => "active_limit_exceeded",
            Self::SsrfBlocked => "ssrf_blocked",
            Self::SsrfPeerMismatch => "ssrf_peer_mismatch",
            Self::ResponseTooLarge => "response_too_large",
            Self::UnsupportedContentEncoding => "unsupported_content_encoding",
            Self::ProxyFetchSecurityUnsupported => "proxy_fetch_security_unsupported",
            Self::NestedResourceBlocked => "nested_resource_blocked",
            Self::ClashInlineProxiesMissing => "clash_inline_proxies_missing",
            Self::ClashProxyProvidersNotImported => "clash_proxy_providers_not_imported",
            Self::LastKnownGoodUsed => "last_known_good_used",
            Self::Unknown(value) => value,
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "empty_input" => Self::EmptyInput,
            "input_too_large" => Self::InputTooLarge,
            "invalid_utf8" => Self::InvalidUtf8,
            "nul_byte" => Self::NulByte,
            "invalid_percent_encoding" => Self::InvalidPercentEncoding,
            "invalid_uri" => Self::InvalidUri,
            "query_limit_exceeded" => Self::QueryLimitExceeded,
            "node_limit_exceeded" => Self::NodeLimitExceeded,
            "duplicate_query_parameter" => Self::DuplicateQueryParameter,
            "fragment_too_long" => Self::FragmentTooLong,
            "base64_nesting_exceeded" => Self::Base64NestingExceeded,
            "vmess_inner_json_too_large" => Self::VmessInnerJsonTooLarge,
            "unknown_format" => Self::UnknownFormat,
            "ambiguous_format" => Self::AmbiguousFormat,
            "ambiguous_dialect" => Self::AmbiguousDialect,
            "format_hint_mismatch" => Self::FormatHintMismatch,
            "invalid_base64" => Self::InvalidBase64,
            "invalid_json" => Self::InvalidJson,
            "invalid_yaml" => Self::InvalidYaml,
            "invalid_ini" => Self::InvalidIni,
            "duplicate_key" => Self::DuplicateKey,
            "duplicate_credential_key" => Self::DuplicateCredentialKey,
            "yaml_alias_limit_exceeded" => Self::YamlAliasLimitExceeded,
            "yaml_node_limit_exceeded" => Self::YamlNodeLimitExceeded,
            "yaml_merge_key_unsupported" => Self::YamlMergeKeyUnsupported,
            "missing_required_field" => Self::MissingRequiredField,
            "invalid_endpoint" => Self::InvalidEndpoint,
            "unsupported_protocol" => Self::UnsupportedProtocol,
            "unsupported_transport" => Self::UnsupportedTransport,
            "unsupported_semantics" => Self::UnsupportedSemantics,
            "invalid_tls_combination" => Self::InvalidTlsCombination,
            "insecure_tls" => Self::InsecureTls,
            "invalid_credential" => Self::InvalidCredential,
            "unknown_field" => Self::UnknownField,
            "non_node_section_ignored" => Self::NonNodeSectionIgnored,
            "duplicate_node" => Self::DuplicateNode,
            "source_all_failed" => Self::SourceAllFailed,
            "active_limit_exceeded" => Self::ActiveLimitExceeded,
            "ssrf_blocked" => Self::SsrfBlocked,
            "ssrf_peer_mismatch" => Self::SsrfPeerMismatch,
            "response_too_large" => Self::ResponseTooLarge,
            "unsupported_content_encoding" => Self::UnsupportedContentEncoding,
            "proxy_fetch_security_unsupported" => Self::ProxyFetchSecurityUnsupported,
            "nested_resource_blocked" => Self::NestedResourceBlocked,
            "clash_inline_proxies_missing" => Self::ClashInlineProxiesMissing,
            "clash_proxy_providers_not_imported" => Self::ClashProxyProvidersNotImported,
            "last_known_good_used" => Self::LastKnownGoodUsed,
            other => Self::Unknown(other.to_owned()),
        }
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl Serialize for DiagnosticCode {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        s.serialize_str(self.as_str())
    }
}
impl<'de> Deserialize<'de> for DiagnosticCode {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::parse(&String::deserialize(d)?))
    }
}
impl FromStr for DiagnosticCode {
    type Err = UnknownDiagnosticCode;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let code = Self::parse(value);
        if matches!(code, Self::Unknown(_)) {
            Err(UnknownDiagnosticCode(value.to_owned()))
        } else {
            Ok(code)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LocationError {
    #[error("line and column are one-based")]
    ZeroBased,
    #[error("field path exceeds 256 bytes")]
    FieldPathTooLong,
    #[error("field path contains a control character")]
    FieldPathControl,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLocation {
    pub item_index: u32,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub field_path: Option<String>,
}

impl SourceLocation {
    pub fn new(
        item_index: u32,
        line: Option<u32>,
        column: Option<u32>,
        field_path: Option<String>,
    ) -> Result<Self, LocationError> {
        if line.is_some_and(|value| value == 0) || column.is_some_and(|value| value == 0) {
            return Err(LocationError::ZeroBased);
        }
        if field_path.as_ref().is_some_and(|value| value.len() > 256) {
            return Err(LocationError::FieldPathTooLong);
        }
        if field_path
            .as_ref()
            .is_some_and(|value| value.chars().any(char::is_control))
        {
            return Err(LocationError::FieldPathControl);
        }
        Ok(Self {
            item_index,
            line,
            column,
            field_path,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticParameter {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeDiagnostic {
    pub severity: Severity,
    pub code: DiagnosticCode,
    pub source_id: Option<SourceId>,
    pub location: Option<SourceLocation>,
    pub protocol: Option<ProxyProtocol>,
    pub node_id: Option<String>,
    pub parameters: BTreeMap<String, String>,
}

impl NodeDiagnostic {
    pub fn new(code: DiagnosticCode, severity: Severity) -> Self {
        Self {
            severity,
            code,
            source_id: None,
            location: None,
            protocol: None,
            node_id: None,
            parameters: BTreeMap::new(),
        }
    }

    pub fn with_parameter(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        const ALLOWED: &[&str] = &[
            "field",
            "format",
            "transport",
            "source_kind",
            "expected_format",
            "limit",
            "count",
        ];
        let key = key.into();
        if ALLOWED.contains(&key.as_str()) {
            let value = value.into();
            if value.len() <= 256 && !value.chars().any(char::is_control) {
                self.parameters.insert(key, value);
            }
        }
        self
    }
}
