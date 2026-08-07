use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use nethop_subscription::{
    CandidateAcceptance, FetchClient, FetchPolicy, FetchRequest, ParserLimits, RequestProfile,
    SourceCache, SourceId,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    CandidateChecker, PreparedRuleSet, PublishedRuleSet, RuleSetPreparation, RuleSetProvider,
    RuleSetProviderManifest, RuleSetPurpose, RuleSetReplaceOutcome, RuleSetStore,
    worker_config::atomic_write,
};

const CACHE_SCHEMA: &str = "nethop-ruleset-cache-v1";

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleSetCacheMetadata {
    schema: String,
    body_sha256: String,
    endpoint_sha256: String,
    etag: Option<String>,
    last_modified: Option<String>,
}

pub trait RuleSetBodyFetcher {
    fn fetch(&mut self, provider: &RuleSetProvider) -> Result<Vec<u8>, RuleSetFetchError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum RuleSetFetchError {
    #[error("rule-set fetch failed")]
    Fetch,
    #[error("rule-set response content type is not allowed")]
    ContentType,
    #[error("rule-set conditional cache update failed")]
    Cache,
}

#[derive(Debug, Clone)]
pub struct HttpRuleSetBodyFetcher {
    client: FetchClient,
    policy: FetchPolicy,
    limits: ParserLimits,
    caches: HashMap<String, SourceCache>,
    cache_root: Option<PathBuf>,
}

impl Default for HttpRuleSetBodyFetcher {
    fn default() -> Self {
        let policy = FetchPolicy::default();
        let limits = ParserLimits::default();
        Self {
            client: FetchClient::new(policy.clone(), limits),
            policy,
            limits,
            caches: HashMap::new(),
            cache_root: None,
        }
    }
}

impl HttpRuleSetBodyFetcher {
    pub fn with_cache_root(mut self, root: impl Into<PathBuf>) -> Result<Self, RuleSetFetchError> {
        let root = root.into();
        let metadata = fs::symlink_metadata(&root).map_err(|_| RuleSetFetchError::Cache)?;
        if !root.is_absolute() || !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(RuleSetFetchError::Cache);
        }
        self.cache_root = Some(root.canonicalize().map_err(|_| RuleSetFetchError::Cache)?);
        Ok(self)
    }

    fn cache_paths(&self, provider: &RuleSetProvider) -> Option<(PathBuf, PathBuf)> {
        self.cache_root.as_ref().map(|root| {
            (
                root.join(format!("{}.body", provider.id())),
                root.join(format!("{}.json", provider.id())),
            )
        })
    }

    fn restore_cache(
        &self,
        provider: &RuleSetProvider,
        endpoint_digest: nethop_subscription::Digest,
    ) -> SourceCache {
        let Some((body_path, metadata_path)) = self.cache_paths(provider) else {
            return SourceCache::default();
        };
        let restored = (|| {
            let body_metadata = checked_cache_file(&body_path, provider.max_bytes())?;
            let metadata_metadata = checked_cache_file(&metadata_path, 32 * 1024)?;
            if body_metadata.len() < 4 || metadata_metadata.len() == 0 {
                return None;
            }
            let body = fs::read(body_path).ok()?;
            let metadata: RuleSetCacheMetadata =
                serde_json::from_slice(&fs::read(metadata_path).ok()?).ok()?;
            if metadata.schema != CACHE_SCHEMA
                || !body.starts_with(b"SRS")
                || metadata.body_sha256 != nethop_subscription::Digest::sha256(&body).hex()
                || metadata.endpoint_sha256 != endpoint_digest.hex()
            {
                return None;
            }
            let mut cache = SourceCache::default();
            cache
                .restore(
                    body,
                    metadata.etag,
                    metadata.last_modified,
                    endpoint_digest,
                    &self.limits,
                )
                .ok()?;
            Some(cache)
        })();
        restored.unwrap_or_default()
    }

