use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use nethop_subscription::Digest;
use thiserror::Error;

use crate::config_model::{EffectiveConfig, UserConfigWire};

pub const CONFIG_SCHEMA_VERSION: u32 = 3;
pub const MAX_CONFIG_BYTES: u64 = 256 * 1024;
pub const MAX_SOURCES: usize = 16;
pub(crate) const MAX_AUTO_CANDIDATES: u16 = 64;
const MAX_STABLE_READ_ATTEMPTS: usize = 3;
const MAX_TEMP_ATTEMPTS: u64 = 16;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, ConfigError> {
        let path = path.into();
        if !path.is_absolute() || path.file_name().is_none() {
            return Err(ConfigError::InvalidPath);
        }
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<ConfigSnapshot, ConfigError> {
        validate_private_parent(&self.path)?;
        self.load_inner()
    }

    pub fn observed_digest(&self) -> Result<String, ConfigError> {
        validate_private_parent(&self.path)?;
        Ok(Digest::sha256(&read_stable(&self.path)?).hex())
    }

    #[cfg(feature = "subscription-update")]
    pub(crate) fn checkpoint(&self) -> Result<ConfigStoreCheckpoint, ConfigError> {
        validate_private_parent(&self.path)?;
        let bytes = read_stable(&self.path)?;
        let digest = Digest::sha256(&bytes).hex();
        Ok(ConfigStoreCheckpoint { bytes, digest })
    }

    #[cfg(unix)]
    pub(crate) fn load_without_parent_check(&self) -> Result<ConfigSnapshot, ConfigError> {
        self.load_inner()
    }

    fn load_inner(&self) -> Result<ConfigSnapshot, ConfigError> {
        let bytes = read_stable(&self.path)?;
        parse_snapshot(bytes)
    }

    pub(crate) fn prepare_service_enabled(
        &self,
        expected_digest: &str,
        enabled: bool,
    ) -> Result<PreparedConfigWrite, ConfigError> {
        let current = self.load()?;
        if current.digest() != expected_digest {
            return Err(ConfigError::Conflict);
        }
        let mut wire = current.wire.clone();
        wire.service.enabled = enabled;
        let bytes = canonical_toml(&wire)?;
        let snapshot = parse_snapshot(bytes.clone())?;
        Ok(PreparedConfigWrite { bytes, snapshot })
    }

    #[cfg(feature = "subscription-update")]
    pub(crate) fn prepare_document(
        &self,
        expected_digest: &str,
        document: &serde_json::Value,
    ) -> Result<PreparedConfigWrite, ConfigError> {
        if self.observed_digest()? != expected_digest {
            return Err(ConfigError::Conflict);
        }
        self.prepare_document_candidate(document)
    }

    #[cfg(feature = "subscription-update")]
    pub(crate) fn prepare_document_candidate(
        &self,
        document: &serde_json::Value,
    ) -> Result<PreparedConfigWrite, ConfigError> {
        let wire: UserConfigWire = serde_json::from_value(document.clone()).map_err(|error| {
            if error.to_string().contains("unknown field") {
                ConfigError::UnknownField
            } else {
                ConfigError::InvalidToml
            }
        })?;
        let bytes = canonical_toml(&wire)?;
        let snapshot = parse_snapshot(bytes.clone())?;
        Ok(PreparedConfigWrite { bytes, snapshot })
    }

    pub(crate) fn commit_prepared(
        &self,
        expected_digest: &str,
        prepared: PreparedConfigWrite,
    ) -> Result<ConfigSnapshot, ConfigError> {
        if self.observed_digest()? != expected_digest {
            return Err(ConfigError::Conflict);
        }
        atomic_write(&self.path, &prepared.bytes)?;
        let committed = self.load()?;
        if committed.digest() != prepared.snapshot.digest() {
            return Err(ConfigError::WriteFailed);
        }
        Ok(committed)
    }

    #[cfg(feature = "subscription-update")]
    pub(crate) fn restore_checkpoint(
        &self,
        expected_digest: &str,
        checkpoint: &ConfigStoreCheckpoint,
    ) -> Result<(), ConfigError> {
        if self.observed_digest()? != expected_digest {
            return Err(ConfigError::Conflict);
        }
        atomic_write(&self.path, &checkpoint.bytes)?;
        if self.observed_digest()? != checkpoint.digest {
            return Err(ConfigError::WriteFailed);
        }
        Ok(())
    }

    pub fn set_service_enabled(
        &self,
        expected_digest: &str,
        enabled: bool,
    ) -> Result<ConfigSnapshot, ConfigError> {
        let prepared = self.prepare_service_enabled(expected_digest, enabled)?;
        self.commit_prepared(expected_digest, prepared)
    }
}

