use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read, Write},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

use crate::GroupState;
use ureq::Agent;

const SELECTOR_TAG: &str = "nethop-select";
const AUTO_SELECTOR_TAG: &str = "nethop-auto";
const DEFAULT_LIMIT: u8 = 64;
const GROUP_DELAY_TIMEOUT_MILLIS: u64 = 10_000;
const GROUP_DELAY_REQUEST_TIMEOUT: Duration = Duration::from_secs(12);
const MAX_GROUP_DELAY_RESULTS: usize = 2_000;
const MAX_PROXY_ENTRIES: usize = 2_004;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClashApiLimits {
    timeout: Duration,
    max_response_bytes: usize,
}

impl ClashApiLimits {
    pub fn new(timeout: Duration, max_response_bytes: usize) -> Result<Self, ClashApiError> {
        if timeout.is_zero()
            || timeout > Duration::from_secs(30)
            || !(1024..=1024 * 1024).contains(&max_response_bytes)
        {
            return Err(ClashApiError::InvalidLimits);
        }
        Ok(Self {
            timeout,
            max_response_bytes,
        })
    }
}

impl Default for ClashApiLimits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(3),
            max_response_bytes: 1024 * 1024,
        }
    }
}

#[derive(Clone)]
pub struct ClashApiClient {
    endpoint: SocketAddrV4,
    authorization: String,
    limits: ClashApiLimits,
    agent: Agent,
}

impl std::fmt::Debug for ClashApiClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ClashApiClient")
            .field("endpoint", &self.endpoint)
            .field("authorization", &"[REDACTED]")
            .field("limits", &self.limits)
            .finish()
    }
}

impl ClashApiClient {
    pub fn new(
        endpoint: SocketAddrV4,
        secret: impl Into<String>,
        limits: ClashApiLimits,
    ) -> Result<Self, ClashApiError> {
        let secret = secret.into();
        if *endpoint.ip() != Ipv4Addr::LOCALHOST
            || endpoint.port() == 0
            || !(16..=128).contains(&secret.len())
            || secret.chars().any(char::is_control)
        {
            return Err(ClashApiError::InvalidEndpoint);
        }
        let agent: Agent = Agent::config_builder()
            .http_status_as_error(false)
            .https_only(false)
            .proxy(None)
            .max_redirects(0)
            .max_response_header_size(32 * 1024)
            .max_idle_connections(0)
            .max_idle_connections_per_host(0)
            .timeout_global(Some(limits.timeout))
            .timeout_per_call(Some(limits.timeout))
            .timeout_connect(Some(limits.timeout))
            .timeout_recv_response(Some(limits.timeout))
            .timeout_recv_body(Some(limits.timeout))
            .build()
            .into();
        Ok(Self {
            endpoint,
            authorization: format!("Bearer {secret}"),
            limits,
            agent,
        })
    }

