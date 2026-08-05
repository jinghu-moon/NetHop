#[cfg(feature = "subscription-update")]
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use nethop_core::{
    CapturePolicy, ClashApi, CoreError, GenerationId, GenerationStore, ManagedOptions,
    SealedGeneration, TunStack,
};
#[cfg(feature = "subscription-update")]
use nethop_subscription::{
    CandidateAcceptance, FetchClient, FetchPolicy, FetchRequest, SourceCache,
};
use nethop_subscription::{CapabilityMatrix, ParserLimits, SourceInput, convert_stable_sources};
use thiserror::Error;

use crate::{
    BuildCandidateError, CandidateChecker, RuntimeUpdateError, RuntimeUpdateSource, SourceConfig,
    SourceDefinition, build_candidate, worker_config::atomic_write,
};

#[derive(Debug, Clone)]
pub struct UpdateRuntimePolicy {
    capture: CapturePolicy,
    clash_api: ClashApi,
    tun_stack: TunStack,
    options: ManagedOptions,
}

impl UpdateRuntimePolicy {
    pub const fn new(
        capture: CapturePolicy,
        clash_api: ClashApi,
        tun_stack: TunStack,
        options: ManagedOptions,
    ) -> Self {
        Self {
            capture,
            clash_api,
            tun_stack,
            options,
        }
    }

    fn replace(&mut self, capture: CapturePolicy, tun_stack: TunStack, options: ManagedOptions) {
        self.capture = capture;
        self.tun_stack = tun_stack;
        self.options = options;
    }
}

pub trait SourceBodyFetcher {
    fn fetch(&mut self, source: &SourceDefinition) -> Result<Vec<u8>, SourceUpdateError>;
}

#[cfg(feature = "subscription-update")]
pub struct HttpSourceBodyFetcher {
    client: FetchClient,
    limits: ParserLimits,
    matrix: CapabilityMatrix,
    caches: BTreeMap<String, SourceCache>,
    cache_root: Option<PathBuf>,
}

#[cfg(feature = "subscription-update")]
impl HttpSourceBodyFetcher {
    pub fn new(limits: ParserLimits, matrix: CapabilityMatrix) -> Self {
        Self {
            client: FetchClient::new(FetchPolicy::default(), limits),
            limits,
            matrix,
            caches: BTreeMap::new(),
            cache_root: None,
        }
    }

    pub fn with_cache_root(mut self, root: impl Into<PathBuf>) -> Result<Self, SourceUpdateError> {
        let root = root.into();
        if !root.is_absolute() || !root.is_dir() || root.file_name().is_none() {
            return Err(SourceUpdateError::Cache);
        }
        self.cache_root = Some(root);
        Ok(self)
    }

    fn cache_path(&self, source: &SourceDefinition) -> Option<PathBuf> {
        self.cache_root.as_ref().map(|root| {
            root.join(format!(
                "{}-{}.body",
                source.id().as_str(),
                source.request_identity_digest()
            ))
        })
    }

    fn restore_cache(
        path: Option<&Path>,
        cache: &mut SourceCache,
        limits: &ParserLimits,
    ) -> Result<(), SourceUpdateError> {
        let Some(path) = path else {
            return Ok(());
        };
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(_) => return Err(SourceUpdateError::Cache),
        };
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() == 0
            || metadata.len() > limits.max_body_bytes() as u64
            || !private_cache_file(&metadata)
        {
            return Err(SourceUpdateError::Cache);
        }
        let body = fs::read(path).map_err(|_| SourceUpdateError::Cache)?;
        cache
            .apply_success(body, None, None, limits)
            .map_err(|_| SourceUpdateError::Cache)
    }
}

