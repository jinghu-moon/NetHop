use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use thiserror::Error;

#[cfg(feature = "subscription-update")]
use crate::{SourceBodyOrigin, SourceUpdateReport};
use crate::{
    scheduler::{ScheduleKey, ScheduleRecord, ScheduleStore, SchedulerError},
    stats::CounterDeltaBatch,
};
#[cfg(feature = "subscription-update")]
use nethop_subscription::SourceId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsBucket {
    pub bucket_start: i64,
    pub core_instance_id: String,
    pub counter_name: String,
    pub upload_bytes: u64,
    pub download_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrafficTotal {
    pub counter_name: String,
    pub upload_bytes: u64,
    pub download_bytes: u64,
}

#[derive(Debug, Error)]
pub enum StatsStoreError {
    #[error("stats database path is invalid")]
    InvalidPath,
    #[error("stats bucket timestamp is invalid")]
    InvalidBucket,
    #[error("stats byte counter exceeds SQLite integer range")]
    BytesOutOfRange,
    #[error("stats database operation failed")]
    Database(#[source] rusqlite::Error),
}

#[derive(Debug)]
pub struct StatsStore {
    connection: Connection,
    path: PathBuf,
}

#[cfg(feature = "subscription-update")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceHealth {
    Never,
    Healthy,
    Degraded,
    Failed,
}

#[cfg(feature = "subscription-update")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceStatus {
    pub source_id: String,
    pub health: SourceHealth,
    pub last_attempt_wall_seconds: Option<i64>,
    pub last_success_wall_seconds: Option<i64>,
    pub next_update_wall_seconds: Option<i64>,
    pub generation: Option<u64>,
    pub accepted: u64,
    pub duplicate: u64,
    pub rejected: u64,
    pub warnings: u64,
    pub subscription_upload_bytes: Option<u64>,
    pub subscription_download_bytes: Option<u64>,
    pub subscription_total_bytes: Option<u64>,
    pub subscription_expire_at: Option<i64>,
    pub using_last_known_good: bool,
    pub diagnostic_code: Option<String>,
}

#[cfg(feature = "subscription-update")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceUpdateHistoryEntry {
    pub source_id: String,
    pub attempted_at_wall_seconds: i64,
    pub health: SourceHealth,
    pub generation: Option<u64>,
    pub accepted: u64,
    pub duplicate: u64,
    pub rejected: u64,
    pub warnings: u64,
    pub using_last_known_good: bool,
    pub diagnostic_code: Option<String>,
}

#[cfg(feature = "subscription-update")]
#[derive(Debug)]
pub struct SourceStatusStore {
    connection: Connection,
}