    fn persist_cache(
        &self,
        provider: &RuleSetProvider,
        cache: &SourceCache,
    ) -> Result<(), RuleSetFetchError> {
        let Some((body_path, metadata_path)) = self.cache_paths(provider) else {
            return Ok(());
        };
        let body = cache.last_known_good().ok_or(RuleSetFetchError::Cache)?;
        let (etag, last_modified, endpoint) = cache.validator_snapshot();
        let metadata = RuleSetCacheMetadata {
            schema: CACHE_SCHEMA.to_owned(),
            body_sha256: nethop_subscription::Digest::sha256(body).hex(),
            endpoint_sha256: endpoint.ok_or(RuleSetFetchError::Cache)?.hex(),
            etag: etag.map(str::to_owned),
            last_modified: last_modified.map(str::to_owned),
        };
        let metadata = serde_json::to_vec(&metadata).map_err(|_| RuleSetFetchError::Cache)?;
        atomic_write(&body_path, body).map_err(|_| RuleSetFetchError::Cache)?;
        atomic_write(&metadata_path, &metadata).map_err(|_| RuleSetFetchError::Cache)
    }
}

impl RuleSetBodyFetcher for HttpRuleSetBodyFetcher {
    fn fetch(&mut self, provider: &RuleSetProvider) -> Result<Vec<u8>, RuleSetFetchError> {
        let request = FetchRequest::new(
            SourceId::new(format!("resource:ruleset:{}", provider.id()))
                .map_err(|_| RuleSetFetchError::Fetch)?,
            provider.source_url(),
            std::iter::empty::<&str>(),
            RequestProfile::NetHopGeneric,
            &self.policy,
        )
        .map_err(|_| RuleSetFetchError::Fetch)?;
        let endpoint_digest = request
            .endpoints()
            .first()
            .ok_or(RuleSetFetchError::Fetch)?
            .origin_digest();
        if !self.caches.contains_key(provider.id()) {
            let cache = self.restore_cache(provider, endpoint_digest);
            self.caches.insert(provider.id().to_owned(), cache);
        }
        let cache = self
            .caches
            .get_mut(provider.id())
            .expect("cache entry was initialized");
        let outcome = self
            .client
            .fetch(&request, cache, |bytes| {
                if bytes.len() <= provider.max_bytes() && bytes.starts_with(b"SRS") {
                    CandidateAcceptance::Accepted
                } else {
                    CandidateAcceptance::FormatRejected
                }
            })
            .map_err(|_| RuleSetFetchError::Fetch)?;
        if !outcome.was_not_modified() {
            let content_type = outcome
                .content_type()
                .and_then(|value| value.split(';').next())
                .map(str::trim);
            if !provider.expected_content_types().iter().any(|expected| {
                content_type.is_some_and(|actual| actual.eq_ignore_ascii_case(expected))
            }) {
                return Err(RuleSetFetchError::ContentType);
            }
        }
        cache
            .commit(&outcome, &self.limits)
            .map_err(|_| RuleSetFetchError::Cache)?;
        if !outcome.was_not_modified() {
            let cache = self
                .caches
                .get(provider.id())
                .expect("cache entry was committed");
            self.persist_cache(provider, cache)?;
        }
        Ok(outcome.body().to_vec())
    }
}

fn checked_cache_file(path: &Path, max_bytes: usize) -> Option<fs::Metadata> {
    let metadata = fs::symlink_metadata(path).ok()?;
    (metadata.is_file()
        && !metadata.file_type().is_symlink()
        && metadata.len() <= max_bytes as u64
        && private_cache_file(&metadata))
    .then_some(metadata)
}

#[cfg(target_os = "android")]
fn private_cache_file(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    metadata.uid() == 0 && metadata.permissions().mode() & 0o077 == 0
}

#[cfg(all(unix, not(target_os = "android")))]
fn private_cache_file(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o077 == 0
}