#[cfg(feature = "subscription-update")]
impl SourceBodyFetcher for HttpSourceBodyFetcher {
    fn fetch(&mut self, source: &SourceDefinition) -> Result<Vec<u8>, SourceUpdateError> {
        let cache_path = self.cache_path(source);
        let cache_key = format!(
            "{}:{}",
            source.id().as_str(),
            source.request_identity_digest()
        );
        if !self.caches.contains_key(&cache_key) {
            let mut cache = SourceCache::default();
            Self::restore_cache(cache_path.as_deref(), &mut cache, &self.limits)?;
            self.caches.insert(cache_key.clone(), cache);
        }
        let request = FetchRequest::new(
            source.id().clone(),
            source.url(),
            source.mirrors(),
            source.request_profile(),
            &FetchPolicy::default(),
        )
        .map_err(|_| SourceUpdateError::Fetch)?;
        let cache = self
            .caches
            .get_mut(&cache_key)
            .expect("cache entry was initialized");
        let outcome = match self.client.fetch(&request, cache, |body| {
            let conversion = convert_stable_sources(
                vec![SourceInput {
                    source_id: source.id().clone(),
                    format_hint: source.expected_format(),
                    bytes: body.to_vec(),
                }],
                &self.limits,
                &self.matrix,
            );
            if conversion.report.summary.source_success {
                CandidateAcceptance::Accepted
            } else if conversion.report.summary.accepted == 0 {
                CandidateAcceptance::AcceptedZero
            } else {
                CandidateAcceptance::FormatRejected
            }
        }) {
            Ok(outcome) => outcome,
            Err(_) => {
                return cache
                    .last_known_good()
                    .map(ToOwned::to_owned)
                    .ok_or(SourceUpdateError::Fetch);
            }
        };
        cache
            .commit(&outcome, &self.limits)
            .map_err(|_| SourceUpdateError::Fetch)?;
        if let Some(path) = cache_path {
            atomic_write(&path, outcome.body()).map_err(|_| SourceUpdateError::Cache)?;
        }
        Ok(outcome.body().to_vec())
    }
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

pub struct SourceUpdateService<'a, F, C> {
    store: &'a GenerationStore,
    fetcher: F,
    checker: &'a C,
    limits: ParserLimits,
    matrix: CapabilityMatrix,
    runtime: UpdateRuntimePolicy,
}

impl<'a, F, C> SourceUpdateService<'a, F, C>
where
    F: SourceBodyFetcher,
    C: CandidateChecker,
{
    pub fn new(
        store: &'a GenerationStore,
        fetcher: F,
        checker: &'a C,
        limits: ParserLimits,
        matrix: CapabilityMatrix,
        runtime: UpdateRuntimePolicy,
    ) -> Self {
        Self {
            store,
            fetcher,
            checker,
            limits,
            matrix,
            runtime,
        }
    }

    pub fn update(
        &mut self,
        config: &SourceConfig,
    ) -> Result<SourceUpdateReport, SourceUpdateError> {
        let prepared = self.prepare(config)?;
        self.commit(prepared)
    }

    pub fn is_needed(&self, config: &SourceConfig) -> bool {
        self.store
            .current_manifest()
            .ok()
            .flatten()
            .and_then(|manifest| manifest.source_config_digest)
            .as_deref()
            != Some(config.source_config_digest())
    }

    pub fn replace_runtime_policy(
        &mut self,
        capture: CapturePolicy,
        tun_stack: TunStack,
        options: ManagedOptions,
    ) {
        self.runtime.replace(capture, tun_stack, options);
    }

    pub fn prepare(
        &mut self,
        config: &SourceConfig,
    ) -> Result<PreparedSourceUpdate, SourceUpdateError> {
        let generation = self.next_generation()?;
        let active_sources: Vec<_> = config.active_sources().collect();
        let mut inputs = Vec::with_capacity(active_sources.len());
        for source in &active_sources {
            if let Ok(bytes) = self.fetcher.fetch(source) {
                inputs.push(SourceInput {
                    source_id: source.id().clone(),
                    format_hint: source.expected_format(),
                    bytes,
                });
            }
        }
        let conversion = convert_stable_sources(inputs, &self.limits, &self.matrix);
        if !conversion.report.summary.source_success || conversion.nodes.is_empty() {
            return Err(SourceUpdateError::Conversion);
        }
        let candidate = build_candidate(
            generation,
            &conversion,
            self.runtime.capture.clone(),
            self.runtime.clash_api.clone(),
            self.runtime.tun_stack,
            self.runtime.options.clone(),
        )?
        .bind_sources(
            config.source_config_digest(),
            config
                .active_sources()
                .map(|source| source.id().as_str().to_owned())
                .collect(),
        )?;
        let prepared = self.store.prepare_candidate(&candidate)?;
        if self.checker.check(&prepared.config_path()).is_err() {
            let _ = self.store.discard_prepared(prepared);
            return Err(SourceUpdateError::Publish(CoreError::ValidationFailed));
        }
        let sealed = match self.store.seal_candidate(&prepared) {
            Ok(sealed) => sealed,
            Err(error) => {
                let _ = self.store.discard_prepared(prepared);
                return Err(error.into());
            }
        };
        Ok(PreparedSourceUpdate {
            sealed,
            source_config_digest: config.source_config_digest().to_owned(),
            report: SourceUpdateReport {
                generation,
                source_count: active_sources.len(),
                accepted: conversion.report.summary.accepted,
                duplicate: conversion.report.summary.duplicate,
                node_count: candidate.config().node_count(),
            },
        })
    }

    pub fn commit(
        &self,
        prepared: PreparedSourceUpdate,
    ) -> Result<SourceUpdateReport, SourceUpdateError> {
        if let Err(error) = self.store.commit_generation(&prepared.sealed) {
            let _ = self.store.discard_sealed(prepared.sealed);
            return Err(error.into());
        }
        Ok(prepared.report)
    }

    pub fn discard(&self, prepared: PreparedSourceUpdate) -> Result<(), SourceUpdateError> {
        self.store.discard_sealed(prepared.sealed)?;
        Ok(())
    }

    fn next_generation(&self) -> Result<GenerationId, SourceUpdateError> {
        let next = match self.store.current_generation()? {
            Some(generation) => generation
                .get()
                .checked_add(1)
                .ok_or(SourceUpdateError::GenerationExhausted)?,
            None => 1,
        };
        GenerationId::new(next).map_err(|_| SourceUpdateError::GenerationExhausted)
    }
}