#[cfg(feature = "subscription-update")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConfigStoreCheckpoint {
    bytes: Vec<u8>,
    digest: String,
}

#[cfg(feature = "subscription-update")]
impl ConfigStoreCheckpoint {
    pub(crate) fn digest(&self) -> &str {
        &self.digest
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

fn parse_snapshot(bytes: Vec<u8>) -> Result<ConfigSnapshot, ConfigError> {
    let digest = Digest::sha256(&bytes).hex();
    let text = std::str::from_utf8(&bytes).map_err(|_| ConfigError::InvalidUtf8)?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let wire: UserConfigWire = toml::from_str(text).map_err(|error| {
        if error.message().contains("unknown field") {
            ConfigError::UnknownField
        } else {
            ConfigError::InvalidToml
        }
    })?;
    let effective = EffectiveConfig::from_wire(wire.clone())?;
    Ok(ConfigSnapshot {
        effective,
        wire,
        digest,
    })
}

fn canonical_toml(wire: &UserConfigWire) -> Result<Vec<u8>, ConfigError> {
    let wire = wire.canonicalized()?;
    let document = toml::to_string_pretty(&wire).map_err(|_| ConfigError::WriteFailed)?;
    let document = document
        .replace(
            "[service]\n",
            "[service]\n# Persistent proxy switch. The daemon remains available when disabled.\n",
        )
        .replace(
            "[[subscriptions.sources]]\n",
            "[[subscriptions.sources]]\n# User-visible name and HTTPS subscription URL.\n",
        );
    let output = format!(
        "# NetHop user configuration\n\
         # Source IDs are daemon-owned and never belong in this file.\n\
         {document}"
    );
    Ok(output.into_bytes())
}

pub(crate) struct PreparedConfigWrite {
    bytes: Vec<u8>,
    snapshot: ConfigSnapshot,
}

impl PreparedConfigWrite {
    #[cfg(feature = "subscription-update")]
    pub(crate) const fn snapshot(&self) -> &ConfigSnapshot {
        &self.snapshot
    }

    #[cfg(feature = "subscription-update")]
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ConfigSnapshot {
    effective: EffectiveConfig,
    wire: UserConfigWire,
    digest: String,
}

impl fmt::Debug for ConfigSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConfigSnapshot")
            .field("digest", &self.digest)
            .field("service_enabled", &self.effective.service_enabled())
            .field("source_count", &self.effective.sources().len())
            .finish()
    }
}

impl ConfigSnapshot {
    pub fn effective(&self) -> &EffectiveConfig {
        &self.effective
    }

    pub fn digest(&self) -> &str {
        &self.digest
    }

    pub fn redacted_document(&self) -> serde_json::Value {
        let mut value = serde_json::to_value(&self.wire).expect("validated config is serializable");
        if let Some(sources) = value
            .pointer_mut("/subscriptions/sources")
            .and_then(serde_json::Value::as_array_mut)
        {
            for source in sources {
                if let Some(object) = source.as_object_mut() {
                    let configured = object
                        .get("url")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|url| !url.is_empty());
                    object.insert("url".into(), serde_json::json!(null));
                    object.insert("url_configured".into(), serde_json::json!(configured));
                    if let Some(mirrors) = object.get_mut("mirrors") {
                        let count = mirrors.as_array().map_or(0, Vec::len);
                        *mirrors = serde_json::json!({"configured_count": count});
                    }
                }
            }
        }
        if let Some(rules) = value
            .pointer_mut("/network/wifi_scenes/rules")
            .and_then(serde_json::Value::as_array_mut)
        {
            for rule in rules {
                if let Some(object) = rule.as_object_mut() {
                    for key in ["ssid", "bssid"] {
                        let configured = object
                            .get(key)
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(|value| !value.is_empty());
                        object.insert(key.into(), serde_json::Value::Null);
                        object.insert(format!("{key}_configured"), serde_json::json!(configured));
                    }
                }
            }
        }
        value
    }

    #[cfg(feature = "subscription-update")]
    pub(crate) fn document(&self) -> serde_json::Value {
        serde_json::to_value(&self.wire).expect("validated config is serializable")
    }
}