impl StatsStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, StatsStoreError> {
        let path = path.into();
        validate_path(&path)?;
        let connection = Connection::open(&path).map_err(StatsStoreError::Database)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(1))
            .map_err(StatsStoreError::Database)?;
        initialize(&connection)?;
        set_private_mode(&path).map_err(|_| StatsStoreError::InvalidPath)?;
        Ok(Self { connection, path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn apply_delta(
        &mut self,
        bucket_start: i64,
        delta: &CounterDeltaBatch,
    ) -> Result<(), StatsStoreError> {
        if bucket_start < 0 {
            return Err(StatsStoreError::InvalidBucket);
        }
        let transaction = self
            .connection
            .transaction()
            .map_err(StatsStoreError::Database)?;
        for counter in delta.counters() {
            let upload = i64::try_from(counter.upload_bytes())
                .map_err(|_| StatsStoreError::BytesOutOfRange)?;
            let download = i64::try_from(counter.download_bytes())
                .map_err(|_| StatsStoreError::BytesOutOfRange)?;
            transaction
                .execute(
                    "INSERT INTO stats_bucket
                       (bucket_start, core_instance_id, counter_name, upload_bytes, download_bytes)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(bucket_start, core_instance_id, counter_name)
                     DO UPDATE SET upload_bytes = upload_bytes + excluded.upload_bytes,
                                   download_bytes = download_bytes + excluded.download_bytes",
                    params![
                        bucket_start,
                        delta.core_instance_id(),
                        counter.name().as_wire_name(),
                        upload,
                        download,
                    ],
                )
                .map_err(StatsStoreError::Database)?;
        }
        if delta.attribution_degraded_delta() > 0 {
            let degraded = i64::try_from(delta.attribution_degraded_delta())
                .map_err(|_| StatsStoreError::BytesOutOfRange)?;
            transaction
                .execute(
                    "INSERT INTO stats_degraded_bucket(bucket_start, degraded_count)
                     VALUES (?1, ?2)
                     ON CONFLICT(bucket_start)
                     DO UPDATE SET degraded_count = degraded_count + excluded.degraded_count",
                    params![bucket_start, degraded],
                )
                .map_err(StatsStoreError::Database)?;
        }
        transaction.commit().map_err(StatsStoreError::Database)
    }

    pub fn bucket(
        &self,
        bucket_start: i64,
        core_instance_id: &str,
        counter_name: &str,
    ) -> Result<Option<StatsBucket>, StatsStoreError> {
        self.connection
            .query_row(
                "SELECT bucket_start, core_instance_id, counter_name, upload_bytes, download_bytes
                 FROM stats_bucket
                 WHERE bucket_start = ?1 AND core_instance_id = ?2 AND counter_name = ?3",
                params![bucket_start, core_instance_id, counter_name],
                |row| {
                    let upload: i64 = row.get(3)?;
                    let download: i64 = row.get(4)?;
                    Ok(StatsBucket {
                        bucket_start: row.get(0)?,
                        core_instance_id: row.get(1)?,
                        counter_name: row.get(2)?,
                        upload_bytes: upload.max(0) as u64,
                        download_bytes: download.max(0) as u64,
                    })
                },
            )
            .optional()
            .map_err(StatsStoreError::Database)
    }

    pub fn traffic_totals_since(
        &self,
        since_wall_seconds: i64,
        limit: u8,
    ) -> Result<Vec<TrafficTotal>, StatsStoreError> {
        if since_wall_seconds < 0 || !(1..=128).contains(&limit) {
            return Err(StatsStoreError::InvalidBucket);
        }
        let mut statement = self
            .connection
            .prepare(
                "SELECT counter_name, SUM(upload_bytes), SUM(download_bytes)
                 FROM stats_bucket
                 WHERE bucket_start >= ?1
                 GROUP BY counter_name
                 ORDER BY SUM(upload_bytes) + SUM(download_bytes) DESC, counter_name
                 LIMIT ?2",
            )
            .map_err(StatsStoreError::Database)?;
        let rows = statement
            .query_map(params![since_wall_seconds, limit], |row| {
                let upload: i64 = row.get(1)?;
                let download: i64 = row.get(2)?;
                Ok(TrafficTotal {
                    counter_name: row.get(0)?,
                    upload_bytes: upload.max(0) as u64,
                    download_bytes: download.max(0) as u64,
                })
            })
            .map_err(StatsStoreError::Database)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StatsStoreError::Database)
    }
}

fn initialize(connection: &Connection) -> Result<(), StatsStoreError> {
    connection
        .execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS stats_bucket (
               bucket_start INTEGER NOT NULL,
               core_instance_id TEXT NOT NULL,
               counter_name TEXT NOT NULL,
               upload_bytes INTEGER NOT NULL,
               download_bytes INTEGER NOT NULL,
               PRIMARY KEY (bucket_start, core_instance_id, counter_name)
             );
             CREATE TABLE IF NOT EXISTS stats_degraded_bucket (
               bucket_start INTEGER PRIMARY KEY,
               degraded_count INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS stats_bucket_time_idx
               ON stats_bucket(bucket_start);
             CREATE TABLE IF NOT EXISTS schedule (
               schedule_key TEXT PRIMARY KEY,
               next_run_wall_seconds INTEGER NOT NULL,
               failure_count INTEGER NOT NULL,
               last_observed_wall_seconds INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS source_status (
               source_id TEXT PRIMARY KEY,
               health TEXT NOT NULL,
               last_attempt_wall_seconds INTEGER NOT NULL,
               last_success_wall_seconds INTEGER,
               generation INTEGER,
               accepted INTEGER NOT NULL,
               duplicate_count INTEGER NOT NULL,
               rejected INTEGER NOT NULL,
               warnings INTEGER NOT NULL,
               subscription_upload_bytes INTEGER,
               subscription_download_bytes INTEGER,
               subscription_total_bytes INTEGER,
               subscription_expire_at INTEGER,
               using_last_known_good INTEGER NOT NULL,
               diagnostic_code TEXT
             );
             CREATE TABLE IF NOT EXISTS source_update_history (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               source_id TEXT NOT NULL,
               attempted_at_wall_seconds INTEGER NOT NULL,
               health TEXT NOT NULL,
               generation INTEGER,
               accepted INTEGER NOT NULL,
               duplicate_count INTEGER NOT NULL,
               rejected INTEGER NOT NULL,
               warnings INTEGER NOT NULL,
               using_last_known_good INTEGER NOT NULL,
               diagnostic_code TEXT
             );
             CREATE INDEX IF NOT EXISTS source_update_history_time_idx
               ON source_update_history(attempted_at_wall_seconds DESC, id DESC);",
        )
        .map_err(StatsStoreError::Database)?;
    ensure_source_status_metadata_columns(connection)
}

fn ensure_source_status_metadata_columns(connection: &Connection) -> Result<(), StatsStoreError> {
    for column in [
        "subscription_upload_bytes",
        "subscription_download_bytes",
        "subscription_total_bytes",
        "subscription_expire_at",
    ] {
        let mut statement = connection
            .prepare("PRAGMA table_info(source_status)")
            .map_err(StatsStoreError::Database)?;
        let exists = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(StatsStoreError::Database)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StatsStoreError::Database)?
            .iter()
            .any(|name| name == column);
        if !exists {
            connection
                .execute(
                    &format!("ALTER TABLE source_status ADD COLUMN {column} INTEGER"),
                    [],
                )
                .map_err(StatsStoreError::Database)?;
        }
    }
    Ok(())
}