    pub fn group_snapshot(&self) -> Result<ClashGroupSnapshot, ClashApiError> {
        let document = self.request(ApiMethod::Get, "/proxies", None)?;
        let proxies = document
            .get("proxies")
            .and_then(Value::as_object)
            .filter(|proxies| proxies.len() <= MAX_PROXY_ENTRIES)
            .ok_or(ClashApiError::InvalidResponse)?;
        let mut groups = BTreeMap::new();
        let mut terminals = BTreeMap::new();
        for (tag, value) in proxies {
            if !valid_tag(tag) {
                return Err(ClashApiError::InvalidResponse);
            }
            let object = value.as_object().ok_or(ClashApiError::InvalidResponse)?;
            if object.contains_key("all") || object.contains_key("now") {
                let now = object.get("now").and_then(Value::as_str).map(str::to_owned);
                let all = object
                    .get("all")
                    .and_then(Value::as_array)
                    .ok_or(ClashApiError::InvalidResponse)?
                    .iter()
                    .map(|member| {
                        member
                            .as_str()
                            .filter(|member| valid_tag(member))
                            .map(str::to_owned)
                            .ok_or(ClashApiError::InvalidResponse)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let group =
                    GroupState::new(tag, now, all).map_err(|_| ClashApiError::InvalidResponse)?;
                groups.insert(tag.clone(), group);
            } else {
                let kind = object
                    .get("type")
                    .and_then(Value::as_str)
                    .filter(|kind| !kind.is_empty() && kind.len() <= 32)
                    .ok_or(ClashApiError::InvalidResponse)?
                    .to_owned();
                let latency_ms = object
                    .get("history")
                    .and_then(Value::as_array)
                    .and_then(|history| history.last())
                    .and_then(|entry| entry.get("delay"))
                    .and_then(Value::as_u64)
                    .and_then(|delay| u32::try_from(delay).ok());
                terminals.insert(
                    tag.clone(),
                    ClashTerminalState {
                        kind,
                        latency_ms,
                        alive: object.get("alive").and_then(Value::as_bool),
                    },
                );
            }
        }
        if !groups.contains_key(SELECTOR_TAG) {
            return Err(ClashApiError::InvalidResponse);
        }
        Ok(ClashGroupSnapshot { groups, terminals })
    }

    pub fn test_node(&self, tag: &str) -> Result<DelayResult, ClashApiError> {
        let path = format!(
            "/proxies/{}/delay?timeout=5000&url=http%3A%2F%2Fwww.gstatic.com%2Fgenerate_204",
            encode_path_segment(tag)
        );
        let response = self.request(ApiMethod::Get, &path, None)?;
        let delay_ms = response
            .get("delay")
            .and_then(Value::as_u64)
            .ok_or(ClashApiError::InvalidResponse)?;
        Ok(DelayResult {
            tag: tag.to_owned(),
            delay_ms,
        })
    }

    pub fn test_all_nodes(&self) -> Result<Vec<DelayResult>, ClashApiError> {
        let path = format!(
            "/group/nethop-select/delay?timeout={GROUP_DELAY_TIMEOUT_MILLIS}&url=https%3A%2F%2Fwww.gstatic.com%2Fgenerate_204"
        );
        let response =
            self.request_with_timeout(ApiMethod::Get, &path, None, GROUP_DELAY_REQUEST_TIMEOUT)?;
        let entries = response
            .as_object()
            .filter(|entries| entries.len() <= MAX_GROUP_DELAY_RESULTS)
            .ok_or(ClashApiError::InvalidResponse)?;
        let mut results = entries
            .iter()
            .filter_map(|(tag, delay)| {
                let delay_ms = delay.as_u64()?;
                (valid_stable_node_tag(tag) && delay_ms <= u64::from(u16::MAX)).then(|| {
                    DelayResult {
                        tag: tag.clone(),
                        delay_ms,
                    }
                })
            })
            .collect::<Vec<_>>();
        results.sort_unstable_by(|left, right| left.tag.cmp(&right.tag));
        Ok(results)
    }

    pub fn select_node(&self, tag: &str) -> Result<(), ClashApiError> {
        if tag != AUTO_SELECTOR_TAG && !valid_stable_node_tag(tag) {
            return Err(ClashApiError::UnknownTarget);
        }
        let document = self.request(ApiMethod::Get, "/proxies", None)?;
        let is_member = document
            .get("proxies")
            .and_then(Value::as_object)
            .and_then(|proxies| proxies.get(SELECTOR_TAG))
            .and_then(Value::as_object)
            .and_then(|selector| selector.get("all"))
            .and_then(Value::as_array)
            .is_some_and(|members| members.iter().any(|member| member.as_str() == Some(tag)));
        if !is_member {
            return Err(ClashApiError::UnknownTarget);
        }
        self.request(
            ApiMethod::Put,
            &format!("/proxies/{}", encode_path_segment(SELECTOR_TAG)),
            Some(&json!({"name":tag})),
        )?;
        Ok(())
    }

    pub fn select_auto(&self) -> Result<(), ClashApiError> {
        self.select_node(AUTO_SELECTOR_TAG)
    }

    pub fn select_manual_tag(&self, tag: &str) -> Result<(), ClashApiError> {
        if !valid_stable_node_tag(tag) {
            return Err(ClashApiError::UnknownTarget);
        }
        self.select_node(tag)
    }

    pub fn connections(
        &self,
        query: Option<&str>,
        limit: Option<u8>,
    ) -> Result<Vec<ConnectionSummary>, ClashApiError> {
        let document = self.request(ApiMethod::Get, "/connections", None)?;
        let connections = document
            .get("connections")
            .and_then(Value::as_array)
            .ok_or(ClashApiError::InvalidResponse)?;
        let query = query.map(str::to_lowercase);
        let limit = usize::from(limit.unwrap_or(DEFAULT_LIMIT));
        let mut summaries = Vec::with_capacity(connections.len().min(limit));
        for connection in connections {
            let id = connection.get("id").and_then(Value::as_str);
            let metadata = connection.get("metadata").and_then(Value::as_object);
            let host = metadata
                .and_then(|metadata| metadata.get("host"))
                .and_then(Value::as_str)
                .filter(|host| !host.is_empty());
            let destination = metadata
                .and_then(|metadata| metadata.get("destinationIP"))
                .and_then(Value::as_str);
            let port = metadata
                .and_then(|metadata| metadata.get("destinationPort"))
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| {
                    metadata
                        .and_then(|metadata| metadata.get("destinationPort"))
                        .and_then(Value::as_u64)
                        .map(|port| port.to_string())
                });
            let target_host = host.or(destination).unwrap_or("unknown");
            let target = if let Some(port) = port.filter(|port| !port.is_empty()) {
                format!("{target_host}:{port}")
            } else {
                target_host.to_owned()
            };
            let network = metadata
                .and_then(|metadata| metadata.get("network"))
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_owned();
            let process = metadata
                .and_then(|metadata| metadata.get("process"))
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_owned);
            let chains: Vec<String> = connection
                .get("chains")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .take(16)
                .map(str::to_owned)
                .collect();
            let searchable = format!(
                "{target} {network} {} {}",
                process.as_deref().unwrap_or(""),
                chains.join(" ")
            )
            .to_lowercase();
            if query
                .as_ref()
                .is_some_and(|query| !searchable.contains(query))
            {
                continue;
            }
            summaries.push(ConnectionSummary {
                id: id.ok_or(ClashApiError::InvalidResponse)?.to_owned(),
                network,
                target,
                process,
                chains,
                upload_bytes: connection
                    .get("upload")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                download_bytes: connection
                    .get("download")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            });
            if summaries.len() == limit {
                break;
            }
        }
        Ok(summaries)
    }

