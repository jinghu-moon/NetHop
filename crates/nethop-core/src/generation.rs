use std::{
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
pub struct GenerationManifest {
    pub schema: String,
    pub generation: GenerationId,
    pub config_sha256: String,
    pub node_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_config_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    generation: GenerationId,
    config: ManagedConfig,
    manifest: GenerationManifest,
}

impl Candidate {
    pub fn new(generation: GenerationId, config: ManagedConfig) -> Self {
        let manifest = GenerationManifest {
            schema: MANIFEST_SCHEMA.to_owned(),
            generation,
            config_sha256: config.digest_sha256().to_owned(),
            node_count: config.node_count(),
            source_config_digest: None,
            source_ids: Vec::new(),
        };
        Self {
            generation,
            config,
            manifest,
        }
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
            || !is_regular_non_symlink(&config)
            || !is_regular_non_symlink(&manifest)
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
        let digest = Sha256::digest(&config)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        if manifest.schema != MANIFEST_SCHEMA
            || manifest.generation != generation
            || manifest.config_sha256 != digest
        {
            return Err(publish_error(
                "verify_manifest",
                "generation manifest does not match config",
            ));
        }
        Ok(())
    }
}

fn is_regular_non_symlink(path: &Path) -> bool {
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
