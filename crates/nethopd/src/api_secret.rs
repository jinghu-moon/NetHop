use std::{
    fmt, fs,
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU32, Ordering},
};

use thiserror::Error;

const SECRET_BYTES: usize = 32;
const SECRET_HEX_BYTES: usize = SECRET_BYTES * 2;
const MAX_TEMP_ATTEMPTS: u32 = 16;
static TEMP_SEQUENCE: AtomicU32 = AtomicU32::new(0);

#[derive(Clone, PartialEq, Eq)]
pub struct ApiSecret(String);

impl ApiSecret {
    pub fn expose_for_composer(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ApiSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ApiSecret([REDACTED])")
    }
}

pub trait SecretEntropy {
    fn fill(&mut self, output: &mut [u8]) -> Result<(), ApiSecretError>;
}

#[derive(Debug, Default)]
pub struct SystemSecretEntropy;

#[cfg(unix)]
impl SecretEntropy for SystemSecretEntropy {
    fn fill(&mut self, output: &mut [u8]) -> Result<(), ApiSecretError> {
        use std::io::Read;

        File::open("/dev/urandom")
            .and_then(|mut file| file.read_exact(output))
            .map_err(|_| ApiSecretError::EntropyUnavailable)
    }
}

#[cfg(not(unix))]
impl SecretEntropy for SystemSecretEntropy {
    fn fill(&mut self, _output: &mut [u8]) -> Result<(), ApiSecretError> {
        Err(ApiSecretError::EntropyUnavailable)
    }
}

pub struct ApiSecretStore {
    path: PathBuf,
}

impl ApiSecretStore {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, ApiSecretError> {
        let path = path.into();
        let parent = path.parent().ok_or(ApiSecretError::InvalidPath)?;
        if !path.is_absolute() || path.file_name().is_none() {
            return Err(ApiSecretError::InvalidPath);
        }
        let metadata = fs::symlink_metadata(parent).map_err(|_| ApiSecretError::InvalidPath)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(ApiSecretError::InvalidPath);
        }
        if parent
            .canonicalize()
            .map_err(|_| ApiSecretError::InvalidPath)?
            != parent
        {
            return Err(ApiSecretError::InvalidPath);
        }
        Ok(Self { path })
    }

    pub fn load_or_create(&self) -> Result<ApiSecret, ApiSecretError> {
        self.load_or_create_with(&mut SystemSecretEntropy)
    }

    pub fn load_or_create_with(
        &self,
        entropy: &mut impl SecretEntropy,
    ) -> Result<ApiSecret, ApiSecretError> {
        if self.path.exists() {
            return load_secret(&self.path);
        }
        let mut random = [0_u8; SECRET_BYTES];
        entropy.fill(&mut random)?;
        let encoded = encode_hex(random);
        let temporary = self.reserve_temporary()?;
        let result = publish_secret(&temporary, &self.path, &encoded);
        let _ = fs::remove_file(&temporary);
        match result {
            Ok(()) => load_secret(&self.path),
            Err(ApiSecretError::AlreadyCreated) => load_secret(&self.path),
            Err(error) => Err(error),
        }
    }

    fn reserve_temporary(&self) -> Result<PathBuf, ApiSecretError> {
        let parent = self.path.parent().ok_or(ApiSecretError::InvalidPath)?;
        for _ in 0..MAX_TEMP_ATTEMPTS {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(".api-secret-{}-{sequence}.tmp", std::process::id()));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => {
                    if let Err(error) = set_private_file(&file) {
                        drop(file);
                        let _ = fs::remove_file(&path);
                        return Err(error);
                    }
                    return Ok(path);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(_) => return Err(ApiSecretError::CreateFailed),
            }
        }
        Err(ApiSecretError::CreateFailed)
    }
}

fn publish_secret(
    temporary: &Path,
    final_path: &Path,
    encoded: &str,
) -> Result<(), ApiSecretError> {
    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(temporary)
        .map_err(|_| ApiSecretError::CreateFailed)?;
    file.write_all(encoded.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|_| ApiSecretError::CreateFailed)?;
    drop(file);
    fs::hard_link(temporary, final_path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            ApiSecretError::AlreadyCreated
        } else {
            ApiSecretError::CreateFailed
        }
    })?;
    sync_parent(final_path)
}

fn load_secret(path: &Path) -> Result<ApiSecret, ApiSecretError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| ApiSecretError::InvalidSecret)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() != SECRET_HEX_BYTES as u64
        || !private_file(&metadata)
    {
        return Err(ApiSecretError::InvalidSecret);
    }
    let value = fs::read_to_string(path).map_err(|_| ApiSecretError::InvalidSecret)?;
    if value.len() != SECRET_HEX_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ApiSecretError::InvalidSecret);
    }
    Ok(ApiSecret(value))
}

fn encode_hex(bytes: [u8; SECRET_BYTES]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(SECRET_HEX_BYTES);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(unix)]
fn set_private_file(file: &File) -> Result<(), ApiSecretError> {
    use std::os::unix::fs::PermissionsExt;

    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|_| ApiSecretError::CreateFailed)
}

#[cfg(not(unix))]
fn set_private_file(_file: &File) -> Result<(), ApiSecretError> {
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

#[cfg(unix)]
fn sync_parent(path: &Path) -> Result<(), ApiSecretError> {
    File::open(path.parent().ok_or(ApiSecretError::InvalidPath)?)
        .and_then(|file| file.sync_all())
        .map_err(|_| ApiSecretError::CreateFailed)
}

#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> Result<(), ApiSecretError> {
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ApiSecretError {
    #[error("API secret path must have an absolute real directory parent")]
    InvalidPath,
    #[error("system entropy is unavailable")]
    EntropyUnavailable,
    #[error("API secret could not be created atomically")]
    CreateFailed,
    #[error("API secret was created concurrently")]
    AlreadyCreated,
    #[error("API secret is not a private regular lowercase hexadecimal file")]
    InvalidSecret,
}
