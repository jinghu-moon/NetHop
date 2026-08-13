use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    composer::ManagedConfig,
    diagnostics::{CoreError, io_error},
};

const MANIFEST_SCHEMA: &str = "nethop-generation-v1";
const NODE_REGISTRY_SCHEMA: &str = "nethop-generation-nodes-v2";
const MAX_GENERATION_NODES: usize = 2_000;
const MAX_NODE_NAME_BYTES: usize = 128;
const MAX_TAG_BYTES: usize = 128;
const MAX_PROTOCOL_BYTES: usize = 32;
const MAX_SOURCE_IDS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GenerationId(u64);

impl GenerationId {
    pub fn new(value: u64) -> Result<Self, CoreError> {
        (value != 0)
            .then_some(Self(value))
            .ok_or(CoreError::InvalidGenerationId)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationManifest {
    pub schema: String,
    pub generation: GenerationId,
    pub config_sha256: String,
    pub node_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_registry_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_config_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationNodeRecord {
    stable_node_id: String,
    internal_tag: String,
    display_name: String,
    protocol: String,
    source_ids: Vec<String>,
    auto_candidate: bool,
}

impl GenerationNodeRecord {
    pub fn new(
        stable_node_id: impl Into<String>,
        internal_tag: impl Into<String>,
        display_name: impl Into<String>,
        protocol: impl Into<String>,
        source_ids: Vec<String>,
        auto_candidate: bool,
    ) -> Result<Self, CoreError> {
        let stable_node_id = stable_node_id.into();
        let internal_tag = internal_tag.into();
        let display_name = display_name.into();
        let protocol = protocol.into();
        let unique_sources = source_ids.iter().collect::<HashSet<_>>();
        let valid_node_id = stable_node_id.len() == 21
            && stable_node_id.starts_with("nh1s-")
            && stable_node_id[5..]
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'));
        let valid_tag = !internal_tag.is_empty()
            && internal_tag.len() <= MAX_TAG_BYTES
            && !internal_tag.chars().any(char::is_control);
        let valid_name = !display_name.is_empty()
            && display_name.len() <= MAX_NODE_NAME_BYTES
            && !display_name.chars().any(char::is_control);
        let valid_protocol = !protocol.is_empty()
            && protocol.len() <= MAX_PROTOCOL_BYTES
            && protocol
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit());
        let valid_sources = !source_ids.is_empty()
            && source_ids.len() <= MAX_SOURCE_IDS
            && unique_sources.len() == source_ids.len()
            && source_ids.iter().all(|id| valid_source_id(id));
        if !valid_node_id || !valid_tag || !valid_name || !valid_protocol || !valid_sources {
            return Err(publish_error(
                "node_registry",
                "generation node record is invalid",
            ));
        }
        Ok(Self {
            stable_node_id,
            internal_tag,
            display_name,
            protocol,
            source_ids,
            auto_candidate,
        })
    }

    pub fn stable_node_id(&self) -> &str {
        &self.stable_node_id
    }

    pub fn internal_tag(&self) -> &str {
        &self.internal_tag
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub fn protocol(&self) -> &str {
        &self.protocol
    }

    pub fn source_ids(&self) -> &[String] {
        &self.source_ids
    }

    pub const fn auto_candidate(&self) -> bool {
        self.auto_candidate
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationNodeRegistry {
    schema: String,
    auto_pool: Vec<String>,
    records: Vec<GenerationNodeRecord>,
}

impl GenerationNodeRegistry {
    pub fn new(records: Vec<GenerationNodeRecord>) -> Result<Self, CoreError> {
        let auto_pool = records
            .iter()
            .filter(|record| record.auto_candidate())
            .map(|record| record.stable_node_id().to_owned())
            .collect();
        Self::with_auto_pool(records, auto_pool)
    }

    pub fn with_auto_pool(
        mut records: Vec<GenerationNodeRecord>,
        auto_pool: Vec<String>,
    ) -> Result<Self, CoreError> {
        if records.is_empty() || records.len() > MAX_GENERATION_NODES {
            return Err(publish_error(
                "node_registry",
                "generation node registry size is invalid",
            ));
        }
        records.sort_unstable_by(|left, right| left.stable_node_id.cmp(&right.stable_node_id));
        let stable_ids = records
            .iter()
            .map(GenerationNodeRecord::stable_node_id)
            .collect::<HashSet<_>>();
        let tags = records
            .iter()
            .map(GenerationNodeRecord::internal_tag)
            .collect::<HashSet<_>>();
        if stable_ids.len() != records.len() || tags.len() != records.len() {
            return Err(publish_error(
                "node_registry",
                "generation node registry contains duplicate mappings",
            ));
        }
        let unique_auto = auto_pool.iter().collect::<HashSet<_>>();
        if auto_pool.is_empty()
            || auto_pool.len() > 64
            || unique_auto.len() != auto_pool.len()
            || auto_pool.iter().any(|node_id| {
                records
                    .binary_search_by_key(&node_id.as_str(), |record| record.stable_node_id())
                    .ok()
                    .is_none_or(|index| !records[index].auto_candidate())
            })
            || records.iter().any(|record| {
                record.auto_candidate()
                    != auto_pool
                        .iter()
                        .any(|node_id| node_id == record.stable_node_id())
            })
        {
            return Err(publish_error(
                "node_registry",
                "generation auto pool is invalid",
            ));
        }
        Ok(Self {
            schema: NODE_REGISTRY_SCHEMA.to_owned(),
            auto_pool,
            records,
        })
    }

    pub fn records(&self) -> &[GenerationNodeRecord] {
        &self.records
    }

    pub fn auto_pool(&self) -> &[String] {
        &self.auto_pool
    }

    pub fn by_stable_id(&self, stable_node_id: &str) -> Option<&GenerationNodeRecord> {
        self.records
            .binary_search_by_key(&stable_node_id, |record| record.stable_node_id())
            .ok()
            .map(|index| &self.records[index])
    }

    pub fn by_internal_tag(&self, internal_tag: &str) -> Option<&GenerationNodeRecord> {
        self.records
            .iter()
            .find(|record| record.internal_tag() == internal_tag)
    }

    fn bytes(&self) -> Result<Vec<u8>, CoreError> {
        serde_json::to_vec(self).map_err(|error| CoreError::SerializationFailure(error.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    generation: GenerationId,
    config: ManagedConfig,
    manifest: GenerationManifest,
    node_registry: Option<GenerationNodeRegistry>,
}

impl Candidate {
    pub fn new(generation: GenerationId, config: ManagedConfig) -> Self {
        let manifest = GenerationManifest {
            schema: MANIFEST_SCHEMA.to_owned(),
            generation,
            config_sha256: config.digest_sha256().to_owned(),
            node_count: config.node_count(),
            node_registry_sha256: None,
            source_config_digest: None,
            source_ids: Vec::new(),
        };
        Self {
            generation,
            config,
            manifest,
            node_registry: None,
        }
    }

    pub fn with_node_registry(
        mut self,
        registry: GenerationNodeRegistry,
    ) -> Result<Self, CoreError> {
        if registry.records.len() != self.config.node_count() {
            return Err(publish_error(
                "node_registry",
                "generation node registry does not match terminal node count",
            ));
        }
        self.manifest.node_registry_sha256 = Some(sha256_hex(&registry.bytes()?));
        self.node_registry = Some(registry);
        Ok(self)
    }

    pub fn bind_sources(
        mut self,
        source_config_digest: impl Into<String>,
        source_ids: Vec<String>,
    ) -> Result<Self, CoreError> {
        let digest = source_config_digest.into();
        let valid_digest = digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'));
        let valid_ids = !source_ids.is_empty()
            && source_ids.len() <= 16
            && source_ids.iter().all(|id| {
                id.starts_with("src_")
                    && id.len() == 36
                    && id[4..]
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
            });
        if !valid_digest || !valid_ids {
            return Err(publish_error(
                "bind_sources",
                "source generation binding is invalid",
            ));
        }
        self.manifest.source_config_digest = Some(digest);
        self.manifest.source_ids = source_ids;
        Ok(self)
    }

    pub const fn generation(&self) -> GenerationId {
        self.generation
    }

    pub const fn config(&self) -> &ManagedConfig {
        &self.config
    }

    pub const fn manifest(&self) -> &GenerationManifest {
        &self.manifest
    }

    pub const fn node_registry(&self) -> Option<&GenerationNodeRegistry> {
        self.node_registry.as_ref()
    }
}

/// Capability token for a fully written candidate that is not yet at a stable path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedCandidate {
    store_root: PathBuf,
    generation: GenerationId,
    directory: PathBuf,
}

impl PreparedCandidate {
    pub const fn generation(&self) -> GenerationId {
        self.generation
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn config_path(&self) -> PathBuf {
        self.directory.join("config.json")
    }

    pub fn node_registry_path(&self) -> PathBuf {
        self.directory.join("nodes.json")
    }
}

/// Capability token for a generation at its stable path but not necessarily active.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedGeneration {
    store_root: PathBuf,
    generation: GenerationId,
    directory: PathBuf,
}

impl SealedGeneration {
    pub const fn generation(&self) -> GenerationId {
        self.generation
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn config_path(&self) -> PathBuf {
        self.directory.join("config.json")
    }

    pub fn node_registry_path(&self) -> PathBuf {
        self.directory.join("nodes.json")
    }
}

#[derive(Debug, Clone)]
pub struct GenerationStore {
    root: PathBuf,
}

impl GenerationStore {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, CoreError> {
        let root = root.into();
        create_private_dir_all(&root).map_err(|error| io_error("create_root", error))?;
        let generations = root.join("generations");
        create_private_dir_all(&generations).map_err(|error| io_error("create_root", error))?;
        set_private_dir_permissions(&root).map_err(|error| io_error("secure_root", error))?;
        set_private_dir_permissions(&generations)
            .map_err(|error| io_error("secure_generations", error))?;
        let root = root
            .canonicalize()
            .map_err(|error| io_error("canonicalize_root", error))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn generations_root(&self) -> PathBuf {
        self.root.join("generations")
    }

    pub fn current_generation(&self) -> Result<Option<GenerationId>, CoreError> {
        let path = self.generations_root().join("current");
        if !path.exists() {
            return Ok(None);
        }
        let value = fs::read_to_string(path).map_err(|error| io_error("read_current", error))?;
        let value = value
            .trim()
            .parse::<u64>()
            .map_err(|_| CoreError::InvalidCurrentPointer)?;
        GenerationId::new(value).map(Some)
    }

    pub fn current_sealed_generation(&self) -> Result<Option<SealedGeneration>, CoreError> {
        self.current_generation()?
            .map(|generation| self.sealed_generation(generation))
            .transpose()
    }

    pub fn current_manifest(&self) -> Result<Option<GenerationManifest>, CoreError> {
        let Some(generation) = self.current_generation()? else {
            return Ok(None);
        };
        self.verify_generation(generation)?;
        let path = self
            .generations_root()
            .join(generation.get().to_string())
            .join("manifest.json");
        let manifest = serde_json::from_slice(
            &fs::read(path).map_err(|error| io_error("read_manifest", error))?,
        )
        .map_err(|_| publish_error("read_manifest", "generation manifest is invalid"))?;
        Ok(Some(manifest))
    }

    pub fn read_node_registry(
        &self,
        generation: GenerationId,
    ) -> Result<GenerationNodeRegistry, CoreError> {
        self.verify_generation(generation)?;
        let path = self
            .generations_root()
            .join(generation.get().to_string())
            .join("nodes.json");
        serde_json::from_slice(
            &fs::read(path).map_err(|error| io_error("read_node_registry", error))?,
        )
        .map_err(|_| publish_error("read_node_registry", "node registry is invalid"))
    }

    pub fn prepare_candidate(&self, candidate: &Candidate) -> Result<PreparedCandidate, CoreError> {
        let generations = self.generations_root();
        let final_dir = generations.join(candidate.generation.get().to_string());
        if final_dir.exists() {
            return Err(publish_error(
                "reserve_generation",
                "generation already exists",
            ));
        }
        let directory = generations.join(format!(
            ".candidate-{}-{}",
            candidate.generation.get(),
            std::process::id()
        ));
        if directory.exists() {
            fs::remove_dir_all(&directory)
                .map_err(|error| io_error("remove_stale_candidate", error))?;
        }
        create_private_dir(&directory).map_err(|error| io_error("create_candidate", error))?;
        let result = self.write_candidate(candidate, &directory);
        if result.is_err() {
            let _ = fs::remove_dir_all(&directory);
        }
        result?;
        Ok(PreparedCandidate {
            store_root: self.root.clone(),
            generation: candidate.generation,
            directory,
        })
    }

    pub fn seal_candidate(
        &self,
        prepared: &PreparedCandidate,
    ) -> Result<SealedGeneration, CoreError> {
        self.ensure_owned(&prepared.store_root)?;
        let final_dir = self
            .generations_root()
            .join(prepared.generation.get().to_string());
        if final_dir.exists() {
            return Err(publish_error(
                "seal_generation",
                "generation already exists",
            ));
        }
        fs::rename(&prepared.directory, &final_dir)
            .map_err(|error| io_error("seal_generation", error))?;
        sync_directory(&self.generations_root())
            .map_err(|error| io_error("sync_generations", error))?;
        Ok(SealedGeneration {
            store_root: self.root.clone(),
            generation: prepared.generation,
            directory: final_dir,
        })
    }

    pub fn commit_generation(&self, sealed: &SealedGeneration) -> Result<(), CoreError> {
        self.ensure_owned(&sealed.store_root)?;
        self.ensure_sealed_exists(sealed.generation)?;
        self.write_current(sealed.generation)
    }

    pub fn discard_prepared(&self, prepared: PreparedCandidate) -> Result<(), CoreError> {
        self.ensure_owned(&prepared.store_root)?;
        remove_directory_if_present(&prepared.directory, "discard_candidate")
    }

    pub fn discard_sealed(&self, sealed: SealedGeneration) -> Result<(), CoreError> {
        self.ensure_owned(&sealed.store_root)?;
        if self.current_generation()? == Some(sealed.generation) {
            return Err(publish_error(
                "discard_generation",
                "active generation cannot be discarded",
            ));
        }
        remove_directory_if_present(&sealed.directory, "discard_generation")?;
        sync_directory(&self.generations_root())
            .map_err(|error| io_error("sync_generations", error))
    }

    pub fn rollback_to(&self, generation: GenerationId) -> Result<(), CoreError> {
        self.ensure_sealed_exists(generation)?;
        self.write_current(generation)
    }

    pub fn sealed_generation(
        &self,
        generation: GenerationId,
    ) -> Result<SealedGeneration, CoreError> {
        self.ensure_sealed_exists(generation)?;
        Ok(SealedGeneration {
            store_root: self.root.clone(),
            generation,
            directory: self.generations_root().join(generation.get().to_string()),
        })
    }

    pub fn verify_generation(&self, generation: GenerationId) -> Result<(), CoreError> {
        self.ensure_sealed_exists(generation)
    }

    pub fn publish<F>(&self, candidate: &Candidate, validate: F) -> Result<(), CoreError>
    where
        F: FnOnce(&[u8]) -> Result<(), CoreError>,
    {
        self.publish_with_path(candidate, |_, bytes| validate(bytes))
    }

    pub fn publish_with_path<F>(&self, candidate: &Candidate, validate: F) -> Result<(), CoreError>
    where
        F: FnOnce(&Path, &[u8]) -> Result<(), CoreError>,
    {
        let prepared = self.prepare_candidate(candidate)?;
        if let Err(error) = validate(&prepared.config_path(), candidate.config.bytes()) {
            let _ = self.discard_prepared(prepared);
            return Err(error);
        }
        let sealed = match self.seal_candidate(&prepared) {
            Ok(sealed) => sealed,
            Err(error) => {
                let _ = self.discard_prepared(prepared);
                return Err(error);
            }
        };
        if let Err(error) = self.commit_generation(&sealed) {
            let _ = self.discard_sealed(sealed);
            return Err(error);
        }
        Ok(())
    }

    fn write_candidate(&self, candidate: &Candidate, directory: &Path) -> Result<(), CoreError> {
        write_sync(&directory.join("config.json"), candidate.config.bytes())
            .map_err(|error| io_error("write_config", error))?;
        if let Some(registry) = &candidate.node_registry {
            write_sync(&directory.join("nodes.json"), &registry.bytes()?)
                .map_err(|error| io_error("write_node_registry", error))?;
        }
        let manifest = serde_json::to_vec(&candidate.manifest)
            .map_err(|error| CoreError::SerializationFailure(error.to_string()))?;
        write_sync(&directory.join("manifest.json"), &manifest)
            .map_err(|error| io_error("write_manifest", error))?;
        sync_directory(directory).map_err(|error| io_error("sync_candidate", error))
    }

    fn write_current(&self, generation: GenerationId) -> Result<(), CoreError> {
        let generations = self.generations_root();
        let current_temp = generations.join(format!(".current-{}", generation.get()));
        write_sync(&current_temp, format!("{}\n", generation.get()).as_bytes())
            .map_err(|error| io_error("write_current", error))?;
        fs::rename(&current_temp, generations.join("current"))
            .map_err(|error| io_error("publish_current", error))?;
        sync_directory(&generations).map_err(|error| io_error("sync_generations", error))
    }

    fn ensure_owned(&self, token_root: &Path) -> Result<(), CoreError> {
        if token_root == self.root {
            Ok(())
        } else {
            Err(publish_error(
                "verify_store_owner",
                "generation token belongs to another store",
            ))
        }
    }

    fn ensure_sealed_exists(&self, generation: GenerationId) -> Result<(), CoreError> {
        let directory = self.generations_root().join(generation.get().to_string());
        let config = directory.join("config.json");
        let manifest = directory.join("manifest.json");
        let directory_valid = fs::symlink_metadata(&directory)
            .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink());
        if !directory_valid
            || !is_private_regular_file(&config)
            || !is_private_regular_file(&manifest)
        {
            return Err(publish_error(
                "verify_generation",
                "generation is incomplete or missing",
            ));
        }
        let manifest: GenerationManifest = serde_json::from_slice(
            &fs::read(&manifest).map_err(|error| io_error("read_manifest", error))?,
        )
        .map_err(|_| publish_error("verify_manifest", "generation manifest is invalid"))?;
        let config = fs::read(config).map_err(|error| io_error("read_config", error))?;
        let digest = sha256_hex(&config);
        if manifest.schema != MANIFEST_SCHEMA
            || manifest.generation != generation
            || manifest.config_sha256 != digest
        {
            return Err(publish_error(
                "verify_manifest",
                "generation manifest does not match config",
            ));
        }
        match manifest.node_registry_sha256 {
            Some(expected) => {
                let registry_path = directory.join("nodes.json");
                if !is_private_regular_file(&registry_path) {
                    return Err(publish_error(
                        "verify_node_registry",
                        "generation node registry is missing or insecure",
                    ));
                }
                let bytes = fs::read(&registry_path)
                    .map_err(|error| io_error("read_node_registry", error))?;
                let registry: GenerationNodeRegistry =
                    serde_json::from_slice(&bytes).map_err(|_| {
                        publish_error(
                            "verify_node_registry",
                            "generation node registry is invalid",
                        )
                    })?;
                if expected != sha256_hex(&bytes)
                    || registry.schema != NODE_REGISTRY_SCHEMA
                    || registry.records.len() != manifest.node_count
                {
                    return Err(publish_error(
                        "verify_node_registry",
                        "generation node registry does not match manifest",
                    ));
                }
            }
            None if directory.join("nodes.json").exists() => {
                return Err(publish_error(
                    "verify_node_registry",
                    "unbound generation node registry is not allowed",
                ));
            }
            None => {}
        }
        Ok(())
    }
}

fn valid_source_id(value: &str) -> bool {
    value.starts_with("src_")
        && value.len() == 36
        && value[4..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(unix)]
fn is_private_regular_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.permissions().mode() & 0o077 == 0
    })
}

#[cfg(not(unix))]
fn is_private_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
}

fn remove_directory_if_present(path: &Path, operation: &str) -> Result<(), CoreError> {
    if path.exists() {
        fs::remove_dir_all(path).map_err(|error| io_error(operation, error))?;
    }
    Ok(())
}

fn publish_error(operation: &str, message: &str) -> CoreError {
    CoreError::GenerationPublishFailed {
        operation: operation.to_owned(),
        message: message.to_owned(),
    }
}

fn write_sync(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = open_private_file(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(unix)]
fn create_private_dir_all(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;

    let mut builder = fs::DirBuilder::new();
    builder.recursive(true).mode(0o700).create(path)
}

#[cfg(not(unix))]
fn create_private_dir_all(path: &Path) -> std::io::Result<()> {
    fs::create_dir_all(path)
}

#[cfg(unix)]
fn create_private_dir(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;

    let mut builder = fs::DirBuilder::new();
    builder.mode(0o700).create(path)
}

#[cfg(not(unix))]
fn create_private_dir(path: &Path) -> std::io::Result<()> {
    fs::create_dir(path)
}

#[cfg(unix)]
fn set_private_dir_permissions(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn set_private_dir_permissions(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn open_private_file(path: &Path) -> std::io::Result<fs::File> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    Ok(file)
}

#[cfg(not(unix))]
fn open_private_file(path: &Path) -> std::io::Result<fs::File> {
    fs::File::create(path)
}

#[cfg(not(windows))]
fn sync_directory(path: &Path) -> std::io::Result<()> {
    fs::File::open(path)?.sync_all()
}

#[cfg(windows)]
fn sync_directory(_path: &Path) -> std::io::Result<()> {
    // Windows does not expose a portable directory fsync primitive. The Android/Linux
    // release path uses the implementation above; Windows remains a deterministic test host.
    Ok(())
}
