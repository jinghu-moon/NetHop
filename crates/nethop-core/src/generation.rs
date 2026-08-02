use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

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
        };
        Self {
            generation,
            config,
            manifest,
        }
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

#[derive(Debug, Clone)]
pub struct GenerationStore {
    root: PathBuf,
}

impl GenerationStore {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, CoreError> {
        let root = root.into();
        fs::create_dir_all(root.join("generations"))
            .map_err(|error| io_error("create_root", error))?;
        Ok(Self { root })
    }

    pub fn current_generation(&self) -> Result<Option<GenerationId>, CoreError> {
        let path = self.root.join("current");
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

    pub fn publish<F>(&self, candidate: &Candidate, validate: F) -> Result<(), CoreError>
    where
        F: FnOnce(&[u8]) -> Result<(), CoreError>,
    {
        let generations = self.root.join("generations");
        let final_dir = generations.join(candidate.generation.get().to_string());
        if final_dir.exists() {
            return Err(CoreError::GenerationPublishFailed {
                operation: "reserve_generation".into(),
                message: "generation already exists".into(),
            });
        }
        let temp_dir = generations.join(format!(
            ".candidate-{}-{}",
            candidate.generation.get(),
            std::process::id()
        ));
        if temp_dir.exists() {
            fs::remove_dir_all(&temp_dir)
                .map_err(|error| io_error("remove_stale_candidate", error))?;
        }
        fs::create_dir(&temp_dir).map_err(|error| io_error("create_candidate", error))?;
        let result = self.publish_inner(candidate, &temp_dir, &final_dir, validate);
        if result.is_err() {
            let _ = fs::remove_dir_all(&temp_dir);
        }
        result
    }

    fn publish_inner<F>(
        &self,
        candidate: &Candidate,
        temp_dir: &Path,
        final_dir: &Path,
        validate: F,
    ) -> Result<(), CoreError>
    where
        F: FnOnce(&[u8]) -> Result<(), CoreError>,
    {
        let config_path = temp_dir.join("config.json");
        let manifest_path = temp_dir.join("manifest.json");
        write_sync(&config_path, candidate.config.bytes())
            .map_err(|error| io_error("write_config", error))?;
        let manifest = serde_json::to_vec(&candidate.manifest)
            .map_err(|error| CoreError::SerializationFailure(error.to_string()))?;
        write_sync(&manifest_path, &manifest).map_err(|error| io_error("write_manifest", error))?;
        sync_directory(temp_dir).map_err(|error| io_error("sync_candidate", error))?;
        validate(candidate.config.bytes())?;
        fs::rename(temp_dir, final_dir).map_err(|error| io_error("publish_generation", error))?;
        sync_directory(final_dir.parent().expect("generation directory has parent"))
            .map_err(|error| io_error("sync_generations", error))?;
        let current_temp = self
            .root
            .join(format!(".current-{}", candidate.generation.get()));
        write_sync(
            &current_temp,
            format!("{}\n", candidate.generation.get()).as_bytes(),
        )
        .map_err(|error| io_error("write_current", error))?;
        fs::rename(&current_temp, self.root.join("current"))
            .map_err(|error| io_error("publish_current", error))?;
        sync_directory(&self.root).map_err(|error| io_error("sync_root", error))?;
        Ok(())
    }
}

fn write_sync(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
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
