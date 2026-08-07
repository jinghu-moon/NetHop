use std::{fmt, fs, io::Write, path::PathBuf};

#[cfg(feature = "subscription-update")]
use nethop_subscription::{
    CandidateAcceptance, FetchClient, FetchPolicy, FetchRequest, ParserLimits, RequestProfile,
    SourceCache, SourceId,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const SING_BOX_LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/SagerNet/sing-box/releases/latest";
pub const MAX_RELEASE_RESPONSE_BYTES: usize = 256 * 1024;
pub const MAX_RUNTIME_STATE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CoreVersion {
    major: u32,
    minor: u32,
    patch: u32,
}

impl CoreVersion {
    pub fn parse(value: &str) -> Result<Self, CoreVersionCheckError> {
        let value = value.strip_prefix('v').unwrap_or(value);
        let mut components = value.split('.');
        let major = parse_component(components.next())?;
        let minor = parse_component(components.next())?;
        let patch = parse_component(components.next())?;
        if components.next().is_some() {
            return Err(CoreVersionCheckError::InvalidVersion);
        }
        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

impl fmt::Display for CoreVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl Serialize for CoreVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for CoreVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

fn parse_component(value: Option<&str>) -> Result<u32, CoreVersionCheckError> {
    let value = value.ok_or(CoreVersionCheckError::InvalidVersion)?;
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(CoreVersionCheckError::InvalidVersion);
    }
    value
        .parse()
        .map_err(|_| CoreVersionCheckError::InvalidVersion)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreUpdateAvailability {
    UpToDate,
    Available,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoreVersionStatus {
    current: CoreVersion,
    latest: CoreVersion,
    availability: CoreUpdateAvailability,
}

impl CoreVersionStatus {
    pub const fn current(&self) -> CoreVersion {
        self.current
    }

    pub const fn latest(&self) -> CoreVersion {
        self.latest
    }

    pub const fn availability(&self) -> CoreUpdateAvailability {
        self.availability
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseMetadata {
    version: CoreVersion,
}

impl ReleaseMetadata {
    pub fn parse(bytes: &[u8]) -> Result<Self, CoreVersionCheckError> {
        if bytes.is_empty() || bytes.len() > MAX_RELEASE_RESPONSE_BYTES {
            return Err(CoreVersionCheckError::ResponseSize);
        }
        let release: GithubRelease =
            serde_json::from_slice(bytes).map_err(|_| CoreVersionCheckError::InvalidResponse)?;
        if release.draft || release.prerelease {
            return Err(CoreVersionCheckError::UnstableRelease);
        }
        if release.tag_name.len() > 32 || release.tag_name.chars().any(char::is_control) {
            return Err(CoreVersionCheckError::InvalidVersion);
        }
        Ok(Self {
            version: CoreVersion::parse(&release.tag_name)?,
        })
    }

    pub const fn version(&self) -> CoreVersion {
        self.version
    }
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
}

pub trait CoreReleaseBodyFetcher {
    fn fetch_release_body(&mut self) -> Result<Vec<u8>, CoreVersionCheckError>;
}

pub trait CoreVersionStateSink {
    fn restore(
        &mut self,
    ) -> Result<Option<(CoreVersionStatus, Option<CoreVersion>)>, CoreVersionCheckError> {
        Ok(None)
    }

    fn persist(
        &mut self,
        status: &CoreVersionStatus,
        notification: &str,
    ) -> Result<(), CoreVersionCheckError>;
}

pub struct JsonCoreVersionStateStore {
    path: PathBuf,
}

impl JsonCoreVersionStateStore {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, CoreVersionCheckError> {
        let path = path.into();
        let parent = path.parent().ok_or(CoreVersionCheckError::StateIo)?;
        if !parent.is_dir()
            || fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink())
        {
            return Err(CoreVersionCheckError::StateIo);
        }
        Ok(Self { path })
    }
}

impl CoreVersionStateSink for JsonCoreVersionStateStore {
    fn restore(
        &mut self,
    ) -> Result<Option<(CoreVersionStatus, Option<CoreVersion>)>, CoreVersionCheckError> {
        let metadata = match fs::symlink_metadata(&self.path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err(CoreVersionCheckError::StateIo);
            }
            Ok(metadata) if metadata.len() as usize > MAX_RUNTIME_STATE_BYTES => {
                return Err(CoreVersionCheckError::StateIo);
            }
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(CoreVersionCheckError::StateIo),
        };
        if metadata.len() == 0 {
            return Err(CoreVersionCheckError::StateIo);
        }
        let document: serde_json::Value = serde_json::from_slice(
            &fs::read(&self.path).map_err(|_| CoreVersionCheckError::StateIo)?,
        )
        .map_err(|_| CoreVersionCheckError::StateIo)?;
        let Some(core_update) = document.get("core_update") else {
            return Ok(None);
        };
        let status: CoreVersionStatus = serde_json::from_value(
            core_update
                .get("status")
                .cloned()
                .ok_or(CoreVersionCheckError::StateIo)?,
        )
        .map_err(|_| CoreVersionCheckError::StateIo)?;
        let notification = core_update
            .get("notification")
            .and_then(serde_json::Value::as_str)
            .ok_or(CoreVersionCheckError::StateIo)?;
        let last_notified =
            matches!(notification, "posted" | "already_notified").then_some(status.latest());
        Ok(Some((status, last_notified)))
    }

    fn persist(
        &mut self,
        status: &CoreVersionStatus,
        notification: &str,
    ) -> Result<(), CoreVersionCheckError> {
        let mut document = match fs::symlink_metadata(&self.path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err(CoreVersionCheckError::StateIo);
            }
            Ok(metadata) if metadata.len() as usize > MAX_RUNTIME_STATE_BYTES => {
                return Err(CoreVersionCheckError::StateIo);
            }
            Ok(_) => {
                let bytes = fs::read(&self.path).map_err(|_| CoreVersionCheckError::StateIo)?;
                serde_json::from_slice::<serde_json::Value>(&bytes)
                    .map_err(|_| CoreVersionCheckError::StateIo)?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                serde_json::json!({})
            }
            Err(_) => return Err(CoreVersionCheckError::StateIo),
        };
        let object = document
            .as_object_mut()
            .ok_or(CoreVersionCheckError::StateIo)?;
        object.insert(
            "schema".into(),
            serde_json::Value::String("nethop.runtime.v1".into()),
        );
        object.insert(
            "core_update".into(),
            serde_json::json!({"status": status, "notification": notification}),
        );
        let bytes = serde_json::to_vec(&document).map_err(|_| CoreVersionCheckError::StateIo)?;
        if bytes.len() > MAX_RUNTIME_STATE_BYTES {
            return Err(CoreVersionCheckError::StateIo);
        }
        let temporary = self
            .path
            .with_extension(format!("json.{}.tmp", std::process::id()));
        if fs::symlink_metadata(&temporary).is_ok() {
            return Err(CoreVersionCheckError::StateIo);
        }
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| CoreVersionCheckError::StateIo)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|_| CoreVersionCheckError::StateIo)?;
        }
        file.write_all(&bytes)
            .map_err(|_| CoreVersionCheckError::StateIo)?;
        file.sync_all()
            .map_err(|_| CoreVersionCheckError::StateIo)?;
        fs::rename(&temporary, &self.path).map_err(|_| CoreVersionCheckError::StateIo)
    }
}