pub struct ConfiguredSourceUpdater<'a, F, C> {
    service: SourceUpdateService<'a, F, C>,
    config: SourceConfig,
}

impl<'a, F, C> ConfiguredSourceUpdater<'a, F, C>
where
    F: SourceBodyFetcher,
    C: CandidateChecker,
{
    pub const fn new(service: SourceUpdateService<'a, F, C>, config: SourceConfig) -> Self {
        Self { service, config }
    }
}

impl<F, C> RuntimeUpdateSource for ConfiguredSourceUpdater<'_, F, C>
where
    F: SourceBodyFetcher,
    C: CandidateChecker,
{
    type Prepared = PreparedSourceUpdate;

    fn is_available(&self) -> bool {
        self.config.active_sources().next().is_some()
    }

    fn is_needed(&self) -> bool {
        self.service.is_needed(&self.config)
    }

    fn replace_config(&mut self, config: SourceConfig) {
        self.config = config;
    }

    fn replace_runtime_policy(
        &mut self,
        capture: CapturePolicy,
        tun_stack: TunStack,
        options: ManagedOptions,
    ) {
        self.service
            .replace_runtime_policy(capture, tun_stack, options);
    }

    fn prepare(&mut self) -> Result<Self::Prepared, RuntimeUpdateError> {
        self.service
            .prepare(&self.config)
            .map_err(|_| RuntimeUpdateError::Prepare)
    }

    fn generation(&self, prepared: &Self::Prepared) -> GenerationId {
        prepared.report.generation
    }

    fn is_current(&self, prepared: &Self::Prepared) -> bool {
        prepared.source_config_digest == self.config.source_config_digest()
    }

    fn commit(&mut self, prepared: Self::Prepared) -> Result<GenerationId, RuntimeUpdateError> {
        self.service
            .commit(prepared)
            .map(|report| report.generation)
            .map_err(|_| RuntimeUpdateError::Commit)
    }

    fn discard(&mut self, prepared: Self::Prepared) -> Result<(), RuntimeUpdateError> {
        self.service
            .discard(prepared)
            .map_err(|_| RuntimeUpdateError::Discard)
    }
}

#[derive(Debug)]
pub struct PreparedSourceUpdate {
    sealed: SealedGeneration,
    source_config_digest: String,
    report: SourceUpdateReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceUpdateReport {
    pub generation: GenerationId,
    pub source_count: usize,
    pub accepted: usize,
    pub duplicate: usize,
    pub node_count: usize,
}

#[derive(Debug, Error)]
pub enum SourceUpdateError {
    #[error("subscription source could not be fetched")]
    Fetch,
    #[error("subscription last-known-good cache is unavailable")]
    Cache,
    #[error("subscription conversion did not produce a publishable candidate")]
    Conversion,
    #[error("generation identifier space is exhausted")]
    GenerationExhausted,
    #[error("candidate build failed")]
    Build(#[from] BuildCandidateError),
    #[error("generation publication failed")]
    Publish(#[from] CoreError),
}
