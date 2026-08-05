use std::{fmt, path::PathBuf};

use nethop_android::AppCatalog;
use nethop_protocol::{ConfigMutation, ErrorDomain, RoutingCidrList};
use nethop_subscription::SourceId;
use thiserror::Error;

#[cfg(unix)]
use crate::worker_config::{atomic_write, read_stable};
use crate::{
    ChangePlan, ConfigError, ConfigSnapshot, ConfigStore, SourceConfig, SourceRegistry,
    SourceRegistryError, SystemSourceIdEntropy,
};
use crate::{source_config::SourceRegistryCheckpoint, worker_config::ConfigStoreCheckpoint};

pub struct ConfigRuntime {
    store: ConfigStore,
    registry: SourceRegistry,
    current: ConfigSnapshot,
    current_source_digest: String,
    current_sources: SourceConfig,
    module_entry: Option<PathBuf>,
    app_catalog: Option<AppCatalog>,
    candidate_sequence: u64,
    last_reload: ConfigReloadState,
}

impl ConfigRuntime {
    pub fn new(
        store: ConfigStore,
        registry: SourceRegistry,
        current: ConfigSnapshot,
        current_sources: &SourceConfig,
    ) -> Self {
        Self {
            store,
            registry,
            current,
            current_source_digest: current_sources.source_config_digest().to_owned(),
            current_sources: current_sources.clone(),
            module_entry: None,
            app_catalog: None,
            candidate_sequence: 0,
            last_reload: ConfigReloadState::Accepted,
        }
    }

    pub fn with_module_entry(
        mut self,
        path: impl Into<PathBuf>,
    ) -> Result<Self, ConfigRuntimeError> {
        let path = path.into();
        if !path.is_absolute()
            || path.file_name().and_then(|name| name.to_str()) != Some("nethop.toml")
        {
            return Err(ConfigRuntimeError::ModuleEntry);
        }
        self.module_entry = Some(path);
        Ok(self)
    }

    pub fn with_app_catalog(mut self, catalog: AppCatalog) -> Result<Self, ConfigRuntimeError> {
        self.current.effective().admitted_capture(Some(&catalog))?;
        self.app_catalog = Some(catalog);
        Ok(self)
    }

    pub fn current(&self) -> &ConfigSnapshot {
        &self.current
    }

    pub fn observed_digest(&self) -> Result<String, ConfigRuntimeError> {
        self.store.observed_digest().map_err(Into::into)
    }

    pub const fn candidate_sequence(&self) -> u64 {
        self.candidate_sequence
    }

    pub const fn last_reload(&self) -> ConfigReloadState {
        self.last_reload
    }

    pub fn redacted_document(&self) -> serde_json::Value {
        let mut document = self.current.redacted_document();
        if let Some(values) = document
            .pointer_mut("/subscriptions/sources")
            .and_then(serde_json::Value::as_array_mut)
        {
            for (value, source) in values.iter_mut().zip(self.current_sources.sources()) {
                if let Some(object) = value.as_object_mut() {
                    object.insert("source_id".into(), serde_json::json!(source.id().as_str()));
                }
            }
        }
        document
    }

    pub fn update_schedule(&self) -> (bool, u16, &SourceConfig) {
        (
            self.current.effective().subscriptions().auto_update(),
            self.current
                .effective()
                .subscriptions()
                .update_interval_hours(),
            &self.current_sources,
        )
    }

    pub fn capture_policy(&self) -> Result<nethop_core::CapturePolicy, ConfigRuntimeError> {
        self.current
            .effective()
            .admitted_capture(self.app_catalog.as_ref())
            .map_err(Into::into)
    }

    pub fn checkpoint(&self) -> Result<ConfigRuntimeCheckpoint, ConfigRuntimeError> {
        Ok(ConfigRuntimeCheckpoint {
            store: self.store.checkpoint()?,
            registry: self.registry.checkpoint()?,
            current: self.current.clone(),
            current_source_digest: self.current_source_digest.clone(),
            current_sources: self.current_sources.clone(),
        })
    }