    pub fn close_connection(&self, id: &str) -> Result<(), ClashApiError> {
        self.request(
            ApiMethod::Delete,
            &format!("/connections/{}", encode_path_segment(id)),
            None,
        )?;
        Ok(())
    }

    pub fn close_all_connections(&self) -> Result<(), ClashApiError> {
        self.request(ApiMethod::Delete, "/connections", None)?;
        Ok(())
    }

    pub fn traffic_sample(&self) -> Result<TrafficSample, ClashApiError> {
        let mut stream =
            TcpStream::connect_timeout(&SocketAddr::V4(self.endpoint), self.limits.timeout)
                .map_err(|_| ClashApiError::Unavailable)?;
        stream
            .set_read_timeout(Some(self.limits.timeout))
            .and_then(|()| stream.set_write_timeout(Some(self.limits.timeout)))
            .map_err(|_| ClashApiError::Unavailable)?;
        write!(
            stream,
            "GET /traffic HTTP/1.1\r\nHost: {}\r\nAuthorization: {}\r\nAccept: application/json\r\nConnection: close\r\n\r\n",
            self.endpoint, self.authorization
        )
        .and_then(|()| stream.flush())
        .map_err(|_| ClashApiError::Unavailable)?;

        let mut reader = BufReader::new(stream);
        let status = read_bounded_line(&mut reader, 1024)?;
        if !status.starts_with("HTTP/1.1 200 ") && !status.starts_with("HTTP/1.0 200 ") {
            return Err(ClashApiError::Rejected);
        }
        let mut chunked = false;
        let mut header_bytes = status.len();
        loop {
            let line = read_bounded_line(&mut reader, 8 * 1024)?;
            header_bytes = header_bytes.saturating_add(line.len());
            if header_bytes > 32 * 1024 {
                return Err(ClashApiError::ResponseTooLarge);
            }
            if line == "\r\n" || line == "\n" {
                break;
            }
            if let Some((name, value)) = line.split_once(':')
                && name.eq_ignore_ascii_case("transfer-encoding")
                && value
                    .split(',')
                    .any(|encoding| encoding.trim().eq_ignore_ascii_case("chunked"))
            {
                chunked = true;
            }
        }
        let bytes = if chunked {
            read_first_chunk(&mut reader, self.limits.max_response_bytes)?
        } else {
            let mut bytes = Vec::new();
            reader
                .by_ref()
                .take((self.limits.max_response_bytes + 1) as u64)
                .read_until(b'\n', &mut bytes)
                .map_err(|_| ClashApiError::Unavailable)?;
            if bytes.len() > self.limits.max_response_bytes {
                return Err(ClashApiError::ResponseTooLarge);
            }
            bytes
        };
        serde_json::from_slice(&bytes).map_err(ClashApiError::from)
    }