#[cfg(feature = "subscription-update")]
impl SourceStatusStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, StatsStoreError> {
        let path = path.into();
        validate_path(&path)?;
        let connection = Connection::open(&path).map_err(StatsStoreError::Database)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(1))
            .map_err(StatsStoreError::Database)?;
        initialize(&connection)?;
        set_private_mode(&path).map_err(|_| StatsStoreError::InvalidPath)?;
        Ok(Self { connection })
    }

    pub fn record_report(
        &mut self,
        wall_seconds: i64,
        report: &SourceUpdateReport,
    ) -> Result<(), StatsStoreError> {
        if wall_seconds < 0 {
            return Err(StatsStoreError::InvalidBucket);
        }
        let generation =
            i64::try_from(report.generation.get()).map_err(|_| StatsStoreError::BytesOutOfRange)?;
        let transaction = self
            .connection
            .transaction()
            .map_err(StatsStoreError::Database)?;
        for detail in &report.sources {
            let accepted = count_i64(detail.accepted)?;
            let duplicate = count_i64(detail.duplicate)?;
            let rejected = count_i64(detail.rejected)?;
            let warnings = count_i64(detail.warnings)?;
            let subscription_upload = optional_count_i64(
                detail
                    .subscription_userinfo
                    .and_then(|info| info.upload_bytes()),
            )?;
            let subscription_download = optional_count_i64(
                detail
                    .subscription_userinfo
                    .and_then(|info| info.download_bytes()),
            )?;
            let subscription_total = optional_count_i64(
                detail
                    .subscription_userinfo
                    .and_then(|info| info.total_bytes()),
            )?;
            let subscription_expire = detail
                .subscription_userinfo
                .and_then(|info| info.expire_at());
            let replace_subscription_info = matches!(
                detail.origin,
                Some(SourceBodyOrigin::Fresh | SourceBodyOrigin::NotModified)
            );
            let success =
                detail.diagnostic_code.is_none() && detail.accepted + detail.duplicate > 0;
            let using_last_known_good = detail.origin == Some(SourceBodyOrigin::LastKnownGood);
            let health = if !success {
                "failed"
            } else if using_last_known_good {
                "degraded"
            } else {
                "healthy"
            };
            let advance_success = success && !using_last_known_good;
            transaction
                .execute(
                    "INSERT INTO source_status
                       (source_id, health, last_attempt_wall_seconds,
                        last_success_wall_seconds, generation, accepted,
                        duplicate_count, rejected, warnings,
                        subscription_upload_bytes, subscription_download_bytes,
                        subscription_total_bytes, subscription_expire_at,
                        using_last_known_good, diagnostic_code)
                     VALUES (?1, ?2, ?3, CASE WHEN ?4 THEN ?3 ELSE NULL END,
                             ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
                     ON CONFLICT(source_id) DO UPDATE SET
                       health = excluded.health,
                       last_attempt_wall_seconds = excluded.last_attempt_wall_seconds,
                       last_success_wall_seconds = CASE WHEN ?4
                         THEN excluded.last_attempt_wall_seconds
                         ELSE source_status.last_success_wall_seconds END,
                       generation = excluded.generation,
                       accepted = excluded.accepted,
                       duplicate_count = excluded.duplicate_count,
                       rejected = excluded.rejected,
                       warnings = excluded.warnings,
                       subscription_upload_bytes = CASE WHEN ?16
                         THEN excluded.subscription_upload_bytes
                         ELSE source_status.subscription_upload_bytes END,
                       subscription_download_bytes = CASE WHEN ?16
                         THEN excluded.subscription_download_bytes
                         ELSE source_status.subscription_download_bytes END,
                       subscription_total_bytes = CASE WHEN ?16
                         THEN excluded.subscription_total_bytes
                         ELSE source_status.subscription_total_bytes END,
                       subscription_expire_at = CASE WHEN ?16
                         THEN excluded.subscription_expire_at
                         ELSE source_status.subscription_expire_at END,
                       using_last_known_good = excluded.using_last_known_good,
                       diagnostic_code = excluded.diagnostic_code",
                    params![
                        detail.source_id.as_str(),
                        health,
                        wall_seconds,
                        advance_success,
                        generation,
                        accepted,
                        duplicate,
                        rejected,
                        warnings,
                        subscription_upload,
                        subscription_download,
                        subscription_total,
                        subscription_expire,
                        using_last_known_good,
                        detail.diagnostic_code.as_deref(),
                        replace_subscription_info,
                    ],
                )
                .map_err(StatsStoreError::Database)?;
            transaction
                .execute(
                    "INSERT INTO source_update_history
                       (source_id, attempted_at_wall_seconds, health, generation,
                        accepted, duplicate_count, rejected, warnings,
                        using_last_known_good, diagnostic_code)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        detail.source_id.as_str(),
                        wall_seconds,
                        health,
                        generation,
                        accepted,
                        duplicate,
                        rejected,
                        warnings,
                        using_last_known_good,
                        detail.diagnostic_code.as_deref(),
                    ],
                )
                .map_err(StatsStoreError::Database)?;
        }
        prune_source_history(&transaction)?;
        transaction.commit().map_err(StatsStoreError::Database)
    }

    pub fn record_failure<I, S>(
        &mut self,
        wall_seconds: i64,
        source_ids: I,
        diagnostic_code: &str,
    ) -> Result<(), StatsStoreError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        if wall_seconds < 0
            || diagnostic_code.is_empty()
            || diagnostic_code.len() > 64
            || !diagnostic_code
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
        {
            return Err(StatsStoreError::InvalidBucket);
        }
        let transaction = self
            .connection
            .transaction()
            .map_err(StatsStoreError::Database)?;
        for source_id in source_ids {
            let source_id =
                SourceId::new(source_id.as_ref()).map_err(|_| StatsStoreError::InvalidBucket)?;
            transaction
                .execute(
                    "INSERT INTO source_status
                       (source_id, health, last_attempt_wall_seconds,
                        last_success_wall_seconds, generation, accepted,
                        duplicate_count, rejected, warnings,
                        using_last_known_good, diagnostic_code)
                     VALUES (?1, 'failed', ?2, NULL, NULL, 0, 0, 0, 0, 0, ?3)
                     ON CONFLICT(source_id) DO UPDATE SET
                       health = 'failed',
                       last_attempt_wall_seconds = excluded.last_attempt_wall_seconds,
                       generation = NULL,
                       accepted = 0,
                       duplicate_count = 0,
                       rejected = 0,
                       warnings = 0,
                       using_last_known_good = 0,
                       diagnostic_code = excluded.diagnostic_code",
                    params![source_id.as_str(), wall_seconds, diagnostic_code],
                )
                .map_err(StatsStoreError::Database)?;
            transaction
                .execute(
                    "INSERT INTO source_update_history
                       (source_id, attempted_at_wall_seconds, health, generation,
                        accepted, duplicate_count, rejected, warnings,
                        using_last_known_good, diagnostic_code)
                     VALUES (?1, ?2, 'failed', NULL, 0, 0, 0, 0, 0, ?3)",
                    params![source_id.as_str(), wall_seconds, diagnostic_code],
                )
                .map_err(StatsStoreError::Database)?;
        }
        prune_source_history(&transaction)?;
        transaction.commit().map_err(StatsStoreError::Database)
    }

    pub fn statuses<I, S>(&self, source_ids: I) -> Result<Vec<SourceStatus>, StatsStoreError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut statuses = Vec::new();
        for source_id in source_ids {
            let source_id =
                SourceId::new(source_id.as_ref()).map_err(|_| StatsStoreError::InvalidBucket)?;
            let next_update = self
                .connection
                .query_row(
                    "SELECT next_run_wall_seconds FROM schedule WHERE schedule_key = ?1",
                    [source_id.as_str()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(StatsStoreError::Database)?;
            let status = self
                .connection
                .query_row(
                    "SELECT health, last_attempt_wall_seconds,
                            last_success_wall_seconds, generation, accepted,
                            duplicate_count, rejected, warnings,
                            subscription_upload_bytes, subscription_download_bytes,
                            subscription_total_bytes, subscription_expire_at,
                            using_last_known_good, diagnostic_code
                     FROM source_status WHERE source_id = ?1",
                    [source_id.as_str()],
                    |row| {
                        let health: String = row.get(0)?;
                        Ok(SourceStatus {
                            source_id: source_id.as_str().to_owned(),
                            health: health_from_db(&health),
                            last_attempt_wall_seconds: Some(row.get(1)?),
                            last_success_wall_seconds: row.get(2)?,
                            next_update_wall_seconds: next_update,
                            generation: row.get::<_, Option<i64>>(3)?.map(|value| value as u64),
                            accepted: row.get::<_, i64>(4)?.max(0) as u64,
                            duplicate: row.get::<_, i64>(5)?.max(0) as u64,
                            rejected: row.get::<_, i64>(6)?.max(0) as u64,
                            warnings: row.get::<_, i64>(7)?.max(0) as u64,
                            subscription_upload_bytes: optional_non_negative(row.get(8)?),
                            subscription_download_bytes: optional_non_negative(row.get(9)?),
                            subscription_total_bytes: optional_non_negative(row.get(10)?),
                            subscription_expire_at: row.get(11)?,
                            using_last_known_good: row.get(12)?,
                            diagnostic_code: row.get(13)?,
                        })
                    },
                )
                .optional()
                .map_err(StatsStoreError::Database)?
                .unwrap_or(SourceStatus {
                    source_id: source_id.as_str().to_owned(),
                    health: SourceHealth::Never,
                    last_attempt_wall_seconds: None,
                    last_success_wall_seconds: None,
                    next_update_wall_seconds: next_update,
                    generation: None,
                    accepted: 0,
                    duplicate: 0,
                    rejected: 0,
                    warnings: 0,
                    subscription_upload_bytes: None,
                    subscription_download_bytes: None,
                    subscription_total_bytes: None,
                    subscription_expire_at: None,
                    using_last_known_good: false,
                    diagnostic_code: None,
                });
            statuses.push(status);
        }
        Ok(statuses)
    }

    pub fn history<I, S>(
        &self,
        source_ids: I,
        limit: u16,
    ) -> Result<Vec<SourceUpdateHistoryEntry>, StatsStoreError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        if !(1..=128).contains(&limit) {
            return Err(StatsStoreError::InvalidBucket);
        }
        let source_ids = source_ids
            .into_iter()
            .map(|value| {
                SourceId::new(value.as_ref())
                    .map(|source_id| source_id.as_str().to_owned())
                    .map_err(|_| StatsStoreError::InvalidBucket)
            })
            .collect::<Result<HashSet<_>, _>>()?;
        if source_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut statement = self
            .connection
            .prepare(
                "SELECT source_id, attempted_at_wall_seconds, health, generation,
                        accepted, duplicate_count, rejected, warnings,
                        using_last_known_good, diagnostic_code
                 FROM source_update_history ORDER BY id DESC LIMIT 512",
            )
            .map_err(StatsStoreError::Database)?;
        let rows = statement
            .query_map([], |row| {
                let health: String = row.get(2)?;
                Ok(SourceUpdateHistoryEntry {
                    source_id: row.get(0)?,
                    attempted_at_wall_seconds: row.get(1)?,
                    health: health_from_db(&health),
                    generation: row
                        .get::<_, Option<i64>>(3)?
                        .map(|value| value.max(0) as u64),
                    accepted: row.get::<_, i64>(4)?.max(0) as u64,
                    duplicate: row.get::<_, i64>(5)?.max(0) as u64,
                    rejected: row.get::<_, i64>(6)?.max(0) as u64,
                    warnings: row.get::<_, i64>(7)?.max(0) as u64,
                    using_last_known_good: row.get(8)?,
                    diagnostic_code: row.get(9)?,
                })
            })
            .map_err(StatsStoreError::Database)?;
        let mut history = Vec::with_capacity(usize::from(limit));
        for row in rows {
            let entry = row.map_err(StatsStoreError::Database)?;
            if source_ids.contains(&entry.source_id) {
                history.push(entry);
                if history.len() == usize::from(limit) {
                    break;
                }
            }
        }
        Ok(history)
    }
}

