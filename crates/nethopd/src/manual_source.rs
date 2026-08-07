use std::{fmt, fs, path::PathBuf};

use nethop_subscription::{Digest, FormatHint, SourceId};
use thiserror::Error;

use crate::worker_config::atomic_write;

const HEADER_PREFIX: &str = "nethop-manual-source-v1 ";
const MAX_BODY_BYTES: usize = 5 * 1024 * 1024;
const MANUAL_SOURCE_ID: &str = "src_00000000000000000000000000000000";

#[derive(Clone, PartialEq, Eq)]
pub struct ManualSource {
    format_hint: FormatHint,
    bytes: Vec<u8>,
    digest: String,
}

impl ManualSource {
    pub const fn format_hint(&self) -> FormatHint {
        self.format_hint
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn digest(&self) -> &str {
        &self.digest
    }

    pub fn source_id() -> SourceId {
        SourceId::new(MANUAL_SOURCE_ID).expect("manual source ID is frozen and valid")
    }
}

impl fmt::Debug for ManualSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ManualSource")
            .field("format_hint", &self.format_hint)
            .field("body_bytes", &self.bytes.len())
            .field("digest", &self.digest)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct ManualSourceStore {
    path: PathBuf,
}

impl ManualSourceStore {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, ManualSourceError> {
        let path = path.into();
        if !path.is_absolute() || path.file_name().is_none() {
            return Err(ManualSourceError::InvalidPath);
        }
        let parent = path.parent().ok_or(ManualSourceError::InvalidPath)?;
        let metadata = fs::symlink_metadata(parent).map_err(|_| ManualSourceError::InvalidPath)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(ManualSourceError::InvalidPath);
        }
        Ok(Self { path })
    }

    pub fn load(&self) -> Result<Option<ManualSource>, ManualSourceError> {
        let metadata = match fs::symlink_metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(ManualSourceError::Read),
        };
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() > (MAX_BODY_BYTES + 128) as u64
            || !private_file(&metadata)
        {
            return Err(ManualSourceError::InvalidFile);
        }
        decode(&fs::read(&self.path).map_err(|_| ManualSourceError::Read)?).map(Some)
    }

    pub fn replace(
        &self,
        format_hint: FormatHint,
        bytes: &[u8],
    ) -> Result<ManualSourceCheckpoint, ManualSourceError> {
        validate_body(bytes)?;
        let checkpoint = ManualSourceCheckpoint {
            previous: match fs::read(&self.path) {
                Ok(previous) => Some(previous),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(_) => return Err(ManualSourceError::Read),
            },
        };
        atomic_write(&self.path, &encode(format_hint, bytes))
            .map_err(|_| ManualSourceError::Write)?;
        Ok(checkpoint)
    }

    pub fn restore(&self, checkpoint: ManualSourceCheckpoint) -> Result<(), ManualSourceError> {
        match checkpoint.previous {
            Some(previous) => {
                decode(&previous)?;
                atomic_write(&self.path, &previous).map_err(|_| ManualSourceError::Write)
            }
            None => match fs::remove_file(&self.path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(_) => Err(ManualSourceError::Write),
            },
        }
    }
}

#[derive(Debug)]
pub struct ManualSourceCheckpoint {
    previous: Option<Vec<u8>>,
}

fn encode(format_hint: FormatHint, bytes: &[u8]) -> Vec<u8> {
    let header = format!("{HEADER_PREFIX}{}\n", format_name(format_hint));
    let mut encoded = Vec::with_capacity(header.len() + bytes.len());
    encoded.extend_from_slice(header.as_bytes());
    encoded.extend_from_slice(bytes);
    encoded
}

fn decode(encoded: &[u8]) -> Result<ManualSource, ManualSourceError> {
    let newline = encoded
        .iter()
        .position(|byte| *byte == b'\n')
        .ok_or(ManualSourceError::InvalidFile)?;
    let header =
        std::str::from_utf8(&encoded[..newline]).map_err(|_| ManualSourceError::InvalidFile)?;
    let format = header
        .strip_prefix(HEADER_PREFIX)
        .and_then(parse_format)
        .ok_or(ManualSourceError::InvalidFile)?;
    let bytes = encoded[newline + 1..].to_vec();
    validate_body(&bytes)?;
    Ok(ManualSource {
        format_hint: format,
        digest: Digest::sha256(&bytes).hex(),
        bytes,
    })
}

fn validate_body(bytes: &[u8]) -> Result<(), ManualSourceError> {
    if bytes.is_empty() || bytes.len() > MAX_BODY_BYTES {
        Err(ManualSourceError::InvalidBody)
    } else {
        Ok(())
    }
}

const fn format_name(format: FormatHint) -> &'static str {
    match format {
        FormatHint::Auto => "auto",
        FormatHint::UriList => "uri_list",
        FormatHint::Base64List => "base64_list",
        FormatHint::ClashYaml => "clash_yaml",
        FormatHint::SingboxJson => "singbox_json",
        FormatHint::IniProfile => "ini_profile",
        FormatHint::SurfboardIni => "surfboard_ini",
    }
}

fn parse_format(value: &str) -> Option<FormatHint> {
    match value {
        "auto" => Some(FormatHint::Auto),
        "uri_list" => Some(FormatHint::UriList),
        "base64_list" => Some(FormatHint::Base64List),
        "clash_yaml" => Some(FormatHint::ClashYaml),
        "singbox_json" => Some(FormatHint::SingboxJson),
        "ini_profile" => Some(FormatHint::IniProfile),
        "surfboard_ini" => Some(FormatHint::SurfboardIni),
        _ => None,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ManualSourceError {
    #[error("manual source path is invalid")]
    InvalidPath,
    #[error("manual source file is invalid")]
    InvalidFile,
    #[error("manual source body is invalid")]
    InvalidBody,
    #[error("manual source could not be read")]
    Read,
    #[error("manual source could not be written")]
    Write,
}