pub struct CoreVersionChecker<F> {
    fetcher: F,
    current: CoreVersion,
}

impl<F: CoreReleaseBodyFetcher> CoreVersionChecker<F> {
    pub const fn new(fetcher: F, current: CoreVersion) -> Self {
        Self { fetcher, current }
    }

    pub fn check(&mut self) -> Result<CoreVersionStatus, CoreVersionCheckError> {
        let release = ReleaseMetadata::parse(&self.fetcher.fetch_release_body()?)?;
        let latest = release.version();
        let availability = if latest > self.current {
            CoreUpdateAvailability::Available
        } else {
            CoreUpdateAvailability::UpToDate
        };
        Ok(CoreVersionStatus {
            current: self.current,
            latest,
            availability,
        })
    }
}

#[cfg(feature = "subscription-update")]
pub struct HttpCoreReleaseBodyFetcher {
    client: FetchClient,
    request: FetchRequest,
    cache: SourceCache,
}

#[cfg(feature = "subscription-update")]
impl Default for HttpCoreReleaseBodyFetcher {
    fn default() -> Self {
        let policy = FetchPolicy::default();
        let limits = ParserLimits::new(MAX_RELEASE_RESPONSE_BYTES, 1, 1, 1, 32)
            .expect("release response limits are below parser security ceilings");
        let request = FetchRequest::new(
            SourceId::new("sing-box-release").expect("fixed source ID is valid"),
            SING_BOX_LATEST_RELEASE_URL,
            std::iter::empty::<&str>(),
            RequestProfile::NetHopGeneric,
            &policy,
        )
        .expect("fixed release URL is valid and HTTPS");
        Self {
            client: FetchClient::new(policy, limits),
            request,
            cache: SourceCache::default(),
        }
    }
}

#[cfg(feature = "subscription-update")]
impl CoreReleaseBodyFetcher for HttpCoreReleaseBodyFetcher {
    fn fetch_release_body(&mut self) -> Result<Vec<u8>, CoreVersionCheckError> {
        self.client
            .fetch(&self.request, &self.cache, |_| {
                CandidateAcceptance::Accepted
            })
            .map(|outcome| outcome.body().to_vec())
            .map_err(|_| CoreVersionCheckError::Fetch)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum CoreVersionCheckError {
    #[error("core version is invalid")]
    InvalidVersion,
    #[error("release response is empty or exceeds the bounded limit")]
    ResponseSize,
    #[error("release response is invalid")]
    InvalidResponse,
    #[error("release is a draft or prerelease")]
    UnstableRelease,
    #[cfg(feature = "subscription-update")]
    #[error("release request failed")]
    Fetch,
    #[error("runtime state could not be persisted")]
    StateIo,
}
