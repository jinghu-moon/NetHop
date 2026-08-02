use std::{collections::BTreeSet, fmt};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    base64_container::{Base64ContainerError, decode_base64_with_limit},
    diagnostics::{DiagnosticCode, NodeDiagnostic, Severity, SourceLocation},
    limits::ParserLimits,
    normalize::normalize_bytes,
    payload::{FormatHint, SourceId},
    protocol::ProxyProtocol,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UriScheme {
    Vless,
    Vmess,
    Shadowsocks,
    Trojan,
    Hysteria2,
    Hysteria2Short,
    Tuic,
    AnyTls,
}

impl UriScheme {
    pub const ALL: [Self; 8] = [
        Self::Vless,
        Self::Vmess,
        Self::Shadowsocks,
        Self::Trojan,
        Self::Hysteria2,
        Self::Hysteria2Short,
        Self::Tuic,
        Self::AnyTls,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vless => "vless",
            Self::Vmess => "vmess",
            Self::Shadowsocks => "ss",
            Self::Trojan => "trojan",
            Self::Hysteria2 => "hysteria2",
            Self::Hysteria2Short => "hy2",
            Self::Tuic => "tuic",
            Self::AnyTls => "anytls",
        }
    }

    pub const fn protocol(self) -> ProxyProtocol {
        match self {
            Self::Vless => ProxyProtocol::Vless,
            Self::Vmess => ProxyProtocol::Vmess,
            Self::Shadowsocks => ProxyProtocol::Shadowsocks,
            Self::Trojan => ProxyProtocol::Trojan,
            Self::Hysteria2 | Self::Hysteria2Short => ProxyProtocol::Hysteria2,
            Self::Tuic => ProxyProtocol::Tuic,
            Self::AnyTls => ProxyProtocol::AnyTls,
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|scheme| scheme.as_str() == value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UriContainerError {
    #[error("unsupported or incorrectly cased URI scheme")]
    UnsupportedScheme,
    #[error("input line does not contain a URI scheme")]
    MissingScheme,
    #[error("URI structure is invalid")]
    InvalidUri,
    #[error("URI line exceeds the configured limit")]
    LineTooLong,
    #[error("percent encoding is invalid")]
    InvalidPercentEncoding,
    #[error("decoded field is not valid UTF-8")]
    InvalidUtf8,
    #[error("decoded field contains a control character")]
    ControlCharacter,
    #[error("URI has too many query parameters")]
    QueryLimitExceeded,
    #[error("URI fragment exceeds the configured limit")]
    FragmentTooLong,
    #[error("VMess inner JSON exceeds the configured limit")]
    VmessInnerJsonTooLarge,
    #[error("VMess inner JSON is invalid Base64")]
    InvalidVmessBase64,
}

impl UriContainerError {
    pub const fn code(&self) -> DiagnosticCode {
        match self {
            Self::UnsupportedScheme => DiagnosticCode::UnsupportedProtocol,
            Self::MissingScheme => DiagnosticCode::UnknownFormat,
            Self::InvalidUri => DiagnosticCode::InvalidUri,
            Self::LineTooLong => DiagnosticCode::InputTooLarge,
            Self::InvalidPercentEncoding => DiagnosticCode::InvalidPercentEncoding,
            Self::InvalidUtf8 => DiagnosticCode::InvalidUtf8,
            Self::ControlCharacter => DiagnosticCode::InvalidUri,
            Self::QueryLimitExceeded => DiagnosticCode::QueryLimitExceeded,
            Self::FragmentTooLong => DiagnosticCode::FragmentTooLong,
            Self::VmessInnerJsonTooLarge => DiagnosticCode::VmessInnerJsonTooLarge,
            Self::InvalidVmessBase64 => DiagnosticCode::InvalidBase64,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct UriQueryParameter<'a> {
    key: &'a str,
    value: &'a str,
}

impl fmt::Debug for UriQueryParameter<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UriQueryParameter")
            .field("key", &"<redacted>")
            .field("value", &"<redacted>")
            .finish()
    }
}

impl<'a> UriQueryParameter<'a> {
    pub const fn raw_key(&self) -> &'a str {
        self.key
    }

    pub const fn raw_value(&self) -> &'a str {
        self.value
    }

    pub fn decoded_key(&self) -> Result<String, UriContainerError> {
        percent_decode_field(self.key)
    }

    pub fn decoded_value(&self) -> Result<String, UriContainerError> {
        percent_decode_field(self.value)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct UriNodeCandidate<'a> {
    scheme: UriScheme,
    raw_without_fragment: &'a str,
    userinfo: Option<&'a str>,
    server: &'a str,
    port: Option<u16>,
    query: Vec<UriQueryParameter<'a>>,
    fragment: Option<&'a str>,
    line: u32,
    item_index: u32,
}

impl fmt::Debug for UriNodeCandidate<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UriNodeCandidate")
            .field("scheme", &self.scheme)
            .field("line", &self.line)
            .field("item_index", &self.item_index)
            .field("port", &self.port)
            .field("query_count", &self.query.len())
            .field("has_userinfo", &self.userinfo.is_some())
            .field("has_fragment", &self.fragment.is_some())
            .finish()
    }
}

