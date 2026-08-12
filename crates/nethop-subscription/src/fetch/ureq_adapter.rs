use std::collections::BTreeSet;
use std::fmt;
use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

use serde::Deserialize;
use ureq::config::Config;
use ureq::unversioned::resolver::{ArrayVec, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::{
    Buffers, ConnectProxyConnector, ConnectionDetails, Connector, Either, LazyBuffers, NextTimeout,
    RustlsConnector, Transport,
};
use ureq::{Agent, Error as UreqError, Proxy, ProxyProtocol};

use super::{
    ContentEncoding, FetchEndpoint, FetchError, FetchOutcome, FetchPolicy, FetchPolicyError,
    LocalFetchProxy, ParserLimits, RequestProfile, SourceCache, decode_response_body,
    is_denied_ssrf_address, next_redirect, parse_subscription_userinfo, read_bounded,
};

#[derive(Debug)]
enum SecurityAdapterError {
    DeniedAddress,
    PeerMismatch,
}

impl fmt::Display for SecurityAdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeniedAddress => formatter.write_str("resolved address rejected"),
            Self::PeerMismatch => formatter.write_str("connected peer rejected"),
        }
    }
}

impl std::error::Error for SecurityAdapterError {}

const DOH_HOST: &str = "dns.alidns.com";
const DOH_ADDRESS: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(223, 5, 5, 5)), 443);
const MAX_DOH_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_RESOLVED_ADDRESSES: usize = 16;

#[derive(Debug)]
struct SafeDohResolver {
    agent: Agent,
    local_proxy: Option<SocketAddr>,
}

impl Default for SafeDohResolver {
    fn default() -> Self {
        Self {
            agent: build_doh_agent(),
            local_proxy: None,
        }
    }
}

impl SafeDohResolver {
    fn with_local_proxy(proxy: Option<&LocalFetchProxy>) -> Self {
        Self {
            agent: build_doh_agent(),
            local_proxy: proxy.map(|value| SocketAddr::V4(value.endpoint())),
        }
    }

    fn resolve_addresses(
        &self,
        uri: &ureq::http::Uri,
        addresses: impl IntoIterator<Item = IpAddr>,
        port: u16,
    ) -> Result<ResolvedSocketAddrs, UreqError> {
        let mut resolved: ResolvedSocketAddrs =
            ArrayVec::from_fn(|_| SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0));
        for address in addresses {
            let socket = SocketAddr::new(address, port);
            let allowed_proxy = uri.scheme_str() == Some("http")
                && self.local_proxy == Some(socket)
                && address.is_loopback();
            if is_denied_ssrf_address(address) && !allowed_proxy {
                return Err(UreqError::Other(Box::new(
                    SecurityAdapterError::DeniedAddress,
                )));
            }
            resolved.push(socket);
        }
        if resolved.is_empty() {
            return Err(UreqError::HostNotFound);
        }
        Ok(resolved)
    }
}

impl Resolver for SafeDohResolver {
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        _config: &Config,
        _timeout: ureq::unversioned::transport::NextTimeout,
    ) -> Result<ResolvedSocketAddrs, UreqError> {
        let host = uri.host().ok_or(UreqError::HostNotFound)?;
        let port = uri.port_u16().unwrap_or(443);
        if let Ok(address) = host.parse::<IpAddr>() {
            return self.resolve_addresses(uri, [address], port);
        }

        let mut addresses = BTreeSet::new();
        self.resolve_record_type(host, "A", &mut addresses)?;
        self.resolve_record_type(host, "AAAA", &mut addresses)?;
        self.resolve_addresses(uri, addresses, port)
    }
}

impl SafeDohResolver {
    fn resolve_record_type(
        &self,
        host: &str,
        record_type: &str,
        addresses: &mut BTreeSet<IpAddr>,
    ) -> Result<(), UreqError> {
        let mut endpoint = url::Url::parse(&format!("https://{DOH_HOST}/resolve"))
            .map_err(|_| UreqError::HostNotFound)?;
        endpoint
            .query_pairs_mut()
            .append_pair("name", host)
            .append_pair("type", record_type);
        let response = self
            .agent
            .get(endpoint.as_str())
            .header("Accept", "application/dns-json")
            .call()?;
        if response.status().as_u16() != 200 {
            return Err(UreqError::HostNotFound);
        }
        let body = read_bounded(response.into_body().into_reader(), MAX_DOH_RESPONSE_BYTES)
            .map_err(|_| UreqError::HostNotFound)?;
        extend_doh_addresses(&body, addresses)
    }
}

#[derive(Debug, Deserialize)]
struct DohResponse {
    #[serde(rename = "Status")]
    status: u16,
    #[serde(rename = "Answer", default)]
    answers: Vec<DohRecord>,
}

#[derive(Debug, Deserialize)]
struct DohRecord {
    #[serde(rename = "type")]
    record_type: u16,
    data: String,
}

fn extend_doh_addresses(body: &[u8], addresses: &mut BTreeSet<IpAddr>) -> Result<(), UreqError> {
    let answer: DohResponse = serde_json::from_slice(body).map_err(|_| UreqError::HostNotFound)?;
    if answer.status != 0 {
        return Err(UreqError::HostNotFound);
    }
    for record in answer.answers {
        if !matches!(record.record_type, 1 | 28) {
            continue;
        }
        let address = record
            .data
            .parse::<IpAddr>()
            .map_err(|_| UreqError::HostNotFound)?;
        addresses.insert(address);
        if addresses.len() > MAX_RESOLVED_ADDRESSES {
            return Err(UreqError::HostNotFound);
        }
    }
    Ok(())
}