    pub fn rollback(
        &mut self,
        checkpoint: ConfigRuntimeCheckpoint,
    ) -> Result<ConfigChange, ConfigRuntimeError> {
        let candidate = self.current.clone();
        let candidate_store = self.store.checkpoint()?;
        self.store
            .restore_checkpoint(candidate_store.digest(), &checkpoint.store)?;
        if self
            .registry
            .restore_checkpoint(candidate.digest(), &checkpoint.registry)
            .is_err()
        {
            let _ = self
                .store
                .restore_checkpoint(checkpoint.store.digest(), &candidate_store);
            return Err(ConfigRuntimeError::Rollback);
        }

        let plan = candidate
            .effective()
            .change_plan(checkpoint.current.effective());
        let enabled = checkpoint.current.effective().service_enabled();
        let service_changed = enabled != candidate.effective().service_enabled();
        let sources_changed = checkpoint.current_source_digest != self.current_source_digest;
        let digest = checkpoint.current.digest().to_owned();
        let sources = checkpoint.current_sources.clone();
        self.current = checkpoint.current;
        self.current_source_digest = checkpoint.current_source_digest;
        self.current_sources = checkpoint.current_sources;
        Ok(ConfigChange::Changed {
            digest,
            enabled,
            service_changed,
            sources_changed,
            sources,
            plan,
        })
    }

    pub fn reload(&mut self) -> Result<ConfigChange, ConfigRuntimeError> {
        self.begin_candidate();
        let result = self.reload_inner();
        self.finish_candidate(&result);
        result
    }

    fn reload_inner(&mut self) -> Result<ConfigChange, ConfigRuntimeError> {
        let imported = self.reconcile_module_entry()?;
        let candidate = match imported {
            Some(candidate) => candidate,
            None => self.store.load()?,
        };
        if candidate.digest() == self.current.digest() {
            return Ok(ConfigChange::Unchanged);
        }
        self.admit(&candidate)?;
        self.accept(candidate)
    }

    pub fn disk_matches_current(&self) -> bool {
        self.store
            .load()
            .is_ok_and(|snapshot| snapshot.digest() == self.current.digest())
    }

    pub fn validate_document(
        &self,
        expected_digest: &str,
        document: &serde_json::Value,
    ) -> Result<ConfigPreview, ConfigRuntimeError> {
        let prepared = self.store.prepare_document(expected_digest, document)?;
        self.preview_prepared(prepared)
    }

    pub fn preview_document(
        &self,
        document: &serde_json::Value,
    ) -> Result<ConfigPreview, ConfigRuntimeError> {
        let prepared = self.store.prepare_document_candidate(document)?;
        self.preview_prepared(prepared)
    }

    fn preview_prepared(
        &self,
        prepared: crate::worker_config::PreparedConfigWrite,
    ) -> Result<ConfigPreview, ConfigRuntimeError> {
        self.admit(prepared.snapshot())?;
        Ok(ConfigPreview {
            observed_digest: self.observed_digest()?,
            candidate_digest: prepared.snapshot().digest().to_owned(),
            plan: self
                .current
                .effective()
                .change_plan(prepared.snapshot().effective()),
        })
    }

    pub fn apply_document(
        &mut self,
        expected_digest: &str,
        document: &serde_json::Value,
    ) -> Result<ConfigChange, ConfigRuntimeError> {
        self.begin_candidate();
        let result = self.apply_document_with_ids(
            expected_digest,
            document,
            &[],
            &mut SystemSourceIdEntropy,
        );
        self.finish_candidate(&result);
        result
    }

    fn apply_document_with_ids(
        &mut self,
        expected_digest: &str,
        document: &serde_json::Value,
        preferred_ids: &[Option<SourceId>],
        entropy: &mut impl crate::SourceIdEntropy,
    ) -> Result<ConfigChange, ConfigRuntimeError> {
        let prepared_config = self.store.prepare_document(expected_digest, document)?;
        self.admit(prepared_config.snapshot())?;
        let prepared_sources = self.registry.prepare_with_preferred_ids(
            prepared_config.snapshot(),
            entropy,
            preferred_ids,
        )?;
        let store_checkpoint = self.store.checkpoint()?;
        let registry_checkpoint = self.registry.checkpoint()?;
        let candidate = self
            .store
            .commit_prepared(expected_digest, prepared_config)?;
        let sources = match self.registry.activate(prepared_sources) {
            Ok(sources) => sources,
            Err(error) => {
                if self
                    .store
                    .restore_checkpoint(candidate.digest(), &store_checkpoint)
                    .is_err()
                    || self
                        .registry
                        .restore_checkpoint_exact(&registry_checkpoint)
                        .is_err()
                {
                    return Err(ConfigRuntimeError::Rollback);
                }
                return Err(error.into());
            }
        };
        self.accept_resolved(candidate, sources)
    }

    fn admit(&self, snapshot: &ConfigSnapshot) -> Result<(), ConfigRuntimeError> {
        snapshot
            .effective()
            .admitted_capture(self.app_catalog.as_ref())?;
        Ok(())
    }

