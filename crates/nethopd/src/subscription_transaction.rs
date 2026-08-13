//! Source/mode commit coordination and crash recovery metadata.
//!
//! The journal is deliberately smaller than the configuration model.  It records only
//! digests, generation numbers and daemon-owned relative staging names; subscription URLs,
//! node outbounds and credentials never cross this boundary.

use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
};

use nethop_subscription::Digest;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const JOURNAL_SCHEMA: u8 = 1;
const JOURNAL_FILE: &str = "subscription.commit.json";
const MAX_JOURNAL_BYTES: u64 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitPhase {
    Prepared,
    Checked,
    Sealed,
    Journaled,
    ConfigPublished,
    GenerationPublished,
    Committed,
}

impl CommitPhase {
    fn rank(self) -> u8 {
        match self {
            Self::Prepared => 0,
            Self::Checked => 1,
            Self::Sealed => 2,
            Self::Journaled => 3,
            Self::ConfigPublished => 4,
            Self::GenerationPublished => 5,
            Self::Committed => 6,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommitJournal {
    schema: u8,
    old_config_digest: String,
    new_config_digest: String,
    old_generation: Option<u64>,
    new_generation: u64,
    phase: CommitPhase,
    staged_generation: String,
    staged_config: String,
}

impl CommitJournal {
    pub fn new(
        old_config_digest: impl Into<String>,
        new_config_digest: impl Into<String>,
        old_generation: Option<u64>,
        new_generation: u64,
        staged_generation: impl Into<String>,
        staged_config: impl Into<String>,
    ) -> Result<Self, TransactionError> {
        let journal = Self {
            schema: JOURNAL_SCHEMA,
            old_config_digest: old_config_digest.into(),
            new_config_digest: new_config_digest.into(),
            old_generation,
            new_generation,
            phase: CommitPhase::Prepared,
            staged_generation: staged_generation.into(),
            staged_config: staged_config.into(),
        };
        journal.validate()?;
        Ok(journal)
    }

    pub fn phase(&self) -> CommitPhase {
        self.phase
    }

    pub fn old_config_digest(&self) -> &str {
        &self.old_config_digest
    }

    pub fn new_config_digest(&self) -> &str {
        &self.new_config_digest
    }

    pub fn old_generation(&self) -> Option<u64> {
        self.old_generation
    }

    pub fn new_generation(&self) -> u64 {
        self.new_generation
    }

    pub fn staged_generation(&self) -> &str {
        &self.staged_generation
    }

    pub fn staged_config(&self) -> &str {
        &self.staged_config
    }

    pub fn advance(&mut self, phase: CommitPhase) -> Result<(), TransactionError> {
        if phase.rank() < self.phase.rank() {
            return Err(TransactionError::PhaseRegression);
        }
        self.phase = phase;
        Ok(())
    }

    fn validate(&self) -> Result<(), TransactionError> {
        let digest = |value: &str| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        };
        if self.schema != JOURNAL_SCHEMA
            || !digest(&self.old_config_digest)
            || !digest(&self.new_config_digest)
            || self.new_generation == 0
            || self
                .old_generation
                .is_some_and(|generation| generation == 0)
            || !valid_staged_name(&self.staged_generation, ".candidate-")
            || !valid_staged_name(&self.staged_config, ".candidate-config-")
        {
            return Err(TransactionError::InvalidJournal);
        }
        Ok(())
    }
}

pub struct CommitJournalStore {
    root: PathBuf,
}

impl CommitJournalStore {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, TransactionError> {
        let root = root.into();
        let metadata = fs::symlink_metadata(&root).map_err(|_| TransactionError::InvalidRoot)?;
        if !root.is_absolute() || !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(TransactionError::InvalidRoot);
        }
        Ok(Self { root })
    }

    pub fn path(&self) -> PathBuf {
        self.root.join(JOURNAL_FILE)
    }

    pub fn load(&self) -> Result<Option<CommitJournal>, TransactionError> {
        let path = self.path();
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(TransactionError::ReadFailed),
        };
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() > MAX_JOURNAL_BYTES
            || !private_file(&metadata)
        {
            return Err(TransactionError::InvalidJournal);
        }
        let journal: CommitJournal =
            serde_json::from_slice(&fs::read(path).map_err(|_| TransactionError::ReadFailed)?)
                .map_err(|_| TransactionError::InvalidJournal)?;
        journal.validate()?;
        Ok(Some(journal))
    }

    pub fn write(&self, journal: &CommitJournal) -> Result<(), TransactionError> {
        journal.validate()?;
        let bytes = serde_json::to_vec(journal).map_err(|_| TransactionError::WriteFailed)?;
        if bytes.len() as u64 > MAX_JOURNAL_BYTES {
            return Err(TransactionError::InvalidJournal);
        }
        super::worker_config::atomic_write(&self.path(), &bytes)
            .map_err(|_| TransactionError::WriteFailed)
    }

    pub fn stage_config(
        &self,
        journal: &CommitJournal,
        bytes: &[u8],
    ) -> Result<(), TransactionError> {
        journal.validate()?;
        if bytes.is_empty()
            || bytes.len() > super::worker_config::MAX_CONFIG_BYTES as usize
            || Digest::sha256(bytes).hex() != journal.new_config_digest()
        {
            return Err(TransactionError::InvalidJournal);
        }
        super::worker_config::atomic_write(&self.root.join(journal.staged_config()), bytes)
            .map_err(|_| TransactionError::WriteFailed)
    }

    pub fn advance_and_write(
        &self,
        journal: &mut CommitJournal,
        phase: CommitPhase,
    ) -> Result<(), TransactionError> {
        let mut candidate = journal.clone();
        candidate.advance(phase)?;
        self.write(&candidate)?;
        *journal = candidate;
        Ok(())
    }