    pub fn traffic_totals(&self) -> Result<TrafficTotals, ClashApiError> {
        let document = self.request(ApiMethod::Get, "/connections", None)?;
        Ok(TrafficTotals {
            upload: document
                .get("uploadTotal")
                .and_then(Value::as_u64)
                .ok_or(ClashApiError::InvalidResponse)?,
            download: document
                .get("downloadTotal")
                .and_then(Value::as_u64)
                .ok_or(ClashApiError::InvalidResponse)?,
        })
    }

    fn request(
        &self,
        method: ApiMethod,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value, ClashApiError> {
        self.request_with_timeout(method, path, body, self.limits.timeout)
    }

    fn request_with_timeout(
        &self,
        method: ApiMethod,
        path: &str,
        body: Option<&Value>,
        timeout: Duration,
    ) -> Result<Value, ClashApiError> {
        if !path.starts_with('/') || path.chars().any(|character| character.is_control()) {
            return Err(ClashApiError::InvalidRequest);
        }
        let url = format!("http://{}{path}", self.endpoint);
        let response = match method {
            ApiMethod::Get => self
                .agent
                .get(&url)
                .header("Authorization", &self.authorization)
                .header("Accept", "application/json")
                .config()
                .timeout_global(Some(timeout))
                .timeout_per_call(Some(timeout))
                .timeout_recv_response(Some(timeout))
                .timeout_recv_body(Some(timeout))
                .build()
                .call(),
            ApiMethod::Put => self
                .agent
                .put(&url)
                .header("Authorization", &self.authorization)
                .header("Accept", "application/json")
                .header("Content-Type", "application/json")
                .send(serde_json::to_vec(
                    body.ok_or(ClashApiError::InvalidRequest)?,
                )?),
            ApiMethod::Delete => self
                .agent
                .delete(&url)
                .header("Authorization", &self.authorization)
                .header("Accept", "application/json")
                .call(),
        }
        .map_err(|_| ClashApiError::Unavailable)?;
        if !(200..300).contains(&response.status().as_u16()) {
            return Err(ClashApiError::Rejected);
        }
        if response
            .headers()
            .get("content-length")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<usize>().ok())
            .is_some_and(|length| length > self.limits.max_response_bytes)
        {
            return Err(ClashApiError::ResponseTooLarge);
        }
        let mut bytes = Vec::new();
        response
            .into_body()
            .into_reader()
            .take((self.limits.max_response_bytes + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| ClashApiError::Unavailable)?;
        if bytes.len() > self.limits.max_response_bytes {
            return Err(ClashApiError::ResponseTooLarge);
        }
        if bytes.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&bytes).map_err(ClashApiError::from)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClashGroupSnapshot {
    groups: BTreeMap<String, GroupState>,
    terminals: BTreeMap<String, ClashTerminalState>,
}

impl ClashGroupSnapshot {
    pub fn groups(&self) -> &BTreeMap<String, GroupState> {
        &self.groups
    }

    pub fn terminal(&self, tag: &str) -> Option<&ClashTerminalState> {
        self.terminals.get(tag)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClashTerminalState {
    kind: String,
    latency_ms: Option<u32>,
    alive: Option<bool>,
}

impl ClashTerminalState {
    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub const fn latency_ms(&self) -> Option<u32> {
        self.latency_ms
    }

    pub const fn alive(&self) -> Option<bool> {
        self.alive
    }
}

fn valid_stable_node_tag(tag: &str) -> bool {
    tag.len() == 21
        && tag.starts_with("nh1s-")
        && tag[5..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn read_bounded_line(reader: &mut impl BufRead, maximum: usize) -> Result<String, ClashApiError> {
    let mut line = String::new();
    let count = reader
        .take((maximum + 1) as u64)
        .read_line(&mut line)
        .map_err(|_| ClashApiError::Unavailable)?;
    if count == 0 || line.len() > maximum {
        return Err(ClashApiError::ResponseTooLarge);
    }
    Ok(line)
}

fn read_first_chunk(reader: &mut impl BufRead, maximum: usize) -> Result<Vec<u8>, ClashApiError> {
    let size_line = read_bounded_line(reader, 128)?;
    let size = size_line
        .trim()
        .split(';')
        .next()
        .and_then(|value| usize::from_str_radix(value, 16).ok())
        .filter(|size| *size > 0 && *size <= maximum)
        .ok_or(ClashApiError::InvalidResponse)?;
    let mut bytes = vec![0_u8; size];
    reader
        .read_exact(&mut bytes)
        .map_err(|_| ClashApiError::Unavailable)?;
    let mut terminator = [0_u8; 2];
    reader
        .read_exact(&mut terminator)
        .map_err(|_| ClashApiError::Unavailable)?;
    if terminator != *b"\r\n" {
        return Err(ClashApiError::InvalidResponse);
    }
    Ok(bytes)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrafficSample {
    pub up: u64,
    pub down: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrafficTotals {
    pub upload: u64,
    pub download: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DelayResult {
    pub tag: String,
    pub delay_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConnectionSummary {
    pub id: String,
    pub network: String,
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process: Option<String>,
    pub chains: Vec<String>,
    pub upload_bytes: u64,
    pub download_bytes: u64,
}

#[derive(Debug, Clone, Copy)]
enum ApiMethod {
    Get,
    Put,
    Delete,
}

fn encode_path_segment(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    encoded
}

fn valid_tag(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
}

#[derive(Debug, Error)]
pub enum ClashApiError {
    #[error("Clash API endpoint or secret is invalid")]
    InvalidEndpoint,
    #[error("Clash API limits are invalid")]
    InvalidLimits,
    #[error("Clash API request is invalid")]
    InvalidRequest,
    #[error("Clash API is unavailable")]
    Unavailable,
    #[error("Clash API rejected the request")]
    Rejected,
    #[error("Clash API response exceeds the bounded limit")]
    ResponseTooLarge,
    #[error("Clash API response is malformed")]
    InvalidResponse,
    #[error("Clash API target is not selectable")]
    UnknownTarget,
    #[error("Clash API JSON could not be encoded or decoded")]
    Json(#[from] serde_json::Error),
}
