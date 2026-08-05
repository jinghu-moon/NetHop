use std::collections::BTreeMap;

use thiserror::Error;

const MAX_KEY_BYTES: usize = 128;
const MAX_FAILURES: u32 = 16;
const DEFAULT_INTERVAL_SECONDS: i64 = 24 * 60 * 60;
const DEFAULT_FAILURE_BACKOFF_SECONDS: i64 = 60 * 60;
const DEFAULT_MAX_BACKOFF_SECONDS: i64 = 24 * 60 * 60;
const DEFAULT_JITTER_SECONDS: i64 = 15 * 60;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ScheduleKey(String);

impl ScheduleKey {
    pub fn new(value: impl Into<String>) -> Result<Self, SchedulerError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_KEY_BYTES
            || value.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err(SchedulerError::InvalidKey);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchedulePolicy {
    interval_seconds: i64,
    failure_backoff_seconds: i64,
    max_backoff_seconds: i64,
    jitter_seconds: i64,
}

impl SchedulePolicy {
    pub fn new(
        interval_seconds: i64,
        failure_backoff_seconds: i64,
        max_backoff_seconds: i64,
        jitter_seconds: i64,
    ) -> Result<Self, SchedulerError> {
        if interval_seconds <= 0
            || failure_backoff_seconds <= 0
            || max_backoff_seconds < failure_backoff_seconds
            || jitter_seconds < 0
            || jitter_seconds > interval_seconds / 2
        {
            return Err(SchedulerError::InvalidPolicy);
        }
        Ok(Self {
            interval_seconds,
            failure_backoff_seconds,
            max_backoff_seconds,
            jitter_seconds,
        })
    }

