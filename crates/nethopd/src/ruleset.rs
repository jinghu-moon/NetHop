use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use nethop_subscription::Digest;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::CandidateChecker;

const DEFAULT_MAX_RULE_SET_BYTES: usize = 5 * 1024 * 1024;
const MAX_RULE_SET_BYTES: usize = 64 * 1024 * 1024;
const DOMAIN_FILE: &str = "cn-domain.srs";
const IP_FILE: &str = "cn-ip.srs";
const JOURNAL_FILE: &str = ".ruleset-transaction.json";
const JOURNAL_SCHEMA: &str = "nethop-ruleset-transaction-v1";
static CANDIDATE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleSetLimits {
    max_bytes_per_file: usize,
}

impl RuleSetLimits {
    pub fn new(max_bytes_per_file: usize) -> Result<Self, RuleSetError> {
        if !(8..=MAX_RULE_SET_BYTES).contains(&max_bytes_per_file) {
            return Err(RuleSetError::InvalidPolicy);
        }
        Ok(Self { max_bytes_per_file })
    }

    pub const fn max_bytes_per_file(self) -> usize {
        self.max_bytes_per_file
    }
}

impl Default for RuleSetLimits {
    fn default() -> Self {
        Self {
            max_bytes_per_file: DEFAULT_MAX_RULE_SET_BYTES,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum RuleSetError {
    #[error("rule-set limits are outside the allowed bounds")]
    InvalidPolicy,
    #[error("rule-set root must be an absolute regular directory")]
    InvalidRoot,
    #[error("current rule-set files must be regular non-symlink files")]
    InvalidCurrent,
    #[error("rule-set candidate is not an SRS binary")]
    InvalidCandidate,
    #[error("rule-set candidate exceeds its size limit")]
    CandidateTooLarge,
    #[error("rule-set staging failed")]
    StageFailed,
    #[error("sing-box rejected the rule-set candidate")]
    CheckFailed,
    #[error("rule-set publication failed; the previous pair was restored")]
    PublishFailed,
    #[error("rule-set transaction belongs to another store")]
    ForeignTransaction,
    #[error("rule-set transaction commit failed")]
    CommitFailed,
    #[error("rule-set publication and rollback both failed")]
    RollbackFailed,
}

#[derive(Debug, Clone)]
pub struct RuleSetStore {
    root: PathBuf,
    limits: RuleSetLimits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleSetReplaceOutcome {
    Unchanged,
    Updated,
}

#[derive(Debug)]
pub enum RuleSetPreparation {
    Unchanged,
    Prepared(PreparedRuleSet),
}

#[derive(Debug)]
#[must_use = "a prepared rule-set transaction must be published or discarded"]
pub struct PreparedRuleSet {
    root: PathBuf,
    candidate: Option<PathBuf>,
    sequence: u64,
}

impl Drop for PreparedRuleSet {
    fn drop(&mut self) {
        if let Some(candidate) = self.candidate.as_ref() {
            let _ = fs::remove_dir_all(candidate);
        }
    }
}

#[derive(Debug)]
#[must_use = "a published rule-set transaction must be committed or rolled back"]
pub struct PublishedRuleSet {
    root: PathBuf,
    previous_domain: PathBuf,
    previous_ip: PathBuf,
    journal: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleSetJournal {
    schema: String,
    candidate: String,
    previous_domain: String,
    previous_ip: String,
}

impl RuleSetStore {
    pub fn open(root: impl Into<PathBuf>, limits: RuleSetLimits) -> Result<Self, RuleSetError> {
        let root = root.into();
        let metadata = fs::symlink_metadata(&root).map_err(|_| RuleSetError::InvalidRoot)?;
        if !root.is_absolute() || !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(RuleSetError::InvalidRoot);
        }
        let canonical = root.canonicalize().map_err(|_| RuleSetError::InvalidRoot)?;
        #[cfg(unix)]
        if canonical != root {
            return Err(RuleSetError::InvalidRoot);
        }
        recover_interrupted_transaction(&canonical)?;
        cleanup_stale_transactions(&canonical)?;
        let root = canonical;
        Ok(Self { root, limits })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn current_digests(&self) -> Result<(String, String), RuleSetError> {
        self.validate_current()?;
        let domain =
            fs::read(self.root.join(DOMAIN_FILE)).map_err(|_| RuleSetError::InvalidCurrent)?;
        let ip = fs::read(self.root.join(IP_FILE)).map_err(|_| RuleSetError::InvalidCurrent)?;
        Ok((Digest::sha256(&domain).hex(), Digest::sha256(&ip).hex()))
    }

    pub fn replace<C: CandidateChecker>(
        &self,
        cn_domain: &[u8],
        cn_ip: &[u8],
        checker: &C,
    ) -> Result<RuleSetReplaceOutcome, RuleSetError> {
        let prepared = match self.prepare(cn_domain, cn_ip, checker)? {
            RuleSetPreparation::Unchanged => return Ok(RuleSetReplaceOutcome::Unchanged),
            RuleSetPreparation::Prepared(prepared) => prepared,
        };
        let published = self.publish(prepared)?;
        self.commit(&published)?;
        Ok(RuleSetReplaceOutcome::Updated)
    }

    pub fn prepare<C: CandidateChecker>(
        &self,
        cn_domain: &[u8],
        cn_ip: &[u8],
        checker: &C,
    ) -> Result<RuleSetPreparation, RuleSetError> {
        self.validate_candidate(cn_domain)?;
        self.validate_candidate(cn_ip)?;
        self.validate_current()?;
        let current_domain =
            fs::read(self.root.join(DOMAIN_FILE)).map_err(|_| RuleSetError::InvalidCurrent)?;
        let current_ip =
            fs::read(self.root.join(IP_FILE)).map_err(|_| RuleSetError::InvalidCurrent)?;
        if current_domain == cn_domain && current_ip == cn_ip {
            return Ok(RuleSetPreparation::Unchanged);
        }

        let sequence = CANDIDATE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let candidate = self.root.join(format!(
            ".candidate-ruleset-{}-{sequence}",
            std::process::id()
        ));
        create_private_directory(&candidate).map_err(|_| RuleSetError::StageFailed)?;
        let result = self.stage_and_check(&candidate, cn_domain, cn_ip, checker);
        if let Err(error) = result {
            let _ = fs::remove_dir_all(&candidate);
            return Err(error);
        }
        Ok(RuleSetPreparation::Prepared(PreparedRuleSet {
            root: self.root.clone(),
            candidate: Some(candidate),
            sequence,
        }))
    }

    pub fn publish(&self, mut prepared: PreparedRuleSet) -> Result<PublishedRuleSet, RuleSetError> {
        if prepared.root != self.root {
            return Err(RuleSetError::ForeignTransaction);
        }
        let candidate = prepared
            .candidate
            .as_deref()
            .ok_or(RuleSetError::PublishFailed)?;
        let transaction = self.publish_candidate(candidate, prepared.sequence)?;
        let candidate = prepared
            .candidate
            .take()
            .expect("candidate was checked above");
        let _ = fs::remove_dir_all(candidate);
        Ok(transaction)
    }

    pub fn commit(&self, published: &PublishedRuleSet) -> Result<(), RuleSetError> {
        self.ensure_transaction_owner(&published.root)?;
        remove_journal(&published.journal, &self.root).map_err(|_| RuleSetError::CommitFailed)?;
        let _ = fs::remove_file(&published.previous_domain);
        let _ = fs::remove_file(&published.previous_ip);
        let _ = sync_directory(&self.root);
        Ok(())
    }

    pub fn rollback(&self, published: &PublishedRuleSet) -> Result<(), RuleSetError> {
        self.ensure_transaction_owner(&published.root)?;
        let current_domain = self.root.join(DOMAIN_FILE);
        let current_ip = self.root.join(IP_FILE);
        let outcome = restore_pair(
            &current_domain,
            &current_ip,
            &published.previous_domain,
            &published.previous_ip,
        );
        if outcome == RuleSetError::PublishFailed
            && remove_journal(&published.journal, &self.root).is_ok()
        {
            Ok(())
        } else {
            Err(RuleSetError::RollbackFailed)
        }
    }

    fn ensure_transaction_owner(&self, root: &Path) -> Result<(), RuleSetError> {
        if root == self.root {
            Ok(())
        } else {
            Err(RuleSetError::ForeignTransaction)
        }
    }

    fn validate_candidate(&self, bytes: &[u8]) -> Result<(), RuleSetError> {
        if bytes.len() > self.limits.max_bytes_per_file {
            return Err(RuleSetError::CandidateTooLarge);
        }
        if bytes.len() < 4 || !bytes.starts_with(b"SRS") {
            return Err(RuleSetError::InvalidCandidate);
        }
        Ok(())
    }

    fn validate_current(&self) -> Result<(), RuleSetError> {
        if is_regular_non_symlink(&self.root.join(DOMAIN_FILE))
            && is_regular_non_symlink(&self.root.join(IP_FILE))
        {
            Ok(())
        } else {
            Err(RuleSetError::InvalidCurrent)
        }
    }

    fn stage_and_check<C: CandidateChecker>(
        &self,
        candidate: &Path,
        cn_domain: &[u8],
        cn_ip: &[u8],
        checker: &C,
    ) -> Result<(), RuleSetError> {
        let domain_path = candidate.join(DOMAIN_FILE);
        let ip_path = candidate.join(IP_FILE);
        write_private_file(&domain_path, cn_domain).map_err(|_| RuleSetError::StageFailed)?;
        write_private_file(&ip_path, cn_ip).map_err(|_| RuleSetError::StageFailed)?;
        let domain = domain_path.to_str().ok_or(RuleSetError::InvalidRoot)?;
        let ip = ip_path.to_str().ok_or(RuleSetError::InvalidRoot)?;
        let config = serde_json::to_vec(&serde_json::json!({
            "outbounds": [{ "type": "direct", "tag": "direct" }],
            "route": {
                "rule_set": [
                    { "type": "local", "tag": "candidate-cn-domain", "format": "binary", "path": domain },
                    { "type": "local", "tag": "candidate-cn-ip", "format": "binary", "path": ip }
                ],
                "rules": [{
                    "rule_set": ["candidate-cn-domain", "candidate-cn-ip"],
                    "outbound": "direct"
                }],
                "final": "direct"
            }
        }))
        .map_err(|_| RuleSetError::StageFailed)?;
        let config_path = candidate.join("config.json");
        write_private_file(&config_path, &config).map_err(|_| RuleSetError::StageFailed)?;
        sync_directory(candidate).map_err(|_| RuleSetError::StageFailed)?;
        checker
            .check(&config_path)
            .map_err(|_| RuleSetError::CheckFailed)
    }

    fn publish_candidate(
        &self,
        candidate: &Path,
        sequence: u64,
    ) -> Result<PublishedRuleSet, RuleSetError> {
        let current_domain = self.root.join(DOMAIN_FILE);
        let current_ip = self.root.join(IP_FILE);
        let process = std::process::id();
        let previous_domain = self
            .root
            .join(format!(".previous-domain-{process}-{sequence}.srs"));
        let previous_ip = self
            .root
            .join(format!(".previous-ip-{process}-{sequence}.srs"));
        let journal_path = self.root.join(JOURNAL_FILE);
        if previous_domain.exists() || previous_ip.exists() {
            return Err(RuleSetError::PublishFailed);
        }

        let journal = RuleSetJournal {
            schema: JOURNAL_SCHEMA.to_owned(),
            candidate: file_name(candidate)?,
            previous_domain: file_name(&previous_domain)?,
            previous_ip: file_name(&previous_ip)?,
        };
        let journal = serde_json::to_vec(&journal).map_err(|_| RuleSetError::PublishFailed)?;
        write_private_file(&journal_path, &journal).map_err(|_| RuleSetError::PublishFailed)?;
        if sync_directory(&self.root).is_err() {
            let _ = fs::remove_file(&journal_path);
            return Err(RuleSetError::PublishFailed);
        }

        if fs::rename(&current_domain, &previous_domain).is_err() {
            let _ = remove_journal(&journal_path, &self.root);
            return Err(RuleSetError::PublishFailed);
        }
        if fs::rename(&current_ip, &previous_ip).is_err() {
            let restored = fs::rename(&previous_domain, &current_domain).is_ok();
            let journal_removed = remove_journal(&journal_path, &self.root).is_ok();
            return if restored && journal_removed {
                Err(RuleSetError::PublishFailed)
            } else {
                Err(RuleSetError::RollbackFailed)
            };
        }

        let candidate_domain = candidate.join(DOMAIN_FILE);
        let candidate_ip = candidate.join(IP_FILE);
        if fs::rename(&candidate_domain, &current_domain).is_err() {
            let error = restore_pair(&current_domain, &current_ip, &previous_domain, &previous_ip);
            let journal_removed = remove_journal(&journal_path, &self.root).is_ok();
            return Err(if error == RuleSetError::PublishFailed && journal_removed {
                RuleSetError::PublishFailed
            } else {
                RuleSetError::RollbackFailed
            });
        }
        if fs::rename(&candidate_ip, &current_ip).is_err() || sync_directory(&self.root).is_err() {
            let error = restore_pair(&current_domain, &current_ip, &previous_domain, &previous_ip);
            let journal_removed = remove_journal(&journal_path, &self.root).is_ok();
            return Err(if error == RuleSetError::PublishFailed && journal_removed {
                RuleSetError::PublishFailed
            } else {
                RuleSetError::RollbackFailed
            });
        }

        Ok(PublishedRuleSet {
            root: self.root.clone(),
            previous_domain,
            previous_ip,
            journal: journal_path,
        })
    }
}

fn restore_pair(
    current_domain: &Path,
    current_ip: &Path,
    previous_domain: &Path,
    previous_ip: &Path,
) -> RuleSetError {
    let _ = fs::remove_file(current_domain);
    let _ = fs::remove_file(current_ip);
    let domain_restored = fs::rename(previous_domain, current_domain).is_ok();
    let ip_restored = fs::rename(previous_ip, current_ip).is_ok();
    if domain_restored && ip_restored {
        RuleSetError::PublishFailed
    } else {
        RuleSetError::RollbackFailed
    }
}

fn file_name(path: &Path) -> Result<String, RuleSetError> {
    path.file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or(RuleSetError::PublishFailed)
}

fn recover_interrupted_transaction(root: &Path) -> Result<(), RuleSetError> {
    let journal_path = root.join(JOURNAL_FILE);
    let metadata = match fs::symlink_metadata(&journal_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(RuleSetError::RollbackFailed),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 4096 {
        return Err(RuleSetError::RollbackFailed);
    }
    let journal: RuleSetJournal =
        serde_json::from_slice(&fs::read(&journal_path).map_err(|_| RuleSetError::RollbackFailed)?)
            .map_err(|_| RuleSetError::RollbackFailed)?;
    if journal.schema != JOURNAL_SCHEMA
        || !valid_transaction_name(&journal.candidate, ".candidate-ruleset-")
        || !valid_transaction_name(&journal.previous_domain, ".previous-domain-")
        || !valid_transaction_name(&journal.previous_ip, ".previous-ip-")
    {
        return Err(RuleSetError::RollbackFailed);
    }

    let current_domain = root.join(DOMAIN_FILE);
    let current_ip = root.join(IP_FILE);
    let previous_domain = root.join(&journal.previous_domain);
    let previous_ip = root.join(&journal.previous_ip);
    if !restore_if_present(&current_domain, &previous_domain)
        || !restore_if_present(&current_ip, &previous_ip)
    {
        return Err(RuleSetError::RollbackFailed);
    }
    let candidate = root.join(journal.candidate);
    if let Ok(metadata) = fs::symlink_metadata(&candidate) {
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(RuleSetError::RollbackFailed);
        }
        fs::remove_dir_all(candidate).map_err(|_| RuleSetError::RollbackFailed)?;
    }
    remove_journal(&journal_path, root).map_err(|_| RuleSetError::RollbackFailed)
}

fn cleanup_stale_transactions(root: &Path) -> Result<(), RuleSetError> {
    let mut changed = false;
    for entry in fs::read_dir(root).map_err(|_| RuleSetError::InvalidRoot)? {
        let entry = entry.map_err(|_| RuleSetError::InvalidRoot)?;
        let name = entry.file_name();
        let name = name.to_str().ok_or(RuleSetError::InvalidRoot)?;
        let metadata = entry.metadata().map_err(|_| RuleSetError::InvalidRoot)?;
        let file_type = entry.file_type().map_err(|_| RuleSetError::InvalidRoot)?;
        if name.starts_with(".previous-domain-") || name.starts_with(".previous-ip-") {
            if !metadata.is_file() || file_type.is_symlink() {
                return Err(RuleSetError::InvalidRoot);
            }
            fs::remove_file(entry.path()).map_err(|_| RuleSetError::InvalidRoot)?;
            changed = true;
        } else if name.starts_with(".candidate-ruleset-") {
            if !metadata.is_dir() || file_type.is_symlink() {
                return Err(RuleSetError::InvalidRoot);
            }
            fs::remove_dir_all(entry.path()).map_err(|_| RuleSetError::InvalidRoot)?;
            changed = true;
        }
    }
    if changed {
        sync_directory(root).map_err(|_| RuleSetError::InvalidRoot)?;
    }
    Ok(())
}

fn valid_transaction_name(value: &str, prefix: &str) -> bool {
    value.starts_with(prefix)
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
}

fn restore_if_present(current: &Path, previous: &Path) -> bool {
    if is_regular_non_symlink(previous) {
        if fs::symlink_metadata(current).is_ok() && fs::remove_file(current).is_err() {
            return false;
        }
        fs::rename(previous, current).is_ok()
    } else {
        is_regular_non_symlink(current)
    }
}

fn remove_journal(journal: &Path, root: &Path) -> std::io::Result<()> {
    fs::remove_file(journal)?;
    sync_directory(root)
}

fn is_regular_non_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
}

fn write_private_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(unix)]
fn create_private_directory(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;

    fs::DirBuilder::new().mode(0o700).create(path)
}

#[cfg(not(unix))]
fn create_private_directory(path: &Path) -> std::io::Result<()> {
    fs::create_dir(path)
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> std::io::Result<()> {
    fs::File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> std::io::Result<()> {
    Ok(())
}
