use std::{collections::HashSet, fmt, fs, io::Read, path::PathBuf};

use nethop_subscription::{Digest, FormatHint, RequestProfile, SourceId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{ConfigSnapshot, SourceName, worker_config::atomic_write};

const REGISTRY_SCHEMA: &str = "nethop-source-registry-v1";
const MAX_REGISTRY_BYTES: u64 = 16 * 1024;
const SOURCE_ID_BYTES: usize = 16;
const MAX_ID_ATTEMPTS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRegistry {
    path: PathBuf,
}

impl SourceRegistry {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, SourceRegistryError> {
        let path = path.into();
        if !path.is_absolute() || path.file_name().is_none() {
            return Err(SourceRegistryError::InvalidPath);
        }
        Ok(Self { path })
    }

    pub fn reconcile(
        &self,
        snapshot: &ConfigSnapshot,
        entropy: &mut impl SourceIdEntropy,
    ) -> Result<SourceConfig, SourceRegistryError> {
        let prepared = self.prepare(snapshot, entropy)?;
        self.activate(prepared)
    }

    pub fn prepare(
        &self,
        snapshot: &ConfigSnapshot,
        entropy: &mut impl SourceIdEntropy,
    ) -> Result<PreparedSourceConfig, SourceRegistryError> {
        self.prepare_with_preferred_ids(snapshot, entropy, &[])
    }

    pub(crate) fn prepare_with_preferred_ids(
        &self,
        snapshot: &ConfigSnapshot,
        entropy: &mut impl SourceIdEntropy,
        preferred_ids: &[Option<SourceId>],
    ) -> Result<PreparedSourceConfig, SourceRegistryError> {
        if !preferred_ids.is_empty() && preferred_ids.len() != snapshot.effective().sources().len()
        {
            return Err(SourceRegistryError::InvalidRegistry);
        }
        let mut state = match self.load() {
            Ok(state) => state,
            Err(SourceRegistryError::CorruptRegistry) => RegistryState::empty(),
            Err(error) => return Err(error),
        };
        if let Some(binding) = state
            .active
            .clone()
            .filter(|binding| binding_matches(binding, snapshot))
        {
            let source_config = source_config_from_binding(snapshot, &binding)?;
            if state.pending.is_some() {
                state.pending = None;
                self.persist(&state)?;
            }
            return Ok(PreparedSourceConfig {
                source_config,
                binding,
                activate: false,
            });
        }
        if let Some(binding) = state
            .pending
            .clone()
            .filter(|binding| binding_matches(binding, snapshot))
        {
            return Ok(PreparedSourceConfig {
                source_config: source_config_from_binding(snapshot, &binding)?,
                binding,
                activate: true,
            });
        }

        let previous_entries = state
            .active
            .as_ref()
            .map_or_else(Vec::new, |binding| binding.entries.clone());
        let mut claimed = HashSet::with_capacity(previous_entries.len());
        let mut assigned = HashSet::with_capacity(snapshot.effective().sources().len());
        let mut entries = Vec::with_capacity(snapshot.effective().sources().len());
        let mut sources = Vec::with_capacity(snapshot.effective().sources().len());

        for (source_index, source) in snapshot.effective().sources().iter().enumerate() {
            let name_digest = digest(source.name().as_str());
            let url_digest = digest(source.url());
            let preferred = preferred_ids
                .get(source_index)
                .and_then(|value| value.as_ref())
                .map(|id| {
                    previous_entries
                        .iter()
                        .enumerate()
                        .find(|(index, entry)| !claimed.contains(index) && &entry.source_id == id)
                        .map(|(index, _)| index)
                        .ok_or(SourceRegistryError::InvalidRegistry)
                })
                .transpose()?;
            let matched = preferred.or_else(|| {
                (!source.url().is_empty())
                    .then(|| {
                        find_entry(&previous_entries, &claimed, |entry| {
                            entry.url_digest == url_digest
                        })
                    })
                    .flatten()
                    .or_else(|| {
                        find_entry(&previous_entries, &claimed, |entry| {
                            entry.name_digest == name_digest
                        })
                    })
            });
            let id = if let Some(index) = matched {
                claimed.insert(index);
                previous_entries[index].source_id.clone()
            } else {
                allocate_id(entropy, &assigned, &previous_entries)?
            };
            assigned.insert(id.clone());
            entries.push(RegistryEntry {
                source_id: id.clone(),
                name_digest,
                url_digest,
            });
            sources.push(SourceDefinition {
                id,
                name: source.name().clone(),
                enabled: source.enabled(),
                url: source.url().to_owned(),
                mirrors: source.mirrors().to_vec(),
                expected_format: source.format_hint().parser_hint(),
                request_profile: source.request_profile(),
            });
        }

        let binding = RegistryBinding {
            config_digest: snapshot.digest().to_owned(),
            entries,
        };
        state.pending = Some(binding.clone());
        self.persist(&state)?;
        Ok(PreparedSourceConfig {
            source_config: SourceConfig {
                source_config_digest: source_config_digest(&sources),
                sources,
            },
            binding,
            activate: true,
        })
    }

    pub fn activate(
        &self,
        prepared: PreparedSourceConfig,
    ) -> Result<SourceConfig, SourceRegistryError> {
        if !prepared.activate {
            return Ok(prepared.source_config);
        }
        let mut state = self.load()?;
        if state.pending.as_ref() != Some(&prepared.binding) {
            return Err(SourceRegistryError::InvalidRegistry);
        }
        state.active = Some(prepared.binding);
        state.pending = None;
        self.persist(&state)?;
        Ok(prepared.source_config)
    }

    pub(crate) fn checkpoint(&self) -> Result<SourceRegistryCheckpoint, SourceRegistryError> {
        Ok(SourceRegistryCheckpoint {
            state: self.load()?,
        })
    }

    pub(crate) fn restore_checkpoint(
        &self,
        expected_config_digest: &str,
        checkpoint: &SourceRegistryCheckpoint,
    ) -> Result<(), SourceRegistryError> {
        let current = self.load()?;
        if current
            .active
            .as_ref()
            .map(|binding| binding.config_digest.as_str())
            != Some(expected_config_digest)
        {
            return Err(SourceRegistryError::InvalidRegistry);
        }
        self.persist(&checkpoint.state)
    }

    pub(crate) fn restore_checkpoint_exact(
        &self,
        checkpoint: &SourceRegistryCheckpoint,
    ) -> Result<(), SourceRegistryError> {
        self.persist(&checkpoint.state)
    }

    fn persist(&self, state: &RegistryState) -> Result<(), SourceRegistryError> {
        let bytes = serde_json::to_vec(state).map_err(|_| SourceRegistryError::WriteFailed)?;
        atomic_write(&self.path, &bytes).map_err(|_| SourceRegistryError::WriteFailed)
    }

    fn load(&self) -> Result<RegistryState, SourceRegistryError> {
        let metadata = match fs::symlink_metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(RegistryState::empty());
            }
            Err(_) => return Err(SourceRegistryError::InvalidRegistry),
        };
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() == 0
            || metadata.len() > MAX_REGISTRY_BYTES
            || !private_file(&metadata)
        {
            return Err(SourceRegistryError::InvalidRegistry);
        }
        let bytes = fs::read(&self.path).map_err(|_| SourceRegistryError::InvalidRegistry)?;
        let state: RegistryState =
            serde_json::from_slice(&bytes).map_err(|_| SourceRegistryError::CorruptRegistry)?;
        state.validate()?;
        Ok(state)
    }
}

