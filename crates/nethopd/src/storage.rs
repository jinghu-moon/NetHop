use std::{
    fs,
    path::{Path, PathBuf},
};

use rusqlite::{Connection, OptionalExtension, params};
use thiserror::Error;

use crate::stats::CounterDeltaBatch;

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
                   ON stats_bucket(bucket_start);",
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