pub(crate) fn read_stable(path: &Path) -> Result<Vec<u8>, ConfigError> {
    if !path.is_absolute() {
        return Err(ConfigError::InvalidPath);
    }
    for _ in 0..MAX_STABLE_READ_ATTEMPTS {
        let mut file = open_readonly(path)?;
        let before_metadata = file.metadata().map_err(|_| ConfigError::InvalidPath)?;
        let before = file_stamp(&before_metadata)?;
        if before.len == 0 || before.len > MAX_CONFIG_BYTES {
            return Err(if before.len > MAX_CONFIG_BYTES {
                ConfigError::TooLarge
            } else {
                ConfigError::InvalidPath
            });
        }
        let mut bytes = Vec::with_capacity(before.len as usize);
        Read::by_ref(&mut file)
            .take(MAX_CONFIG_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| ConfigError::InvalidPath)?;
        if bytes.len() as u64 > MAX_CONFIG_BYTES {
            return Err(ConfigError::TooLarge);
        }
        let after = file_stamp(&file.metadata().map_err(|_| ConfigError::InvalidPath)?)?;
        let current = fs::symlink_metadata(path)
            .map_err(|_| ConfigError::InvalidPath)
            .and_then(|metadata| file_stamp(&metadata))?;
        if before == after && after == current && bytes.len() as u64 == before.len {
            return Ok(bytes);
        }
    }
    Err(ConfigError::UnstableSnapshot)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileStamp {
    len: u64,
    modified: Option<std::time::SystemTime>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(unix)]
    changed_seconds: i64,
    #[cfg(unix)]
    changed_nanoseconds: i64,
}

fn file_stamp(metadata: &fs::Metadata) -> Result<FileStamp, ConfigError> {
    if !metadata.is_file() || metadata.file_type().is_symlink() || !private_file(metadata) {
        return Err(ConfigError::InvalidPath);
    }
    Ok(FileStamp {
        len: metadata.len(),
        modified: metadata.modified().ok(),
        #[cfg(unix)]
        device: std::os::unix::fs::MetadataExt::dev(metadata),
        #[cfg(unix)]
        inode: std::os::unix::fs::MetadataExt::ino(metadata),
        #[cfg(unix)]
        changed_seconds: std::os::unix::fs::MetadataExt::ctime(metadata),
        #[cfg(unix)]
        changed_nanoseconds: std::os::unix::fs::MetadataExt::ctime_nsec(metadata),
    })
}

#[cfg(unix)]
fn open_readonly(path: &Path) -> Result<File, ConfigError> {
    use std::os::unix::fs::OpenOptionsExt;

    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                ConfigError::Missing
            } else {
                ConfigError::InvalidPath
            }
        })
}

