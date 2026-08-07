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
use nethop_subscription::{
    CapabilityMatrix, Digest, FilteredSourceInput, FormatHint, NodeFilter, ParserLimits, SourceId,
    SourceInput, convert_filtered_sources,
};
use serde::Serialize;
use thiserror::Error;

use crate::{
    BuildCandidateError, CandidateChecker, ManualSource, ManualSourceStore, RuntimeUpdateError,
    RuntimeUpdateSource, SourceConfig, SourceDefinition, build_candidate,
    worker_config::atomic_write,
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
    fn fetch(&mut self, source: &SourceDefinition) -> Result<SourceBody, SourceUpdateError>;

    fn cached(&mut self, _source: &SourceDefinition) -> Result<SourceBody, SourceUpdateError> {
        Err(SourceUpdateError::Cache)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceBodyOrigin {
    Fresh,
    NotModified,
    LastKnownGood,
    Local,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceBody {
    bytes: Vec<u8>,
    origin: SourceBodyOrigin,
}

impl SourceBody {
    pub fn new(bytes: Vec<u8>, origin: SourceBodyOrigin) -> Self {
        Self { bytes, origin }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub const fn origin(&self) -> SourceBodyOrigin {
        self.origin
    }
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
    fn fetch(&mut self, source: &SourceDefinition) -> Result<SourceBody, SourceUpdateError> {
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
            let conversion = convert_filtered_sources(
                vec![FilteredSourceInput {
                    source: SourceInput {
                        source_id: source.id().clone(),
                        format_hint: source.expected_format(),
                        bytes: body.to_vec(),
                    },
                    filter: source.filter().clone(),
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
                    .map(|body| SourceBody::new(body.to_vec(), SourceBodyOrigin::LastKnownGood))
                    .ok_or(SourceUpdateError::Fetch);
            }
        };
        let origin = if outcome.was_not_modified() {
            SourceBodyOrigin::NotModified
        } else {
            SourceBodyOrigin::Fresh
        };
        cache
            .commit(&outcome, &self.limits)
            .map_err(|_| SourceUpdateError::Fetch)?;
        if let Some(path) = cache_path {
            atomic_write(&path, outcome.body()).map_err(|_| SourceUpdateError::Cache)?;
        }
        Ok(SourceBody::new(outcome.body().to_vec(), origin))
    }

    fn cached(&mut self, source: &SourceDefinition) -> Result<SourceBody, SourceUpdateError> {
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
        self.caches
            .get(&cache_key)
            .and_then(SourceCache::last_known_good)
            .map(|body| SourceBody::new(body.to_vec(), SourceBodyOrigin::LastKnownGood))
            .ok_or(SourceUpdateError::Cache)
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
    manual_store: Option<ManualSourceStore>,
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
            manual_store: None,
        }
    }

    pub fn with_manual_source_store(mut self, store: ManualSourceStore) -> Self {
        self.manual_store = Some(store);
        self
    }

    pub fn update(
        &mut self,
        config: &SourceConfig,
    ) -> Result<SourceUpdateReport, SourceUpdateError> {
        let prepared = self.prepare(config)?;
        self.commit(prepared)
    }

    pub fn update_source(
        &mut self,
        config: &SourceConfig,
        source_id: &SourceId,
    ) -> Result<SourceUpdateReport, SourceUpdateError> {
        let prepared = self.prepare_source(config, source_id)?;
        self.commit(prepared)
    }

    fn cached_inputs_with_manual(
        &mut self,
        config: &SourceConfig,
        bytes: &[u8],
        format_hint: FormatHint,
    ) -> Result<Vec<FilteredSourceInput>, SourceUpdateError> {
        if self.manual_store.is_none() {
            return Err(SourceUpdateError::ManualSource);
        }
        let mut inputs = Vec::new();
        for source in config.active_sources() {
            let body = self.fetcher.cached(source)?;
            inputs.push(FilteredSourceInput {
                source: SourceInput {
                    source_id: source.id().clone(),
                    format_hint: source.expected_format(),
                    bytes: body.bytes,
                },
                filter: source.filter().clone(),
            });
        }
        inputs.push(FilteredSourceInput {
            source: SourceInput {
                source_id: ManualSource::source_id(),
                format_hint,
                bytes: bytes.to_vec(),
            },
            filter: manual_filter(config)?,
        });
        Ok(inputs)
    }

    pub fn preview_import(
        &mut self,
        config: &SourceConfig,
        bytes: &[u8],
        format_hint: FormatHint,
    ) -> Result<ImportPreview, SourceUpdateError> {
        let inputs = self.cached_inputs_with_manual(config, bytes, format_hint)?;
        let conversion = convert_filtered_sources(inputs, &self.limits, &self.matrix);
        if !conversion.report.summary.source_success || conversion.nodes.is_empty() {
            return Err(SourceUpdateError::Conversion);
        }
        let generation = self.next_generation()?;
        let candidate = build_candidate(
            generation,
            &conversion,
            self.runtime.capture.clone(),
            self.runtime.clash_api.clone(),
            self.runtime.tun_stack,
            self.runtime.options.clone(),
        )?
        .bind_sources(
            effective_source_digest(config, Some(Digest::sha256(bytes).hex().as_str())),
            source_ids_with_manual(config),
        )?;
        Ok(ImportPreview {
            candidate_digest: candidate.config().digest_sha256().to_owned(),
            detected_format: conversion.report.summary.detected_format,
            accepted: conversion.report.summary.accepted,
            duplicate: conversion.report.summary.duplicate,
            rejected: conversion.report.summary.rejected,
            node_count: candidate.config().node_count(),
        })
    }

    pub fn prepare_import(
        &mut self,
        config: &SourceConfig,
        bytes: &[u8],
        format_hint: FormatHint,
        expected_candidate_digest: &str,
    ) -> Result<PreparedSourceUpdate, SourceUpdateError> {
        let inputs = self.cached_inputs_with_manual(config, bytes, format_hint)?;
        let conversion = convert_filtered_sources(inputs, &self.limits, &self.matrix);
        if !conversion.report.summary.source_success || conversion.nodes.is_empty() {
            return Err(SourceUpdateError::Conversion);
        }
        let generation = self.next_generation()?;
        let candidate = build_candidate(
            generation,
            &conversion,
            self.runtime.capture.clone(),
            self.runtime.clash_api.clone(),
            self.runtime.tun_stack,
            self.runtime.options.clone(),
        )?
        .bind_sources(
            effective_source_digest(config, Some(Digest::sha256(bytes).hex().as_str())),
            source_ids_with_manual(config),
        )?;
        if candidate.config().digest_sha256() != expected_candidate_digest {
            return Err(SourceUpdateError::CandidateDigestMismatch);
        }
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
            pending_manual: Some(PendingManualSource {
                bytes: bytes.to_vec(),
                format_hint,
            }),
            report: SourceUpdateReport {
                generation,
                source_count: conversion.source_outcomes.len(),
                accepted: conversion.report.summary.accepted,
                duplicate: conversion.report.summary.duplicate,
                node_count: candidate.config().node_count(),
                sources: conversion
                    .source_outcomes
                    .into_iter()
                    .map(|(source_id, outcome)| SourceUpdateDetail {
                        origin: Some(if source_id == ManualSource::source_id() {
                            SourceBodyOrigin::Local
                        } else {
                            SourceBodyOrigin::LastKnownGood
                        }),
                        source_id,
                        accepted: outcome.accepted,
                        duplicate: outcome.duplicate,
                        rejected: outcome.rejected,
                        warnings: outcome.warnings,
                        diagnostic_code: None,
                    })
                    .collect(),
            },
        })
    }

    pub fn is_needed(&self, config: &SourceConfig) -> bool {
        let manual_digest = self
            .manual_store
            .as_ref()
            .and_then(|store| store.load().ok().flatten())
            .map(|source| source.digest().to_owned());
        self.store
            .current_manifest()
            .ok()
            .flatten()
            .and_then(|manifest| manifest.source_config_digest)
            .as_deref()
            != Some(effective_source_digest(config, manual_digest.as_deref()).as_str())
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
        self.prepare_selected(config, None)
    }

    pub fn prepare_source(
        &mut self,
        config: &SourceConfig,
        source_id: &SourceId,
    ) -> Result<PreparedSourceUpdate, SourceUpdateError> {
        self.prepare_selected(config, Some(source_id))
    }

    fn prepare_selected(
        &mut self,
        config: &SourceConfig,
        selected: Option<&SourceId>,
    ) -> Result<PreparedSourceUpdate, SourceUpdateError> {
        let generation = self.next_generation()?;
        let active_sources: Vec<_> = config.active_sources().collect();
        if let Some(source_id) = selected
            && !active_sources.iter().any(|source| source.id() == source_id)
        {
            return Err(SourceUpdateError::UnknownSource);
        }
        let mut inputs = Vec::with_capacity(active_sources.len());
        let mut details = BTreeMap::new();
        for source in &active_sources {
            let body = if selected.is_none_or(|source_id| source.id() == source_id) {
                self.fetcher.fetch(source)
            } else {
                self.fetcher.cached(source)
            };
            match body {
                Ok(body) => {
                    details.insert(
                        source.id().clone(),
                        SourceUpdateDetail::pending(source.id().clone(), body.origin()),
                    );
                    inputs.push(FilteredSourceInput {
                        source: SourceInput {
                            source_id: source.id().clone(),
                            format_hint: source.expected_format(),
                            bytes: body.bytes,
                        },
                        filter: source.filter().clone(),
                    });
                }
                Err(error) if selected.is_none() => {
                    details.insert(
                        source.id().clone(),
                        SourceUpdateDetail::failed(source.id().clone(), error.code()),
                    );
                }
                Err(error) => return Err(error),
            }
        }
        let manual = self
            .manual_store
            .as_ref()
            .map(ManualSourceStore::load)
            .transpose()
            .map_err(|_| SourceUpdateError::ManualSource)?
            .flatten();
        if let Some(manual) = &manual {
            let source_id = ManualSource::source_id();
            details.insert(
                source_id.clone(),
                SourceUpdateDetail::pending(source_id.clone(), SourceBodyOrigin::Local),
            );
            inputs.push(FilteredSourceInput {
                source: SourceInput {
                    source_id,
                    format_hint: manual.format_hint(),
                    bytes: manual.bytes().to_vec(),
                },
                filter: manual_filter(config)?,
            });
        }
        let conversion = convert_filtered_sources(inputs, &self.limits, &self.matrix);
        for (source_id, outcome) in &conversion.source_outcomes {
            if let Some(detail) = details.get_mut(source_id) {
                detail.accepted = outcome.accepted;
                detail.duplicate = outcome.duplicate;
                detail.rejected = outcome.rejected;
                detail.warnings = outcome.warnings;
                if !outcome.success() {
                    detail.diagnostic_code = Some("conversion_empty".to_owned());
                }
            }
        }
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
            effective_source_digest(config, manual.as_ref().map(ManualSource::digest)),
            if manual.is_some() {
                source_ids_with_manual(config)
            } else {
                config
                    .active_sources()
                    .map(|source| source.id().as_str().to_owned())
                    .collect()
            },
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
            pending_manual: None,
            report: SourceUpdateReport {
                generation,
                source_count: active_sources.len() + usize::from(manual.is_some()),
                accepted: conversion.report.summary.accepted,
                duplicate: conversion.report.summary.duplicate,
                node_count: candidate.config().node_count(),
                sources: details.into_values().collect(),
            },
        })
    }

    pub fn commit(
        &mut self,
        prepared: PreparedSourceUpdate,
    ) -> Result<SourceUpdateReport, SourceUpdateError> {
        let manual_checkpoint = if let Some(manual) = &prepared.pending_manual {
            let store = self
                .manual_store
                .as_ref()
                .ok_or(SourceUpdateError::ManualSource)?;
            match store.replace(manual.format_hint, &manual.bytes) {
                Ok(checkpoint) => Some(checkpoint),
                Err(_) => {
                    let _ = self.store.discard_sealed(prepared.sealed);
                    return Err(SourceUpdateError::ManualSource);
                }
            }
        } else {
            None
        };
        if let Err(error) = self.store.commit_generation(&prepared.sealed) {
            if let (Some(store), Some(checkpoint)) = (self.manual_store.as_ref(), manual_checkpoint)
            {
                let _ = store.restore(checkpoint);
            }
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
    pending_import: Option<PendingImport>,
    pending_source: Option<SourceId>,
    last_report: Option<SourceUpdateReport>,
}

fn effective_source_digest(config: &SourceConfig, manual_digest: Option<&str>) -> String {
    let Some(manual_digest) = manual_digest else {
        return config.source_config_digest().to_owned();
    };
    let mut canonical =
        Vec::with_capacity(config.source_config_digest().len() + manual_digest.len() + 32);
    canonical.extend_from_slice(b"nethop-effective-sources-v1\0");
    canonical.extend_from_slice(config.source_config_digest().as_bytes());
    canonical.push(0);
    canonical.extend_from_slice(manual_digest.as_bytes());
    Digest::sha256(&canonical).hex()
}

fn source_ids_with_manual(config: &SourceConfig) -> Vec<String> {
    config
        .active_sources()
        .map(|source| source.id().as_str().to_owned())
        .chain(std::iter::once(
            ManualSource::source_id().as_str().to_owned(),
        ))
        .collect()
}

fn manual_filter(config: &SourceConfig) -> Result<NodeFilter, SourceUpdateError> {
    let mut excluded = config
        .sources()
        .iter()
        .flat_map(|source| source.filter().excluded_node_ids().iter().cloned())
        .collect::<Vec<_>>();
    excluded.sort();
    excluded.dedup();
    NodeFilter::new_with_node_ids(Vec::new(), Vec::new(), excluded, Vec::new())
        .map_err(|_| SourceUpdateError::Conversion)
}

struct PendingImport {
    bytes: Vec<u8>,
    format_hint: FormatHint,
    candidate_digest: String,
}

impl<'a, F, C> ConfiguredSourceUpdater<'a, F, C>
where
    F: SourceBodyFetcher,
    C: CandidateChecker,
{
    pub fn new(service: SourceUpdateService<'a, F, C>, config: SourceConfig) -> Self {
        Self {
            service,
            config,
            pending_import: None,
            pending_source: None,
            last_report: None,
        }
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

    fn request_source_update(&mut self, source_id: Option<&str>) -> Result<(), RuntimeUpdateError> {
        let parsed = source_id
            .map(SourceId::new)
            .transpose()
            .map_err(|_| RuntimeUpdateError::Prepare)?;
        if parsed.as_ref().is_some_and(|source_id| {
            !self
                .config
                .sources()
                .iter()
                .any(|source| source.id() == source_id)
        }) {
            return Err(RuntimeUpdateError::Prepare);
        }
        if self.pending_source.is_some() {
            return Err(RuntimeUpdateError::Prepare);
        }
        self.pending_source = parsed;
        Ok(())
    }

    fn replace_config(&mut self, config: SourceConfig) {
        self.config = config;
    }

    fn take_source_update_report(&mut self) -> Option<SourceUpdateReport> {
        self.last_report.take()
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
        if let Some(import) = self.pending_import.take() {
            self.service
                .prepare_import(
                    &self.config,
                    &import.bytes,
                    import.format_hint,
                    &import.candidate_digest,
                )
                .map_err(|_| RuntimeUpdateError::Prepare)
        } else {
            let selected = self.pending_source.take();
            let prepared = match selected.as_ref() {
                Some(source_id) => self.service.prepare_source(&self.config, source_id),
                None => self.service.prepare(&self.config),
            };
            prepared.map_err(|_| RuntimeUpdateError::Prepare)
        }
    }

    fn generation(&self, prepared: &Self::Prepared) -> GenerationId {
        prepared.report.generation
    }

    fn is_current(&self, prepared: &Self::Prepared) -> bool {
        prepared.source_config_digest == self.config.source_config_digest()
    }

    fn commit(&mut self, prepared: Self::Prepared) -> Result<GenerationId, RuntimeUpdateError> {
        let report = self
            .service
            .commit(prepared)
            .map_err(|_| RuntimeUpdateError::Commit)?;
        let generation = report.generation;
        self.last_report = Some(report);
        Ok(generation)
    }

    fn discard(&mut self, prepared: Self::Prepared) -> Result<(), RuntimeUpdateError> {
        self.service
            .discard(prepared)
            .map_err(|_| RuntimeUpdateError::Discard)
    }

    fn preview_import(
        &mut self,
        bytes: &[u8],
        format_hint: FormatHint,
    ) -> Result<serde_json::Value, RuntimeUpdateError> {
        self.service
            .preview_import(&self.config, bytes, format_hint)
            .and_then(|preview| {
                serde_json::to_value(preview).map_err(|_| SourceUpdateError::Conversion)
            })
            .map_err(|_| RuntimeUpdateError::Prepare)
    }

    fn request_import(
        &mut self,
        bytes: Vec<u8>,
        format_hint: FormatHint,
        candidate_digest: String,
    ) -> Result<(), RuntimeUpdateError> {
        if self.pending_import.is_some() {
            return Err(RuntimeUpdateError::Prepare);
        }
        self.pending_import = Some(PendingImport {
            bytes,
            format_hint,
            candidate_digest,
        });
        Ok(())
    }
}

#[derive(Debug)]
pub struct PreparedSourceUpdate {
    sealed: SealedGeneration,
    source_config_digest: String,
    pending_manual: Option<PendingManualSource>,
    report: SourceUpdateReport,
}

#[derive(Debug)]
struct PendingManualSource {
    bytes: Vec<u8>,
    format_hint: FormatHint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportPreview {
    pub candidate_digest: String,
    pub detected_format: FormatHint,
    pub accepted: usize,
    pub duplicate: usize,
    pub rejected: usize,
    pub node_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceUpdateReport {
    pub generation: GenerationId,
    pub source_count: usize,
    pub accepted: usize,
    pub duplicate: usize,
    pub node_count: usize,
    pub sources: Vec<SourceUpdateDetail>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceUpdateDetail {
    pub source_id: SourceId,
    pub origin: Option<SourceBodyOrigin>,
    pub accepted: usize,
    pub duplicate: usize,
    pub rejected: usize,
    pub warnings: usize,
    pub diagnostic_code: Option<String>,
}

impl SourceUpdateDetail {
    fn pending(source_id: SourceId, origin: SourceBodyOrigin) -> Self {
        Self {
            source_id,
            origin: Some(origin),
            accepted: 0,
            duplicate: 0,
            rejected: 0,
            warnings: 0,
            diagnostic_code: None,
        }
    }

    fn failed(source_id: SourceId, diagnostic_code: &'static str) -> Self {
        Self {
            source_id,
            origin: None,
            accepted: 0,
            duplicate: 0,
            rejected: 0,
            warnings: 0,
            diagnostic_code: Some(diagnostic_code.to_owned()),
        }
    }
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
    #[error("local import candidate digest does not match the preview")]
    CandidateDigestMismatch,
    #[error("subscription source is unknown or inactive")]
    UnknownSource,
    #[error("persistent manual source is unavailable")]
    ManualSource,
}

impl SourceUpdateError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Fetch => "fetch_failed",
            Self::Cache => "cache_unavailable",
            Self::Conversion => "conversion_failed",
            Self::GenerationExhausted => "generation_exhausted",
            Self::Build(_) => "candidate_build_failed",
            Self::Publish(_) => "candidate_publish_failed",
            Self::CandidateDigestMismatch => "candidate_digest_mismatch",
            Self::UnknownSource => "source_unknown_or_inactive",
            Self::ManualSource => "manual_source_unavailable",
        }
    }
}
