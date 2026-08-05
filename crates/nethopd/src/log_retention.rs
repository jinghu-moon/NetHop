use std::{
    fs,
    path::PathBuf,
    time::{Duration, SystemTime},
};

use thiserror::Error;

const CLEANUP_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_ENTRIES: usize = 1_024;

pub trait RuntimeLogRetention {
    fn configure(&mut self, retention_days: u8, now: Duration) -> Result<(), LogRetentionError>;
    fn next_wakeup_in(&self, now: Duration) -> Option<Duration>;
    fn run_due(&mut self, now: Duration) -> Result<(), LogRetentionError>;
}

#[derive(Debug, Default)]
pub struct UnavailableLogRetention;

impl RuntimeLogRetention for UnavailableLogRetention {
    fn configure(&mut self, _retention_days: u8, _now: Duration) -> Result<(), LogRetentionError> {
        Ok(())
    }

    fn next_wakeup_in(&self, _now: Duration) -> Option<Duration> {
        None
    }

    fn run_due(&mut self, _now: Duration) -> Result<(), LogRetentionError> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct FileLogRetention {
    directory: PathBuf,
    retention_days: u8,
    next_cleanup: Duration,
}

impl FileLogRetention {
    pub fn new(directory: impl Into<PathBuf>) -> Result<Self, LogRetentionError> {
        let directory = directory.into();
        if !directory.is_absolute() {
            return Err(LogRetentionError::InvalidDirectory);
        }
        let metadata =
            fs::symlink_metadata(&directory).map_err(|_| LogRetentionError::InvalidDirectory)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(LogRetentionError::InvalidDirectory);
        }
        Ok(Self {
            directory,
            retention_days: 7,
            next_cleanup: Duration::ZERO,
        })
    }

    fn cleanup(&self) -> Result<(), LogRetentionError> {
        let cutoff = SystemTime::now()
            .checked_sub(Duration::from_secs(
                u64::from(self.retention_days) * 24 * 60 * 60,
            ))
            .ok_or(LogRetentionError::Clock)?;
        let entries = fs::read_dir(&self.directory).map_err(|_| LogRetentionError::Read)?;
        for entry in entries.take(MAX_ENTRIES) {
            let entry = entry.map_err(|_| LogRetentionError::Read)?;
            let path = entry.path();
            if path.parent() != Some(self.directory.as_path())
                || path.extension().and_then(|value| value.to_str()) != Some("log")
            {
                continue;
            }
            let metadata = fs::symlink_metadata(&path).map_err(|_| LogRetentionError::Read)?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                continue;
            }
            let modified = metadata.modified().map_err(|_| LogRetentionError::Read)?;
            if modified < cutoff {
                fs::remove_file(&path).map_err(|_| LogRetentionError::Remove)?;
            }
        }
        Ok(())
    }
}

impl RuntimeLogRetention for FileLogRetention {
    fn configure(&mut self, retention_days: u8, now: Duration) -> Result<(), LogRetentionError> {
        if !(1..=30).contains(&retention_days) {
            return Err(LogRetentionError::InvalidPolicy);
        }
        self.retention_days = retention_days;
        self.cleanup()?;
        self.next_cleanup = now.saturating_add(CLEANUP_INTERVAL);
        Ok(())
    }

    fn next_wakeup_in(&self, now: Duration) -> Option<Duration> {
        Some(self.next_cleanup.saturating_sub(now))
    }

    fn run_due(&mut self, now: Duration) -> Result<(), LogRetentionError> {
        if now < self.next_cleanup {
            return Ok(());
        }
        self.cleanup()?;
        self.next_cleanup = now.saturating_add(CLEANUP_INTERVAL);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum LogRetentionError {
    #[error("log retention directory is invalid")]
    InvalidDirectory,
    #[error("log retention policy is invalid")]
    InvalidPolicy,
    #[error("system clock cannot represent the retention cutoff")]
    Clock,
    #[error("log directory could not be read safely")]
    Read,
    #[error("expired log could not be removed")]
    Remove,
}
