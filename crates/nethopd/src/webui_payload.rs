use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use base64::{Engine as _, engine::general_purpose};
use nethop_protocol::WebUiPayloadNamespace;
use thiserror::Error;

pub const MAX_PAYLOAD_CHUNK_BYTES: usize = 12 * 1024;
pub const MAX_PAYLOAD_BYTES: usize = 1024 * 1024;
pub const PAYLOAD_TTL: Duration = Duration::from_secs(15 * 60);
const MAX_CLEANUP_ENTRIES: usize = 1_024;

#[derive(Debug, Clone)]
pub struct WebUiPayloadStore {
    root: PathBuf,
}

impl WebUiPayloadStore {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, WebUiPayloadError> {
        let root = root.into();
        if !root.is_absolute() {
            return Err(WebUiPayloadError::InvalidRoot);
        }
        fs::create_dir_all(&root).map_err(|_| WebUiPayloadError::Io)?;
        set_private_directory(&root)?;
        let metadata = fs::symlink_metadata(&root).map_err(|_| WebUiPayloadError::Io)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(WebUiPayloadError::InvalidRoot);
        }
        let store = Self { root };
        for namespace in [
            WebUiPayloadNamespace::Config,
            WebUiPayloadNamespace::Subscription,
            WebUiPayloadNamespace::Backup,
            WebUiPayloadNamespace::Node,
        ] {
            let directory = store.namespace_directory(namespace);
            fs::create_dir_all(&directory).map_err(|_| WebUiPayloadError::Io)?;
            set_private_directory(&directory)?;
            let metadata = fs::symlink_metadata(&directory).map_err(|_| WebUiPayloadError::Io)?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err(WebUiPayloadError::InvalidRoot);
            }
        }
        let _ = store.cleanup_expired(SystemTime::now(), PAYLOAD_TTL);
        Ok(store)
    }

    pub fn create(&self, namespace: WebUiPayloadNamespace) -> Result<String, WebUiPayloadError> {
        for _ in 0..8 {
            let handle = random_handle()?;
            let path = self.payload_path(namespace, &handle)?;
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            configure_private_open(&mut options);
            match options.open(path) {
                Ok(file) => {
                    set_private_file(&file)?;
                    return Ok(handle);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => return Err(WebUiPayloadError::Io),
            }
        }
        Err(WebUiPayloadError::Conflict)
    }

    pub fn append(
        &self,
        namespace: WebUiPayloadNamespace,
        handle: &str,
        encoded_chunk: &str,
    ) -> Result<usize, WebUiPayloadError> {
        let decoded = decode_chunk(encoded_chunk)?;
        if decoded.is_empty() || decoded.len() > MAX_PAYLOAD_CHUNK_BYTES {
            return Err(WebUiPayloadError::InvalidChunk);
        }
        let path = self.payload_path(namespace, handle)?;
        let mut file = open_owned_payload(&path, true)?;
        let current = usize::try_from(file.metadata().map_err(|_| WebUiPayloadError::Io)?.len())
            .map_err(|_| WebUiPayloadError::LimitExceeded)?;
        let next = current
            .checked_add(decoded.len())
            .ok_or(WebUiPayloadError::LimitExceeded)?;
        if next > MAX_PAYLOAD_BYTES {
            drop(file);
            let _ = fs::remove_file(path);
            return Err(WebUiPayloadError::LimitExceeded);
        }
        file.write_all(&decoded)
            .map_err(|_| WebUiPayloadError::Io)?;
        file.flush().map_err(|_| WebUiPayloadError::Io)?;
        Ok(next)
    }

    pub fn consume(
        &self,
        namespace: WebUiPayloadNamespace,
        handle: &str,
    ) -> Result<Vec<u8>, WebUiPayloadError> {
        let path = self.payload_path(namespace, handle)?;
        validate_owned_payload(&path)?;
        let consumed = path.with_file_name(format!(".consume-{handle}"));
        if fs::symlink_metadata(&consumed).is_ok() {
            return Err(WebUiPayloadError::Conflict);
        }
        fs::rename(&path, &consumed).map_err(|_| WebUiPayloadError::Unavailable)?;
        let result = read_consumed(&consumed);
        let _ = fs::remove_file(&consumed);
        result
    }

    pub fn remove(
        &self,
        namespace: WebUiPayloadNamespace,
        handle: &str,
    ) -> Result<(), WebUiPayloadError> {
        let path = self.payload_path(namespace, handle)?;
        match fs::symlink_metadata(&path) {
            Ok(_) => {
                validate_owned_payload(&path)?;
                fs::remove_file(path).map_err(|_| WebUiPayloadError::Io)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(WebUiPayloadError::Io),
        }
    }

    pub fn cleanup_expired(
        &self,
        now: SystemTime,
        ttl: Duration,
    ) -> Result<usize, WebUiPayloadError> {
        let mut removed = 0;
        for namespace in [
            WebUiPayloadNamespace::Config,
            WebUiPayloadNamespace::Subscription,
            WebUiPayloadNamespace::Backup,
            WebUiPayloadNamespace::Node,
        ] {
            let directory = self.namespace_directory(namespace);
            for entry in fs::read_dir(directory)
                .map_err(|_| WebUiPayloadError::Io)?
                .take(MAX_CLEANUP_ENTRIES)
            {
                let entry = entry.map_err(|_| WebUiPayloadError::Io)?;
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                let handle = name.strip_prefix(".consume-").unwrap_or(name);
                if !valid_handle(handle) {
                    continue;
                }
                let path = entry.path();
                let metadata = fs::symlink_metadata(&path).map_err(|_| WebUiPayloadError::Io)?;
                if !metadata.is_file() || metadata.file_type().is_symlink() {
                    continue;
                }
                let Ok(age) =
                    now.duration_since(metadata.modified().map_err(|_| WebUiPayloadError::Io)?)
                else {
                    continue;
                };
                if age >= ttl {
                    fs::remove_file(path).map_err(|_| WebUiPayloadError::Io)?;
                    removed += 1;
                }
            }
        }
        Ok(removed)
    }

    fn namespace_directory(&self, namespace: WebUiPayloadNamespace) -> PathBuf {
        self.root.join(match namespace {
            WebUiPayloadNamespace::Config => "config",
            WebUiPayloadNamespace::Subscription => "subscription",
            WebUiPayloadNamespace::Backup => "backup",
            WebUiPayloadNamespace::Node => "node",
        })
    }

    fn payload_path(
        &self,
        namespace: WebUiPayloadNamespace,
        handle: &str,
    ) -> Result<PathBuf, WebUiPayloadError> {
        if !valid_handle(handle) {
            return Err(WebUiPayloadError::InvalidHandle);
        }
        Ok(self.namespace_directory(namespace).join(handle))
    }
}

fn random_handle() -> Result<String, WebUiPayloadError> {
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random).map_err(|_| WebUiPayloadError::Unavailable)?;
    let mut handle = String::with_capacity(34);
    handle.push_str("p_");
    for byte in random {
        use std::fmt::Write as _;
        write!(&mut handle, "{byte:02x}").expect("writing to a String cannot fail");
    }
    Ok(handle)
}

