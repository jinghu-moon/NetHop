use std::{
    fs,
    path::{Path, PathBuf},
};

use rusqlite::{Connection, OptionalExtension, params};
use thiserror::Error;

use crate::{
    scheduler::{ScheduleKey, ScheduleRecord, ScheduleStore, SchedulerError},
    stats::CounterDeltaBatch,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsBucket {
    pub bucket_start: i64,
    pub core_instance_id: String,
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

impl StatsStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, StatsStoreError> {
        let path = path.into();
        validate_path(&path)?;
        let connection = Connection::open(&path).map_err(StatsStoreError::Database)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(1))
            .map_err(StatsStoreError::Database)?;
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
                 );",
            )
            .map_err(StatsStoreError::Database)?;
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