impl<'a> UriNodeCandidate<'a> {
    pub const fn scheme(&self) -> UriScheme {
        self.scheme
    }

    pub const fn protocol(&self) -> ProxyProtocol {
        self.scheme.protocol()
    }

    pub const fn line(&self) -> u32 {
        self.line
    }

    pub const fn item_index(&self) -> u32 {
        self.item_index
    }

    pub const fn raw_userinfo(&self) -> Option<&'a str> {
        self.userinfo
    }

    pub const fn server(&self) -> &'a str {
        self.server
    }

    pub const fn port(&self) -> Option<u16> {
        self.port
    }

    pub fn query(&self) -> &[UriQueryParameter<'a>] {
        &self.query
    }

    pub fn query_count(&self) -> usize {
        self.query.len()
    }

    pub fn display_name(&self) -> Result<Option<String>, UriContainerError> {
        self.fragment.map(percent_decode_field).transpose()
    }

    pub fn duplicate_query_keys(&self) -> Vec<String> {
        let mut seen = BTreeSet::new();
        let mut duplicates = BTreeSet::new();
        for parameter in &self.query {
            let key = percent_decode_field(parameter.key).unwrap_or_else(|_| parameter.key.into());
            if !seen.insert(key.clone()) {
                duplicates.insert(key);
            }
        }
        duplicates.into_iter().collect()
    }

    pub fn canonical_key(&self) -> &str {
        self.raw_without_fragment
    }

    pub fn vmess_inner_json(&self) -> Result<Vec<u8>, UriContainerError> {
        if self.scheme != UriScheme::Vmess || self.userinfo.is_some() {
            return Err(UriContainerError::InvalidUri);
        }
        decode_vmess_inner_json(self.server, &ParserLimits::default())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UriNodeResult<'a> {
    item_index: u32,
    line: u32,
    format: FormatHint,
    result: Result<UriNodeCandidate<'a>, NodeDiagnostic>,
}

impl<'a> UriNodeResult<'a> {
    pub const fn item_index(&self) -> u32 {
        self.item_index
    }

    pub const fn line(&self) -> u32 {
        self.line
    }

    pub const fn format(&self) -> FormatHint {
        self.format
    }

    pub fn is_accepted(&self) -> bool {
        self.result.is_ok()
    }

    pub fn is_rejected(&self) -> bool {
        self.result.is_err()
    }

    pub fn candidate(&self) -> Option<&UriNodeCandidate<'a>> {
        self.result.as_ref().ok()
    }

    pub fn diagnostic(&self) -> Option<&NodeDiagnostic> {
        self.result.as_ref().err()
    }
}

pub fn parse_uri_list<'a>(
    bytes: &'a [u8],
    source_id: Option<&SourceId>,
    limits: &ParserLimits,
) -> Vec<UriNodeResult<'a>> {
    let payload = match normalize_bytes(bytes, limits) {
        Ok(payload) => payload,
        Err(error) => {
            return vec![rejected_result(0, 1, error.code(), source_id.cloned())];
        }
    };
    let mut results = Vec::new();
    let mut item_index = 0u32;
    for line in payload.lines() {
        let text = line.text().trim();
        if text.is_empty() || text.starts_with('#') {
            continue;
        }
        let result = if text.len() > limits.max_line_bytes() {
            Err(UriContainerError::LineTooLong)
        } else {
            parse_uri_line(text, line.number(), item_index, limits)
        };
        results.push(match result {
            Ok(candidate) => UriNodeResult {
                item_index,
                line: line.number(),
                format: FormatHint::UriList,
                result: Ok(candidate),
            },
            Err(error) => {
                rejected_result(item_index, line.number(), error.code(), source_id.cloned())
            }
        });
        item_index = item_index.saturating_add(1);
    }
    results
}