fn valid_handle(handle: &str) -> bool {
    handle.len() == 34
        && handle.starts_with("p_")
        && handle[2..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn decode_chunk(value: &str) -> Result<Vec<u8>, WebUiPayloadError> {
    if value.is_empty() || value.len() > 16 * 1024 {
        return Err(WebUiPayloadError::InvalidChunk);
    }
    for engine in [
        &general_purpose::STANDARD,
        &general_purpose::STANDARD_NO_PAD,
        &general_purpose::URL_SAFE,
        &general_purpose::URL_SAFE_NO_PAD,
    ] {
        if let Ok(decoded) = engine.decode(value) {
            return Ok(decoded);
        }
    }
    Err(WebUiPayloadError::InvalidChunk)
}

fn validate_owned_payload(path: &Path) -> Result<(), WebUiPayloadError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            WebUiPayloadError::Unavailable
        } else {
            WebUiPayloadError::Io
        }
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(WebUiPayloadError::UnsafeFile);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(WebUiPayloadError::UnsafeFile);
        }
    }
    Ok(())
}

fn open_owned_payload(path: &Path, append: bool) -> Result<File, WebUiPayloadError> {
    validate_owned_payload(path)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).append(append);
    configure_private_open(&mut options);
    let file = options
        .open(path)
        .map_err(|_| WebUiPayloadError::Unavailable)?;
    set_private_file(&file)?;
    Ok(file)
}

fn read_consumed(path: &Path) -> Result<Vec<u8>, WebUiPayloadError> {
    let file = open_owned_payload(path, false)?;
    let length = usize::try_from(file.metadata().map_err(|_| WebUiPayloadError::Io)?.len())
        .map_err(|_| WebUiPayloadError::LimitExceeded)?;
    if length == 0 || length > MAX_PAYLOAD_BYTES {
        return Err(WebUiPayloadError::LimitExceeded);
    }
    let mut bytes = Vec::with_capacity(length);
    file.take((MAX_PAYLOAD_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| WebUiPayloadError::Io)?;
    if bytes.len() != length || bytes.len() > MAX_PAYLOAD_BYTES {
        return Err(WebUiPayloadError::LimitExceeded);
    }
    Ok(bytes)
}

#[cfg(unix)]
fn configure_private_open(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
}

#[cfg(not(unix))]
fn configure_private_open(_options: &mut OpenOptions) {}

#[cfg(unix)]
fn set_private_directory(path: &Path) -> Result<(), WebUiPayloadError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|_| WebUiPayloadError::Io)
}

#[cfg(not(unix))]
fn set_private_directory(_path: &Path) -> Result<(), WebUiPayloadError> {
    Ok(())
}

#[cfg(unix)]
fn set_private_file(file: &File) -> Result<(), WebUiPayloadError> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|_| WebUiPayloadError::Io)
}

#[cfg(not(unix))]
fn set_private_file(_file: &File) -> Result<(), WebUiPayloadError> {
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum WebUiPayloadError {
    #[error("payload root is invalid")]
    InvalidRoot,
    #[error("payload handle is invalid")]
    InvalidHandle,
    #[error("payload chunk is invalid")]
    InvalidChunk,
    #[error("payload limit is exceeded")]
    LimitExceeded,
    #[error("payload already exists")]
    Conflict,
    #[error("payload file is unsafe")]
    UnsafeFile,
    #[error("payload is unavailable")]
    Unavailable,
    #[error("payload storage failed")]
    Io,
}
