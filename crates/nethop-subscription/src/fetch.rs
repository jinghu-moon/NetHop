use std::fmt;
use std::io::{self, Read};
use std::net::{IpAddr, Ipv4Addr, SocketAddrV4};
use std::time::Duration;

use flate2::read::MultiGzDecoder;
use serde::Serialize;
use thiserror::Error;
use url::Url;

use crate::{Digest, ParserLimits, RequestProfile, SourceId};

mod ureq_adapter;

pub const MAX_MIRRORS: usize = 3;
pub const MAX_REDIRECTS: usize = 3;
pub const MAX_RESPONSE_HEADER_BYTES: usize = 64 * 1024;
pub const MAX_SUBSCRIPTION_USERINFO_BYTES: usize = 1024;
pub const MAX_SUBSCRIPTION_COUNTER: u64 = 9_007_199_254_740_991;
pub const UREQ_SECURITY_ADAPTER_VERSION: &str = "3.3.0";

#[derive(Clone, PartialEq, Eq)]
pub struct LocalFetchProxy {
    endpoint: SocketAddrV4,
    username: String,
    password: String,
}

impl LocalFetchProxy {
    pub fn new(
        endpoint: SocketAddrV4,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Result<Self, FetchPolicyError> {
        let username = username.into();
        let password = password.into();
        if !endpoint.ip().is_loopback()
            || endpoint.port() == 0
            || username.is_empty()
            || username.len() > 64
            || username.contains(':')
            || username.chars().any(char::is_control)
            || !(32..=128).contains(&password.len())
            || password.chars().any(char::is_control)
        {
            return Err(FetchPolicyError::InvalidLocalProxy);
        }
        Ok(Self {
            endpoint,
            username,
            password,
        })
    }

    pub const fn endpoint(&self) -> SocketAddrV4 {
        self.endpoint
    }

    pub(crate) fn username(&self) -> &str {
        &self.username
    }

    pub(crate) fn password(&self) -> &str {
        &self.password
    }
}

impl fmt::Debug for LocalFetchProxy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LocalFetchProxy")
            .field("endpoint", &self.endpoint)
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct SubscriptionUserInfo {
    upload_bytes: Option<u64>,
    download_bytes: Option<u64>,
    total_bytes: Option<u64>,
    expire_at: Option<i64>,
}

impl SubscriptionUserInfo {
    pub const fn upload_bytes(self) -> Option<u64> {
        self.upload_bytes
    }

    pub const fn download_bytes(self) -> Option<u64> {
        self.download_bytes
    }

    pub const fn total_bytes(self) -> Option<u64> {
        self.total_bytes
    }

    pub const fn expire_at(self) -> Option<i64> {
        self.expire_at
    }

    pub fn used_bytes(self) -> Option<u64> {
        match (self.upload_bytes, self.download_bytes) {
            (Some(upload), Some(download)) => upload
                .checked_add(download)
                .filter(|value| *value <= MAX_SUBSCRIPTION_COUNTER),
            _ => None,
        }
    }