pub fn parse_uri_line<'a>(
    line: &'a str,
    line_number: u32,
    item_index: u32,
    limits: &ParserLimits,
) -> Result<UriNodeCandidate<'a>, UriContainerError> {
    if line.len() > limits.max_line_bytes() {
        return Err(UriContainerError::LineTooLong);
    }
    let (scheme_text, remainder) = line
        .split_once("://")
        .ok_or(UriContainerError::MissingScheme)?;
    let scheme = UriScheme::parse(scheme_text).ok_or(UriContainerError::UnsupportedScheme)?;
    if remainder.is_empty() {
        return Err(UriContainerError::InvalidUri);
    }

    let (raw_without_fragment, fragment) = line
        .split_once('#')
        .map_or((line, None), |(base, fragment)| (base, Some(fragment)));
    if fragment.is_some_and(|value| value.len() > limits.max_fragment_bytes()) {
        return Err(UriContainerError::FragmentTooLong);
    }
    if let Some(fragment) = fragment {
        percent_decode_field(fragment)?;
    }

    let remainder = &raw_without_fragment[(scheme_text.len() + 3)..];
    let (authority, query_text) = remainder
        .split_once('?')
        .map_or((remainder, None), |(authority, query)| {
            (authority, Some(query))
        });
    if authority.is_empty() || authority.chars().any(|character| character.is_control()) {
        return Err(UriContainerError::InvalidUri);
    }
    let (userinfo, host_port) = authority
        .rsplit_once('@')
        .map_or((None, authority), |(userinfo, host)| (Some(userinfo), host));
    if let Some(userinfo) = userinfo {
        percent_decode_field(userinfo)?;
    }
    let (server, port) = parse_host_port(host_port)?;
    let query = parse_query(query_text, limits)?;

    let candidate = UriNodeCandidate {
        scheme,
        raw_without_fragment,
        userinfo,
        server,
        port,
        query,
        fragment,
        line: line_number,
        item_index,
    };
    if scheme == UriScheme::Vmess && userinfo.is_none() {
        if let Err(UriContainerError::VmessInnerJsonTooLarge) = candidate.vmess_inner_json() {
            return Err(UriContainerError::VmessInnerJsonTooLarge);
        }
    }
    Ok(candidate)
}

pub fn decode_vmess_inner_json(
    encoded: &str,
    limits: &ParserLimits,
) -> Result<Vec<u8>, UriContainerError> {
    decode_base64_with_limit(encoded.as_bytes(), limits.max_vmess_json_bytes())
        .map(|decoded| decoded.into_bytes())
        .map_err(map_vmess_base64_error)
}

pub fn percent_decode_field(value: &str) -> Result<String, UriContainerError> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }
        let high = bytes
            .get(index + 1)
            .and_then(|byte| hex_value(*byte))
            .ok_or(UriContainerError::InvalidPercentEncoding)?;
        let low = bytes
            .get(index + 2)
            .and_then(|byte| hex_value(*byte))
            .ok_or(UriContainerError::InvalidPercentEncoding)?;
        decoded.push((high << 4) | low);
        index += 3;
    }
    let text = String::from_utf8(decoded).map_err(|_| UriContainerError::InvalidUtf8)?;
    if text.chars().any(char::is_control) {
        return Err(UriContainerError::ControlCharacter);
    }
    Ok(text)
}

fn parse_query<'a>(
    query: Option<&'a str>,
    limits: &ParserLimits,
) -> Result<Vec<UriQueryParameter<'a>>, UriContainerError> {
    let Some(query) = query else {
        return Ok(Vec::new());
    };
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let mut parameters = Vec::new();
    for entry in query.split('&') {
        if parameters.len() == limits.max_query_params() {
            return Err(UriContainerError::QueryLimitExceeded);
        }
        let (key, value) = entry
            .split_once('=')
            .map_or((entry, ""), |(key, value)| (key, value));
        if key.is_empty() {
            return Err(UriContainerError::InvalidUri);
        }
        percent_decode_field(key)?;
        percent_decode_field(value)?;
        parameters.push(UriQueryParameter { key, value });
    }
    Ok(parameters)
}

fn parse_host_port(host_port: &str) -> Result<(&str, Option<u16>), UriContainerError> {
    let (server, port) = if let Some(bracketed) = host_port.strip_prefix('[') {
        let (server, suffix) = bracketed
            .split_once(']')
            .ok_or(UriContainerError::InvalidUri)?;
        let port = if suffix.is_empty() {
            None
        } else {
            Some(parse_port(
                suffix
                    .strip_prefix(':')
                    .ok_or(UriContainerError::InvalidUri)?,
            )?)
        };
        (server, port)
    } else if let Some((server, port)) = host_port.rsplit_once(':') {
        if server.contains(':') {
            return Err(UriContainerError::InvalidUri);
        }
        (server, Some(parse_port(port)?))
    } else {
        (host_port, None)
    };
    if server.is_empty()
        || server
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err(UriContainerError::InvalidUri);
    }
    Ok((server, port))
}

fn parse_port(value: &str) -> Result<u16, UriContainerError> {
    let port = value
        .parse::<u16>()
        .map_err(|_| UriContainerError::InvalidUri)?;
    if port == 0 {
        return Err(UriContainerError::InvalidUri);
    }
    Ok(port)
}

fn rejected_result(
    item_index: u32,
    line: u32,
    code: DiagnosticCode,
    source_id: Option<SourceId>,
) -> UriNodeResult<'static> {
    let mut diagnostic = NodeDiagnostic::new(code, Severity::Error);
    diagnostic.source_id = source_id;
    diagnostic.location = SourceLocation::new(item_index, Some(line), Some(1), None).ok();
    UriNodeResult {
        item_index,
        line,
        format: FormatHint::UriList,
        result: Err(diagnostic),
    }
}

fn map_vmess_base64_error(error: Base64ContainerError) -> UriContainerError {
    match error {
        Base64ContainerError::OutputTooLarge { .. } => UriContainerError::VmessInnerJsonTooLarge,
        _ => UriContainerError::InvalidVmessBase64,
    }
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