#[cfg(feature = "subscription-update")]
fn prune_source_history(transaction: &rusqlite::Transaction<'_>) -> Result<(), StatsStoreError> {
    transaction
        .execute(
            "DELETE FROM source_update_history
             WHERE id NOT IN (SELECT id FROM source_update_history ORDER BY id DESC LIMIT 512)",
            [],
        )
        .map_err(StatsStoreError::Database)?;
    Ok(())
}

#[cfg(feature = "subscription-update")]
fn count_i64(value: usize) -> Result<i64, StatsStoreError> {
    i64::try_from(value).map_err(|_| StatsStoreError::BytesOutOfRange)
}

#[cfg(feature = "subscription-update")]
fn optional_count_i64(value: Option<u64>) -> Result<Option<i64>, StatsStoreError> {
    value
        .map(|value| i64::try_from(value).map_err(|_| StatsStoreError::BytesOutOfRange))
        .transpose()
}

#[cfg(feature = "subscription-update")]
fn optional_non_negative(value: Option<i64>) -> Option<u64> {
    value.map(|value| value.max(0) as u64)
}

#[cfg(feature = "subscription-update")]
fn health_from_db(value: &str) -> SourceHealth {
    match value {
        "healthy" => SourceHealth::Healthy,
        "degraded" => SourceHealth::Degraded,
        "failed" => SourceHealth::Failed,
        _ => SourceHealth::Never,
    }
}