    pub const fn is_empty(self) -> bool {
        self.upload_bytes.is_none()
            && self.download_bytes.is_none()
            && self.total_bytes.is_none()
            && self.expire_at.is_none()
    }
}

pub fn parse_subscription_userinfo(value: &str) -> Option<SubscriptionUserInfo> {
    if value.is_empty() || value.len() > MAX_SUBSCRIPTION_USERINFO_BYTES {
        return None;
    }
    let mut info = SubscriptionUserInfo::default();
    for field in value.split(';') {
        let Some((name, raw_value)) = field.split_once('=') else {
            continue;
        };
        let Some(number) = parse_subscription_number(raw_value.trim()) else {
            continue;
        };
        match name.trim().to_ascii_lowercase().as_str() {
            "upload" => info.upload_bytes = Some(number),
            "download" => info.download_bytes = Some(number),
            "total" => info.total_bytes = Some(number),
            "expire" => info.expire_at = i64::try_from(number).ok().filter(|value| *value > 0),
            _ => {}
        }
    }
    (!info.is_empty()).then_some(info)
}

fn parse_subscription_number(value: &str) -> Option<u64> {
    if let Ok(number) = value.parse::<u64>() {
        return (number <= MAX_SUBSCRIPTION_COUNTER).then_some(number);
    }
    let number = value.parse::<f64>().ok()?;
    (number.is_finite() && number >= 0.0 && number <= MAX_SUBSCRIPTION_COUNTER as f64)
        .then_some(number.trunc() as u64)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FetchTimeouts {
    pub resolve: Duration,
    pub connect: Duration,
    pub first_byte: Duration,
    pub body: Duration,
    pub total: Duration,
}

impl Default for FetchTimeouts {
    fn default() -> Self {
        Self {
            resolve: Duration::from_secs(10),
            connect: Duration::from_secs(10),
            first_byte: Duration::from_secs(20),
            body: Duration::from_secs(30),
            total: Duration::from_secs(30),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchPolicy {
    pub max_redirects: usize,
    pub max_mirrors: usize,
    pub max_response_header_bytes: usize,
    pub timeouts: FetchTimeouts,
}

impl Default for FetchPolicy {
    fn default() -> Self {
        Self {
            max_redirects: MAX_REDIRECTS,
            max_mirrors: MAX_MIRRORS,
            max_response_header_bytes: MAX_RESPONSE_HEADER_BYTES,
            timeouts: FetchTimeouts::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FetchAgentConfig {
    pub https_only: bool,
    pub max_redirects: usize,
    pub max_idle_connections: usize,
    pub max_idle_connections_per_host: usize,
    pub max_response_header_bytes: usize,
    pub tls_verification: bool,
    pub environment_proxy: bool,
}

impl FetchAgentConfig {
    pub const fn from_policy(policy: &FetchPolicy) -> Self {
        Self {
            https_only: true,
            max_redirects: 0,
            max_idle_connections: 0,
            max_idle_connections_per_host: 0,
            max_response_header_bytes: policy.max_response_header_bytes,
            tls_verification: true,
            environment_proxy: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentEncoding {
    Identity,
    Gzip,
}

impl ContentEncoding {
    pub fn from_header(value: Option<&str>) -> Result<Self, FetchPolicyError> {
        match value.map(str::trim).filter(|value| !value.is_empty()) {
            None | Some("identity") => Ok(Self::Identity),
            Some("gzip") => Ok(Self::Gzip),
            Some(_) => Err(FetchPolicyError::UnsupportedContentEncoding),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum FetchPolicyError {
    #[error("local fetch proxy configuration is invalid")]
    InvalidLocalProxy,
    #[error("subscription URL must use HTTPS")]
    NonHttps,
    #[error("subscription URL cannot contain user info")]
    UserInfo,
    #[error("subscription URL must contain a host")]
    MissingHost,
    #[error("resolved address is denied by SSRF policy")]
    DeniedAddress,
    #[error("connected peer does not match the approved address set")]
    PeerMismatch,
    #[error("redirect exceeds the configured limit")]
    RedirectLimit,
    #[error("redirect location is invalid")]
    InvalidRedirect,
    #[error("response headers exceed the configured limit")]
    HeadersTooLarge,
    #[error("response body exceeds the parser body limit")]
    BodyTooLarge,
    #[error("response content encoding is unsupported")]
    UnsupportedContentEncoding,
    #[error("response metadata is invalid")]
    InvalidResponseMetadata,
    #[error("response body read timed out")]
    ResponseTimeout,
    #[error("304 response has no last-known-good body")]
    CacheMiss,
    #[error("source contains too many mirrors")]
    TooManyMirrors,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchDiagnosticCode {
    Network,
    Timeout,
    HttpStatus,
    SsrfDenied,
    PeerMismatch,
    RedirectRejected,
    HeadersTooLarge,
    BodyTooLarge,
    UnsupportedContentEncoding,
    InvalidResponse,
    CacheMiss,
    FormatRejected,
    AcceptedZero,
    MirrorsExhausted,
}

impl fmt::Display for FetchDiagnosticCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Network => "fetch_network_error",
            Self::Timeout => "fetch_timeout",
            Self::HttpStatus => "fetch_http_status",
            Self::SsrfDenied => "ssrf_address_denied",
            Self::PeerMismatch => "ssrf_peer_mismatch",
            Self::RedirectRejected => "fetch_redirect_rejected",
            Self::HeadersTooLarge => "fetch_headers_too_large",
            Self::BodyTooLarge => "fetch_body_too_large",
            Self::UnsupportedContentEncoding => "unsupported_content_encoding",
            Self::InvalidResponse => "fetch_invalid_response",
            Self::CacheMiss => "fetch_cache_miss",
            Self::FormatRejected => "fetch_format_rejected",
            Self::AcceptedZero => "fetch_accepted_zero",
            Self::MirrorsExhausted => "fetch_mirrors_exhausted",
        };
        formatter.write_str(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum FetchError {
    #[error("fetch policy rejected the request")]
    Policy(FetchPolicyError),
    #[error("subscription network request failed")]
    Network,
    #[error("subscription request timed out")]
    Timeout,
    #[error("subscription server returned an unsuccessful status")]
    HttpStatus,
    #[error("all configured subscription endpoints failed")]
    MirrorsExhausted,
}

impl From<FetchPolicyError> for FetchError {
    fn from(value: FetchPolicyError) -> Self {
        if value == FetchPolicyError::ResponseTimeout {
            Self::Timeout
        } else {
            Self::Policy(value)
        }
    }
}

impl FetchError {
    pub const fn code(self) -> FetchDiagnosticCode {
        match self {
            Self::Network => FetchDiagnosticCode::Network,
            Self::Timeout => FetchDiagnosticCode::Timeout,
            Self::HttpStatus => FetchDiagnosticCode::HttpStatus,
            Self::MirrorsExhausted => FetchDiagnosticCode::MirrorsExhausted,
            Self::Policy(policy) => match policy {
                FetchPolicyError::DeniedAddress => FetchDiagnosticCode::SsrfDenied,
                FetchPolicyError::PeerMismatch => FetchDiagnosticCode::PeerMismatch,
                FetchPolicyError::RedirectLimit
                | FetchPolicyError::InvalidRedirect
                | FetchPolicyError::NonHttps => FetchDiagnosticCode::RedirectRejected,
                FetchPolicyError::HeadersTooLarge => FetchDiagnosticCode::HeadersTooLarge,
                FetchPolicyError::BodyTooLarge => FetchDiagnosticCode::BodyTooLarge,
                FetchPolicyError::UnsupportedContentEncoding => {
                    FetchDiagnosticCode::UnsupportedContentEncoding
                }
                FetchPolicyError::ResponseTimeout => FetchDiagnosticCode::Timeout,
                FetchPolicyError::CacheMiss => FetchDiagnosticCode::CacheMiss,
                FetchPolicyError::UserInfo
                | FetchPolicyError::MissingHost
                | FetchPolicyError::InvalidLocalProxy
                | FetchPolicyError::InvalidResponseMetadata
                | FetchPolicyError::TooManyMirrors => FetchDiagnosticCode::InvalidResponse,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchEndpointKind {
    Primary,
    Mirror,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchEndpoint {
    url: Url,
    kind: FetchEndpointKind,
}

impl FetchEndpoint {
    pub const fn kind(&self) -> FetchEndpointKind {
        self.kind
    }

    pub fn origin_digest(&self) -> Digest {
        Digest::sha256(self.url.as_str().as_bytes())
    }

    pub(crate) fn url(&self) -> &Url {
        &self.url
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchRequest {
    source_id: SourceId,
    endpoints: Vec<FetchEndpoint>,
    profile: RequestProfile,
}

impl FetchRequest {
    pub fn new<I, S>(
        source_id: SourceId,
        primary: &str,
        mirrors: I,
        profile: RequestProfile,
        policy: &FetchPolicy,
    ) -> Result<Self, FetchPolicyError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut endpoints = vec![FetchEndpoint {
            url: validate_fetch_url(primary)?,
            kind: FetchEndpointKind::Primary,
        }];
        for mirror in mirrors {
            if endpoints.len() > policy.max_mirrors {
                return Err(FetchPolicyError::TooManyMirrors);
            }
            endpoints.push(FetchEndpoint {
                url: validate_fetch_url(mirror.as_ref())?,
                kind: FetchEndpointKind::Mirror,
            });
        }
        Ok(Self {
            source_id,
            endpoints,
            profile,
        })
    }

    pub fn source_id(&self) -> &SourceId {
        &self.source_id
    }

    pub fn endpoints(&self) -> &[FetchEndpoint] {
        &self.endpoints
    }

    pub const fn profile(&self) -> RequestProfile {
        self.profile
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateAcceptance {
    Accepted,
    FormatRejected,
    AcceptedZero,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchOutcome {
    body: Vec<u8>,
    endpoint_kind: FetchEndpointKind,
    endpoint_digest: Digest,
    content_type: Option<String>,
    etag: Option<String>,
    last_modified: Option<String>,
    subscription_userinfo: Option<SubscriptionUserInfo>,
    not_modified: bool,
}

impl FetchOutcome {
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    pub const fn endpoint_kind(&self) -> FetchEndpointKind {
        self.endpoint_kind
    }

    pub const fn was_not_modified(&self) -> bool {
        self.not_modified
    }

    pub fn content_type(&self) -> Option<&str> {
        self.content_type.as_deref()
    }

    pub const fn subscription_userinfo(&self) -> Option<SubscriptionUserInfo> {
        self.subscription_userinfo
    }
}

#[derive(Debug, Clone)]
pub struct FetchClient {
    policy: FetchPolicy,
    limits: ParserLimits,
    local_proxy: Option<LocalFetchProxy>,
}

impl FetchClient {
    pub fn new(policy: FetchPolicy, limits: ParserLimits) -> Self {
        Self {
            policy,
            limits,
            local_proxy: None,
        }
    }

    pub fn with_local_proxy(mut self, proxy: LocalFetchProxy) -> Self {
        self.local_proxy = Some(proxy);
        self
    }

    pub fn fetch<F>(
        &self,
        request: &FetchRequest,
        cache: &SourceCache,
        mut inspect: F,
    ) -> Result<FetchOutcome, FetchError>
    where
        F: FnMut(&[u8]) -> CandidateAcceptance,
    {
        fetch_with_executor(
            request,
            |endpoint| {
                if let Some(proxy) = self.local_proxy.as_ref()
                    && let Ok(outcome) = ureq_adapter::fetch_endpoint(
                        endpoint,
                        request.profile(),
                        cache,
                        &self.policy,
                        &self.limits,
                        Some(proxy),
                    )
                {
                    return Ok(outcome);
                }
                ureq_adapter::fetch_endpoint(
                    endpoint,
                    request.profile(),
                    cache,
                    &self.policy,
                    &self.limits,
                    None,
                )
            },
            &mut inspect,
        )
    }
}

fn fetch_with_executor<E, F>(
    request: &FetchRequest,
    mut execute: E,
    inspect: &mut F,
) -> Result<FetchOutcome, FetchError>
where
    E: FnMut(&FetchEndpoint) -> Result<FetchOutcome, FetchError>,
    F: FnMut(&[u8]) -> CandidateAcceptance,
{
    let mut last_error = None;
    for endpoint in request.endpoints() {
        let outcome = match execute(endpoint) {
            Ok(value) => value,
            Err(error) => {
                last_error = Some(error);
                continue;
            }
        };
        if inspect(outcome.body()) == CandidateAcceptance::Accepted {
            return Ok(outcome);
        }
    }
    Err(last_error.unwrap_or(FetchError::MirrorsExhausted))
}

fn validate_fetch_url(value: &str) -> Result<Url, FetchPolicyError> {
    crate::validate_source_url(value).map_err(|error| match error {
        crate::SourceUrlError::NonHttps => FetchPolicyError::NonHttps,
        crate::SourceUrlError::UserInfo => FetchPolicyError::UserInfo,
        crate::SourceUrlError::MissingHost => FetchPolicyError::MissingHost,
    })
}

pub fn is_denied_ssrf_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(value) => is_denied_ipv4(value),
        IpAddr::V6(value) => {
            if let Some(mapped) = value.to_ipv4_mapped() {
                return is_denied_ipv4(mapped);
            }
            let segments = value.segments();
            value.is_loopback()
                || value.is_unspecified()
                || value.is_multicast()
                || (segments[0] & 0xfe00) == 0xfc00
                || (segments[0] & 0xffc0) == 0xfe80
                || (segments[0] == 0x0100 && segments[1..4] == [0, 0, 0])
                || (segments[0] == 0x2001 && segments[1] == 0x0002)
                || (segments[0] == 0x2001 && segments[1] == 0x0db8)
                || (segments[0] == 0x2001 && (segments[1] & 0xfff0) == 0x0010)
                || (segments[0] == 0x2001 && (segments[1] & 0xfff0) == 0x0020)
                || segments[0] == 0x2002
                || (segments[0] == 0x0064 && segments[1] == 0xff9b)
        }
    }
}

fn is_denied_ipv4(value: Ipv4Addr) -> bool {
    let [a, b, c, _] = value.octets();
    a == 0
        || a == 10
        || (a == 100 && (b & 0xc0) == 64)
        || a == 127
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 192 && b == 0 && c == 2)
        || (a == 192 && b == 88 && c == 99)
        || (a == 192 && b == 168)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
        || a >= 224
}

pub fn validate_resolved_addresses(addresses: &[IpAddr]) -> Result<(), FetchPolicyError> {
    if addresses.is_empty() || addresses.iter().copied().any(is_denied_ssrf_address) {
        return Err(FetchPolicyError::DeniedAddress);
    }
    Ok(())
}

pub fn validate_peer_address(address: IpAddr) -> Result<(), FetchPolicyError> {
    if is_denied_ssrf_address(address) {
        return Err(FetchPolicyError::DeniedAddress);
    }
    Ok(())
}

pub fn validate_peer_in_approved_set(
    peer: IpAddr,
    approved: &[IpAddr],
) -> Result<(), FetchPolicyError> {
    validate_peer_address(peer)?;
    if !approved.contains(&peer) {
        return Err(FetchPolicyError::PeerMismatch);
    }
    Ok(())
}

pub fn next_redirect(
    current: &Url,
    location: &str,
    redirects_seen: usize,
    policy: &FetchPolicy,
) -> Result<Url, FetchPolicyError> {
    if redirects_seen >= policy.max_redirects {
        return Err(FetchPolicyError::RedirectLimit);
    }
    let target = current
        .join(location)
        .map_err(|_| FetchPolicyError::InvalidRedirect)?;
    validate_fetch_url(target.as_str())
}

pub fn validate_response_limits(
    header_bytes: usize,
    encoded_body_bytes: usize,
    decoded_body_bytes: usize,
    policy: &FetchPolicy,
    limits: &ParserLimits,
) -> Result<(), FetchPolicyError> {
    if header_bytes > policy.max_response_header_bytes {
        return Err(FetchPolicyError::HeadersTooLarge);
    }
    if encoded_body_bytes > limits.max_body_bytes() || decoded_body_bytes > limits.max_body_bytes()
    {
        return Err(FetchPolicyError::BodyTooLarge);
    }
    Ok(())
}

pub fn decode_response_body(
    reader: impl Read,
    encoding: ContentEncoding,
    encoded_length: Option<usize>,
    limits: &ParserLimits,
) -> Result<Vec<u8>, FetchPolicyError> {
    if encoded_length.is_some_and(|length| length > limits.max_body_bytes()) {
        return Err(FetchPolicyError::BodyTooLarge);
    }
    let encoded = read_bounded(reader, limits.max_body_bytes())?;
    match encoding {
        ContentEncoding::Identity => Ok(encoded),
        ContentEncoding::Gzip => read_bounded(
            MultiGzDecoder::new(encoded.as_slice()),
            limits.max_body_bytes(),
        ),
    }
}

fn read_bounded(mut reader: impl Read, limit: usize) -> Result<Vec<u8>, FetchPolicyError> {
    let mut output = Vec::with_capacity(limit.min(16 * 1024));
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let remaining = limit.saturating_sub(output.len());
        let read_limit = (remaining + 1).min(buffer.len());
        let count = reader
            .read(&mut buffer[..read_limit])
            .map_err(map_body_io_error)?;
        if count == 0 {
            return Ok(output);
        }
        if count > remaining {
            return Err(FetchPolicyError::BodyTooLarge);
        }
        output.extend_from_slice(&buffer[..count]);
    }
}

fn map_body_io_error(error: io::Error) -> FetchPolicyError {
    if error.kind() == io::ErrorKind::TimedOut
        || error
            .get_ref()
            .and_then(|source| source.downcast_ref::<ureq::Error>())
            .is_some_and(|error| matches!(error, ureq::Error::Timeout(_)))
    {
        FetchPolicyError::ResponseTimeout
    } else {
        FetchPolicyError::InvalidResponseMetadata
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SourceCache {
    etag: Option<String>,
    last_modified: Option<String>,
    validator_endpoint: Option<Digest>,
    last_known_good: Option<Vec<u8>>,
    subscription_userinfo: Option<SubscriptionUserInfo>,
}

impl SourceCache {
    pub fn last_known_good(&self) -> Option<&[u8]> {
        self.last_known_good.as_deref()
    }

    pub const fn subscription_userinfo(&self) -> Option<SubscriptionUserInfo> {
        self.subscription_userinfo
    }

    pub fn conditional_headers(&self) -> Vec<(&'static str, &str)> {
        let mut headers = Vec::new();
        if let Some(etag) = &self.etag {
            headers.push(("If-None-Match", etag.as_str()));
        }
        if let Some(last_modified) = &self.last_modified {
            headers.push(("If-Modified-Since", last_modified.as_str()));
        }
        headers
    }

    pub fn validator_snapshot(&self) -> (Option<&str>, Option<&str>, Option<Digest>) {
        (
            self.etag.as_deref(),
            self.last_modified.as_deref(),
            self.validator_endpoint,
        )
    }

    pub fn restore(
        &mut self,
        body: Vec<u8>,
        etag: Option<String>,
        last_modified: Option<String>,
        validator_endpoint: Digest,
        limits: &ParserLimits,
    ) -> Result<(), FetchPolicyError> {
        if body.len() > limits.max_body_bytes()
            || etag.as_deref().is_some_and(|value| !valid_validator(value))
            || last_modified
                .as_deref()
                .is_some_and(|value| !valid_validator(value))
        {
            return Err(FetchPolicyError::InvalidResponseMetadata);
        }
        self.last_known_good = Some(body);
        self.etag = etag;
        self.last_modified = last_modified;
        self.validator_endpoint = Some(validator_endpoint);
        Ok(())
    }

    pub(crate) fn conditional_headers_for(
        &self,
        endpoint_digest: Digest,
    ) -> Vec<(&'static str, &str)> {
        if self.validator_endpoint == Some(endpoint_digest) {
            self.conditional_headers()
        } else {
            Vec::new()
        }
    }

    pub fn apply_success(
        &mut self,
        body: Vec<u8>,
        etag: Option<String>,
        last_modified: Option<String>,
        limits: &ParserLimits,
    ) -> Result<(), FetchPolicyError> {
        if body.len() > limits.max_body_bytes() {
            return Err(FetchPolicyError::BodyTooLarge);
        }
        self.last_known_good = Some(body);
        self.etag = etag;
        self.last_modified = last_modified;
        self.validator_endpoint = None;
        Ok(())
    }

    pub fn commit(
        &mut self,
        outcome: &FetchOutcome,
        limits: &ParserLimits,
    ) -> Result<(), FetchPolicyError> {
        if outcome.not_modified {
            return Ok(());
        }
        if outcome.body.len() > limits.max_body_bytes() {
            return Err(FetchPolicyError::BodyTooLarge);
        }
        self.last_known_good = Some(outcome.body.clone());
        self.etag.clone_from(&outcome.etag);
        self.last_modified.clone_from(&outcome.last_modified);
        self.validator_endpoint = Some(outcome.endpoint_digest);
        self.subscription_userinfo = outcome.subscription_userinfo;
        Ok(())
    }

    pub fn apply_not_modified(&self) -> Result<&[u8], FetchPolicyError> {
        self.last_known_good
            .as_deref()
            .ok_or(FetchPolicyError::CacheMiss)
    }
}

fn valid_validator(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 8 * 1024
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte == 0x7f)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(endpoint: &FetchEndpoint, body: &[u8]) -> FetchOutcome {
        FetchOutcome {
            body: body.to_vec(),
            endpoint_kind: endpoint.kind(),
            endpoint_digest: endpoint.origin_digest(),
            content_type: None,
            etag: None,
            last_modified: None,
            subscription_userinfo: None,
            not_modified: false,
        }
    }

    #[test]
    fn subscription_userinfo_parses_mihomo_fields_without_a_title() {
        let info = parse_subscription_userinfo(
            " upload = 128.9 ; download=256; total=1024; expire=2000000000; unknown=9 ",
        )
        .unwrap();

        assert_eq!(info.upload_bytes(), Some(128));
        assert_eq!(info.download_bytes(), Some(256));
        assert_eq!(info.used_bytes(), Some(384));
        assert_eq!(info.total_bytes(), Some(1024));
        assert_eq!(info.expire_at(), Some(2_000_000_000));
    }

    #[test]
    fn invalid_subscription_userinfo_is_ignored_and_bounded() {
        assert_eq!(
            parse_subscription_userinfo("title=Premium; expire=-1"),
            None
        );
        assert_eq!(
            parse_subscription_userinfo(&"x".repeat(MAX_SUBSCRIPTION_USERINFO_BYTES + 1)),
            None
        );
        let info = parse_subscription_userinfo(
            "upload=not-a-number; download=12; total=99999999999999999",
        )
        .unwrap();
        assert_eq!(info.download_bytes(), Some(12));
        assert_eq!(info.upload_bytes(), None);
        assert_eq!(info.total_bytes(), None);
    }

    #[test]
    fn cache_keeps_subscription_userinfo_for_not_modified_responses() {
        let request = FetchRequest::new(
            SourceId::new("source").unwrap(),
            "https://primary.example/sub",
            std::iter::empty::<&str>(),
            RequestProfile::NetHopGeneric,
            &FetchPolicy::default(),
        )
        .unwrap();
        let mut fresh = outcome(&request.endpoints()[0], b"valid");
        fresh.subscription_userinfo =
            parse_subscription_userinfo("upload=10; download=20; total=100; expire=2000000000");
        let mut cache = SourceCache::default();
        cache.commit(&fresh, &ParserLimits::default()).unwrap();

        assert_eq!(
            cache.subscription_userinfo().unwrap().used_bytes(),
            Some(30)
        );
    }

    #[test]
    fn primary_success_does_not_request_mirrors() {
        let request = FetchRequest::new(
            SourceId::new("source").unwrap(),
            "https://primary.example/sub",
            ["https://mirror.example/sub"],
            RequestProfile::NetHopGeneric,
            &FetchPolicy::default(),
        )
        .unwrap();
        let mut calls = 0;
        let result = fetch_with_executor(
            &request,
            |endpoint| {
                calls += 1;
                Ok(outcome(endpoint, b"valid"))
            },
            &mut |_| CandidateAcceptance::Accepted,
        )
        .unwrap();
        assert_eq!(result.endpoint_kind(), FetchEndpointKind::Primary);
        assert_eq!(calls, 1);
    }

    #[test]
    fn network_format_and_zero_node_failures_advance_to_mirrors() {
        let request = FetchRequest::new(
            SourceId::new("source").unwrap(),
            "https://primary.example/sub",
            [
                "https://mirror-1.example/sub",
                "https://mirror-2.example/sub",
            ],
            RequestProfile::NetHopGeneric,
            &FetchPolicy::default(),
        )
        .unwrap();
        let mut calls = 0;
        let result = fetch_with_executor(
            &request,
            |endpoint| {
                calls += 1;
                if calls == 1 {
                    Err(FetchError::Network)
                } else if calls == 2 {
                    Ok(outcome(endpoint, b"invalid"))
                } else {
                    Ok(outcome(endpoint, b"valid"))
                }
            },
            &mut |body| {
                if body == b"invalid" {
                    CandidateAcceptance::AcceptedZero
                } else {
                    CandidateAcceptance::Accepted
                }
            },
        )
        .unwrap();
        assert_eq!(result.endpoint_kind(), FetchEndpointKind::Mirror);
        assert_eq!(calls, 3);
    }

    #[test]
    fn rejected_candidate_never_overwrites_last_known_good() {
        let limits = ParserLimits::default();
        let mut cache = SourceCache::default();
        cache
            .apply_success(b"old".to_vec(), None, None, &limits)
            .unwrap();
        let request = FetchRequest::new(
            SourceId::new("source").unwrap(),
            "https://primary.example/sub",
            std::iter::empty::<&str>(),
            RequestProfile::NetHopGeneric,
            &FetchPolicy::default(),
        )
        .unwrap();
        let result = fetch_with_executor(
            &request,
            |endpoint| Ok(outcome(endpoint, b"bad")),
            &mut |_| CandidateAcceptance::FormatRejected,
        );
        assert_eq!(result.unwrap_err(), FetchError::MirrorsExhausted);
        assert_eq!(cache.apply_not_modified().unwrap(), b"old");
    }
}