    pub const fn interval_seconds(self) -> i64 {
        self.interval_seconds
    }
}

impl Default for SchedulePolicy {
    fn default() -> Self {
        Self {
            interval_seconds: DEFAULT_INTERVAL_SECONDS,
            failure_backoff_seconds: DEFAULT_FAILURE_BACKOFF_SECONDS,
            max_backoff_seconds: DEFAULT_MAX_BACKOFF_SECONDS,
            jitter_seconds: DEFAULT_JITTER_SECONDS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleRecord {
    key: ScheduleKey,
    next_run_wall_seconds: i64,
    failure_count: u32,
    last_observed_wall_seconds: i64,
}

impl ScheduleRecord {
    pub(crate) fn from_persisted(
        key: ScheduleKey,
        next_run_wall_seconds: i64,
        failure_count: u32,
        last_observed_wall_seconds: i64,
    ) -> Result<Self, SchedulerError> {
        if next_run_wall_seconds < 0
            || last_observed_wall_seconds < 0
            || failure_count > MAX_FAILURES
        {
            return Err(SchedulerError::PersistenceFailed);
        }
        Ok(Self {
            key,
            next_run_wall_seconds,
            failure_count,
            last_observed_wall_seconds,
        })
    }

    pub fn key(&self) -> &ScheduleKey {
        &self.key
    }

    pub const fn next_run_wall_seconds(&self) -> i64 {
        self.next_run_wall_seconds
    }

    pub const fn failure_count(&self) -> u32 {
        self.failure_count
    }

    pub(crate) const fn last_observed_wall_seconds(&self) -> i64 {
        self.last_observed_wall_seconds
    }
}

pub trait ScheduleStore {
    fn load(&mut self) -> Result<Vec<ScheduleRecord>, SchedulerError>;
    fn save(&mut self, record: &ScheduleRecord) -> Result<(), SchedulerError>;
}

#[derive(Debug, Default, Clone)]
pub struct InMemoryScheduleStore {
    records: BTreeMap<ScheduleKey, ScheduleRecord>,
}

impl ScheduleStore for InMemoryScheduleStore {
    fn load(&mut self) -> Result<Vec<ScheduleRecord>, SchedulerError> {
        Ok(self.records.values().cloned().collect())
    }

    fn save(&mut self, record: &ScheduleRecord) -> Result<(), SchedulerError> {
        self.records.insert(record.key.clone(), record.clone());
        Ok(())
    }
}

#[derive(Debug)]
pub struct SchedulerEngine<S> {
    store: S,
    policy: SchedulePolicy,
    records: BTreeMap<ScheduleKey, ScheduleRecord>,
}

impl<S> SchedulerEngine<S>
where
    S: ScheduleStore,
{
    pub fn load(mut store: S, policy: SchedulePolicy) -> Result<Self, SchedulerError> {
        let records = store
            .load()?
            .into_iter()
            .map(|record| (record.key.clone(), record))
            .collect();
        Ok(Self {
            store,
            policy,
            records,
        })
    }

    pub fn ensure(
        &mut self,
        key: ScheduleKey,
        now_wall_seconds: i64,
    ) -> Result<&ScheduleRecord, SchedulerError> {
        validate_wall_time(now_wall_seconds)?;
        if !self.records.contains_key(&key) {
            let record = ScheduleRecord {
                key: key.clone(),
                next_run_wall_seconds: now_wall_seconds,
                failure_count: 0,
                last_observed_wall_seconds: now_wall_seconds,
            };
            self.store.save(&record)?;
            self.records.insert(key.clone(), record);
        }
        self.records.get(&key).ok_or(SchedulerError::MissingRecord)
    }

    pub fn due(&mut self, now_wall_seconds: i64) -> Result<Vec<ScheduleKey>, SchedulerError> {
        validate_wall_time(now_wall_seconds)?;
        let mut due = Vec::new();
        for record in self.records.values_mut() {
            if now_wall_seconds < record.last_observed_wall_seconds {
                return Err(SchedulerError::ClockRegressed);
            }
            record.last_observed_wall_seconds = now_wall_seconds;
            if now_wall_seconds >= record.next_run_wall_seconds {
                due.push(record.key.clone());
            }
        }
        Ok(due)
    }

    pub fn mark_success(
        &mut self,
        key: &ScheduleKey,
        now_wall_seconds: i64,
    ) -> Result<(), SchedulerError> {
        self.update(key, now_wall_seconds, false)
    }

    pub fn mark_failure(
        &mut self,
        key: &ScheduleKey,
        now_wall_seconds: i64,
    ) -> Result<(), SchedulerError> {
        self.update(key, now_wall_seconds, true)
    }

    pub fn record(&self, key: &ScheduleKey) -> Option<&ScheduleRecord> {
        self.records.get(key)
    }

    pub fn set_policy(&mut self, policy: SchedulePolicy) {
        self.policy = policy;
    }

    fn update(
        &mut self,
        key: &ScheduleKey,
        now_wall_seconds: i64,
        failure: bool,
    ) -> Result<(), SchedulerError> {
        validate_wall_time(now_wall_seconds)?;
        let record = self
            .records
            .get_mut(key)
            .ok_or(SchedulerError::MissingRecord)?;
        if now_wall_seconds < record.last_observed_wall_seconds {
            return Err(SchedulerError::ClockRegressed);
        }
        record.last_observed_wall_seconds = now_wall_seconds;
        if failure {
            record.failure_count = record.failure_count.saturating_add(1).min(MAX_FAILURES);
        } else {
            record.failure_count = 0;
        }
        let base = if failure {
            let exponent = record.failure_count.saturating_sub(1).min(6);
            self.policy
                .failure_backoff_seconds
                .saturating_mul(1_i64 << exponent)
                .min(self.policy.max_backoff_seconds)
        } else {
            self.policy.interval_seconds
        };
        let jitter = stable_jitter(key.as_str(), self.policy.jitter_seconds);
        record.next_run_wall_seconds = now_wall_seconds.saturating_add(base).saturating_add(jitter);
        self.store.save(record)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SchedulerError {
    #[error("schedule key is invalid")]
    InvalidKey,
    #[error("schedule policy is invalid")]
    InvalidPolicy,
    #[error("wall clock value is invalid")]
    InvalidWallTime,
    #[error("wall clock moved backwards")]
    ClockRegressed,
    #[error("schedule record is missing")]
    MissingRecord,
    #[error("schedule persistence failed")]
    PersistenceFailed,
}

fn validate_wall_time(value: i64) -> Result<(), SchedulerError> {
    (value >= 0)
        .then_some(())
        .ok_or(SchedulerError::InvalidWallTime)
}

fn stable_jitter(key: &str, max: i64) -> i64 {
    if max == 0 {
        return 0;
    }
    let mut hash = 2_166_136_261u32;
    for byte in key.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    let span = (max * 2 + 1) as u32;
    i64::from(hash % span) - max
}