    pub fn complete(&self, journal: &mut CommitJournal) -> Result<(), TransactionError> {
        self.advance_and_write(journal, CommitPhase::Committed)?;
        remove_owned_staged(&self.root.join(journal.staged_config()))?;
        self.clear()
    }

    pub fn abort(&self, journal: &CommitJournal) -> Result<(), TransactionError> {
        remove_owned_staged(&self.root.join(journal.staged_config()))?;
        self.clear()
    }

    pub fn clear(&self) -> Result<(), TransactionError> {
        match fs::remove_file(self.path()) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(TransactionError::WriteFailed),
        }
    }

    pub fn recovery_action(
        &self,
        journal: &CommitJournal,
        current_generation: Option<u64>,
        staged_config_digest: Option<&str>,
    ) -> Result<RecoveryAction, TransactionError> {
        journal.validate()?;
        if journal.phase == CommitPhase::Committed {
            return Ok(RecoveryAction::ClearJournal);
        }
        if current_generation == Some(journal.new_generation)
            || journal.phase.rank() >= CommitPhase::GenerationPublished.rank()
        {
            if staged_config_digest == Some(journal.new_config_digest()) {
                return Ok(RecoveryAction::CompleteConfigPublish);
            }
            return Err(TransactionError::RecoveryBlocked);
        }
        Ok(RecoveryAction::DiscardStaged)
    }

    pub fn recover(
        &self,
        canonical_config: &PathBuf,
        current_generation: Option<u64>,
    ) -> Result<RecoveryAction, TransactionError> {
        let Some(mut journal) = self.load()? else {
            return Ok(RecoveryAction::ClearJournal);
        };
        let staged_config = self.root.join(journal.staged_config());
        let staged_digest = fs::read(&staged_config)
            .ok()
            .map(|bytes| Digest::sha256(&bytes).hex());
        let canonical_digest = fs::read(canonical_config)
            .ok()
            .map(|bytes| Digest::sha256(&bytes).hex());
        let action = if current_generation == Some(journal.new_generation()) {
            if canonical_digest.as_deref() == Some(journal.new_config_digest()) {
                RecoveryAction::ClearJournal
            } else if staged_digest.as_deref() == Some(journal.new_config_digest()) {
                RecoveryAction::CompleteConfigPublish
            } else {
                return Err(TransactionError::RecoveryBlocked);
            }
        } else {
            RecoveryAction::DiscardStaged
        };
        match action {
            RecoveryAction::DiscardStaged => {
                remove_owned_staged(&self.root.join(journal.staged_generation()))?;
                remove_owned_staged(
                    &self
                        .root
                        .join("generations")
                        .join(journal.new_generation().to_string()),
                )?;
                remove_owned_staged(&staged_config)?;
                self.clear()?;
            }
            RecoveryAction::CompleteConfigPublish => {
                let bytes =
                    fs::read(&staged_config).map_err(|_| TransactionError::RecoveryBlocked)?;
                if Digest::sha256(&bytes).hex() != journal.new_config_digest() {
                    return Err(TransactionError::RecoveryBlocked);
                }
                super::worker_config::atomic_write(canonical_config, &bytes)
                    .map_err(|_| TransactionError::WriteFailed)?;
                journal.advance(CommitPhase::Committed)?;
                self.write(&journal)?;
                remove_owned_staged(&staged_config)?;
                self.clear()?;
            }
            RecoveryAction::ClearJournal => {
                remove_owned_staged(&staged_config)?;
                self.clear()?;
            }
        }
        Ok(action)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryAction {
    DiscardStaged,
    CompleteConfigPublish,
    ClearJournal,
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum TransactionError {
    #[error("transaction root is invalid")]
    InvalidRoot,
    #[error("commit journal is invalid")]
    InvalidJournal,
    #[error("commit phase regressed")]
    PhaseRegression,
    #[error("commit journal read failed")]
    ReadFailed,
    #[error("commit journal write failed")]
    WriteFailed,
    #[error("recovery requires manual intervention")]
    RecoveryBlocked,
}

#[derive(Clone, Default)]
pub struct MutationCoordinator {
    lock: Arc<Mutex<()>>,
}

impl MutationCoordinator {
    pub fn acquire(&self) -> Result<MutationGuard<'_>, TransactionError> {
        self.lock
            .lock()
            .map(MutationGuard)
            .map_err(|_| TransactionError::RecoveryBlocked)
    }
}

pub struct MutationGuard<'a>(MutexGuard<'a, ()>);

impl<'a> MutationGuard<'a> {
    pub fn held(&self) -> bool {
        let _ = &self.0;
        true
    }
}

fn valid_staged_name(value: &str, prefix: &str) -> bool {
    value.starts_with(prefix)
        && value.len() <= 128
        && value[prefix.len()..]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn remove_owned_staged(path: &PathBuf) -> Result<(), TransactionError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(TransactionError::WriteFailed),
    };
    if metadata.file_type().is_symlink() {
        return Err(TransactionError::InvalidJournal);
    }
    if metadata.is_dir() {
        fs::remove_dir_all(path).map_err(|_| TransactionError::WriteFailed)
    } else if metadata.is_file() {
        fs::remove_file(path).map_err(|_| TransactionError::WriteFailed)
    } else {
        Err(TransactionError::InvalidJournal)
    }
}

#[cfg(unix)]
fn private_file(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o077 == 0
}

#[cfg(not(unix))]
fn private_file(_metadata: &fs::Metadata) -> bool {
    true
}
