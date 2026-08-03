use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

const MAX_COUNTERS: usize = 4_096;
const MAX_CORE_INSTANCE_ID_BYTES: usize = 64;
const MAX_TERMINAL_TAG_BYTES: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum CounterName {
    InboundTproxy,
    InboundTun,
    RouteDirect,
    RouteBlock,
    Terminal(String),
}

impl CounterName {
    pub fn terminal(tag: impl Into<String>) -> Result<Self, StatsError> {
        let tag = tag.into();
        if tag.is_empty() || tag.len() > MAX_TERMINAL_TAG_BYTES || tag.chars().any(char::is_control)
        {
            return Err(StatsError::InvalidCounter);
        }
        Ok(Self::Terminal(tag))
    }

    pub fn as_wire_name(&self) -> String {
        match self {
            Self::InboundTproxy => "inbound:tproxy".to_owned(),
            Self::InboundTun => "inbound:tun".to_owned(),
            Self::RouteDirect => "route:direct".to_owned(),
            Self::RouteBlock => "route:block".to_owned(),
            Self::Terminal(tag) => format!("terminal:{tag}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CounterReading {
    name: CounterName,
    upload_bytes: u64,
    download_bytes: u64,
}

impl CounterReading {
    pub const fn new(name: CounterName, upload_bytes: u64, download_bytes: u64) -> Self {
        Self {
            name,
            upload_bytes,
            download_bytes,
        }
    }

    pub fn name(&self) -> &CounterName {
        &self.name
    }

    pub const fn upload_bytes(&self) -> u64 {
        self.upload_bytes
    }

    pub const fn download_bytes(&self) -> u64 {
        self.download_bytes
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CounterBatch {
    core_instance_id: String,
    attribution_degraded_total: u64,
    counters: Vec<CounterReading>,
}

impl CounterBatch {
    pub fn new(
        core_instance_id: impl Into<String>,
        attribution_degraded_total: u64,
        mut counters: Vec<CounterReading>,
    ) -> Result<Self, StatsError> {
        let core_instance_id = core_instance_id.into();
        if core_instance_id.is_empty()
            || core_instance_id.len() > MAX_CORE_INSTANCE_ID_BYTES
            || !core_instance_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err(StatsError::InvalidCoreInstance);
        }
        if counters.len() > MAX_COUNTERS {
            return Err(StatsError::TooManyCounters);
        }
        counters.sort_by(|left, right| left.name.cmp(&right.name));
        if counters.windows(2).any(|pair| pair[0].name == pair[1].name) {
            return Err(StatsError::DuplicateCounter);
        }
        Ok(Self {
            core_instance_id,
            attribution_degraded_total,
            counters,
        })
    }

    pub fn core_instance_id(&self) -> &str {
        &self.core_instance_id
    }

    pub const fn attribution_degraded_total(&self) -> u64 {
        self.attribution_degraded_total
    }

    pub fn counters(&self) -> &[CounterReading] {
        &self.counters
    }
}

pub trait CounterTransport {
    fn read_counters(&mut self) -> Result<CounterBatch, StatsError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CounterDelta {
    name: CounterName,
    upload_bytes: u64,
    download_bytes: u64,
}

impl CounterDelta {
    pub fn name(&self) -> &CounterName {
        &self.name
    }

    pub const fn upload_bytes(&self) -> u64 {
        self.upload_bytes
    }

    pub const fn download_bytes(&self) -> u64 {
        self.download_bytes
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CounterDeltaBatch {
    core_instance_id: String,
    attribution_degraded_delta: u64,
    counters: Vec<CounterDelta>,
    baseline_only: bool,
}

impl CounterDeltaBatch {
    pub fn core_instance_id(&self) -> &str {
        &self.core_instance_id
    }

    pub const fn attribution_degraded_delta(&self) -> u64 {
        self.attribution_degraded_delta
    }

    pub fn counters(&self) -> &[CounterDelta] {
        &self.counters
    }

    pub const fn baseline_only(&self) -> bool {
        self.baseline_only
    }
}

#[derive(Debug, Default, Clone)]
pub struct CounterDeltaTracker {
    active_core_instance: Option<String>,
    degraded_total: u64,
    last: BTreeMap<CounterName, (u64, u64)>,
}

impl CounterDeltaTracker {
    pub fn apply(&mut self, batch: CounterBatch) -> Result<CounterDeltaBatch, StatsError> {
        if self.active_core_instance.as_deref() != Some(batch.core_instance_id()) {
            self.active_core_instance = Some(batch.core_instance_id.clone());
            self.degraded_total = batch.attribution_degraded_total;
            self.last = batch
                .counters
                .iter()
                .map(|reading| {
                    (
                        reading.name.clone(),
                        (reading.upload_bytes, reading.download_bytes),
                    )
                })
                .collect();
            return Ok(CounterDeltaBatch {
                core_instance_id: batch.core_instance_id,
                attribution_degraded_delta: 0,
                counters: Vec::new(),
                baseline_only: true,
            });
        }
        if batch.attribution_degraded_total < self.degraded_total {
            return Err(StatsError::CounterRegressed);
        }
        let mut deltas = Vec::with_capacity(batch.counters.len());
        let mut next = BTreeMap::new();
        let mut observed = BTreeSet::new();
        for reading in &batch.counters {
            observed.insert(reading.name.clone());
            let (previous_upload, previous_download) =
                self.last.get(&reading.name).copied().unwrap_or((0, 0));
            if reading.upload_bytes < previous_upload || reading.download_bytes < previous_download
            {
                return Err(StatsError::CounterRegressed);
            }
            deltas.push(CounterDelta {
                name: reading.name.clone(),
                upload_bytes: reading.upload_bytes - previous_upload,
                download_bytes: reading.download_bytes - previous_download,
            });
            next.insert(
                reading.name.clone(),
                (reading.upload_bytes, reading.download_bytes),
            );
        }
        if self.last.keys().any(|name| !observed.contains(name)) {
            return Err(StatsError::CounterMissing);
        }
        let degraded_delta = batch.attribution_degraded_total - self.degraded_total;
        self.degraded_total = batch.attribution_degraded_total;
        self.last = next;
        Ok(CounterDeltaBatch {
            core_instance_id: batch.core_instance_id,
            attribution_degraded_delta: degraded_delta,
            counters: deltas,
            baseline_only: false,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum StatsError {
    #[error("core instance ID is invalid")]
    InvalidCoreInstance,
    #[error("counter name is invalid")]
    InvalidCounter,
    #[error("counter batch exceeds the bounded limit")]
    TooManyCounters,
    #[error("counter batch contains duplicate names")]
    DuplicateCounter,
    #[error("cumulative counter moved backwards")]
    CounterRegressed,
    #[error("a previously observed counter disappeared")]
    CounterMissing,
    #[error("counter transport is unavailable")]
    TransportUnavailable,
}