    pub fn mutate(
        &mut self,
        expected_digest: &str,
        mutation: &ConfigMutation,
    ) -> Result<ConfigMutationOutcome, ConfigRuntimeError> {
        self.mutate_with_entropy(expected_digest, mutation, &mut SystemSourceIdEntropy)
    }

    pub fn mutate_with_entropy(
        &mut self,
        expected_digest: &str,
        mutation: &ConfigMutation,
        entropy: &mut impl crate::SourceIdEntropy,
    ) -> Result<ConfigMutationOutcome, ConfigRuntimeError> {
        self.begin_candidate();
        let result = self.mutate_inner(expected_digest, mutation, entropy);
        self.finish_candidate(&result);
        result
    }

    fn mutate_inner(
        &mut self,
        expected_digest: &str,
        mutation: &ConfigMutation,
        entropy: &mut impl crate::SourceIdEntropy,
    ) -> Result<ConfigMutationOutcome, ConfigRuntimeError> {
        if expected_digest != self.observed_digest()? || !self.disk_matches_current() {
            return Err(ConfigError::Conflict.into());
        }
        let mut document = self.current.document();
        let mut added_index = None;
        let mut preferred_ids = self
            .current_sources
            .sources()
            .iter()
            .map(|source| Some(source.id().clone()))
            .collect::<Vec<_>>();
        apply_mutation(
            &mut document,
            mutation,
            &self.current_sources,
            &mut added_index,
            &mut preferred_ids,
        )?;
        let change =
            self.apply_document_with_ids(expected_digest, &document, &preferred_ids, entropy)?;
        let source_id = added_index.and_then(|index| match &change {
            ConfigChange::Changed { sources, .. } => sources
                .sources()
                .get(index)
                .map(|source| source.id().as_str().to_owned()),
            ConfigChange::Unchanged => None,
        });
        Ok(ConfigMutationOutcome { change, source_id })
    }