impl ScheduleStore for StatsStore {
    fn load(&mut self) -> Result<Vec<ScheduleRecord>, SchedulerError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT schedule_key, next_run_wall_seconds, failure_count,
                        last_observed_wall_seconds
                 FROM schedule ORDER BY schedule_key",
            )
            .map_err(|_| SchedulerError::PersistenceFailed)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, u32>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .map_err(|_| SchedulerError::PersistenceFailed)?;
        let mut records = Vec::new();
        for row in rows {
            let (key, next_run, failure_count, last_observed) =
                row.map_err(|_| SchedulerError::PersistenceFailed)?;
            records.push(ScheduleRecord::from_persisted(
                ScheduleKey::new(key)?,
                next_run,
                failure_count,
                last_observed,
            )?);
        }
        Ok(records)
    }

    fn save(&mut self, record: &ScheduleRecord) -> Result<(), SchedulerError> {
        self.connection
            .execute(
                "INSERT INTO schedule
                   (schedule_key, next_run_wall_seconds, failure_count,
                    last_observed_wall_seconds)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(schedule_key) DO UPDATE SET
                   next_run_wall_seconds = excluded.next_run_wall_seconds,
                   failure_count = excluded.failure_count,
                   last_observed_wall_seconds = excluded.last_observed_wall_seconds",
                params![
                    record.key().as_str(),
                    record.next_run_wall_seconds(),
                    record.failure_count(),
                    record.last_observed_wall_seconds(),
                ],
            )
            .map(|_| ())
            .map_err(|_| SchedulerError::PersistenceFailed)
    }
}

fn validate_path(path: &Path) -> Result<(), StatsStoreError> {
    if !path.is_absolute() || path.file_name().is_none() {
        return Err(StatsStoreError::InvalidPath);
    }
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(StatsStoreError::InvalidPath);
        }
    }
    let parent = path.parent().ok_or(StatsStoreError::InvalidPath)?;
    let metadata = fs::symlink_metadata(parent).map_err(|_| StatsStoreError::InvalidPath)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(StatsStoreError::InvalidPath);
    }
    Ok(())
}

fn set_private_mode(_path: &Path) -> Result<(), std::io::Error> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(_path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