#[cfg(not(unix))]
fn private_cache_file(_metadata: &fs::Metadata) -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum RuleSetUpdateError {
    #[error("rule-set provider fetch failed")]
    Fetch,
    #[error("rule-set provider manifest is incomplete")]
    MissingProvider,
    #[error("rule-set candidate admission or publication failed")]
    Admission,
    #[error("rule-set update transaction is in the wrong state")]
    InvalidState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleSetUpdatePreparation {
    Unchanged,
    Prepared,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuleSetDigestSnapshot {
    domain_sha256: String,
    ip_sha256: String,
}

impl RuleSetDigestSnapshot {
    pub fn domain_sha256(&self) -> &str {
        &self.domain_sha256
    }

    pub fn ip_sha256(&self) -> &str {
        &self.ip_sha256
    }
}

#[derive(Debug)]
enum RuleSetUpdateTransaction {
    Prepared(PreparedRuleSet),
    Published(PublishedRuleSet),
}

pub trait RuntimeRuleSetUpdateSource {
    fn is_available(&self) -> bool {
        true
    }
    fn prepare_update(&mut self) -> Result<RuleSetUpdatePreparation, RuleSetUpdateError>;
    fn publish_update(&mut self) -> Result<(), RuleSetUpdateError>;
    fn commit_update(&mut self) -> Result<(), RuleSetUpdateError>;
    fn rollback_update(&mut self) -> Result<(), RuleSetUpdateError>;
    fn snapshot(&self) -> Result<RuleSetDigestSnapshot, RuleSetUpdateError> {
        Err(RuleSetUpdateError::InvalidState)
    }
}

#[derive(Debug, Default)]
pub struct UnavailableRuleSetUpdateSource;

impl RuntimeRuleSetUpdateSource for UnavailableRuleSetUpdateSource {
    fn is_available(&self) -> bool {
        false
    }

    fn prepare_update(&mut self) -> Result<RuleSetUpdatePreparation, RuleSetUpdateError> {
        Err(RuleSetUpdateError::InvalidState)
    }

    fn publish_update(&mut self) -> Result<(), RuleSetUpdateError> {
        Err(RuleSetUpdateError::InvalidState)
    }

    fn commit_update(&mut self) -> Result<(), RuleSetUpdateError> {
        Err(RuleSetUpdateError::InvalidState)
    }

    fn rollback_update(&mut self) -> Result<(), RuleSetUpdateError> {
        Err(RuleSetUpdateError::InvalidState)
    }

    fn snapshot(&self) -> Result<RuleSetDigestSnapshot, RuleSetUpdateError> {
        Err(RuleSetUpdateError::InvalidState)
    }
}

pub struct RuleSetUpdateService<F, C> {
    store: RuleSetStore,
    fetcher: F,
    checker: C,
    manifest: RuleSetProviderManifest,
    transaction: Option<RuleSetUpdateTransaction>,
}

impl<F, C> RuleSetUpdateService<F, C>
where
    F: RuleSetBodyFetcher,
    C: CandidateChecker,
{
    pub fn new(
        store: RuleSetStore,
        fetcher: F,
        checker: C,
        manifest: RuleSetProviderManifest,
    ) -> Self {
        Self {
            store,
            fetcher,
            checker,
            manifest,
            transaction: None,
        }
    }

    pub fn fetcher(&self) -> &F {
        &self.fetcher
    }

    pub fn checker(&self) -> &C {
        &self.checker
    }

    pub fn update(&mut self) -> Result<RuleSetReplaceOutcome, RuleSetUpdateError> {
        match self.prepare_update()? {
            RuleSetUpdatePreparation::Unchanged => Ok(RuleSetReplaceOutcome::Unchanged),
            RuleSetUpdatePreparation::Prepared => {
                self.publish_update()?;
                self.commit_update()?;
                Ok(RuleSetReplaceOutcome::Updated)
            }
        }
    }

    fn fetch_pair(&mut self) -> Result<(Vec<u8>, Vec<u8>), RuleSetUpdateError> {
        let mut cn_domain = None;
        let mut cn_ip = None;
        for provider in self.manifest.providers().to_vec() {
            let body = self
                .fetcher
                .fetch(&provider)
                .map_err(|_| RuleSetUpdateError::Fetch)?;
            match provider.purpose() {
                RuleSetPurpose::CnDomainDirect => cn_domain = Some(body),
                RuleSetPurpose::CnIpDirect => cn_ip = Some(body),
            }
        }
        Ok((
            cn_domain.ok_or(RuleSetUpdateError::MissingProvider)?,
            cn_ip.ok_or(RuleSetUpdateError::MissingProvider)?,
        ))
    }
}

impl<F, C> RuntimeRuleSetUpdateSource for RuleSetUpdateService<F, C>
where
    F: RuleSetBodyFetcher,
    C: CandidateChecker,
{
    fn prepare_update(&mut self) -> Result<RuleSetUpdatePreparation, RuleSetUpdateError> {
        if self.transaction.is_some() {
            return Err(RuleSetUpdateError::InvalidState);
        }
        let (cn_domain, cn_ip) = self.fetch_pair()?;
        match self
            .store
            .prepare(&cn_domain, &cn_ip, &self.checker)
            .map_err(|_| RuleSetUpdateError::Admission)?
        {
            RuleSetPreparation::Unchanged => Ok(RuleSetUpdatePreparation::Unchanged),
            RuleSetPreparation::Prepared(prepared) => {
                self.transaction = Some(RuleSetUpdateTransaction::Prepared(prepared));
                Ok(RuleSetUpdatePreparation::Prepared)
            }
        }
    }

    fn publish_update(&mut self) -> Result<(), RuleSetUpdateError> {
        let Some(RuleSetUpdateTransaction::Prepared(prepared)) = self.transaction.take() else {
            return Err(RuleSetUpdateError::InvalidState);
        };
        let published = self
            .store
            .publish(prepared)
            .map_err(|_| RuleSetUpdateError::Admission)?;
        self.transaction = Some(RuleSetUpdateTransaction::Published(published));
        Ok(())
    }

    fn commit_update(&mut self) -> Result<(), RuleSetUpdateError> {
        let Some(RuleSetUpdateTransaction::Published(published)) = self.transaction.as_ref() else {
            return Err(RuleSetUpdateError::InvalidState);
        };
        self.store
            .commit(published)
            .map_err(|_| RuleSetUpdateError::Admission)?;
        self.transaction = None;
        Ok(())
    }

    fn rollback_update(&mut self) -> Result<(), RuleSetUpdateError> {
        let Some(transaction) = self.transaction.as_ref() else {
            return Err(RuleSetUpdateError::InvalidState);
        };
        match transaction {
            RuleSetUpdateTransaction::Prepared(_) => {}
            RuleSetUpdateTransaction::Published(published) => self
                .store
                .rollback(published)
                .map_err(|_| RuleSetUpdateError::Admission)?,
        }
        self.transaction = None;
        Ok(())
    }

    fn snapshot(&self) -> Result<RuleSetDigestSnapshot, RuleSetUpdateError> {
        let (domain_sha256, ip_sha256) = self
            .store
            .current_digests()
            .map_err(|_| RuleSetUpdateError::Admission)?;
        Ok(RuleSetDigestSnapshot {
            domain_sha256,
            ip_sha256,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use nethop_subscription::{Digest, ParserLimits, SourceCache};

    use super::{HttpRuleSetBodyFetcher, RuleSetProviderManifest};

    fn private_directory() -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
        }
        directory
    }

    #[test]
    fn persistent_cache_restores_body_and_conditional_validators() {
        let directory = private_directory();
        let fetcher = HttpRuleSetBodyFetcher::default()
            .with_cache_root(directory.path())
            .unwrap();
        let provider = &RuleSetProviderManifest::bundled().unwrap().providers()[0];
        let endpoint = Digest::sha256(provider.source_url().as_bytes());
        let mut cache = SourceCache::default();
        cache
            .restore(
                b"SRS\x01cached".to_vec(),
                Some("\"fixture-etag\"".to_owned()),
                Some("Wed, 05 Aug 2026 00:00:00 GMT".to_owned()),
                endpoint,
                &ParserLimits::default(),
            )
            .unwrap();
        fetcher.persist_cache(provider, &cache).unwrap();

        let restored = fetcher.restore_cache(provider, endpoint);

        assert_eq!(
            restored.last_known_good(),
            Some(b"SRS\x01cached".as_slice())
        );
        assert_eq!(
            restored.conditional_headers(),
            [
                ("If-None-Match", "\"fixture-etag\""),
                ("If-Modified-Since", "Wed, 05 Aug 2026 00:00:00 GMT")
            ]
        );
    }

    #[test]
    fn corrupt_persistent_cache_is_an_ignored_miss() {
        let directory = private_directory();
        let fetcher = HttpRuleSetBodyFetcher::default()
            .with_cache_root(directory.path())
            .unwrap();
        let provider = &RuleSetProviderManifest::bundled().unwrap().providers()[0];
        let endpoint = Digest::sha256(provider.source_url().as_bytes());
        fs::write(directory.path().join("cn-domain.body"), b"SRS\x01cached").unwrap();
        fs::write(directory.path().join("cn-domain.json"), b"{}").unwrap();

        let restored = fetcher.restore_cache(provider, endpoint);

        assert!(restored.last_known_good().is_none());
        assert!(restored.conditional_headers().is_empty());
    }
}