fn resolved_addresses(
    addresses: impl IntoIterator<Item = IpAddr>,
    port: u16,
) -> Result<ResolvedSocketAddrs, UreqError> {
    let mut resolved: ResolvedSocketAddrs =
        ArrayVec::from_fn(|_| SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0));
    for address in addresses {
        if is_denied_ssrf_address(address) {
            return Err(UreqError::Other(Box::new(
                SecurityAdapterError::DeniedAddress,
            )));
        }
        resolved.push(SocketAddr::new(address, port));
    }
    if resolved.is_empty() {
        return Err(UreqError::HostNotFound);
    }
    Ok(resolved)
}

#[derive(Debug)]
struct FixedResolver(SocketAddr);

impl Resolver for FixedResolver {
    fn resolve(
        &self,
        _uri: &ureq::http::Uri,
        _config: &Config,
        _timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, UreqError> {
        resolved_addresses([self.0.ip()], self.0.port())
    }
}

#[derive(Debug, Default)]
struct SafeTcpConnector {
    local_proxy: Option<SocketAddr>,
}

impl SafeTcpConnector {
    fn with_local_proxy(proxy: Option<&LocalFetchProxy>) -> Self {
        Self {
            local_proxy: proxy.map(|value| SocketAddr::V4(value.endpoint())),
        }
    }

    fn address_is_allowed(&self, details: &ConnectionDetails<'_>, address: SocketAddr) -> bool {
        !is_denied_ssrf_address(address.ip())
            || (details.uri.scheme_str() == Some("http")
                && self.local_proxy == Some(address)
                && address.ip().is_loopback())
    }
}

impl<In: Transport> Connector<In> for SafeTcpConnector {
    type Out = Either<In, SafeTcpTransport>;

    fn connect(
        &self,
        details: &ConnectionDetails<'_>,
        chained: Option<In>,
    ) -> Result<Option<Self::Out>, UreqError> {
        if let Some(transport) = chained {
            return Ok(Some(Either::A(transport)));
        }
        if details.addrs.is_empty()
            || details
                .addrs
                .iter()
                .any(|address| !self.address_is_allowed(details, *address))
        {
            return Err(UreqError::Other(Box::new(
                SecurityAdapterError::DeniedAddress,
            )));
        }

        let mut last_error = None;
        for address in details.addrs.iter().copied() {
            match connect_one(address, details) {
                Ok(stream) => {
                    let peer = stream.peer_addr().map_err(UreqError::Io)?;
                    if peer != address || !details.addrs.contains(&peer) {
                        return Err(UreqError::Other(Box::new(
                            SecurityAdapterError::PeerMismatch,
                        )));
                    }
                    if details.config.no_delay() {
                        stream.set_nodelay(true).map_err(UreqError::Io)?;
                    }
                    let buffers = LazyBuffers::new(
                        details.config.input_buffer_size(),
                        details.config.output_buffer_size(),
                    );
                    return Ok(Some(Either::B(SafeTcpTransport::new(stream, buffers))));
                }
                Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
                    last_error = Some(error);
                }
                Err(error) if error.kind() == io::ErrorKind::TimedOut => {
                    return Err(UreqError::Timeout(details.timeout.reason));
                }
                Err(error) => return Err(UreqError::Io(error)),
            }
        }
        Err(UreqError::Io(last_error.unwrap_or_else(|| {
            io::Error::new(io::ErrorKind::ConnectionRefused, "connection failed")
        })))
    }
}

struct SafeTcpTransport {
    stream: TcpStream,
    buffers: LazyBuffers,
    read_timeout: Option<Duration>,
    write_timeout: Option<Duration>,
}

impl SafeTcpTransport {
    fn new(stream: TcpStream, buffers: LazyBuffers) -> Self {
        Self {
            stream,
            buffers,
            read_timeout: None,
            write_timeout: None,
        }
    }

    fn update_read_timeout(&mut self, timeout: NextTimeout) -> Result<(), UreqError> {
        let requested = timeout.not_zero().map(|value| *value);
        if requested != self.read_timeout {
            self.stream
                .set_read_timeout(requested)
                .map_err(UreqError::Io)?;
            self.read_timeout = requested;
        }
        Ok(())
    }

    fn update_write_timeout(&mut self, timeout: NextTimeout) -> Result<(), UreqError> {
        let requested = timeout.not_zero().map(|value| *value);
        if requested != self.write_timeout {
            self.stream
                .set_write_timeout(requested)
                .map_err(UreqError::Io)?;
            self.write_timeout = requested;
        }
        Ok(())
    }
}

impl fmt::Debug for SafeTcpTransport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SafeTcpTransport")
            .field("peer", &self.stream.peer_addr().ok())
            .finish()
    }
}