    #[cfg(unix)]
    fn reconcile_module_entry(&self) -> Result<Option<ConfigSnapshot>, ConfigRuntimeError> {
        use std::os::unix::fs::symlink;

        let Some(entry) = &self.module_entry else {
            return Ok(None);
        };
        match std::fs::symlink_metadata(entry) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let target =
                    std::fs::read_link(entry).map_err(|_| ConfigRuntimeError::ModuleEntry)?;
                if target == self.store.path() {
                    Ok(None)
                } else {
                    Err(ConfigRuntimeError::ModuleEntry)
                }
            }
            Ok(metadata) if metadata.is_file() => {
                let candidate = ConfigStore::new(entry)?.load_without_parent_check()?;
                self.admit(&candidate)?;
                let bytes = read_stable(entry)?;
                let checkpoint = self.store.checkpoint()?;
                let temporary =
                    entry.with_file_name(format!(".nethop.toml.link.{}", std::process::id()));
                if std::fs::symlink_metadata(&temporary).is_ok() {
                    return Err(ConfigRuntimeError::ModuleEntry);
                }
                symlink(self.store.path(), &temporary)
                    .map_err(|_| ConfigRuntimeError::ModuleEntry)?;
                if let Err(error) = atomic_write(self.store.path(), &bytes) {
                    let _ = std::fs::remove_file(&temporary);
                    return Err(error.into());
                }
                if std::fs::rename(&temporary, entry).is_err() {
                    let _ = std::fs::remove_file(&temporary);
                    let observed = self.store.observed_digest()?;
                    if self
                        .store
                        .restore_checkpoint(&observed, &checkpoint)
                        .is_err()
                    {
                        return Err(ConfigRuntimeError::Rollback);
                    }
                    return Err(ConfigRuntimeError::ModuleEntry);
                }
                Ok(Some(self.store.load()?))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                symlink(self.store.path(), entry).map_err(|_| ConfigRuntimeError::ModuleEntry)?;
                Ok(None)
            }
            _ => Err(ConfigRuntimeError::ModuleEntry),
        }
    }

    #[cfg(not(unix))]
    fn reconcile_module_entry(&self) -> Result<Option<ConfigSnapshot>, ConfigRuntimeError> {
        if self.module_entry.is_some() {
            Err(ConfigRuntimeError::ModuleEntry)
        } else {
            Ok(None)
        }
    }

    pub fn set_service_enabled(
        &mut self,
        enabled: bool,
    ) -> Result<ConfigChange, ConfigRuntimeError> {
        self.begin_candidate();
        let result = self.set_service_enabled_inner(enabled);
        self.finish_candidate(&result);
        result
    }

    fn set_service_enabled_inner(
        &mut self,
        enabled: bool,
    ) -> Result<ConfigChange, ConfigRuntimeError> {
        if self.current.effective().service_enabled() == enabled {
            return Ok(ConfigChange::Unchanged);
        }
        let prepared_config = self
            .store
            .prepare_service_enabled(self.current.digest(), enabled)?;
        let prepared_sources = self
            .registry
            .prepare(prepared_config.snapshot(), &mut SystemSourceIdEntropy)?;
        let store_checkpoint = self.store.checkpoint()?;
        let registry_checkpoint = self.registry.checkpoint()?;
        let candidate = self
            .store
            .commit_prepared(self.current.digest(), prepared_config)?;
        let sources = match self.registry.activate(prepared_sources) {
            Ok(sources) => sources,
            Err(error) => {
                if self
                    .store
                    .restore_checkpoint(candidate.digest(), &store_checkpoint)
                    .is_err()
                    || self
                        .registry
                        .restore_checkpoint_exact(&registry_checkpoint)
                        .is_err()
                {
                    return Err(ConfigRuntimeError::Rollback);
                }
                return Err(error.into());
            }
        };
        self.accept_resolved(candidate, sources)
    }

    fn begin_candidate(&mut self) {
        self.candidate_sequence = self.candidate_sequence.saturating_add(1).max(1);
    }

    fn finish_candidate<T>(&mut self, result: &Result<T, ConfigRuntimeError>) {
        self.last_reload = if result.is_ok() {
            ConfigReloadState::Accepted
        } else {
            ConfigReloadState::Rejected
        };
    }

    fn accept(&mut self, candidate: ConfigSnapshot) -> Result<ConfigChange, ConfigRuntimeError> {
        let sources = self
            .registry
            .reconcile(&candidate, &mut SystemSourceIdEntropy)?;
        self.accept_resolved(candidate, sources)
    }

    fn accept_resolved(
        &mut self,
        candidate: ConfigSnapshot,
        sources: SourceConfig,
    ) -> Result<ConfigChange, ConfigRuntimeError> {
        let plan = self.current.effective().change_plan(candidate.effective());
        let enabled = candidate.effective().service_enabled();
        let service_changed = enabled != self.current.effective().service_enabled();
        let sources_changed = sources.source_config_digest() != self.current_source_digest;
        let digest = candidate.digest().to_owned();
        self.current_source_digest = sources.source_config_digest().to_owned();
        self.current_sources = sources.clone();
        self.current = candidate;
        Ok(ConfigChange::Changed {
            digest,
            enabled,
            service_changed,
            sources_changed,
            sources,
            plan,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigMutationOutcome {
    change: ConfigChange,
    source_id: Option<String>,
}

pub struct ConfigRuntimeCheckpoint {
    store: ConfigStoreCheckpoint,
    registry: SourceRegistryCheckpoint,
    current: ConfigSnapshot,
    current_source_digest: String,
    current_sources: SourceConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigReloadState {
    Accepted,
    Rejected,
}

impl ConfigReloadState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
        }
    }
}

impl fmt::Debug for ConfigRuntimeCheckpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConfigRuntimeCheckpoint")
            .field("active_digest", &self.current.digest())
            .field("source_count", &self.current_sources.sources().len())
            .finish_non_exhaustive()
    }
}

impl ConfigMutationOutcome {
    pub const fn change(&self) -> &ConfigChange {
        &self.change
    }

    pub fn source_id(&self) -> Option<&str> {
        self.source_id.as_deref()
    }

    pub fn into_change(self) -> ConfigChange {
        self.change
    }
}

