use std::{
    fs,
    net::UdpSocket,
    time::{Duration, Instant},
};

use serde::Serialize;

use crate::ProcessIdentity;

const MAX_CPU_SAMPLE_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProcessMetrics {
    pub pid: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_rss_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
struct ProcessCpuObservation {
    identity: ProcessIdentity,
    cpu_ticks: u64,
    observed_at: Instant,
}

#[derive(Debug, Default)]
pub struct ProcessMetricsSampler {
    previous: Option<ProcessCpuObservation>,
}

impl ProcessMetricsSampler {
    pub fn sample(&mut self, identity: ProcessIdentity) -> ProcessMetrics {
        let pid = identity.pid();
        let observed_at = Instant::now();
        let stat = fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .and_then(|document| parse_process_stat(&document))
            .filter(|(_, start_ticks)| {
                identity
                    .start_time_ticks()
                    .is_none_or(|expected| expected == *start_ticks)
            });
        let cpu_percent = stat.and_then(|(cpu_ticks, _)| {
            let current = ProcessCpuObservation {
                identity,
                cpu_ticks,
                observed_at,
            };
            let percent = self.previous.and_then(|previous| {
                let elapsed = current.observed_at.duration_since(previous.observed_at);
                (previous.identity == identity && elapsed <= MAX_CPU_SAMPLE_INTERVAL).then(
                    || {
                        calculate_cpu_percent(
                            previous.cpu_ticks,
                            current.cpu_ticks,
                            elapsed,
                            clock_ticks_per_second()?,
                        )
                    },
                )?
            });
            self.previous = Some(current);
            percent
        });
        if stat.is_none() {
            self.previous = None;
        }
        ProcessMetrics {
            pid,
            cpu_percent,
            memory_rss_bytes: stat.and_then(|_| process_rss_bytes(pid)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OutboundRoute {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interface: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_address: Option<String>,
    pub public_ip: Option<String>,
}

pub fn collect_outbound_route() -> OutboundRoute {
    OutboundRoute {
        interface: fs::read_to_string("/proc/net/route")
            .ok()
            .and_then(|document| parse_default_route_interface(&document)),
        local_address: UdpSocket::bind("0.0.0.0:0")
            .ok()
            .and_then(|socket| socket.connect("1.1.1.1:53").ok().map(|()| socket))
            .and_then(|socket| socket.local_addr().ok())
            .map(|address| address.ip().to_string()),
        public_ip: None,
    }
}

pub fn calculate_cpu_percent(
    previous_ticks: u64,
    current_ticks: u64,
    elapsed: Duration,
    ticks_per_second: u64,
) -> Option<f64> {
    if elapsed.is_zero() || ticks_per_second == 0 {
        return None;
    }
    let delta = current_ticks.checked_sub(previous_ticks)?;
    Some(delta as f64 / ticks_per_second as f64 / elapsed.as_secs_f64() * 100.0)
}

pub fn parse_default_route_interface(document: &str) -> Option<String> {
    document.lines().skip(1).find_map(|line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 4 || fields[1] != "00000000" {
            return None;
        }
        let flags = u16::from_str_radix(fields[3], 16).ok()?;
        ((flags & 0x3) == 0x3 && valid_interface(fields[0])).then(|| fields[0].to_owned())
    })
}

pub fn parse_process_stat(document: &str) -> Option<(u64, u64)> {
    let end = document.rfind(')')?;
    let fields: Vec<&str> = document.get(end + 1..)?.split_whitespace().collect();
    let cpu_ticks = fields
        .get(11)?
        .parse::<u64>()
        .ok()?
        .saturating_add(fields.get(12)?.parse::<u64>().ok()?);
    let start_ticks = fields.get(19)?.parse::<u64>().ok()?;
    Some((cpu_ticks, start_ticks))
}

pub fn parse_statm_rss_bytes(document: &str, page_size: u64) -> Option<u64> {
    document
        .split_whitespace()
        .nth(1)?
        .parse::<u64>()
        .ok()?
        .checked_mul(page_size)
}

fn process_rss_bytes(pid: u32) -> Option<u64> {
    let document = fs::read_to_string(format!("/proc/{pid}/statm")).ok()?;
    parse_statm_rss_bytes(&document, page_size()?)
}

#[cfg(unix)]
fn clock_ticks_per_second() -> Option<u64> {
    // SAFETY: sysconf is read-only and the queried name has no pointer arguments.
    let value = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    (value > 0).then_some(value as u64)
}

#[cfg(not(unix))]
fn clock_ticks_per_second() -> Option<u64> {
    None
}

#[cfg(unix)]
fn page_size() -> Option<u64> {
    // SAFETY: sysconf is read-only and the queried name has no pointer arguments.
    let value = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    (value > 0).then_some(value as u64)
}

#[cfg(not(unix))]
fn page_size() -> Option<u64> {
    None
}

fn valid_interface(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

pub const fn uptime_seconds(value: Duration) -> u64 {
    value.as_secs()
}