impl Transport for SafeTcpTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        &mut self.buffers
    }

    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), UreqError> {
        self.update_write_timeout(timeout)?;
        let output = &self.buffers.output()[..amount];
        self.stream.write_all(output).map_err(|error| {
            if matches!(
                error.kind(),
                io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
            ) {
                UreqError::Timeout(timeout.reason)
            } else {
                UreqError::Io(error)
            }
        })
    }

    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, UreqError> {
        self.update_read_timeout(timeout)?;
        let amount = self
            .stream
            .read(self.buffers.input_append_buf())
            .map_err(|error| {
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) {
                    UreqError::Timeout(timeout.reason)
                } else {
                    UreqError::Io(error)
                }
            })?;
        self.buffers.input_appended(amount);
        Ok(amount > 0)
    }

    fn is_open(&mut self) -> bool {
        if self.stream.set_nonblocking(true).is_err() {
            return false;
        }
        let mut byte = [0_u8; 1];
        let open = matches!(
            self.stream.read(&mut byte),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock
        );
        self.stream.set_nonblocking(false).is_ok() && open
    }
}

fn connect_one(
    address: SocketAddr,
    details: &ConnectionDetails<'_>,
) -> Result<TcpStream, io::Error> {
    if let Some(timeout) = details.timeout.not_zero() {
        TcpStream::connect_timeout(&address, *timeout)
    } else {
        TcpStream::connect(address)
    }
}

pub(super) fn fetch_endpoint(
    endpoint: &FetchEndpoint,
    profile: RequestProfile,
    cache: &SourceCache,
    policy: &FetchPolicy,
    limits: &ParserLimits,
    local_proxy: Option<&LocalFetchProxy>,
) -> Result<FetchOutcome, FetchError> {
    let config = build_ureq_config(policy, local_proxy);
    debug_assert!(!config.tls_config().disable_verification());
    debug_assert!(config.tls_config().use_sni());

    let connector =
        ().chain(ConnectProxyConnector::default())
            .chain(SafeTcpConnector::with_local_proxy(local_proxy))
            .chain(RustlsConnector::default());
    let agent = Agent::with_parts(
        config,
        connector,
        SafeDohResolver::with_local_proxy(local_proxy),
    );
    fetch_with_agent(&agent, endpoint, profile, cache, policy, limits)
}

fn build_doh_agent() -> Agent {
    let config = Agent::config_builder()
        .http_status_as_error(false)
        .https_only(true)
        .proxy(None)
        .max_redirects(0)
        .user_agent("NetHop/0.1")
        .accept("application/dns-json")
        .max_response_header_size(16 * 1024)
        .input_buffer_size(8 * 1024)
        .output_buffer_size(4 * 1024)
        .max_idle_connections(0)
        .max_idle_connections_per_host(0)
        .timeout_global(Some(Duration::from_secs(10)))
        .timeout_per_call(Some(Duration::from_secs(10)))
        .timeout_connect(Some(Duration::from_secs(5)))
        .timeout_recv_response(Some(Duration::from_secs(5)))
        .timeout_recv_body(Some(Duration::from_secs(5)))
        .build();
    let connector = ().chain(SafeTcpConnector::default()).chain(RustlsConnector::default());
    Agent::with_parts(config, connector, FixedResolver(DOH_ADDRESS))
}

fn build_ureq_config(policy: &FetchPolicy, local_proxy: Option<&LocalFetchProxy>) -> Config {
    let proxy = local_proxy.map(build_ureq_proxy);
    Agent::config_builder()
        .http_status_as_error(false)
        .https_only(true)
        .proxy(proxy)
        .max_redirects(0)
        .max_redirects_will_error(false)
        .save_redirect_history(false)
        .user_agent("")
        .accept("")
        .accept_encoding("gzip")
        .max_response_header_size(policy.max_response_header_bytes)
        .input_buffer_size(16 * 1024)
        .output_buffer_size(8 * 1024)
        .max_idle_connections(0)
        .max_idle_connections_per_host(0)
        .timeout_global(Some(policy.timeouts.total))
        .timeout_per_call(Some(policy.timeouts.total))
        .timeout_resolve(Some(policy.timeouts.resolve))
        .timeout_connect(Some(policy.timeouts.connect))
        .timeout_recv_response(Some(policy.timeouts.first_byte))
        .timeout_recv_body(Some(policy.timeouts.body))
        .build()
}

fn build_ureq_proxy(proxy: &LocalFetchProxy) -> Proxy {
    Proxy::builder(ProxyProtocol::Http)
        .host(&proxy.endpoint().ip().to_string())
        .port(proxy.endpoint().port())
        .username(proxy.username())
        .password(proxy.password())
        .resolve_target(false)
        .build()
        .expect("validated local fetch proxy must produce a valid ureq proxy")
}