fn apply_mutation(
    document: &mut serde_json::Value,
    mutation: &ConfigMutation,
    sources: &SourceConfig,
    added_index: &mut Option<usize>,
    preferred_ids: &mut Vec<Option<SourceId>>,
) -> Result<(), ConfigRuntimeError> {
    match mutation {
        ConfigMutation::SetServiceEnabled { enabled } => {
            set_value(document, "/service/enabled", serde_json::json!(enabled))?;
        }
        ConfigMutation::AddSource { name, url } => {
            let list = array_mut(document, "/subscriptions/sources")?;
            *added_index = Some(list.len());
            list.push(serde_json::json!({"name": name, "url": url}));
            preferred_ids.push(None);
        }
        ConfigMutation::UpdateSource {
            source_id,
            name,
            url,
            enabled,
        } => {
            let index = source_index(sources, source_id)?;
            let source = array_mut(document, "/subscriptions/sources")?
                .get_mut(index)
                .and_then(serde_json::Value::as_object_mut)
                .ok_or(ConfigError::InvalidToml)?;
            if let Some(value) = name {
                source.insert("name".into(), serde_json::json!(value));
            }
            if let Some(value) = url {
                source.insert("url".into(), serde_json::json!(value));
            }
            if let Some(value) = enabled {
                source.insert("enabled".into(), serde_json::json!(value));
            }
        }
        ConfigMutation::RemoveSource { source_id } => {
            let index = source_index(sources, source_id)?;
            array_mut(document, "/subscriptions/sources")?.remove(index);
            preferred_ids.remove(index);
        }
        ConfigMutation::MoveSource {
            source_id,
            before_source_id,
        } => {
            let from = source_index(sources, source_id)?;
            let before = before_source_id
                .as_ref()
                .map(|id| source_index(sources, id))
                .transpose()?;
            let list = array_mut(document, "/subscriptions/sources")?;
            let value = list.remove(from);
            let preferred_id = preferred_ids.remove(from);
            let target = before.map_or(
                list.len(),
                |index| {
                    if from < index { index - 1 } else { index }
                },
            );
            list.insert(target, value);
            preferred_ids.insert(target, preferred_id);
        }
        ConfigMutation::AddPackage { package } => {
            array_mut(document, "/applications/packages")?.push(serde_json::json!(package));
        }
        ConfigMutation::RemovePackage { package } => {
            remove_string(array_mut(document, "/applications/packages")?, package)?;
        }
        ConfigMutation::ReplacePackages { packages } => {
            set_value(
                document,
                "/applications/packages",
                serde_json::json!(packages),
            )?;
        }
        ConfigMutation::AddRoutingCidr { list, cidr } => {
            array_mut(document, routing_pointer(*list))?.push(serde_json::json!(cidr));
        }
        ConfigMutation::RemoveRoutingCidr { list, cidr } => {
            remove_string(array_mut(document, routing_pointer(*list))?, cidr)?;
        }
        ConfigMutation::SetScalarField { field_id, value } => {
            let pointer = scalar_pointer(field_id).ok_or(ConfigError::UnknownField)?;
            set_value(document, pointer, value.clone())?;
        }
    }
    Ok(())
}

fn source_index(sources: &SourceConfig, source_id: &str) -> Result<usize, ConfigRuntimeError> {
    sources
        .sources()
        .iter()
        .position(|source| source.id().as_str() == source_id)
        .ok_or(ConfigError::InvalidValue.into())
}

fn array_mut<'a>(
    document: &'a mut serde_json::Value,
    pointer: &str,
) -> Result<&'a mut Vec<serde_json::Value>, ConfigRuntimeError> {
    document
        .pointer_mut(pointer)
        .and_then(serde_json::Value::as_array_mut)
        .ok_or(ConfigError::InvalidToml.into())
}

fn set_value(
    document: &mut serde_json::Value,
    pointer: &str,
    value: serde_json::Value,
) -> Result<(), ConfigRuntimeError> {
    *document
        .pointer_mut(pointer)
        .ok_or(ConfigError::InvalidToml)? = value;
    Ok(())
}

fn remove_string(
    values: &mut Vec<serde_json::Value>,
    target: &str,
) -> Result<(), ConfigRuntimeError> {
    let index = values
        .iter()
        .position(|value| value.as_str() == Some(target))
        .ok_or(ConfigError::InvalidValue)?;
    values.remove(index);
    Ok(())
}

const fn routing_pointer(list: RoutingCidrList) -> &'static str {
    match list {
        RoutingCidrList::ForceProxy => "/routing/force_proxy_cidrs",
        RoutingCidrList::Bypass => "/routing/bypass_cidrs",
    }
}