pub struct PreparedSourceConfig {
    source_config: SourceConfig,
    binding: RegistryBinding,
    activate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceRegistryCheckpoint {
    state: RegistryState,
}

fn binding_matches(binding: &RegistryBinding, snapshot: &ConfigSnapshot) -> bool {
    binding.config_digest == snapshot.digest()
        && binding.entries.len() == snapshot.effective().sources().len()
        && binding
            .entries
            .iter()
            .zip(snapshot.effective().sources())
            .all(|(entry, source)| {
                entry.name_digest == digest(source.name().as_str())
                    && entry.url_digest == digest(source.url())
            })
}

fn source_config_from_binding(
    snapshot: &ConfigSnapshot,
    binding: &RegistryBinding,
) -> Result<SourceConfig, SourceRegistryError> {
    let mut sources = Vec::with_capacity(binding.entries.len());
    for (entry, source) in binding.entries.iter().zip(snapshot.effective().sources()) {
        sources.push(SourceDefinition {
            id: entry.source_id.clone(),
            name: source.name().clone(),
            enabled: source.enabled(),
            url: source.url().to_owned(),
            mirrors: source.mirrors().to_vec(),
            expected_format: source.format_hint().parser_hint(),
            request_profile: source.request_profile(),
        });
    }
    Ok(SourceConfig {
        source_config_digest: source_config_digest(&sources),
        sources,
    })
}

pub trait SourceIdEntropy {
    fn fill(&mut self, output: &mut [u8; SOURCE_ID_BYTES]) -> Result<(), SourceRegistryError>;
}

#[derive(Debug, Default)]
pub struct SystemSourceIdEntropy;

impl SourceIdEntropy for SystemSourceIdEntropy {
    fn fill(&mut self, output: &mut [u8; SOURCE_ID_BYTES]) -> Result<(), SourceRegistryError> {
        let mut file =
            fs::File::open("/dev/urandom").map_err(|_| SourceRegistryError::EntropyUnavailable)?;
        file.read_exact(output)
            .map_err(|_| SourceRegistryError::EntropyUnavailable)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct SourceConfig {
    source_config_digest: String,
    sources: Vec<SourceDefinition>,
}

impl SourceConfig {
    pub fn source_config_digest(&self) -> &str {
        &self.source_config_digest
    }

    pub fn sources(&self) -> &[SourceDefinition] {
        &self.sources
    }

    pub fn active_sources(&self) -> impl Iterator<Item = &SourceDefinition> {
        self.sources
            .iter()
            .filter(|source| source.enabled && !source.url.is_empty())
    }
}

impl fmt::Debug for SourceConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceConfig")
            .field("source_config_digest", &self.source_config_digest)
            .field("source_count", &self.sources.len())
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct SourceDefinition {
    id: SourceId,
    name: SourceName,
    enabled: bool,
    url: String,
    mirrors: Vec<String>,
    expected_format: FormatHint,
    request_profile: RequestProfile,
}

impl SourceDefinition {
    pub fn id(&self) -> &SourceId {
        &self.id
    }

    pub fn name(&self) -> &SourceName {
        &self.name
    }

    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn url(&self) -> &str {
        &self.url
    }

    pub(crate) fn request_identity_digest(&self) -> String {
        digest(&self.url)
    }

    pub(crate) fn mirrors(&self) -> &[String] {
        &self.mirrors
    }

    pub const fn expected_format(&self) -> FormatHint {
        self.expected_format
    }

    pub const fn request_profile(&self) -> RequestProfile {
        self.request_profile
    }
}

impl fmt::Debug for SourceDefinition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceDefinition")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("enabled", &self.enabled)
            .field("url", &"[REDACTED]")
            .field("mirror_count", &self.mirrors.len())
            .field("expected_format", &self.expected_format)
            .field("request_profile", &self.request_profile)
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct RegistryState {
    schema: String,
    active: Option<RegistryBinding>,
    pending: Option<RegistryBinding>,
}

impl RegistryState {
    fn empty() -> Self {
        Self {
            schema: REGISTRY_SCHEMA.to_owned(),
            active: None,
            pending: None,
        }
    }

    fn validate(&self) -> Result<(), SourceRegistryError> {
        if self.schema != REGISTRY_SCHEMA {
            return Err(SourceRegistryError::CorruptRegistry);
        }
        for binding in [&self.active, &self.pending].into_iter().flatten() {
            if binding.config_digest.len() != 64 || binding.entries.len() > 16 {
                return Err(SourceRegistryError::CorruptRegistry);
            }
            let mut ids = HashSet::with_capacity(binding.entries.len());
            for entry in &binding.entries {
                SourceId::new(entry.source_id.as_str())
                    .map_err(|_| SourceRegistryError::CorruptRegistry)?;
                if !ids.insert(entry.source_id.clone())
                    || entry.name_digest.len() != 64
                    || entry.url_digest.len() != 64
                {
                    return Err(SourceRegistryError::CorruptRegistry);
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct RegistryBinding {
    config_digest: String,
    entries: Vec<RegistryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct RegistryEntry {
    source_id: SourceId,
    name_digest: String,
    url_digest: String,
}

fn find_entry(
    entries: &[RegistryEntry],
    claimed: &HashSet<usize>,
    predicate: impl Fn(&RegistryEntry) -> bool,
) -> Option<usize> {
    entries
        .iter()
        .enumerate()
        .find(|(index, entry)| !claimed.contains(index) && predicate(entry))
        .map(|(index, _)| index)
}

fn allocate_id(
    entropy: &mut impl SourceIdEntropy,
    assigned: &HashSet<SourceId>,
    previous: &[RegistryEntry],
) -> Result<SourceId, SourceRegistryError> {
    for _ in 0..MAX_ID_ATTEMPTS {
        let mut bytes = [0_u8; SOURCE_ID_BYTES];
        entropy.fill(&mut bytes)?;
        let value = format!("src_{}", hex(&bytes));
        let id = SourceId::new(value).map_err(|_| SourceRegistryError::InvalidRegistry)?;
        if !assigned.contains(&id) && previous.iter().all(|entry| entry.source_id != id) {
            return Ok(id);
        }
    }
    Err(SourceRegistryError::IdentityCollision)
}

fn digest(value: &str) -> String {
    Digest::sha256(value.as_bytes()).hex()
}

fn source_config_digest(sources: &[SourceDefinition]) -> String {
    let mut canonical = Vec::new();
    canonical.extend_from_slice(b"nethop-source-config-v1\0");
    for source in sources {
        push_field(&mut canonical, source.id.as_str().as_bytes());
        canonical.push(u8::from(source.enabled));
        push_field(&mut canonical, digest(&source.url).as_bytes());
        canonical.push(format_code(source.expected_format));
        canonical.push(profile_code(source.request_profile));
        canonical.extend_from_slice(&(source.mirrors.len() as u32).to_be_bytes());
        for mirror in &source.mirrors {
            push_field(&mut canonical, digest(mirror).as_bytes());
        }
    }
    Digest::sha256(&canonical).hex()
}

fn push_field(output: &mut Vec<u8>, value: &[u8]) {
    output.extend_from_slice(&(value.len() as u32).to_be_bytes());
    output.extend_from_slice(value);
}

const fn format_code(format: FormatHint) -> u8 {
    match format {
        FormatHint::Auto => 0,
        FormatHint::UriList => 1,
        FormatHint::Base64List => 2,
        FormatHint::ClashYaml => 3,
        FormatHint::SingboxJson => 4,
        FormatHint::IniProfile => 5,
        FormatHint::SurfboardIni => 6,
    }
}

const fn profile_code(profile: RequestProfile) -> u8 {
    match profile {
        RequestProfile::NetHopGeneric => 0,
        RequestProfile::Mihomo => 1,
        RequestProfile::ClashStandard => 2,
        RequestProfile::Surfboard => 3,
        RequestProfile::SingBox => 4,
        RequestProfile::SingBoxAndroid => 5,
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(value, "{byte:02x}");
    }
    value
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
pub enum SourceRegistryError {
    #[error("source registry path is invalid")]
    InvalidPath,
    #[error("source registry state is invalid")]
    InvalidRegistry,
    #[error("source registry contents are corrupt")]
    CorruptRegistry,
    #[error("source ID entropy is unavailable")]
    EntropyUnavailable,
    #[error("source ID collision retry budget was exhausted")]
    IdentityCollision,
    #[error("source registry could not be written atomically")]
    WriteFailed,
}

impl SourceRegistryError {
    pub const fn diagnostic_detail(self) -> &'static str {
        match self {
            Self::InvalidPath | Self::InvalidRegistry | Self::CorruptRegistry => "REGISTRY-INVALID",
            Self::EntropyUnavailable => "ID-ENTROPY-UNAVAILABLE",
            Self::IdentityCollision => "ID-COLLISION",
            Self::WriteFailed => "REGISTRY-PUBLISH-FAILED",
        }
    }
}