fn fetch_with_agent(
    agent: &Agent,
    endpoint: &FetchEndpoint,
    profile: RequestProfile,
    cache: &SourceCache,
    policy: &FetchPolicy,
    limits: &ParserLimits,
) -> Result<FetchOutcome, FetchError> {
    let mut current = endpoint.url().clone();
    let mut redirects_seen = 0;

    loop {
        let endpoint_digest = crate::Digest::sha256(current.as_str().as_bytes());
        let mut request = agent
            .get(current.as_str())
            .header("User-Agent", profile.user_agent())
            .header("Accept", profile.accept())
            .header("Accept-Encoding", "gzip");
        for (name, value) in cache.conditional_headers_for(endpoint_digest) {
            request = request.header(name, value);
        }

        let response = request.call().map_err(map_ureq_error)?;
        let status = response.status().as_u16();
        if status == 304 {
            return Ok(FetchOutcome {
                body: cache.apply_not_modified()?.to_vec(),
                endpoint_kind: endpoint.kind(),
                endpoint_digest,
                content_type: None,
                etag: None,
                last_modified: None,
                subscription_userinfo: cache.subscription_userinfo(),
                not_modified: true,
            });
        }
        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            let location = response
                .headers()
                .get("location")
                .and_then(|value| value.to_str().ok())
                .ok_or(FetchPolicyError::InvalidRedirect)?;
            current = next_redirect(&current, location, redirects_seen, policy)?;
            redirects_seen += 1;
            continue;
        }
        if !(200..300).contains(&status) {
            return Err(FetchError::HttpStatus);
        }

        let encoded_length = response
            .headers()
            .get("content-length")
            .map(|value| {
                value
                    .to_str()
                    .ok()
                    .and_then(|text| text.parse::<usize>().ok())
                    .ok_or(FetchPolicyError::InvalidResponseMetadata)
            })
            .transpose()?;
        if encoded_length.is_some_and(|length| length > limits.max_body_bytes()) {
            return Err(FetchPolicyError::BodyTooLarge.into());
        }
        let encoding = ContentEncoding::from_header(
            response
                .headers()
                .get("content-encoding")
                .and_then(|value| value.to_str().ok()),
        )?;
        let etag = bounded_header(&response, "etag")?;
        let last_modified = bounded_header(&response, "last-modified")?;
        let content_type = bounded_header(&response, "content-type")?;
        let subscription_userinfo = response
            .headers()
            .get("subscription-userinfo")
            .and_then(|value| value.to_str().ok())
            .and_then(parse_subscription_userinfo);
        let body = decode_response_body(
            response.into_body().into_reader(),
            encoding,
            encoded_length,
            limits,
        )?;

        return Ok(FetchOutcome {
            body,
            endpoint_kind: endpoint.kind(),
            endpoint_digest,
            content_type,
            etag,
            last_modified,
            subscription_userinfo,
            not_modified: false,
        });
    }
}

fn bounded_header(
    response: &ureq::http::Response<ureq::Body>,
    name: &str,
) -> Result<Option<String>, FetchPolicyError> {
    let value = response
        .headers()
        .get(name)
        .map(|value| value.to_str().map(str::to_owned))
        .transpose()
        .map_err(|_| FetchPolicyError::InvalidResponseMetadata)?;
    if value.as_ref().is_some_and(|value| value.len() > 8 * 1024) {
        return Err(FetchPolicyError::InvalidResponseMetadata);
    }
    Ok(value)
}