fn scalar_pointer(field_id: &str) -> Option<&'static str> {
    match field_id {
        "service.enabled" => Some("/service/enabled"),
        "subscriptions.auto_update" => Some("/subscriptions/auto_update"),
        "subscriptions.update_interval_hours" => Some("/subscriptions/update_interval_hours"),
        "proxy.outbound_mode" => Some("/proxy/outbound_mode"),
        "proxy.selector_mode" => Some("/proxy/selector_mode"),
        "proxy.urltest.interval_minutes" => Some("/proxy/urltest/interval_minutes"),
        "proxy.urltest.tolerance_ms" => Some("/proxy/urltest/tolerance_ms"),
        "proxy.urltest.max_candidates" => Some("/proxy/urltest/max_candidates"),
        "proxy.urltest.concurrency" => Some("/proxy/urltest/concurrency"),
        "applications.mode" => Some("/applications/mode"),
        "network.capture_mode" => Some("/network/capture_mode"),
        "network.proxy_tcp" => Some("/network/proxy_tcp"),
        "network.proxy_udp" => Some("/network/proxy_udp"),
        "network.ipv6_mode" => Some("/network/ipv6_mode"),
        "network.dns_mode" => Some("/network/dns_mode"),
        "network.tun_stack" => Some("/network/tun_stack"),
        "network.interfaces.mobile" => Some("/network/interfaces/mobile"),
        "network.interfaces.wifi" => Some("/network/interfaces/wifi"),
        "network.interfaces.hotspot" => Some("/network/interfaces/hotspot"),
        "network.interfaces.usb" => Some("/network/interfaces/usb"),
        "routing.bypass_private" => Some("/routing/bypass_private"),
        "routing.bypass_cn" => Some("/routing/bypass_cn"),
        "routing.block_quic" => Some("/routing/block_quic"),
        "logging.level" => Some("/logging/level"),
        "logging.retention_days" => Some("/logging/retention_days"),
        "advanced.inbound_port" => Some("/advanced/inbound_port"),
        "advanced.bypass_mark" => Some("/advanced/bypass_mark"),
        "advanced.ipv6_guard" => Some("/advanced/ipv6_guard"),
        "advanced.dry_run" => Some("/advanced/dry_run"),
        "advanced.health_timeout_seconds" => Some("/advanced/health_timeout_seconds"),
        "advanced.reconcile_interval_seconds" => Some("/advanced/reconcile_interval_seconds"),
        _ => None,
    }
}

impl fmt::Debug for ConfigRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConfigRuntime")
            .field("store", &self.store)
            .field("registry", &self.registry)
            .field("current", &self.current)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigChange {
    Unchanged,
    Changed {
        digest: String,
        enabled: bool,
        service_changed: bool,
        sources_changed: bool,
        sources: SourceConfig,
        plan: ChangePlan,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigPreview {
    observed_digest: String,
    candidate_digest: String,
    plan: ChangePlan,
}

impl ConfigPreview {
    pub fn observed_digest(&self) -> &str {
        &self.observed_digest
    }

    pub fn candidate_digest(&self) -> &str {
        &self.candidate_digest
    }

    pub const fn plan(&self) -> &ChangePlan {
        &self.plan
    }

    pub fn changed_field_ids(&self) -> Vec<&'static str> {
        self.plan
            .changes()
            .iter()
            .flat_map(|change| match change {
                crate::ChangeKind::Service => vec!["service.enabled"],
                crate::ChangeKind::SubscriptionSchedule => vec![
                    "subscriptions.auto_update",
                    "subscriptions.update_interval_hours",
                ],
                crate::ChangeKind::Sources => vec!["subscriptions.sources"],
                crate::ChangeKind::Proxy => vec!["proxy"],
                crate::ChangeKind::Applications => vec!["applications"],
                crate::ChangeKind::Network => vec!["network"],
                crate::ChangeKind::Routing => vec!["routing"],
                crate::ChangeKind::Logging => vec!["logging"],
                crate::ChangeKind::Advanced => vec!["advanced"],
            })
            .collect()
    }
}

#[derive(Debug, Error)]
pub enum ConfigRuntimeError {
    #[error("configuration candidate is invalid")]
    Config(#[from] ConfigError),
    #[error("source identity registry could not accept the candidate")]
    Registry(#[from] SourceRegistryError),
    #[error("module configuration entry is invalid")]
    ModuleEntry,
    #[error("configuration rollback failed")]
    Rollback,
}

impl ConfigRuntimeError {
    pub const fn diagnostic(&self) -> (ErrorDomain, &'static str) {
        match self {
            Self::Config(error) => (ErrorDomain::Config, error.diagnostic_detail()),
            Self::Registry(error) => (ErrorDomain::Source, error.diagnostic_detail()),
            Self::ModuleEntry => (ErrorDomain::Config, "ENTRY-DIVERGED"),
            Self::Rollback => (ErrorDomain::Config, "APPLY-ROLLED-BACK"),
        }
    }

    pub const fn is_conflict(&self) -> bool {
        matches!(self, Self::Config(ConfigError::Conflict))
    }
}