#[cfg(not(unix))]
fn open_readonly(path: &Path) -> Result<File, ConfigError> {
    File::open(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ConfigError::Missing
        } else {
            ConfigError::InvalidPath
        }
    })
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), ConfigError> {
    let parent = path.parent().ok_or(ConfigError::InvalidPath)?;
    validate_private_parent(path)?;
    let _file_name = path.file_name().ok_or(ConfigError::InvalidPath)?;
    let pid = std::process::id();
    let mut temporary = None;
    for _ in 0..MAX_TEMP_ATTEMPTS {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(".nethop.toml.{pid}.{sequence}.tmp"));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                temporary = Some((candidate, file));
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(ConfigError::WriteFailed),
        }
    }
    let (temporary, mut file) = temporary.ok_or(ConfigError::WriteFailed)?;
    let result = (|| {
        set_private(&file)?;
        file.write_all(bytes)
            .map_err(|_| ConfigError::WriteFailed)?;
        file.sync_all().map_err(|_| ConfigError::WriteFailed)?;
        drop(file);
        #[cfg(unix)]
        {
            fs::rename(&temporary, path).map_err(|_| ConfigError::WriteFailed)?;
            File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(|_| ConfigError::WriteFailed)?;
        }
        #[cfg(not(unix))]
        {
            let _ = fs::remove_file(path);
            fs::rename(&temporary, path).map_err(|_| ConfigError::WriteFailed)?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn validate_private_parent(path: &Path) -> Result<(), ConfigError> {
    let parent = path.parent().ok_or(ConfigError::InvalidPath)?;
    let metadata = fs::symlink_metadata(parent).map_err(|_| ConfigError::InvalidPath)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() || !private_directory(&metadata) {
        return Err(ConfigError::InvalidPath);
    }
    Ok(())
}

#[cfg(target_os = "android")]
fn private_directory(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    metadata.uid() == 0 && metadata.permissions().mode() & 0o077 == 0
}

#[cfg(all(unix, not(target_os = "android")))]
fn private_directory(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o077 == 0
}

#[cfg(not(unix))]
fn private_directory(_metadata: &fs::Metadata) -> bool {
    true
}

#[cfg(unix)]
fn set_private(file: &File) -> Result<(), ConfigError> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|_| ConfigError::WriteFailed)
}

#[cfg(not(unix))]
fn set_private(_file: &File) -> Result<(), ConfigError> {
    Ok(())
}

#[cfg(target_os = "android")]
fn private_file(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    metadata.uid() == 0 && metadata.permissions().mode() & 0o077 == 0
}

#[cfg(all(unix, not(target_os = "android")))]
fn private_file(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o077 == 0
}

#[cfg(not(unix))]
fn private_file(_metadata: &fs::Metadata) -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ConfigError {
    #[error("configuration file is missing")]
    Missing,
    #[error("configuration path is invalid or not a private regular file")]
    InvalidPath,
    #[error("configuration exceeds the bounded size")]
    TooLarge,
    #[error("configuration snapshot changed while it was being read")]
    UnstableSnapshot,
    #[error("configuration is not valid UTF-8")]
    InvalidUtf8,
    #[error("configuration is not valid strict TOML")]
    InvalidToml,
    #[error("configuration contains an unknown field")]
    UnknownField,
    #[error("configuration mutation value is invalid")]
    InvalidValue,
    #[error("configuration schema is unsupported")]
    UnsupportedSchema,
    #[error("configuration source count is invalid")]
    InvalidSourceCount,
    #[error("single subscription mode requires one unique active source")]
    SingleSourceNotUnique,
    #[error("configured subscriptions require an active source")]
    NoActiveSource,
    #[error("configuration source name is invalid")]
    InvalidSourceName,
    #[error("configuration source names are duplicated")]
    DuplicateSourceName,
    #[error("configuration source URLs are duplicated")]
    DuplicateSourceUrl,
    #[error("configuration source URL is invalid")]
    InvalidSourceUrl,
    #[error("configuration source URL must use HTTPS")]
    SourceUrlNonHttps,
    #[error("capture policy defaults are invalid")]
    InvalidCapture,
    #[error("subscription update schedule is invalid")]
    InvalidUpdateSchedule,
    #[error("subscription source options are invalid")]
    InvalidSourceOptions,
    #[error("proxy selection settings are invalid")]
    InvalidProxy,
    #[error("application selection is invalid")]
    InvalidApplications,
    #[error("Android package catalog is unavailable")]
    ApplicationCatalogUnavailable,
    #[error("network settings are invalid")]
    InvalidNetwork,
    #[error("network settings require an unavailable component")]
    UnsupportedNetwork,
    #[error("routing settings are invalid")]
    InvalidRouting,
    #[error("routing settings require an unavailable component")]
    UnsupportedRouting,
    #[error("logging settings are invalid")]
    InvalidLogging,
    #[error("advanced settings are invalid")]
    InvalidAdvanced,
    #[error("configuration changed concurrently")]
    Conflict,
    #[error("configuration could not be written atomically")]
    WriteFailed,
}

impl ConfigError {
    pub const fn diagnostic_detail(self) -> &'static str {
        match self {
            Self::Missing => "MISSING",
            Self::InvalidPath => "NOT-PRIVATE",
            Self::TooLarge => "TOO-LARGE",
            Self::UnstableSnapshot => "UNSTABLE-SNAPSHOT",
            Self::InvalidUtf8 => "INVALID-UTF8",
            Self::InvalidToml => "INVALID-TOML",
            Self::UnknownField => "UNKNOWN-FIELD",
            Self::UnsupportedSchema => "UNSUPPORTED-SCHEMA",
            Self::InvalidValue
            | Self::InvalidSourceCount
            | Self::SingleSourceNotUnique
            | Self::NoActiveSource
            | Self::InvalidSourceName
            | Self::InvalidCapture
            | Self::InvalidUpdateSchedule
            | Self::InvalidSourceOptions
            | Self::InvalidProxy
            | Self::InvalidApplications
            | Self::InvalidNetwork
            | Self::InvalidRouting
            | Self::InvalidLogging
            | Self::InvalidAdvanced => "INVALID-VALUE",
            Self::ApplicationCatalogUnavailable => "CAPABILITY-UNAVAILABLE",
            Self::UnsupportedNetwork | Self::UnsupportedRouting => "UNSUPPORTED-VALUE",
            Self::DuplicateSourceName => "DUPLICATE-SOURCE-NAME",
            Self::DuplicateSourceUrl => "DUPLICATE-SOURCE-URL",
            Self::InvalidSourceUrl => "URL-DENIED",
            Self::SourceUrlNonHttps => "URL-NON-HTTPS",
            Self::Conflict => "CONFLICT",
            Self::WriteFailed => "APPLY-ROLLED-BACK",
        }
    }
}