fn map_ureq_error(error: UreqError) -> FetchError {
    match error {
        UreqError::Timeout(_) => FetchError::Timeout,
        UreqError::Other(value) => {
            if let Some(security) = value.downcast_ref::<SecurityAdapterError>() {
                match security {
                    SecurityAdapterError::DeniedAddress => {
                        FetchError::Policy(FetchPolicyError::DeniedAddress)
                    }
                    SecurityAdapterError::PeerMismatch => {
                        FetchError::Policy(FetchPolicyError::PeerMismatch)
                    }
                }
            } else {
                FetchError::Network
            }
        }
        UreqError::StatusCode(_) => FetchError::HttpStatus,
        UreqError::Io(error) if error.kind() == io::ErrorKind::TimedOut => FetchError::Timeout,
        _ => FetchError::Network,
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
    use std::sync::Arc;
    use std::thread;

    use base64::Engine;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
    use rustls::{ServerConfig, ServerConnection, StreamOwned};
    use ureq::tls::{Certificate, RootCerts, TlsConfig};
    use ureq::unversioned::resolver::{ResolvedSocketAddrs, Resolver};
    use ureq::unversioned::transport::{Connector, RustlsConnector};

    use super::*;

    const CERT_DER_BASE64: &str = "MIIDWzCCAkOgAwIBAgIUbYFcSzns6cFOyzu9oy3S6SWUiWgwDQYJKoZIhvcNAQELBQAwHDEaMBgGA1UEAwwRc3Vic2NyaXB0aW9uLnRlc3QwHhcNMjYwODAyMDYzMzA2WhcNMzYwNzMwMDYzMzA2WjAcMRowGAYDVQQDDBFzdWJzY3JpcHRpb24udGVzdDCCASIwDQYJKoZIhvcNAQEBBQADggEPADCCAQoCggEBALgMCmEB+O1pcJvfKt8g3JMuWe6aUU26dtBDWO4kmG7cGQD4+aXendfwB7DJ4NxVhJMiM/i/THjeWi43BQPuOHZA3U8AzhRKsPjorCe5fAi1CUbXUrF2BnxjAQlMBBYkXSLObGZPQ6CycGXDrYcXrPwKftkZpy8q8FEHmMnXRidGuWVgpPs7ng4DwohrxuJCmD5WL37pxX+Ulx09SHfCZz0+yL4uSGenwP2JMk16ciRJYPzD5ZmOjugtFuTCSQhuRs8XaLqvswRngFBqYXeX/nPK2KJ5jOPHJr+4OAeeu5F6CAmMu19CsE6plet+tevopfY6vBJ/5D4mEVIohmwXGaECAwEAAaOBlDCBkTAdBgNVHQ4EFgQUAACdEg5Vxr8hkb/eE0e5zc7VOeYwHwYDVR0jBBgwFoAUAACdEg5Vxr8hkb/eE0e5zc7VOeYwHAYDVR0RBBUwE4IRc3Vic2NyaXB0aW9uLnRlc3QwDAYDVR0TAQH/BAIwADAOBgNVHQ8BAf8EBAMCBaAwEwYDVR0lBAwwCgYIKwYBBQUHAwEwDQYJKoZIhvcNAQELBQADggEBAIbAykBaX9kKuWdZCfAGfhddILCp5K80RRtdAwLp9hCVjhtdywps3F3d9Fp8gsAY3VR4xQQ1yrZb/mCjSIBSPIcW5V3RvyXtZeMQQumm60ANQErEOhDJmaMfmxertLJhZPuorwLy1FZw0DCstupyxI0Vk4tqAEjCHiKDnHrkAFG7Q8gkD4lsA4Zc2wI3t8f0q1CBlScBNHZnhC7m3dMfQIdWc0eF9giXONm1Fqt6yftwUGHUfdu8Vit5XvbxNS2h09o9yYMWSa0gZM7v1+H6D8eREwXUL5RDd4erv/ENKuE2TaSO+gptfcrs990wO9yql3A5Hha+N4vjSA6wlb+k/Uw=";
    const KEY_DER_BASE64: &str = "MIIEvAIBADANBgkqhkiG9w0BAQEFAASCBKYwggSiAgEAAoIBAQC4DAphAfjtaXCb3yrfINyTLlnumlFNunbQQ1juJJhu3BkA+Pml3p3X8AewyeDcVYSTIjP4v0x43louNwUD7jh2QN1PAM4USrD46KwnuXwItQlG11KxdgZ8YwEJTAQWJF0izmxmT0OgsnBlw62HF6z8Cn7ZGacvKvBRB5jJ10YnRrllYKT7O54OA8KIa8biQpg+Vi9+6cV/lJcdPUh3wmc9Psi+Lkhnp8D9iTJNenIkSWD8w+WZjo7oLRbkwkkIbkbPF2i6r7MEZ4BQamF3l/5zytiieYzjxya/uDgHnruReggJjLtfQrBOqZXrfrXr6KX2OrwSf+Q+JhFSKIZsFxmhAgMBAAECggEAGdpnItVaGE91aA/jP9Cn62zZaTD4Nsj4o6yyM1Gkr/3u7ToMJ4ar+YdYLTdOhOTmaJynXEvS/C+Pz2ofJDw0ZjgaXlyoliUf0vcsJ7BgggCcOv1IOnFv2800eg/Zixq0ko0YoQ6FW291ZnYkTBPBUu5Of0ShPXj0pQ1CIyhATIffIwv5XDNASXf0pliXkKtSwouOfIhGgysWeIAjIg85DCB2OeLdGiensOoo6XGtoWgBS2WJX8R7eQnhzcD5n5ELEEDnqO+b4/eLkAU7u4ctGLvHnQDIpK4PohyJcoB3g70Ubp7GkFH0jCgkkDNFkrmrJiLPj1rsi+4o03k+/5KWOQKBgQD7Y+FkvlqYQeBW1BoDf6ax0pUIyUuqTb005mPGRAVvlY+PuaJf10MZbKchYz9IhKlSexthkMBjnaAzd9yV3x6nAuqBrQAJnfafcAGnxSFKdidMOVUPfr3W9/1PlofQ1nUKoxRyYBZ2d2okvOAudGglF6oamo1TQBMOJteuXJZ5swKBgQC7bAa4TsUzfmFt3XDMo1I5kgJqZDr4Ok5bKZidcsvz8mPost8/zAxJAMlSYso7gG9RUVxM8DokBNNxCJmididXPQVcr35Advl78iDgYnZ2l+ScBFsA1HiXIW8PZk1riADQXBd7BUasy7dUeYnYqoXYlMOHGhwreD4mN6kX4oVNWwKBgCSHX/IWou1q7SFQ0rLdcqh2NAfB0Eff4fV04Nynd66+Kc01qT2J9wsTubllRYXRGRWOI+1qbjpLZkL0UM5KTJbyGodbTx0WogaK7QKm5259erpdvllxDj7VbC6LbhLPhtRT3B2+jqUKNxc9hsnZSmTRantRJ+YH8nzk8gQ5Gfh3AoGAcY52U92GJjkAlyyAV7zs6OzKgePQxu2s5BdD3MHdSSUn26nlEiZzmxfa4wvwNDURPVfqcMNstr4lzmrDi2fDVlwmj43VFQIBD1QZD1sZI6nMXatV6B7UId2kCNSXO/vfYl8p6uO7ep7DqW8qUhifmCYqggUT5FKqdUVsMoiQ89kCgYA21jRCIqCw8skWhFOK6iLDbCsLxc5p3NaMoRTwumPDVQG8TGaxTfV6rdAHPQdeS3iVPNmnhKI1tlqTunOOFc7HBhRRuhffHKvKYNZxW0jX/hV/Mn6ZGIqA9oFB1oAsNPRFP/naGgL1TctSCHXqlo5Xpa5U+xlZMQ850uNgFRh4Sg==";

    #[test]
    fn doh_response_accepts_only_bounded_ip_answers() {
        let mut addresses = BTreeSet::new();
        extend_doh_addresses(
            br#"{"Status":0,"Answer":[{"type":5,"data":"alias.example."},{"type":1,"data":"35.78.253.2"},{"type":28,"data":"2001:db8::1"}]}"#,
            &mut addresses,
        )
        .unwrap();

        assert_eq!(addresses.len(), 2);
        assert!(addresses.contains(&"35.78.253.2".parse().unwrap()));
        assert!(addresses.contains(&"2001:db8::1".parse().unwrap()));
        assert!(extend_doh_addresses(br#"{"Status":3}"#, &mut addresses).is_err());
        assert!(
            extend_doh_addresses(
                br#"{"Status":0,"Answer":[{"type":1,"data":"not-an-ip"}]}"#,
                &mut addresses
            )
            .is_err()
        );
    }

    #[test]
    fn resolved_addresses_reject_fake_ip_and_preserve_https_port() {
        let denied = resolved_addresses(["198.18.0.5".parse().unwrap()], 443).unwrap_err();
        assert!(matches!(denied, UreqError::Other(_)));

        let resolved = resolved_addresses(
            [
                "35.78.253.2".parse().unwrap(),
                "2606:4700::6810:84e5".parse().unwrap(),
            ],
            8443,
        )
        .unwrap();
        assert_eq!(resolved.len(), 2);
        assert!(resolved.iter().all(|address| address.port() == 8443));
    }

    #[test]
    fn local_proxy_exception_never_allows_an_https_loopback_target() {
        let proxy = LocalFetchProxy::new(
            "127.0.0.1:7894".parse().unwrap(),
            "nethop",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .unwrap();
        let resolver = SafeDohResolver::with_local_proxy(Some(&proxy));
        let proxy_uri = "http://127.0.0.1:7894".parse().unwrap();
        let target_uri = "https://127.0.0.1:7894/subscription".parse().unwrap();

        assert!(
            resolver
                .resolve_addresses(&proxy_uri, [Ipv4Addr::LOCALHOST.into()], 7894)
                .is_ok()
        );
        assert!(
            resolver
                .resolve_addresses(&target_uri, [Ipv4Addr::LOCALHOST.into()], 7894)
                .is_err()
        );
    }

    #[test]
    fn doh_response_rejects_more_than_resolver_capacity() {
        let answers = (1..=MAX_RESOLVED_ADDRESSES + 1)
            .map(|index| serde_json::json!({"type": 1, "data": format!("8.8.8.{index}")}))
            .collect::<Vec<_>>();
        let body =
            serde_json::to_vec(&serde_json::json!({"Status": 0, "Answer": answers})).unwrap();
        assert!(extend_doh_addresses(&body, &mut BTreeSet::new()).is_err());
    }

    #[derive(Debug)]
    struct FixedResolver(SocketAddr);

    impl Resolver for FixedResolver {
        fn resolve(
            &self,
            _: &ureq::http::Uri,
            _: &Config,
            _: NextTimeout,
        ) -> Result<ResolvedSocketAddrs, UreqError> {
            let mut addresses = self.empty();
            addresses.push(self.0);
            Ok(addresses)
        }
    }

    #[derive(Debug)]
    struct ProxyFixtureResolver {
        proxy: SocketAddr,
        target_port: u16,
    }

    impl Resolver for ProxyFixtureResolver {
        fn resolve(
            &self,
            uri: &ureq::http::Uri,
            _: &Config,
            _: NextTimeout,
        ) -> Result<ResolvedSocketAddrs, UreqError> {
            let mut addresses = self.empty();
            match uri.host() {
                Some("127.0.0.1") => addresses.push(self.proxy),
                Some("subscription.test") => addresses.push(SocketAddr::new(
                    "35.78.253.2".parse().unwrap(),
                    self.target_port,
                )),
                _ => return Err(UreqError::HostNotFound),
            }
            Ok(addresses)
        }
    }

    #[derive(Debug)]
    struct LocalTlsConnector;

    impl Connector<()> for LocalTlsConnector {
        type Out = SafeTcpTransport;

        fn connect(
            &self,
            details: &ConnectionDetails<'_>,
            _: Option<()>,
        ) -> Result<Option<Self::Out>, UreqError> {
            let expected = *details.addrs.first().ok_or(UreqError::HostNotFound)?;
            let stream = TcpStream::connect(expected).map_err(UreqError::Io)?;
            if stream.peer_addr().map_err(UreqError::Io)? != expected {
                return Err(UreqError::Other(Box::new(
                    SecurityAdapterError::PeerMismatch,
                )));
            }
            let buffers = LazyBuffers::new(
                details.config.input_buffer_size(),
                details.config.output_buffer_size(),
            );
            Ok(Some(SafeTcpTransport::new(stream, buffers)))
        }
    }

    #[test]
    fn configured_agent_keeps_tls_verification_and_sni_enabled() {
        let policy = FetchPolicy::default();
        let config = build_ureq_config(&policy, None);
        assert!(!config.tls_config().disable_verification());
        assert!(config.tls_config().use_sni());
        assert!(config.https_only());
        assert_eq!(config.max_redirects(), 0);
        assert_eq!(config.max_idle_connections(), 0);
        assert_eq!(config.max_idle_connections_per_host(), 0);
        assert_eq!(
            config.max_response_header_size(),
            policy.max_response_header_bytes
        );
        let timeouts = config.timeouts();
        assert_eq!(timeouts.global, Some(policy.timeouts.total));
        assert_eq!(timeouts.per_call, Some(policy.timeouts.total));
        assert_eq!(timeouts.resolve, Some(policy.timeouts.resolve));
        assert_eq!(timeouts.connect, Some(policy.timeouts.connect));
        assert_eq!(timeouts.recv_response, Some(policy.timeouts.first_byte));
        assert_eq!(timeouts.recv_body, Some(policy.timeouts.body));
    }

    #[test]
    fn local_tls_smoke_covers_redirect_gzip_and_conditional_cache() {
        let cert_der = base64::engine::general_purpose::STANDARD
            .decode(CERT_DER_BASE64)
            .unwrap();
        let key_der = base64::engine::general_purpose::STANDARD
            .decode(KEY_DER_BASE64)
            .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server_cert = CertificateDer::from(cert_der.clone());
        let server_key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_der));
        let server_config = Arc::new(
            ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(vec![server_cert], server_key)
                .unwrap(),
        );
        let server = thread::spawn(move || run_tls_fixture(listener, server_config, 4));

        let root = Certificate::from_der(&cert_der).to_owned();
        let tls_config = TlsConfig::builder()
            .root_certs(RootCerts::new_with_certs(&[root]))
            .build();
        let policy = FetchPolicy::default();
        let config = Agent::config_builder()
            .http_status_as_error(false)
            .https_only(true)
            .proxy(None)
            .max_redirects(0)
            .max_idle_connections(0)
            .max_idle_connections_per_host(0)
            .tls_config(tls_config)
            .build();
        let connector = ().chain(LocalTlsConnector).chain(RustlsConnector::default());
        let agent = Agent::with_parts(config, connector, FixedResolver(address));
        let source = crate::SourceId::new("tls-smoke").unwrap();
        let request = crate::fetch::FetchRequest::new(
            source,
            &format!("https://subscription.test:{}/redirect", address.port()),
            std::iter::empty::<&str>(),
            RequestProfile::NetHopGeneric,
            &policy,
        )
        .unwrap();
        let endpoint = &request.endpoints()[0];
        let limits = ParserLimits::default();
        let mut cache = SourceCache::default();

        let first = fetch_with_agent(
            &agent,
            endpoint,
            RequestProfile::NetHopGeneric,
            &cache,
            &policy,
            &limits,
        )
        .unwrap();
        assert_eq!(first.body(), b"trojan://secret@example.com:443");
        assert!(!first.was_not_modified());
        assert_eq!(first.content_type(), Some("application/octet-stream"));
        let first_info = first.subscription_userinfo().unwrap();
        assert_eq!(first_info.used_bytes(), Some(384));
        assert_eq!(first_info.total_bytes(), Some(1024));
        assert_eq!(first_info.expire_at(), Some(2_000_000_000));
        cache.commit(&first, &limits).unwrap();

        let second = fetch_with_agent(
            &agent,
            endpoint,
            RequestProfile::NetHopGeneric,
            &cache,
            &policy,
            &limits,
        )
        .unwrap();
        assert!(second.was_not_modified());
        assert_eq!(second.content_type(), None);
        assert_eq!(second.body(), first.body());
        assert_eq!(
            second.subscription_userinfo(),
            first.subscription_userinfo()
        );
        server.join().unwrap();
    }

    #[test]
    fn authenticated_local_connect_proxy_preserves_target_tls_validation() {
        let cert_der = base64::engine::general_purpose::STANDARD
            .decode(CERT_DER_BASE64)
            .unwrap();
        let key_der = base64::engine::general_purpose::STANDARD
            .decode(KEY_DER_BASE64)
            .unwrap();
        let origin_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin_address = origin_listener.local_addr().unwrap();
        let server_config = Arc::new(
            ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(
                    vec![CertificateDer::from(cert_der.clone())],
                    PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_der)),
                )
                .unwrap(),
        );
        let origin = thread::spawn(move || {
            let (tcp, _) = origin_listener.accept().unwrap();
            let connection = ServerConnection::new(server_config).unwrap();
            let mut stream = StreamOwned::new(connection, tcp);
            let request = read_request_headers(&mut stream);
            assert!(String::from_utf8(request).unwrap().starts_with("GET /sub "));
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 32\r\nConnection: close\r\n\r\ntrojan://secret@example.com:443\n",
                )
                .unwrap();
        });

        let proxy_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let proxy_address = proxy_listener.local_addr().unwrap();
        let proxy_password = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let expected_auth = format!(
            "Proxy-Authorization: Basic {}",
            base64::engine::general_purpose::STANDARD.encode(format!("nethop:{proxy_password}"))
        );
        let proxy = thread::spawn(move || {
            let (mut client, _) = proxy_listener.accept().unwrap();
            let headers = String::from_utf8(read_request_headers(&mut client)).unwrap();
            assert!(headers.starts_with(&format!(
                "CONNECT subscription.test:{} HTTP/1.1",
                origin_address.port()
            )));
            assert!(
                headers
                    .to_ascii_lowercase()
                    .contains(&expected_auth.to_ascii_lowercase())
            );
            let mut upstream = TcpStream::connect(origin_address).unwrap();
            client
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .unwrap();
            let mut client_reader = client.try_clone().unwrap();
            let mut upstream_writer = upstream.try_clone().unwrap();
            let upload = thread::spawn(move || {
                let _ = std::io::copy(&mut client_reader, &mut upstream_writer);
                let _ = upstream_writer.shutdown(Shutdown::Write);
            });
            let _ = std::io::copy(&mut upstream, &mut client);
            let _ = client.shutdown(Shutdown::Write);
            upload.join().unwrap();
        });

        let local_proxy = LocalFetchProxy::new(
            proxy_address.to_string().parse().unwrap(),
            "nethop",
            proxy_password,
        )
        .unwrap();
        let root = Certificate::from_der(&cert_der).to_owned();
        let config = Agent::config_builder()
            .http_status_as_error(false)
            .https_only(true)
            .proxy(Some(build_ureq_proxy(&local_proxy)))
            .max_redirects(0)
            .max_idle_connections(0)
            .max_idle_connections_per_host(0)
            .tls_config(
                TlsConfig::builder()
                    .root_certs(RootCerts::new_with_certs(&[root]))
                    .build(),
            )
            .build();
        let connector =
            ().chain(ConnectProxyConnector::default())
                .chain(SafeTcpConnector::with_local_proxy(Some(&local_proxy)))
                .chain(RustlsConnector::default());
        let agent = Agent::with_parts(
            config,
            connector,
            ProxyFixtureResolver {
                proxy: proxy_address,
                target_port: origin_address.port(),
            },
        );
        let policy = FetchPolicy::default();
        let request = crate::fetch::FetchRequest::new(
            crate::SourceId::new("proxy-smoke").unwrap(),
            &format!("https://subscription.test:{}/sub", origin_address.port()),
            std::iter::empty::<&str>(),
            RequestProfile::NetHopGeneric,
            &policy,
        )
        .unwrap();
        let outcome = fetch_with_agent(
            &agent,
            &request.endpoints()[0],
            RequestProfile::NetHopGeneric,
            &SourceCache::default(),
            &policy,
            &ParserLimits::default(),
        )
        .unwrap();
        assert_eq!(outcome.body(), b"trojan://secret@example.com:443\n");
        proxy.join().unwrap();
        origin.join().unwrap();
    }

    #[test]
    fn local_tls_slow_response_maps_to_stable_timeout() {
        let cert_der = base64::engine::general_purpose::STANDARD
            .decode(CERT_DER_BASE64)
            .unwrap();
        let key_der = base64::engine::general_purpose::STANDARD
            .decode(KEY_DER_BASE64)
            .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server_config = Arc::new(
            ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(
                    vec![CertificateDer::from(cert_der.clone())],
                    PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_der)),
                )
                .unwrap(),
        );
        let server = thread::spawn(move || {
            let (tcp, _) = listener.accept().unwrap();
            let connection = ServerConnection::new(server_config).unwrap();
            let mut stream = StreamOwned::new(connection, tcp);
            let _ = read_request_headers(&mut stream);
            thread::sleep(Duration::from_millis(100));
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
        });

        let root = Certificate::from_der(&cert_der).to_owned();
        let config = Agent::config_builder()
            .http_status_as_error(false)
            .https_only(true)
            .proxy(None)
            .max_redirects(0)
            .max_idle_connections(0)
            .max_idle_connections_per_host(0)
            .tls_config(
                TlsConfig::builder()
                    .root_certs(RootCerts::new_with_certs(&[root]))
                    .build(),
            )
            .timeout_global(Some(Duration::from_secs(1)))
            .timeout_recv_response(Some(Duration::from_millis(20)))
            .build();
        let connector = ().chain(LocalTlsConnector).chain(RustlsConnector::default());
        let agent = Agent::with_parts(config, connector, FixedResolver(address));
        let policy = FetchPolicy::default();
        let request = crate::fetch::FetchRequest::new(
            crate::SourceId::new("timeout-smoke").unwrap(),
            &format!("https://subscription.test:{}/slow", address.port()),
            std::iter::empty::<&str>(),
            RequestProfile::NetHopGeneric,
            &policy,
        )
        .unwrap();
        let error = fetch_with_agent(
            &agent,
            &request.endpoints()[0],
            RequestProfile::NetHopGeneric,
            &SourceCache::default(),
            &policy,
            &ParserLimits::default(),
        )
        .unwrap_err();
        assert_eq!(error, FetchError::Timeout);
        server.join().unwrap();
    }

    fn run_tls_fixture(listener: TcpListener, config: Arc<ServerConfig>, requests: usize) {
        for _ in 0..requests {
            let (tcp, _) = listener.accept().unwrap();
            let connection = ServerConnection::new(config.clone()).unwrap();
            let mut stream = StreamOwned::new(connection, tcp);
            let request = read_request_headers(&mut stream);
            let request_text = String::from_utf8(request).unwrap();
            if request_text.starts_with("GET /redirect ") {
                stream
                    .write_all(
                        b"HTTP/1.1 302 Found\r\nLocation: /gzip\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .unwrap();
            } else if request_text.contains("If-None-Match: \"fixture-v1\"")
                || request_text.contains("if-none-match: \"fixture-v1\"")
            {
                stream
                    .write_all(
                        b"HTTP/1.1 304 Not Modified\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .unwrap();
            } else {
                let mut encoder =
                    flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
                encoder
                    .write_all(b"trojan://secret@example.com:443")
                    .unwrap();
                let body = encoder.finish().unwrap();
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\nETag: \"fixture-v1\"\r\nSubscription-Userinfo: upload=128; download=256; total=1024; expire=2000000000\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(header.as_bytes()).unwrap();
                stream.write_all(&body).unwrap();
            }
            stream.flush().unwrap();
        }
    }

    fn read_request_headers(stream: &mut impl Read) -> Vec<u8> {
        let mut request = Vec::new();
        let mut byte = [0_u8; 1];
        while request.len() < 64 * 1024 {
            if stream.read(&mut byte).unwrap() == 0 {
                break;
            }
            request.push(byte[0]);
            if request.ends_with(b"\r\n\r\n") {
                break;
            }
        }
        request
    }
}
