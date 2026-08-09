use std::time::{Duration, Instant, SystemTime};

#[cfg(feature = "subscription-update")]
use std::time::UNIX_EPOCH;

#[cfg(feature = "subscription-update")]
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use nethop_android::{
    CapabilityError, CapabilityReport, PlanSlot, PrivateDnsFactsSource, UpdateNotificationOutcome,
    UpdateNotificationSink,
};
#[cfg(feature = "subscription-update")]
use nethop_android::{CapabilityStatus, ResourceCandidate, WifiFactsSource};
use nethop_core::{CapturePolicy, GenerationId, RuntimeState};
#[cfg(feature = "subscription-update")]
use nethop_core::{ManagedOptions, TunStack};
use nethop_protocol::{
    ControlError, ControlMethod, ControlParams, ControlRequest, ControlResponse, ErrorDomain,
    EventKind, PROTOCOL_VERSION, WebUiErrorKind, WebUiPayloadOperation,
};
use serde_json::json;

#[cfg(feature = "subscription-update")]
use crate::worker_services::{unavailable_control_error, unavailable_control_error_with_details};
use crate::{
    ActiveRuntime, CandidateProcess, CapabilitySource, ControlCommand, ControlRequestHandler,
    ControlSnapshot, CurrentGenerationActivator, DataPlaneHealthProbe, HealthProbe,
    NetworkController, RestartBudget, RestartDecision, RuntimeHealthVerifier, RuntimeTick,
    UpdateStatus, WorkerControlHandler, WorkerRecoveryError, WorkerRuntime, WorkerRuntimeLimits,
    WorkerServiceError, WorkerServiceTasks,
};
use crate::{CandidateChecker, CoreLauncher, OperationalControl, WebUiPayloadStore};
#[cfg(feature = "subscription-update")]
use crate::{
    ConfigChange, ConfigRuntime, ConfigRuntimeCheckpoint, RuleSetUpdatePreparation,
    RuntimeCoreVersionSchedule, RuntimeLogRetention, RuntimeRuleSetSchedule,
    RuntimeRuleSetUpdateSource, RuntimeUpdateSchedule, SourceConfig, SourceStatusStore,
    SourceUpdateReport, UnavailableCoreVersionSchedule, UnavailableLogRetention,
    UnavailableRuleSetSchedule, UnavailableRuleSetUpdateSource, UnavailableUpdateSchedule,
};
use crate::{
    CoreReleaseBodyFetcher, CoreUpdateAvailability, CoreVersion, CoreVersionCheckError,
    CoreVersionChecker, CoreVersionStateSink, CoreVersionStatus,
};
#[cfg(feature = "subscription-update")]
use nethop_subscription::FormatHint;

const IDLE_WAKEUP: Duration = Duration::from_secs(1);

#[cfg(feature = "subscription-update")]
fn rule_set_wall_seconds() -> Option<i64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
}

pub trait WorkerClock {
    fn now(&self) -> Duration;
}

pub trait RuntimeCoreVersionSource {
    fn check(&mut self) -> Result<CoreVersionStatus, CoreVersionCheckError>;
}

impl<F: CoreReleaseBodyFetcher> RuntimeCoreVersionSource for CoreVersionChecker<F> {
    fn check(&mut self) -> Result<CoreVersionStatus, CoreVersionCheckError> {
        CoreVersionChecker::check(self)
    }
}

pub type ApplicationRecovery<P, R> = Result<Option<ActiveRuntime<P, R>>, WorkerRecoveryError>;

#[derive(Debug)]
pub struct MonotonicClock {
    started: Instant,
}

impl MonotonicClock {
    pub fn start() -> Self {
        Self {
            started: Instant::now(),
        }
    }
}

impl WorkerClock for MonotonicClock {
    fn now(&self) -> Duration {
        self.started.elapsed()
    }
}

pub trait RuntimeRecoverySource<N>
where
    N: NetworkController,
{
    type Process: CandidateProcess;

    fn recover(
        &mut self,
        network: &mut N,
        policy: &CapturePolicy,
        slot: PlanSlot,
    ) -> ApplicationRecovery<Self::Process, N::Receipt>;

    fn recover_generation(
        &mut self,
        network: &mut N,
        policy: &CapturePolicy,
        slot: PlanSlot,
        generation: GenerationId,
    ) -> ApplicationRecovery<Self::Process, N::Receipt>;

    fn probe(&mut self) -> Result<CapabilityReport, CapabilityError>;

    #[cfg(feature = "subscription-update")]
    fn replace_runtime_policy(
        &mut self,
        _candidates: Vec<ResourceCandidate>,
        _inbound_port: u16,
        _health_timeout: Duration,
    ) -> Result<(), RuntimePolicyError> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeUpdateError {
    Prepare,
    Commit,
    Discard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimePolicyError {
    Capability,
    CoreHealth,
    DataPlaneHealth,
}

pub trait RuntimeUpdateSource {
    type Prepared;

    fn is_available(&self) -> bool {
        true
    }

    fn is_needed(&self) -> bool {
        true
    }

    #[cfg(feature = "subscription-update")]
    fn take_source_update_report(&mut self) -> Option<SourceUpdateReport> {
        None
    }

    #[cfg(feature = "subscription-update")]
    fn request_source_update(
        &mut self,
        _source_id: Option<&str>,
    ) -> Result<(), RuntimeUpdateError> {
        Ok(())
    }

    #[cfg(feature = "subscription-update")]
    fn replace_config(&mut self, _config: SourceConfig) {}

    #[cfg(feature = "subscription-update")]
    fn replace_runtime_policy(
        &mut self,
        _capture: CapturePolicy,
        _tun_stack: TunStack,
        _options: ManagedOptions,
    ) {
    }

    fn prepare(&mut self) -> Result<Self::Prepared, RuntimeUpdateError>;
    fn generation(&self, prepared: &Self::Prepared) -> GenerationId;
    fn is_current(&self, _prepared: &Self::Prepared) -> bool {
        true
    }
    fn commit(&mut self, prepared: Self::Prepared) -> Result<GenerationId, RuntimeUpdateError>;
    fn discard(&mut self, prepared: Self::Prepared) -> Result<(), RuntimeUpdateError>;

    #[cfg(feature = "subscription-update")]
    fn preview_import(
        &mut self,
        _bytes: &[u8],
        _format_hint: FormatHint,
    ) -> Result<serde_json::Value, RuntimeUpdateError> {
        Err(RuntimeUpdateError::Prepare)
    }

    #[cfg(feature = "subscription-update")]
    fn request_import(
        &mut self,
        _bytes: Vec<u8>,
        _format_hint: FormatHint,
        _candidate_digest: String,
    ) -> Result<(), RuntimeUpdateError> {
        Err(RuntimeUpdateError::Prepare)
    }
}

#[derive(Debug, Default)]
pub struct UnavailableRuntimeUpdateSource;

impl RuntimeUpdateSource for UnavailableRuntimeUpdateSource {
    type Prepared = ();

    fn is_available(&self) -> bool {
        false
    }

    fn prepare(&mut self) -> Result<Self::Prepared, RuntimeUpdateError> {
        Err(RuntimeUpdateError::Prepare)
    }

    fn generation(&self, _prepared: &Self::Prepared) -> GenerationId {
        GenerationId::new(1).expect("one is a valid generation")
    }

    fn commit(&mut self, _prepared: Self::Prepared) -> Result<GenerationId, RuntimeUpdateError> {
        Err(RuntimeUpdateError::Commit)
    }

    fn discard(&mut self, _prepared: Self::Prepared) -> Result<(), RuntimeUpdateError> {
        Err(RuntimeUpdateError::Discard)
    }
}

pub struct OptionalRuntimeUpdateSource<U> {
    inner: Option<U>,
}

impl<U> OptionalRuntimeUpdateSource<U> {
    pub const fn new(inner: Option<U>) -> Self {
        Self { inner }
    }
}

impl<U> RuntimeUpdateSource for OptionalRuntimeUpdateSource<U>
where
    U: RuntimeUpdateSource,
{
    type Prepared = U::Prepared;

    fn is_available(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(RuntimeUpdateSource::is_available)
    }

    fn is_needed(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(RuntimeUpdateSource::is_needed)
    }

    #[cfg(feature = "subscription-update")]
    fn take_source_update_report(&mut self) -> Option<SourceUpdateReport> {
        self.inner
            .as_mut()
            .and_then(RuntimeUpdateSource::take_source_update_report)
    }

    #[cfg(feature = "subscription-update")]
    fn request_source_update(&mut self, source_id: Option<&str>) -> Result<(), RuntimeUpdateError> {
        self.inner
            .as_mut()
            .ok_or(RuntimeUpdateError::Prepare)?
            .request_source_update(source_id)
    }

    #[cfg(feature = "subscription-update")]
    fn replace_config(&mut self, config: SourceConfig) {
        if let Some(inner) = &mut self.inner {
            inner.replace_config(config);
        }
    }

    #[cfg(feature = "subscription-update")]
    fn replace_runtime_policy(
        &mut self,
        capture: CapturePolicy,
        tun_stack: TunStack,
        options: ManagedOptions,
    ) {
        if let Some(inner) = &mut self.inner {
            inner.replace_runtime_policy(capture, tun_stack, options);
        }
    }

    fn prepare(&mut self) -> Result<Self::Prepared, RuntimeUpdateError> {
        self.inner
            .as_mut()
            .ok_or(RuntimeUpdateError::Prepare)?
            .prepare()
    }

    fn generation(&self, prepared: &Self::Prepared) -> GenerationId {
        self.inner
            .as_ref()
            .expect("prepared update requires an available source")
            .generation(prepared)
    }

    fn is_current(&self, prepared: &Self::Prepared) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|inner| inner.is_current(prepared))
    }

    fn commit(&mut self, prepared: Self::Prepared) -> Result<GenerationId, RuntimeUpdateError> {
        self.inner
            .as_mut()
            .ok_or(RuntimeUpdateError::Commit)?
            .commit(prepared)
    }

    fn discard(&mut self, prepared: Self::Prepared) -> Result<(), RuntimeUpdateError> {
        self.inner
            .as_mut()
            .ok_or(RuntimeUpdateError::Discard)?
            .discard(prepared)
    }

    #[cfg(feature = "subscription-update")]
    fn preview_import(
        &mut self,
        bytes: &[u8],
        format_hint: FormatHint,
    ) -> Result<serde_json::Value, RuntimeUpdateError> {
        self.inner
            .as_mut()
            .ok_or(RuntimeUpdateError::Prepare)?
            .preview_import(bytes, format_hint)
    }

    #[cfg(feature = "subscription-update")]
    fn request_import(
        &mut self,
        bytes: Vec<u8>,
        format_hint: FormatHint,
        candidate_digest: String,
    ) -> Result<(), RuntimeUpdateError> {
        self.inner
            .as_mut()
            .ok_or(RuntimeUpdateError::Prepare)?
            .request_import(bytes, format_hint, candidate_digest)
    }
}

pub struct WorkerRecoveryCoordinator<'a, C, L, H, S, D> {
    store: &'a nethop_core::GenerationStore,
    checker: &'a C,
    launcher: &'a L,
    core_health: &'a mut H,
    capability_source: S,
    data_plane_health: D,
}

impl<'a, C, L, H, S, D> WorkerRecoveryCoordinator<'a, C, L, H, S, D> {
    pub const fn new(
        store: &'a nethop_core::GenerationStore,
        checker: &'a C,
        launcher: &'a L,
        core_health: &'a mut H,
        capability_source: S,
        data_plane_health: D,
    ) -> Self {
        Self {
            store,
            checker,
            launcher,
            core_health,
            capability_source,
            data_plane_health,
        }
    }
}

impl<C, L, H, S, D, N> RuntimeRecoverySource<N> for WorkerRecoveryCoordinator<'_, C, L, H, S, D>
where
    C: CandidateChecker,
    L: CoreLauncher,
    H: HealthProbe<L::Process>,
    S: CapabilitySource,
    D: DataPlaneHealthProbe<L::Process>,
    N: NetworkController,
{
    type Process = L::Process;

    fn recover(
        &mut self,
        network: &mut N,
        policy: &CapturePolicy,
        slot: PlanSlot,
    ) -> ApplicationRecovery<Self::Process, N::Receipt> {
        CurrentGenerationActivator::new(
            self.store,
            self.checker,
            self.launcher,
            self.core_health,
            &mut self.capability_source,
            network,
            &mut self.data_plane_health,
        )
        .recover(policy, slot)
    }

    fn recover_generation(
        &mut self,
        network: &mut N,
        policy: &CapturePolicy,
        slot: PlanSlot,
        generation: GenerationId,
    ) -> ApplicationRecovery<Self::Process, N::Receipt> {
        CurrentGenerationActivator::new(
            self.store,
            self.checker,
            self.launcher,
            self.core_health,
            &mut self.capability_source,
            network,
            &mut self.data_plane_health,
        )
        .recover_generation(generation, policy, slot)
    }

    fn probe(&mut self) -> Result<CapabilityReport, CapabilityError> {
        self.capability_source.probe()
    }

    #[cfg(feature = "subscription-update")]
    fn replace_runtime_policy(
        &mut self,
        candidates: Vec<ResourceCandidate>,
        inbound_port: u16,
        health_timeout: Duration,
    ) -> Result<(), RuntimePolicyError> {
        self.capability_source
            .replace_policy(candidates, inbound_port)
            .map_err(|_| RuntimePolicyError::Capability)?;
        self.core_health
            .replace_timeout(health_timeout)
            .map_err(|_| RuntimePolicyError::CoreHealth)?;
        self.data_plane_health
            .replace_inbound_port(inbound_port)
            .map_err(|_| RuntimePolicyError::DataPlaneHealth)?;
        self.data_plane_health
            .replace_health_timeout(health_timeout)
            .map_err(|_| RuntimePolicyError::DataPlaneHealth)
    }
}

pub struct WorkerApplication<S, N, V, C, U = UnavailableRuntimeUpdateSource>
where
    N: NetworkController,
    S: RuntimeRecoverySource<N>,
    S::Process: CandidateProcess,
    U: RuntimeUpdateSource,
{
    control: WorkerControlHandler,
    recovery: S,
    runtime: Option<WorkerRuntime<S::Process, N::Receipt>>,
    network: N,
    verifier: V,
    clock: C,
    policy: CapturePolicy,
    slot: PlanSlot,
    limits: WorkerRuntimeLimits,
    restart_budget: RestartBudget,
    restart_at: Option<Duration>,
    start_pending: bool,
    capability_probe_sequence: u64,
    #[cfg(feature = "subscription-update")]
    dry_run: bool,
    event_hub: crate::EventHub,
    next_traffic_sample: Duration,
    webui_payload_store: Option<WebUiPayloadStore>,
    next_payload_cleanup: Duration,
    #[cfg(feature = "subscription-update")]
    update_schedule: Box<dyn RuntimeUpdateSchedule>,
    #[cfg(feature = "subscription-update")]
    core_version_schedule: Box<dyn RuntimeCoreVersionSchedule>,
    #[cfg(feature = "subscription-update")]
    core_version_schedule_retry_at: Option<Duration>,
    #[cfg(feature = "subscription-update")]
    rule_set_schedule: Box<dyn RuntimeRuleSetSchedule>,
    #[cfg(feature = "subscription-update")]
    rule_set_schedule_retry_at: Option<Duration>,
    #[cfg(feature = "subscription-update")]
    rule_set_updater: Box<dyn RuntimeRuleSetUpdateSource>,
    #[cfg(feature = "subscription-update")]
    rule_set_state: &'static str,
    #[cfg(feature = "subscription-update")]
    rule_set_diagnostic: Option<&'static str>,
    #[cfg(feature = "subscription-update")]
    rule_set_last_attempt_wall: Option<i64>,
    #[cfg(feature = "subscription-update")]
    rule_set_last_success_wall: Option<i64>,
    #[cfg(feature = "subscription-update")]
    log_retention: Box<dyn RuntimeLogRetention>,
    #[cfg_attr(not(feature = "subscription-update"), allow(dead_code))]
    updater: U,
    #[cfg(feature = "subscription-update")]
    config: Option<ConfigRuntime>,
    #[cfg(feature = "subscription-update")]
    config_dirty: Option<Arc<AtomicBool>>,
    #[cfg(feature = "subscription-update")]
    config_watch_healthy: Option<Arc<AtomicBool>>,
    #[cfg(feature = "subscription-update")]
    last_watch_health: Option<bool>,
    #[cfg(feature = "subscription-update")]
    source_status: Option<SourceStatusStore>,
    #[cfg(feature = "subscription-update")]
    wifi_facts: Option<Box<dyn WifiFactsSource>>,
    #[cfg(feature = "subscription-update")]
    wifi_scene_next_probe: Duration,
    #[cfg(feature = "subscription-update")]
    wifi_scene_override: Option<bool>,
    operational_control: Option<OperationalControl>,
    core_version_source: Option<Box<dyn RuntimeCoreVersionSource>>,
    core_update_notifier: Option<Box<dyn UpdateNotificationSink>>,
    core_version_state: Option<Box<dyn CoreVersionStateSink>>,
    core_version_status: Option<CoreVersionStatus>,
    last_notified_core_version: Option<CoreVersion>,
    private_dns_source: Option<Box<dyn PrivateDnsFactsSource>>,
}

impl<S, N, V, C> WorkerApplication<S, N, V, C, UnavailableRuntimeUpdateSource>
where
    N: NetworkController,
    S: RuntimeRecoverySource<N>,
    S::Process: CandidateProcess,
    V: RuntimeHealthVerifier,
    C: WorkerClock,
{
    pub fn new(
        recovery: S,
        network: N,
        verifier: V,
        clock: C,
        policy: CapturePolicy,
        slot: PlanSlot,
        limits: WorkerRuntimeLimits,
    ) -> Self {
        Self::new_inner(
            recovery,
            network,
            verifier,
            clock,
            policy,
            slot,
            limits,
            UnavailableRuntimeUpdateSource,
        )
    }

    pub fn with_updater<U>(self, updater: U) -> WorkerApplication<S, N, V, C, U>
    where
        U: RuntimeUpdateSource,
    {
        WorkerApplication {
            control: WorkerControlHandler::new(self.control.snapshot())
                .with_update_available_if(updater.is_available()),
            recovery: self.recovery,
            runtime: self.runtime,
            network: self.network,
            verifier: self.verifier,
            clock: self.clock,
            policy: self.policy,
            slot: self.slot,
            limits: self.limits,
            restart_budget: self.restart_budget,
            restart_at: self.restart_at,
            start_pending: self.start_pending,
            capability_probe_sequence: self.capability_probe_sequence,
            #[cfg(feature = "subscription-update")]
            dry_run: self.dry_run,
            event_hub: self.event_hub,
            next_traffic_sample: self.next_traffic_sample,
            webui_payload_store: self.webui_payload_store,
            next_payload_cleanup: self.next_payload_cleanup,
            #[cfg(feature = "subscription-update")]
            update_schedule: self.update_schedule,
            #[cfg(feature = "subscription-update")]
            core_version_schedule: self.core_version_schedule,
            #[cfg(feature = "subscription-update")]
            core_version_schedule_retry_at: self.core_version_schedule_retry_at,
            #[cfg(feature = "subscription-update")]
            rule_set_schedule: self.rule_set_schedule,
            #[cfg(feature = "subscription-update")]
            rule_set_schedule_retry_at: self.rule_set_schedule_retry_at,
            #[cfg(feature = "subscription-update")]
            rule_set_updater: self.rule_set_updater,
            #[cfg(feature = "subscription-update")]
            rule_set_state: self.rule_set_state,
            #[cfg(feature = "subscription-update")]
            rule_set_diagnostic: self.rule_set_diagnostic,
            #[cfg(feature = "subscription-update")]
            rule_set_last_attempt_wall: self.rule_set_last_attempt_wall,
            #[cfg(feature = "subscription-update")]
            rule_set_last_success_wall: self.rule_set_last_success_wall,
            #[cfg(feature = "subscription-update")]
            log_retention: self.log_retention,
            updater,
            #[cfg(feature = "subscription-update")]
            config: self.config,
            #[cfg(feature = "subscription-update")]
            config_dirty: self.config_dirty,
            #[cfg(feature = "subscription-update")]
            config_watch_healthy: self.config_watch_healthy,
            #[cfg(feature = "subscription-update")]
            last_watch_health: self.last_watch_health,
            #[cfg(feature = "subscription-update")]
            source_status: self.source_status,
            #[cfg(feature = "subscription-update")]
            wifi_facts: self.wifi_facts,
            #[cfg(feature = "subscription-update")]
            wifi_scene_next_probe: self.wifi_scene_next_probe,
            #[cfg(feature = "subscription-update")]
            wifi_scene_override: self.wifi_scene_override,
            operational_control: self.operational_control,
            core_version_source: self.core_version_source,
            core_update_notifier: self.core_update_notifier,
            core_version_state: self.core_version_state,
            core_version_status: self.core_version_status,
            last_notified_core_version: self.last_notified_core_version,
            private_dns_source: self.private_dns_source,
        }
    }
}

impl<S, N, V, C, U> WorkerApplication<S, N, V, C, U>
where
    N: NetworkController,
    S: RuntimeRecoverySource<N>,
    S::Process: CandidateProcess,
    V: RuntimeHealthVerifier,
    C: WorkerClock,
    U: RuntimeUpdateSource,
{
    #[allow(clippy::too_many_arguments)]
    fn new_inner(
        recovery: S,
        network: N,
        verifier: V,
        clock: C,
        policy: CapturePolicy,
        slot: PlanSlot,
        limits: WorkerRuntimeLimits,
        updater: U,
    ) -> Self {
        let restart_budget = RestartBudget::new(limits.failure_window())
            .expect("validated worker limits contain a valid failure window");
        Self {
            control: WorkerControlHandler::new(ControlSnapshot {
                state: RuntimeState::Init,
                generation: None,
                last_update: UpdateStatus::Never,
            })
            .with_update_available_if(updater.is_available()),
            recovery,
            runtime: None,
            network,
            verifier,
            clock,
            policy,
            slot,
            limits,
            restart_budget,
            restart_at: None,
            start_pending: true,
            capability_probe_sequence: 0,
            #[cfg(feature = "subscription-update")]
            dry_run: false,
            event_hub: crate::EventHub::default(),
            next_traffic_sample: Duration::ZERO,
            webui_payload_store: None,
            next_payload_cleanup: Duration::ZERO,
            #[cfg(feature = "subscription-update")]
            update_schedule: Box::new(UnavailableUpdateSchedule),
            #[cfg(feature = "subscription-update")]
            core_version_schedule: Box::new(UnavailableCoreVersionSchedule),
            #[cfg(feature = "subscription-update")]
            core_version_schedule_retry_at: None,
            #[cfg(feature = "subscription-update")]
            rule_set_schedule: Box::new(UnavailableRuleSetSchedule),
            #[cfg(feature = "subscription-update")]
            rule_set_schedule_retry_at: None,
            #[cfg(feature = "subscription-update")]
            rule_set_updater: Box::new(UnavailableRuleSetUpdateSource),
            #[cfg(feature = "subscription-update")]
            rule_set_state: "never",
            #[cfg(feature = "subscription-update")]
            rule_set_diagnostic: None,
            #[cfg(feature = "subscription-update")]
            rule_set_last_attempt_wall: None,
            #[cfg(feature = "subscription-update")]
            rule_set_last_success_wall: None,
            #[cfg(feature = "subscription-update")]
            log_retention: Box::new(UnavailableLogRetention),
            updater,
            #[cfg(feature = "subscription-update")]
            config: None,
            #[cfg(feature = "subscription-update")]
            config_dirty: None,
            #[cfg(feature = "subscription-update")]
            config_watch_healthy: None,
            #[cfg(feature = "subscription-update")]
            last_watch_health: None,
            #[cfg(feature = "subscription-update")]
            source_status: None,
            #[cfg(feature = "subscription-update")]
            wifi_facts: None,
            #[cfg(feature = "subscription-update")]
            wifi_scene_next_probe: Duration::ZERO,
            #[cfg(feature = "subscription-update")]
            wifi_scene_override: None,
            operational_control: None,
            core_version_source: None,
            core_update_notifier: None,
            core_version_state: None,
            core_version_status: None,
            last_notified_core_version: None,
            private_dns_source: None,
        }
    }

    pub fn with_operational_control(mut self, control: OperationalControl) -> Self {
        self.operational_control = Some(control);
        self
    }

    pub fn with_webui_payload_store(mut self, store: WebUiPayloadStore) -> Self {
        self.webui_payload_store = Some(store);
        self
    }

    pub fn with_private_dns_source<T: PrivateDnsFactsSource + 'static>(
        mut self,
        source: T,
    ) -> Self {
        self.private_dns_source = Some(Box::new(source));
        self
    }

    pub fn with_core_version_source<T: RuntimeCoreVersionSource + 'static>(
        mut self,
        source: T,
    ) -> Self {
        self.core_version_source = Some(Box::new(source));
        self
    }

    pub fn with_core_update_notifier<T: UpdateNotificationSink + 'static>(
        mut self,
        notifier: T,
    ) -> Self {
        self.core_update_notifier = Some(Box::new(notifier));
        self
    }

    pub fn with_core_version_state<T: CoreVersionStateSink + 'static>(
        mut self,
        mut state: T,
    ) -> Self {
        match state.restore() {
            Ok(Some((status, last_notified))) => {
                self.core_version_status = Some(status);
                self.last_notified_core_version = last_notified;
            }
            Ok(None) => {}
            Err(_) => self.event_hub.publish(
                EventKind::Runtime,
                json!({"kind":"core_update","state":"state_restore_failed"}),
            ),
        }
        self.core_version_state = Some(Box::new(state));
        self
    }

    #[cfg(feature = "subscription-update")]
    pub fn with_core_version_schedule<T: RuntimeCoreVersionSchedule + 'static>(
        mut self,
        schedule: T,
    ) -> Self {
        self.core_version_schedule = Box::new(schedule);
        self
    }

    #[cfg(feature = "subscription-update")]
    pub fn with_rule_set_update_source<T: RuntimeRuleSetUpdateSource + 'static>(
        mut self,
        source: T,
    ) -> Self {
        self.rule_set_updater = Box::new(source);
        self
    }

    #[cfg(feature = "subscription-update")]
    pub fn with_rule_set_schedule<T: RuntimeRuleSetSchedule + 'static>(
        mut self,
        schedule: T,
    ) -> Self {
        self.rule_set_schedule = Box::new(schedule);
        self
    }

    #[cfg(feature = "subscription-update")]
    pub fn with_wifi_facts_source(mut self, source: impl WifiFactsSource + 'static) -> Self {
        self.wifi_facts = Some(Box::new(source));
        self.wifi_scene_next_probe = Duration::ZERO;
        self
    }

    #[cfg(feature = "subscription-update")]
    pub fn with_configuration(mut self, config: ConfigRuntime, restore_current: bool) -> Self {
        let enabled = config.current().effective().service_enabled();
        self.dry_run = config.current().effective().advanced().dry_run();
        if !enabled || !self.updater.is_available() || self.dry_run {
            self.start_pending = false;
            self.publish_snapshot(RuntimeState::FailOpenDirect, None);
            if enabled && self.updater.is_available() && self.dry_run {
                self.control.queue_command(ControlCommand::Update);
            }
        } else if !restore_current {
            self.start_pending = false;
            self.control.queue_command(ControlCommand::Update);
        }
        self.config = Some(config);
        if let Some(config) = self.config.as_ref() {
            let (enabled, interval, sources) = config.update_schedule();
            let _ = self.update_schedule.configure(enabled, interval, sources);
        }
        self
    }

    #[cfg(feature = "subscription-update")]
    pub fn with_update_schedule<T: RuntimeUpdateSchedule + 'static>(
        mut self,
        mut schedule: T,
    ) -> Self {
        if let Some(config) = self.config.as_ref() {
            let (enabled, interval, sources) = config.update_schedule();
            let _ = schedule.configure(enabled, interval, sources);
        }
        self.update_schedule = Box::new(schedule);
        self
    }

    #[cfg(feature = "subscription-update")]
    pub fn with_source_status_store(mut self, store: SourceStatusStore) -> Self {
        self.source_status = Some(store);
        self
    }

    #[cfg(feature = "subscription-update")]
    pub fn with_log_retention<T: RuntimeLogRetention + 'static>(
        mut self,
        mut retention: T,
    ) -> Self {
        if let Some(config) = self.config.as_ref() {
            let days = config.current().effective().logging().retention_days();
            if retention.configure(days, self.clock.now()).is_err() {
                self.event_hub.publish(
                    EventKind::Runtime,
                    json!({"kind":"logging","state":"retention_degraded"}),
                );
            }
        }
        self.log_retention = Box::new(retention);
        self
    }

    #[cfg(feature = "subscription-update")]
    pub fn with_event_log_directory(
        self,
        directory: impl Into<std::path::PathBuf>,
    ) -> Result<Self, crate::EventError> {
        self.event_hub.install_file_log(directory)?;
        Ok(self)
    }

    #[cfg(feature = "subscription-update")]
    pub fn with_config_wake(mut self, dirty: Arc<AtomicBool>, healthy: Arc<AtomicBool>) -> Self {
        self.config_dirty = Some(dirty);
        self.last_watch_health = Some(healthy.load(Ordering::Acquire));
        self.config_watch_healthy = Some(healthy);
        self
    }

    #[cfg(feature = "subscription-update")]
    fn watcher_health_wire(&self) -> &'static str {
        match &self.config_watch_healthy {
            Some(healthy) if healthy.load(Ordering::Acquire) => "healthy",
            Some(_) => "degraded",
            None => "not_configured",
        }
    }

    #[cfg(not(feature = "subscription-update"))]
    fn watcher_health_wire(&self) -> &'static str {
        "not_configured"
    }

    #[cfg(feature = "subscription-update")]
    fn config_observed_digest(&self) -> Option<String> {
        self.config
            .as_ref()
            .and_then(|config| config.observed_digest().ok())
    }

    #[cfg(not(feature = "subscription-update"))]
    fn config_observed_digest(&self) -> Option<String> {
        None
    }

    #[cfg(feature = "subscription-update")]
    fn config_active_digest(&self) -> Option<String> {
        self.config
            .as_ref()
            .map(|config| config.current().digest().to_owned())
    }

    #[cfg(not(feature = "subscription-update"))]
    fn config_active_digest(&self) -> Option<String> {
        None
    }

    #[cfg(feature = "subscription-update")]
    fn config_candidate_sequence(&self) -> u64 {
        self.config
            .as_ref()
            .map_or(0, ConfigRuntime::candidate_sequence)
    }

    #[cfg(not(feature = "subscription-update"))]
    fn config_candidate_sequence(&self) -> u64 {
        0
    }

    #[cfg(feature = "subscription-update")]
    fn observe_config_watch_health(&mut self) {
        let Some(healthy) = self.config_watch_healthy.as_ref() else {
            return;
        };
        let healthy = healthy.load(Ordering::Acquire);
        if self.last_watch_health == Some(healthy) {
            return;
        }
        self.last_watch_health = Some(healthy);
        self.event_hub.publish(
            EventKind::Config,
            json!({
                "kind": "config",
                "state": if healthy { "watch_restored" } else { "watch_degraded" },
                "watcher_health": if healthy { "healthy" } else { "degraded" },
            }),
        );
    }

    #[cfg(feature = "subscription-update")]
    fn reconcile_watched_config(&mut self) {
        let Some(dirty) = &self.config_dirty else {
            return;
        };
        if !dirty.swap(false, Ordering::AcqRel) {
            return;
        }
        let result = self.config.as_mut().map(ConfigRuntime::reload);
        if let Some(config) = self.config.as_ref() {
            self.event_hub.publish(
                EventKind::Config,
                json!({
                    "kind": "config",
                    "state": "observed",
                    "candidate_sequence": config.candidate_sequence(),
                    "observed_config_digest": config.observed_digest().ok(),
                    "active_config_digest": config.current().digest(),
                }),
            );
        }
        match result {
            Some(Ok(ConfigChange::Unchanged)) => {
                if let Some(config) = self.config.as_ref() {
                    self.event_hub.publish(
                        EventKind::Config,
                        json!({
                            "kind":"config",
                            "state":"accepted",
                            "candidate_sequence": config.candidate_sequence(),
                            "observed_config_digest": config.observed_digest().ok(),
                            "active_config_digest": config.current().digest(),
                        }),
                    );
                }
            }
            Some(Ok(change)) => self.apply_config_change(change),
            Some(Err(_)) => {
                if let Some(config) = self.config.as_ref() {
                    self.event_hub.publish(
                        EventKind::Config,
                        json!({
                            "kind":"config",
                            "state":"rejected",
                            "candidate_sequence": config.candidate_sequence(),
                            "observed_config_digest": config.observed_digest().ok(),
                            "active_config_digest": config.current().digest(),
                        }),
                    );
                }
            }
            None => {}
        }
    }

    #[cfg(feature = "subscription-update")]
    fn apply_config_change(&mut self, change: ConfigChange) {
        let ConfigChange::Changed {
            enabled,
            service_changed,
            sources_changed,
            sources,
            digest,
            plan,
        } = change
        else {
            return;
        };
        self.wifi_scene_next_probe = Duration::ZERO;
        self.wifi_scene_override = None;
        if let Some(config) = self.config.as_ref() {
            let settings = config.current().effective().subscriptions();
            if self
                .update_schedule
                .configure(
                    settings.auto_update(),
                    settings.update_interval_hours(),
                    &sources,
                )
                .is_err()
            {
                self.event_hub.publish(
                    EventKind::Subscription,
                    json!({"kind":"subscription","state":"schedule_degraded"}),
                );
            }
        }
        self.updater.replace_config(sources);
        if let Some(config) = self.config.as_ref() {
            let effective = config.current().effective();
            if self
                .log_retention
                .configure(effective.logging().retention_days(), self.clock.now())
                .is_err()
            {
                self.event_hub.publish(
                    EventKind::Runtime,
                    json!({"kind":"logging","state":"retention_degraded"}),
                );
            }
            self.dry_run = effective.advanced().dry_run();
            if let (Ok(capture), Ok(options)) =
                (config.capture_policy(), effective.managed_options())
            {
                self.policy = capture.clone();
                self.updater.replace_runtime_policy(
                    capture,
                    effective.managed_tun_stack(),
                    options,
                );
            }
            let advanced = effective.advanced();
            let runtime_policy_ready = self
                .recovery
                .replace_runtime_policy(
                    effective.allocations().to_vec(),
                    advanced.inbound_port(),
                    Duration::from_secs(u64::from(advanced.health_timeout_seconds())),
                )
                .is_ok()
                && self
                    .verifier
                    .replace_inbound_port(advanced.inbound_port())
                    .is_ok();
            let runtime_policy_ready = runtime_policy_ready
                && self
                    .verifier
                    .replace_health_timeout(Duration::from_secs(u64::from(
                        advanced.health_timeout_seconds(),
                    )))
                    .is_ok();
            if !runtime_policy_ready {
                self.event_hub.publish(
                    EventKind::Config,
                    json!({"kind":"config","state":"capability_rejected"}),
                );
                self.control.queue_command(ControlCommand::Stop);
            }
            let limits = WorkerRuntimeLimits::new(
                self.limits.core_poll_interval(),
                Duration::from_secs(u64::from(advanced.reconcile_interval_seconds())),
                self.limits.failure_window(),
            )
            .expect("validated advanced settings produce valid runtime limits");
            self.limits = limits;
            if let Some(runtime) = self.runtime.as_mut() {
                runtime.replace_limits(limits);
            }
        }
        self.event_hub.publish(
            EventKind::Config,
            json!({
                "kind": "config",
                "state": "accepted",
                "candidate_sequence": self.config.as_ref().map_or(0, ConfigRuntime::candidate_sequence),
                "observed_config_digest": digest,
                "active_config_digest": digest,
            }),
        );
        self.control
            .set_update_available(self.updater.is_available());
        if !enabled || !self.updater.is_available() {
            self.control.queue_command(ControlCommand::Stop);
        } else if self.dry_run
            || sources_changed
            || plan.impact() == crate::ApplyImpact::GenerationActivation
        {
            self.control.queue_command(ControlCommand::Stop);
            self.control.queue_command(ControlCommand::Update);
        } else if plan.impact() == crate::ApplyImpact::NetworkPlan {
            self.control.queue_command(ControlCommand::Stop);
            self.control.queue_command(ControlCommand::Start);
        } else if service_changed {
            if self.updater.is_needed() {
                self.control.queue_command(ControlCommand::Update);
            } else {
                self.control.queue_command(ControlCommand::Start);
            }
        }
    }

    #[cfg(feature = "subscription-update")]
    fn rollback_config_transaction(&mut self, checkpoint: ConfigRuntimeCheckpoint) -> bool {
        let result = self
            .config
            .as_mut()
            .ok_or(())
            .and_then(|config| config.rollback(checkpoint).map_err(|_| ()));
        match result {
            Ok(change) => {
                self.apply_config_change(change);
                self.event_hub.publish(
                    EventKind::Config,
                    json!({"kind":"config","state":"rolled_back"}),
                );
                true
            }
            Err(()) => {
                self.event_hub.publish(
                    EventKind::Config,
                    json!({"kind":"config","state":"rollback_failed"}),
                );
                false
            }
        }
    }

    #[cfg(feature = "subscription-update")]
    fn reconcile_wifi_scene(&mut self, now: Duration) {
        let Some(config) = self.config.as_ref() else {
            return;
        };
        let baseline_enabled = config.current().effective().service_enabled();
        let settings = config.current().effective().network().wifi_scenes().clone();
        if !settings.enabled() || self.wifi_facts.is_none() {
            self.wifi_scene_override = None;
            return;
        }
        if now < self.wifi_scene_next_probe {
            return;
        }
        self.wifi_scene_next_probe = now.saturating_add(Duration::from_secs(u64::from(
            settings.probe_interval_seconds(),
        )));

        let decision = self
            .wifi_facts
            .as_mut()
            .and_then(|source| source.current().ok())
            .and_then(|facts| settings.matcher().evaluate(&facts));
        let desired_enabled = baseline_enabled
            && decision
                .as_ref()
                .is_none_or(|decision| decision.action().service_enabled());
        let next_override = decision.as_ref().map(|_| desired_enabled);
        if self.wifi_scene_override == next_override {
            return;
        }
        self.wifi_scene_override = next_override;

        if desired_enabled {
            if self.runtime.is_none() && !self.start_pending {
                self.control.queue_command(ControlCommand::Start);
            }
        } else if self.runtime.is_some() || self.start_pending {
            self.start_pending = false;
            self.control.queue_command(ControlCommand::Stop);
        }
        self.event_hub.publish(
            EventKind::Network,
            json!({
                "kind": "wifi_scene",
                "scene_id": decision.as_ref().map(|decision| decision.scene_id()),
                "proxy_enabled": desired_enabled,
            }),
        );
    }

    pub fn snapshot(&self) -> ControlSnapshot {
        let state = self
            .runtime
            .as_ref()
            .map_or(self.control_snapshot().state, WorkerRuntime::state);
        let generation = self.runtime.as_ref().and_then(WorkerRuntime::generation);
        ControlSnapshot {
            state,
            generation,
            last_update: self.control_snapshot().last_update,
        }
    }

    fn control_snapshot(&self) -> ControlSnapshot {
        self.control.snapshot()
    }

    fn publish_snapshot(&mut self, state: RuntimeState, generation: Option<GenerationId>) {
        let previous = self.control_snapshot();
        let snapshot = ControlSnapshot {
            state,
            generation,
            last_update: self.control_snapshot().last_update,
        };
        self.control.update_snapshot(snapshot);
        let payload = json!({
            "kind": "runtime",
            "state": state_wire_for_event(state),
            "generation": generation.map(GenerationId::get),
            "last_update": snapshot.last_update.as_str(),
        });
        self.event_hub.replace_snapshot(json!({
            "kind": "snapshot",
            "runtime": payload,
            "observed_config_digest": self.config_observed_digest(),
            "active_config_digest": self.config_active_digest(),
            "candidate_sequence": self.config_candidate_sequence(),
            "watcher_health": self.watcher_health_wire(),
        }));
        self.event_hub.publish(EventKind::Runtime, payload);
        if previous.generation != generation {
            self.event_hub.publish(
                EventKind::Generation,
                json!({
                    "kind": "generation",
                    "previous": previous.generation.map(GenerationId::get),
                    "active": generation.map(GenerationId::get),
                }),
            );
        }
        if captures_traffic(previous.state) != captures_traffic(state) {
            self.event_hub.publish(
                EventKind::Network,
                json!({
                    "kind": "network",
                    "capturing": captures_traffic(state),
                    "state": state_wire_for_event(state),
                }),
            );
        }
    }

    #[cfg(feature = "subscription-update")]
    fn publish_update_status(&mut self, last_update: UpdateStatus) {
        let snapshot = self.snapshot();
        self.control.update_snapshot(ControlSnapshot {
            last_update,
            ..snapshot
        });
        self.event_hub.publish(
            EventKind::Subscription,
            json!({"kind":"subscription","last_update":last_update.as_str()}),
        );
    }

    fn start(&mut self, now: Duration) {
        if self.runtime.is_some() {
            return;
        }
        self.restart_at = None;
        match self
            .recovery
            .recover(&mut self.network, &self.policy, self.slot)
        {
            Ok(Some(active)) => {
                let generation = active.generation();
                let state = active.state();
                self.restart_budget.clear();
                self.runtime = Some(WorkerRuntime::new(active, now, self.limits));
                self.publish_snapshot(state, Some(generation));
                self.replay_selector();
            }
            Ok(None) => {
                self.publish_snapshot(RuntimeState::FailOpenDirect, None);
            }
            Err(_) => match self.restart_budget.register_failure(now) {
                RestartDecision::RetryAfter(after) => {
                    self.restart_at = Some(now.saturating_add(after));
                    self.publish_snapshot(RuntimeState::Backoff, None);
                }
                RestartDecision::CircuitOpen => {
                    self.publish_snapshot(RuntimeState::CircuitOpen, None);
                }
            },
        }
    }

    fn stop(&mut self) -> Result<(), WorkerServiceError> {
        self.restart_at = None;
        self.start_pending = false;
        let cleanup_failed = self
            .runtime
            .as_mut()
            .is_some_and(|runtime| runtime.stop(&mut self.network, &mut self.verifier).is_err());
        self.runtime = None;
        self.publish_snapshot(RuntimeState::FailOpenDirect, None);
        if cleanup_failed {
            Err(WorkerServiceError::ShutdownFailed)
        } else {
            Ok(())
        }
    }

    fn check_core_version(
        &mut self,
    ) -> Option<Result<(CoreVersionStatus, &'static str), CoreVersionCheckError>> {
        let status = match self.core_version_source.as_mut()?.check() {
            Ok(status) => status,
            Err(error) => return Some(Err(error)),
        };
        let mut notification = "not_needed";
        if status.availability() == CoreUpdateAvailability::Available
            && self.last_notified_core_version != Some(status.latest())
        {
            notification = match self
                .core_update_notifier
                .as_mut()
                .map(|notifier| notifier.notify_core_update())
            {
                Some(UpdateNotificationOutcome::Posted) => {
                    self.last_notified_core_version = Some(status.latest());
                    "posted"
                }
                Some(UpdateNotificationOutcome::Unavailable) | None => "unavailable",
            };
        } else if status.availability() == CoreUpdateAvailability::Available {
            notification = "already_notified";
        }
        if let Some(state) = self.core_version_state.as_mut()
            && state.persist(&status, notification).is_err()
        {
            self.event_hub.publish(
                EventKind::Runtime,
                json!({"kind":"core_update","state":"state_persist_failed"}),
            );
        }
        self.core_version_status = Some(status.clone());
        self.event_hub.publish(
            EventKind::Runtime,
            json!({
                "kind": "core_update",
                "availability": status.availability(),
                "latest": status.latest().to_string(),
                "notification": notification,
            }),
        );
        Some(Ok((status, notification)))
    }

    #[cfg(feature = "subscription-update")]
    fn run_scheduled_core_version_check(&mut self, now: Duration) {
        if self
            .core_version_schedule_retry_at
            .is_some_and(|deadline| now < deadline)
        {
            return;
        }
        self.core_version_schedule_retry_at = None;
        match self.core_version_schedule.take_due() {
            Ok(false) => return,
            Ok(true) => {}
            Err(_) => {
                self.event_hub.publish(
                    EventKind::Runtime,
                    json!({"kind":"core_update","state":"schedule_read_failed"}),
                );
                self.core_version_schedule_retry_at =
                    Some(now.saturating_add(Duration::from_secs(60 * 60)));
                return;
            }
        }
        let succeeded = matches!(self.check_core_version(), Some(Ok(_)));
        if !succeeded {
            self.event_hub.publish(
                EventKind::Runtime,
                json!({"kind":"core_update","state":"check_failed"}),
            );
        }
        if self.core_version_schedule.record_result(succeeded).is_err() {
            self.event_hub.publish(
                EventKind::Runtime,
                json!({"kind":"core_update","state":"schedule_persist_failed"}),
            );
            self.core_version_schedule_retry_at =
                Some(now.saturating_add(Duration::from_secs(60 * 60)));
        }
    }

    #[cfg(feature = "subscription-update")]
    fn run_scheduled_rule_set_update(&mut self, now: Duration) {
        if !self.rule_set_updater.is_available()
            || self
                .rule_set_schedule_retry_at
                .is_some_and(|deadline| now < deadline)
        {
            return;
        }
        self.rule_set_schedule_retry_at = None;
        match self.rule_set_schedule.take_due() {
            Ok(false) => return,
            Ok(true) => {}
            Err(_) => {
                self.publish_rule_set_event("schedule_read_failed");
                self.rule_set_schedule_retry_at =
                    Some(now.saturating_add(Duration::from_secs(60 * 60)));
                return;
            }
        }
        let succeeded = self.update_rule_sets(now);
        if self.rule_set_schedule.record_result(succeeded).is_err() {
            self.publish_rule_set_event("schedule_persist_failed");
            self.rule_set_schedule_retry_at =
                Some(now.saturating_add(Duration::from_secs(60 * 60)));
        }
    }

    #[cfg(feature = "subscription-update")]
    fn update_rule_sets(&mut self, now: Duration) -> bool {
        self.rule_set_last_attempt_wall = rule_set_wall_seconds();
        match self.rule_set_updater.prepare_update() {
            Ok(RuleSetUpdatePreparation::Unchanged) => {
                self.publish_rule_set_event("unchanged");
                return true;
            }
            Ok(RuleSetUpdatePreparation::Prepared) => {}
            Err(_) => {
                self.publish_rule_set_event("prepare_failed");
                return false;
            }
        }

        let restart_required = self.runtime.is_some();
        if restart_required && self.stop().is_err() {
            let _ = self.rule_set_updater.rollback_update();
            self.publish_rule_set_event("stop_failed");
            return false;
        }
        if self.rule_set_updater.publish_update().is_err() {
            self.publish_rule_set_event("publish_failed");
            if restart_required {
                self.start(now);
            }
            return false;
        }
        if !restart_required {
            return if self.rule_set_updater.commit_update().is_ok() {
                self.publish_rule_set_event("updated_inactive");
                true
            } else {
                let _ = self.rule_set_updater.rollback_update();
                self.publish_rule_set_event("commit_failed");
                false
            };
        }

        let active = match self
            .recovery
            .recover(&mut self.network, &self.policy, self.slot)
        {
            Ok(Some(active)) => active,
            Ok(None) | Err(_) => {
                self.restore_previous_rule_sets(now, "activation_failed");
                return false;
            }
        };
        let generation = active.generation();
        if self.rule_set_updater.commit_update().is_err() {
            let cleanup_failed = active.stop(&mut self.network, &mut self.verifier).is_err();
            if cleanup_failed {
                self.publish_snapshot(RuntimeState::CircuitOpen, None);
                self.publish_rule_set_event("commit_cleanup_failed");
                return false;
            }
            self.restore_previous_rule_sets(now, "commit_failed");
            return false;
        }

        self.restart_budget.clear();
        let state = active.state();
        self.runtime = Some(WorkerRuntime::new(active, now, self.limits));
        self.publish_snapshot(state, Some(generation));
        self.replay_selector();
        self.publish_rule_set_event("updated");
        true
    }

    #[cfg(feature = "subscription-update")]
    fn restore_previous_rule_sets(&mut self, now: Duration, failure: &'static str) {
        if self.rule_set_updater.rollback_update().is_err() {
            self.publish_snapshot(RuntimeState::CircuitOpen, None);
            self.publish_rule_set_event("rollback_failed");
            return;
        }
        self.publish_rule_set_event(failure);
        self.start(now);
    }

    #[cfg(feature = "subscription-update")]
    fn publish_rule_set_event(&mut self, state: &'static str) {
        match state {
            "unchanged" | "updated" | "updated_inactive" => {
                self.rule_set_state = state;
                self.rule_set_diagnostic = None;
                self.rule_set_last_success_wall = rule_set_wall_seconds();
            }
            "schedule_read_failed" | "schedule_persist_failed" => {
                self.rule_set_diagnostic = Some(state);
            }
            _ => {
                self.rule_set_state = "failed";
                self.rule_set_diagnostic = Some(state);
            }
        }
        self.event_hub.publish(
            EventKind::Runtime,
            json!({"kind":"ruleset_update","state":state}),
        );
    }

    #[cfg(feature = "subscription-update")]
    fn rule_set_status_document(&self) -> serde_json::Value {
        let snapshot = self.rule_set_updater.snapshot().ok();
        json!({
            "available": self.rule_set_updater.is_available(),
            "state": self.rule_set_state,
            "last_attempt_wall_seconds": self.rule_set_last_attempt_wall,
            "last_success_wall_seconds": self.rule_set_last_success_wall,
            "next_update_in_seconds": self.rule_set_schedule.next_wakeup_in().map(|duration| duration.as_secs()),
            "domain_sha256": snapshot.as_ref().map(|value| value.domain_sha256()),
            "ip_sha256": snapshot.as_ref().map(|value| value.ip_sha256()),
            "diagnostic_code": self.rule_set_diagnostic,
        })
    }

    #[cfg(feature = "subscription-update")]
    fn update(&mut self, now: Duration) -> Result<(), WorkerServiceError> {
        let result = self.update_inner(now);
        let succeeded = result.is_ok() && self.snapshot().last_update == UpdateStatus::Succeeded;
        if let Some(report) = self.updater.take_source_update_report()
            && let Some(store) = self.source_status.as_mut()
            && let Ok(wall_seconds) = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .ok()
                .and_then(|duration| i64::try_from(duration.as_secs()).ok())
                .ok_or(())
        {
            let _ = store.record_report(wall_seconds, &report);
        }
        if self.update_schedule.record_result(succeeded).is_err() {
            return Err(WorkerServiceError::TaskFailed);
        }
        result
    }

    #[cfg(not(feature = "subscription-update"))]
    fn update(&mut self, _now: Duration) -> Result<(), WorkerServiceError> {
        Ok(())
    }

    #[cfg(feature = "subscription-update")]
    fn update_inner(&mut self, now: Duration) -> Result<(), WorkerServiceError> {
        let prepared = match self.updater.prepare() {
            Ok(prepared) => prepared,
            Err(_) => {
                self.publish_update_status(UpdateStatus::Failed);
                return Ok(());
            }
        };
        let service_enabled = self
            .config
            .as_ref()
            .is_none_or(|config| config.current().effective().service_enabled());
        if !service_enabled || !self.updater.is_current(&prepared) {
            let _ = self.updater.discard(prepared);
            self.event_hub.publish(
                EventKind::Config,
                json!({
                    "kind":"config",
                    "state":"superseded",
                    "candidate_sequence":self.config_candidate_sequence(),
                    "observed_config_digest":self.config_observed_digest(),
                    "active_config_digest":self.config_active_digest(),
                }),
            );
            self.publish_snapshot(RuntimeState::FailOpenDirect, None);
            return Ok(());
        }
        #[cfg(feature = "subscription-update")]
        if self
            .config
            .as_ref()
            .is_some_and(|config| !config.disk_matches_current())
        {
            let _ = self.updater.discard(prepared);
            self.publish_update_status(UpdateStatus::Failed);
            return Ok(());
        }
        if self.dry_run {
            let discarded = self.updater.discard(prepared).is_ok();
            self.publish_update_status(if discarded {
                UpdateStatus::Succeeded
            } else {
                UpdateStatus::Failed
            });
            return if discarded {
                Ok(())
            } else {
                Err(WorkerServiceError::TaskFailed)
            };
        }
        let generation = self.updater.generation(&prepared);
        if self.stop().is_err() {
            let _ = self.updater.discard(prepared);
            self.publish_update_status(UpdateStatus::Failed);
            return Err(WorkerServiceError::ShutdownFailed);
        }
        let active = match self.recovery.recover_generation(
            &mut self.network,
            &self.policy,
            self.slot,
            generation,
        ) {
            Ok(Some(active)) => active,
            Ok(None) | Err(_) => {
                let _ = self.updater.discard(prepared);
                self.publish_update_status(UpdateStatus::Failed);
                self.start(now);
                return Ok(());
            }
        };
        #[cfg(feature = "subscription-update")]
        if self
            .config
            .as_ref()
            .is_some_and(|config| !config.disk_matches_current())
        {
            let cleanup_failed = active.stop(&mut self.network, &mut self.verifier).is_err();
            let _ = self.updater.discard(prepared);
            self.publish_update_status(UpdateStatus::Failed);
            if cleanup_failed {
                return Err(WorkerServiceError::ShutdownFailed);
            }
            return Ok(());
        }
        if self.updater.commit(prepared).is_err() {
            let cleanup_failed = active.stop(&mut self.network, &mut self.verifier).is_err();
            self.publish_update_status(UpdateStatus::Failed);
            if cleanup_failed {
                return Err(WorkerServiceError::ShutdownFailed);
            }
            self.start(now);
            return Ok(());
        }
        #[cfg(feature = "subscription-update")]
        if self
            .config
            .as_ref()
            .is_some_and(|config| !config.disk_matches_current())
        {
            let cleanup_failed = active.stop(&mut self.network, &mut self.verifier).is_err();
            self.publish_update_status(UpdateStatus::Failed);
            if cleanup_failed {
                return Err(WorkerServiceError::ShutdownFailed);
            }
            return Ok(());
        }
        self.restart_budget.clear();
        let state = active.state();
        self.runtime = Some(WorkerRuntime::new(active, now, self.limits));
        self.publish_update_status(UpdateStatus::Succeeded);
        self.publish_snapshot(state, Some(generation));
        self.replay_selector();
        Ok(())
    }

    fn replay_selector(&mut self) {
        let Some(control) = self.operational_control.as_mut() else {
            return;
        };
        if let Ok(result) = control.replay_selection() {
            self.event_hub.publish(
                EventKind::Runtime,
                json!({
                    "kind": "selector",
                    "replay": format!("{result:?}").to_lowercase(),
                }),
            );
        }
    }

    fn handle_start(&mut self, now: Duration) {
        if self.start_pending {
            self.start_pending = false;
            self.start(now);
        }
    }

    fn handle_commands(&mut self, now: Duration) -> Result<(), WorkerServiceError> {
        while let Some(command) = self.control.take_command() {
            match command {
                ControlCommand::Start => {
                    self.restart_budget.clear();
                    self.start_pending = true;
                }
                ControlCommand::Stop => self.stop()?,
                ControlCommand::Probe => {
                    let _ = self.recovery.probe();
                }
                ControlCommand::Update => {
                    #[cfg(feature = "subscription-update")]
                    self.updater
                        .request_source_update(None)
                        .map_err(|_| WorkerServiceError::TaskFailed)?;
                    self.update(now)?;
                }
                ControlCommand::UpdateSource(source_id) => {
                    #[cfg(feature = "subscription-update")]
                    {
                        self.updater
                            .request_source_update(Some(&source_id))
                            .map_err(|_| WorkerServiceError::TaskFailed)?;
                        self.update(now)?;
                    }
                    #[cfg(not(feature = "subscription-update"))]
                    {
                        let _ = source_id;
                        return Err(WorkerServiceError::TaskFailed);
                    }
                }
                ControlCommand::RuleSetUpdate => {
                    #[cfg(feature = "subscription-update")]
                    {
                        let succeeded = self.update_rule_sets(now);
                        if self.rule_set_schedule.record_result(succeeded).is_err() {
                            self.publish_rule_set_event("schedule_persist_failed");
                        }
                    }
                    #[cfg(not(feature = "subscription-update"))]
                    return Err(WorkerServiceError::TaskFailed);
                }
            }
        }
        self.handle_start(now);
        Ok(())
    }

    fn tick_runtime(&mut self, now: Duration) -> Result<(), WorkerServiceError> {
        let Some(runtime) = self.runtime.as_mut() else {
            if self.restart_at.is_some_and(|deadline| now >= deadline) {
                self.start(now);
            }
            return Ok(());
        };
        let tick = runtime
            .tick(
                now,
                &mut self.network,
                &mut self.verifier,
                &mut self.restart_budget,
            )
            .map_err(|_| WorkerServiceError::TaskFailed)?;
        match tick {
            RuntimeTick::RestartScheduled { after, .. } => {
                self.runtime = None;
                self.restart_at = Some(now.saturating_add(after));
                self.publish_snapshot(RuntimeState::Backoff, None);
            }
            RuntimeTick::CircuitOpen { .. } => {
                self.runtime = None;
                self.restart_at = None;
                self.publish_snapshot(RuntimeState::CircuitOpen, None);
            }
            _ => {
                let snapshot = self.snapshot();
                self.control.update_snapshot(snapshot);
            }
        }
        Ok(())
    }

    fn sample_traffic_if_due(&mut self, now: Duration) {
        if self.event_hub.traffic_subscribers() == 0 || now < self.next_traffic_sample {
            return;
        }
        self.next_traffic_sample = now.saturating_add(Duration::from_secs(1));
        let snapshot = self.snapshot();
        let Some(control) = self.operational_control.as_mut() else {
            return;
        };
        let Ok(mut result) = control.handle(
            ControlMethod::TrafficGet,
            &ControlParams::default(),
            snapshot.state,
            snapshot.generation,
            &self.policy,
        ) else {
            return;
        };
        if let Some(object) = result.as_object_mut() {
            object.insert("kind".into(), json!("traffic"));
        }
        self.event_hub.publish(EventKind::Traffic, result);
    }

    fn handle_webui_payload(&mut self, request: ControlRequest) -> ControlResponse {
        let request_id = request.request_id().clone();
        let generation = self.snapshot().generation.map(GenerationId::get);
        let Some(store) = self.webui_payload_store.clone() else {
            return webui_payload_failure(request_id, generation, WebUiErrorKind::Unavailable);
        };
        let namespace = request
            .params()
            .payload_namespace()
            .expect("protocol validates payload namespace");
        match request.method() {
            ControlMethod::WebUiPayloadCreate => match store.create(namespace) {
                Ok(handle) => ControlResponse::success(
                    request_id,
                    generation,
                    json!({"handle":handle,"namespace":namespace}),
                ),
                Err(_) => {
                    webui_payload_failure(request_id, generation, WebUiErrorKind::Unavailable)
                }
            },
            ControlMethod::WebUiPayloadAppend => {
                let handle = request.params().payload_handle().expect("validated handle");
                let chunk = request.params().payload_chunk().expect("validated chunk");
                match store.append(namespace, handle, chunk) {
                    Ok(bytes) => ControlResponse::success(
                        request_id,
                        generation,
                        json!({"accepted":true,"bytes":bytes}),
                    ),
                    Err(crate::WebUiPayloadError::LimitExceeded) => {
                        webui_payload_failure(request_id, generation, WebUiErrorKind::LimitExceeded)
                    }
                    Err(_) => webui_payload_failure(
                        request_id,
                        generation,
                        WebUiErrorKind::InvalidPayload,
                    ),
                }
            }
            ControlMethod::WebUiPayloadRemove => {
                let handle = request.params().payload_handle().expect("validated handle");
                match store.remove(namespace, handle) {
                    Ok(()) => {
                        ControlResponse::success(request_id, generation, json!({"removed":true}))
                    }
                    Err(_) => webui_payload_failure(
                        request_id,
                        generation,
                        WebUiErrorKind::InvalidPayload,
                    ),
                }
            }
            ControlMethod::WebUiPayloadCommit => {
                let handle = request.params().payload_handle().expect("validated handle");
                let operation = request
                    .params()
                    .payload_operation()
                    .expect("validated operation");
                let bytes = match store.consume(namespace, handle) {
                    Ok(bytes) => bytes,
                    Err(_) => {
                        return webui_payload_failure(
                            request_id,
                            generation,
                            WebUiErrorKind::InvalidPayload,
                        );
                    }
                };
                let params = match serde_json::from_slice::<ControlParams>(&bytes) {
                    Ok(params) => params,
                    Err(_) => {
                        return webui_payload_failure(
                            request_id,
                            generation,
                            WebUiErrorKind::InvalidPayload,
                        );
                    }
                };
                let method = match operation {
                    WebUiPayloadOperation::ConfigValidate => ControlMethod::ConfigValidate,
                    WebUiPayloadOperation::ConfigApply => ControlMethod::ConfigApply,
                    WebUiPayloadOperation::ConfigMutate => ControlMethod::ConfigMutate,
                    WebUiPayloadOperation::SubscriptionImportPreview => {
                        ControlMethod::SubscriptionImportPreview
                    }
                    WebUiPayloadOperation::SubscriptionImportApply => {
                        ControlMethod::SubscriptionImportApply
                    }
                    WebUiPayloadOperation::BackupRestore => ControlMethod::ConfigApply,
                };
                let inner =
                    match ControlRequest::new(request_id.clone(), method).with_params(params) {
                        Ok(request) => request,
                        Err(_) => {
                            return webui_payload_failure(
                                request_id,
                                generation,
                                WebUiErrorKind::InvalidPayload,
                            );
                        }
                    };
                self.handle(inner)
            }
            _ => unreachable!("payload handler is called only for payload methods"),
        }
    }
}

fn webui_payload_failure(
    request_id: nethop_protocol::RequestId,
    generation: Option<u64>,
    kind: WebUiErrorKind,
) -> ControlResponse {
    ControlResponse::failure(
        request_id,
        generation,
        ControlError::new(kind.error_code(), "webui operation failed")
            .expect("bounded WebUI error message is valid"),
    )
}

impl<S, N, V, C, U> ControlRequestHandler for WorkerApplication<S, N, V, C, U>
where
    N: NetworkController,
    S: RuntimeRecoverySource<N>,
    S::Process: CandidateProcess,
    V: RuntimeHealthVerifier,
    C: WorkerClock,
    U: RuntimeUpdateSource,
{
    fn handle(&mut self, request: ControlRequest) -> ControlResponse {
        if matches!(
            request.method(),
            ControlMethod::WebUiPayloadCreate
                | ControlMethod::WebUiPayloadAppend
                | ControlMethod::WebUiPayloadCommit
                | ControlMethod::WebUiPayloadRemove
        ) {
            return self.handle_webui_payload(request);
        }
        if request.method() == ControlMethod::RuleSetStatus {
            let request_id = request.request_id().clone();
            let generation = self.snapshot().generation.map(GenerationId::get);
            #[cfg(feature = "subscription-update")]
            return ControlResponse::success(
                request_id,
                generation,
                self.rule_set_status_document(),
            );
            #[cfg(not(feature = "subscription-update"))]
            return ControlResponse::failure(
                request_id,
                generation,
                crate::worker_services::unavailable_control_error(
                    ErrorDomain::Core,
                    "RULESET-UNAVAILABLE",
                ),
            );
        }
        if request.method() == ControlMethod::RuleSetUpdate {
            let request_id = request.request_id().clone();
            let generation = self.snapshot().generation.map(GenerationId::get);
            #[cfg(feature = "subscription-update")]
            if self.rule_set_updater.is_available() {
                self.control.queue_command(ControlCommand::RuleSetUpdate);
                return ControlResponse::success(request_id, generation, json!({"accepted":true}));
            }
            return ControlResponse::failure(
                request_id,
                generation,
                crate::worker_services::unavailable_control_error(
                    ErrorDomain::Core,
                    "RULESET-UNAVAILABLE",
                ),
            );
        }
        if request.method() == ControlMethod::StatusGet {
            let request_id = request.request_id().clone();
            let snapshot = self.snapshot();
            let generation = snapshot.generation.map(GenerationId::get);
            let operational = self.operational_control.as_mut().map_or_else(
                || {
                    json!({
                        "core_api": "unavailable",
                        "selector": {"selected": null, "candidate_count": 0},
                        "active_connection_count": 0,
                    })
                },
                OperationalControl::status_document,
            );
            let capture_active = captures_traffic(snapshot.state);
            let process_health = match snapshot.state {
                RuntimeState::RunningTproxy | RuntimeState::RunningTun => "healthy",
                RuntimeState::Degraded => "degraded",
                RuntimeState::StartingCore | RuntimeState::StartingTun | RuntimeState::Probing => {
                    "starting"
                }
                _ => "stopped",
            };
            let dns_guard = match snapshot.state {
                RuntimeState::RunningTproxy => "verified",
                RuntimeState::Degraded
                    if self.policy.mode() == nethop_core::CaptureMode::Tproxy =>
                {
                    "degraded"
                }
                _ => "inactive",
            };
            let mut capture = crate::operational_control::capture_document(&self.policy);
            if let Some(object) = capture.as_object_mut() {
                object.insert("active".into(), json!(capture_active));
                object.insert("dns_guard".into(), json!(dns_guard));
            }
            let core_update = self.core_version_status.as_ref().map_or_else(
                || {
                    json!({
                        "state": "never_checked",
                        "current": nethop_subscription::PINNED_SING_BOX_VERSION,
                    })
                },
                |status| json!(status),
            );
            let dns_split = self.private_dns_source.as_mut().map_or_else(
                || json!({"mode":"unknown","dns_split":"unknown"}),
                |source| {
                    source.current().map_or_else(
                        |_| json!({"mode":"unknown","dns_split":"unknown"}),
                        |status| json!(status),
                    )
                },
            );
            #[cfg(feature = "subscription-update")]
            let rule_set = self.rule_set_status_document();
            #[cfg(not(feature = "subscription-update"))]
            let rule_set = json!({"available":false,"state":"unavailable"});
            return ControlResponse::success(
                request_id,
                generation,
                json!({
                    "schema_version": 1,
                    "state": state_wire_for_event(snapshot.state),
                    "generation": generation,
                    "last_update": snapshot.last_update.as_str(),
                    "watcher_health": self.watcher_health_wire(),
                    "runtime": {
                        "state": state_wire_for_event(snapshot.state),
                        "process_health": process_health,
                    },
                    "subscription": {"last_update": snapshot.last_update.as_str()},
                    "core_update": core_update,
                    "rule_set": rule_set,
                    "dns_split": dns_split,
                    "capture": capture,
                    "operational": operational,
                }),
            );
        }
        if request.method() == ControlMethod::CoreVersionCheck {
            let request_id = request.request_id().clone();
            let generation = self.snapshot().generation.map(GenerationId::get);
            let Some(result) = self.check_core_version() else {
                return ControlResponse::failure(
                    request_id,
                    generation,
                    crate::worker_services::unavailable_control_error(
                        ErrorDomain::Core,
                        "VERSION-CHECK-UNAVAILABLE",
                    ),
                );
            };
            #[cfg(feature = "subscription-update")]
            if self
                .core_version_schedule
                .record_result(result.is_ok())
                .is_err()
            {
                self.event_hub.publish(
                    EventKind::Runtime,
                    json!({"kind":"core_update","state":"schedule_persist_failed"}),
                );
            }
            return match result {
                Ok((status, notification)) => ControlResponse::success(
                    request_id,
                    generation,
                    json!({"status":status,"notification":notification}),
                ),
                Err(_) => ControlResponse::failure(
                    request_id,
                    generation,
                    crate::worker_services::unavailable_control_error(
                        ErrorDomain::Core,
                        "VERSION-CHECK-FAILED",
                    ),
                ),
            };
        }
        if matches!(
            request.method(),
            ControlMethod::LogsGet | ControlMethod::LogsClear
        ) {
            let request_id = request.request_id().clone();
            let generation = self.snapshot().generation.map(GenerationId::get);
            let result = match request.method() {
                ControlMethod::LogsGet => self
                    .event_hub
                    .structured_log_history(
                        request.params().log_channel(),
                        request.params().limit().unwrap_or(64),
                    )
                    .map(|entries| {
                        json!({
                            "entries":entries,
                            "channel":request.params().log_channel(),
                            "newest_first":true
                        })
                    }),
                ControlMethod::LogsClear => self
                    .event_hub
                    .clear_structured_logs()
                    .map(|removed| json!({"cleared":true,"removed_files":removed})),
                _ => unreachable!(),
            };
            return match result {
                Ok(result) => ControlResponse::success(request_id, generation, result),
                Err(_) => ControlResponse::failure(
                    request_id,
                    generation,
                    crate::worker_services::unavailable_control_error(
                        ErrorDomain::Core,
                        "LOG-CONTROL-FAILED",
                    ),
                ),
            };
        }
        if request.method() == ControlMethod::MetricsGet {
            let request_id = request.request_id().clone();
            let snapshot = self.snapshot();
            let generation = snapshot.generation.map(GenerationId::get);
            let process = self
                .runtime
                .as_ref()
                .and_then(WorkerRuntime::process_identity);
            let Some(control) = self.operational_control.as_ref() else {
                return ControlResponse::failure(
                    request_id,
                    generation,
                    crate::worker_services::unavailable_control_error(
                        ErrorDomain::Core,
                        "METRICS-UNAVAILABLE",
                    ),
                );
            };
            return ControlResponse::success(
                request_id,
                generation,
                control.metrics_document(
                    process,
                    self.clock.now(),
                    snapshot.state,
                    snapshot.generation,
                ),
            );
        }
        if matches!(
            request.method(),
            ControlMethod::NodeList
                | ControlMethod::NodeTest
                | ControlMethod::NodeTestAll
                | ControlMethod::NodeSelect
                | ControlMethod::NodeExport
                | ControlMethod::ConnectionsGet
                | ControlMethod::ConnectionClose
                | ControlMethod::ConnectionsCloseAll
                | ControlMethod::DiagnosticsBundle
                | ControlMethod::TopologyGet
                | ControlMethod::TrafficGet
        ) {
            let request_id = request.request_id().clone();
            let snapshot = self.snapshot();
            let generation = snapshot.generation.map(GenerationId::get);
            let Some(control) = self.operational_control.as_mut() else {
                return ControlResponse::failure(
                    request_id,
                    generation,
                    crate::worker_services::unavailable_control_error(
                        ErrorDomain::Core,
                        "CONTROL-UNAVAILABLE",
                    ),
                );
            };
            return match control.handle(
                request.method(),
                request.params(),
                snapshot.state,
                snapshot.generation,
                &self.policy,
            ) {
                Ok(result) => ControlResponse::success(request_id, generation, result),
                Err(error) => ControlResponse::failure(
                    request_id,
                    generation,
                    crate::worker_services::unavailable_control_error(
                        ErrorDomain::Core,
                        if matches!(error, crate::OperationalControlError::ClashApi(_)) {
                            "CONTROL-FAILED"
                        } else {
                            "CONTROL-INVALID"
                        },
                    ),
                ),
            };
        }
        #[cfg(feature = "subscription-update")]
        if matches!(
            request.method(),
            ControlMethod::SubscriptionImportPreview | ControlMethod::SubscriptionImportApply
        ) {
            let request_id = request.request_id().clone();
            let generation = self.snapshot().generation.map(GenerationId::get);
            let Some(config) = self.config.as_ref() else {
                return ControlResponse::failure(
                    request_id,
                    generation,
                    unavailable_control_error(ErrorDomain::Config, "IMPORT-UNAVAILABLE"),
                );
            };
            let observed = config
                .observed_digest()
                .unwrap_or_else(|_| config.current().digest().to_owned());
            if request.params().expected_config_digest() != Some(observed.as_str()) {
                return ControlResponse::failure(
                    request_id,
                    generation,
                    unavailable_control_error_with_details(
                        ErrorDomain::Config,
                        "CONFLICT",
                        json!({"observed_config_digest":observed}),
                    ),
                );
            }
            let Some((bytes, format_hint)) = import_document(request.params().document()) else {
                return ControlResponse::failure(
                    request_id,
                    generation,
                    unavailable_control_error(ErrorDomain::Subscription, "IMPORT-INVALID"),
                );
            };
            if request.method() == ControlMethod::SubscriptionImportPreview {
                return match self.updater.preview_import(&bytes, format_hint) {
                    Ok(mut preview) => {
                        if let Some(object) = preview.as_object_mut() {
                            object.insert("persistence".into(), json!("persistent_manual_source"));
                        }
                        ControlResponse::success(request_id, generation, preview)
                    }
                    Err(_) => ControlResponse::failure(
                        request_id,
                        generation,
                        unavailable_control_error(ErrorDomain::Subscription, "IMPORT-REJECTED"),
                    ),
                };
            }
            let candidate_digest = request
                .params()
                .candidate_digest()
                .expect("protocol validated import candidate digest")
                .to_owned();
            if self
                .updater
                .request_import(bytes, format_hint, candidate_digest)
                .is_err()
            {
                return ControlResponse::failure(
                    request_id,
                    generation,
                    unavailable_control_error(ErrorDomain::Subscription, "IMPORT-BUSY"),
                );
            }
            self.control.queue_command(ControlCommand::Update);
            let completed = self.handle_commands(self.clock.now()).is_ok()
                && self.snapshot().last_update == UpdateStatus::Succeeded;
            let generation = self.snapshot().generation.map(GenerationId::get);
            return if completed {
                ControlResponse::success(
                    request_id,
                    generation,
                    json!({
                        "accepted":true,
                        "completed":true,
                        "persistence":"persistent_manual_source"
                    }),
                )
            } else {
                ControlResponse::failure(
                    request_id,
                    generation,
                    unavailable_control_error(ErrorDomain::Subscription, "IMPORT-FAILED"),
                )
            };
        }
        #[cfg(feature = "subscription-update")]
        if let Some(config) = self.config.as_ref() {
            let request_id = request.request_id().clone();
            let generation = self.snapshot().generation.map(GenerationId::get);
            match request.method() {
                ControlMethod::ProtocolHello => {
                    let compatible = request.params().manager_protocol_range()
                        == Some((PROTOCOL_VERSION, PROTOCOL_VERSION));
                    return ControlResponse::success(
                        request_id,
                        generation,
                        json!({
                            "manager_version": request.params().manager_version(),
                            "compatible": compatible,
                            "daemon_protocol_min": PROTOCOL_VERSION,
                            "daemon_protocol_max": PROTOCOL_VERSION,
                            "daemon_schema_min": crate::worker_config::CONFIG_SCHEMA_VERSION,
                            "daemon_schema_max": crate::worker_config::CONFIG_SCHEMA_VERSION,
                            "active_schema_version": crate::worker_config::CONFIG_SCHEMA_VERSION,
                            "supported_operations": [
                                "config.get", "config.export", "config.validate", "config.apply", "config.reload",
                                "core.version_check",
                                "config.schema", "capability.get", "config.mutate", "events.subscribe",
                                "subscription.import_preview", "subscription.import_apply",
                                "node.list", "node.test", "node.test_all", "node.select", "node.export", "connections.get",
                                "connection.close", "connections.close_all", "logs.get", "logs.clear",
                                "diagnostics.bundle", "topology.get", "traffic.get", "metrics.get",
                                "webui.payload.create", "webui.payload.append",
                                "webui.payload.commit", "webui.payload.remove"
                            ],
                            "supported_features": [
                                "multi_source", "config_cas", "change_preview", "typed_mutation",
                                "event_stream", "app_scope", "interface_scope",
                                "persistent_update_schedule", "log_retention", "selector_replay",
                                "connection_control", "structured_log_control", "log_channels", "runtime_metrics", "diagnostics_bundle"
                                , "persistent_manual_source", "config_backup_v1"
                                , "core_update_check", "traffic_event", "private_payload"
                            ]
                        }),
                    );
                }
                ControlMethod::ConfigGet => {
                    let observed = config
                        .observed_digest()
                        .unwrap_or_else(|_| config.current().digest().to_owned());
                    let source_status = self
                        .source_status
                        .as_ref()
                        .and_then(|store| {
                            store
                                .statuses(
                                    config
                                        .source_config()
                                        .sources()
                                        .iter()
                                        .map(|source| source.id().as_str()),
                                )
                                .ok()
                        })
                        .unwrap_or_default();
                    return ControlResponse::success(
                        request_id,
                        generation,
                        json!({
                            "observed_config_digest": observed,
                            "active_config_digest": config.current().digest(),
                            "candidate_sequence": config.candidate_sequence(),
                            "watcher_health": self.watcher_health_wire(),
                            "last_reload": config.last_reload().as_str(),
                            "document": config.redacted_document(),
                            "source_status": source_status,
                        }),
                    );
                }
                ControlMethod::ConfigExport => {
                    return ControlResponse::success(
                        request_id,
                        generation,
                        json!({
                            "format": "nethop-config-backup-v1",
                            "config_digest": config.current().digest(),
                            "document": config.current().document(),
                        }),
                    );
                }
                ControlMethod::ConfigValidate => {
                    let result = config.validate_document(
                        request
                            .params()
                            .expected_config_digest()
                            .unwrap_or_default(),
                        request
                            .params()
                            .document()
                            .expect("protocol validated document"),
                    );
                    return match result {
                        Ok(preview) => ControlResponse::success(
                            request_id,
                            generation,
                            json!({
                                "observed_config_digest": preview.observed_digest(),
                                "active_config_digest": config.current().digest(),
                                "candidate_config_digest": preview.candidate_digest(),
                                "changed_field_ids": preview.changed_field_ids(),
                                "change_set": preview.plan().changes(),
                                "apply_impact": preview.plan().impact(),
                                "estimated_disruption": disruption_wire(preview.plan().impact()),
                                "warnings": [],
                            }),
                        ),
                        Err(error) => ControlResponse::failure(
                            request_id,
                            generation,
                            config_control_error_with_document(
                                Some(config),
                                &error,
                                request.params().document(),
                            ),
                        ),
                    };
                }
                ControlMethod::ConfigSchema => {
                    return ControlResponse::success(
                        request_id,
                        generation,
                        config_schema_document(),
                    );
                }
                ControlMethod::CapabilityGet => {
                    return match self.recovery.probe() {
                        Ok(report) => {
                            self.capability_probe_sequence =
                                self.capability_probe_sequence.saturating_add(1);
                            ControlResponse::success(
                                request_id,
                                generation,
                                capability_document(
                                    &report,
                                    self.capability_probe_sequence,
                                    self.clock.now(),
                                ),
                            )
                        }
                        Err(error) => ControlResponse::failure(
                            request_id,
                            generation,
                            unavailable_control_error(
                                ErrorDomain::Capability,
                                error.code().as_str(),
                            ),
                        ),
                    };
                }
                _ => {}
            }
        }
        #[cfg(feature = "subscription-update")]
        if request.method() == ControlMethod::SubscriptionUpdate && self.updater.is_available() {
            let request_id = request.request_id().clone();
            if request.params().if_needed() && !self.updater.is_needed() {
                return ControlResponse::success(
                    request_id,
                    self.snapshot().generation.map(GenerationId::get),
                    json!({"accepted": true, "needed": false, "completed": true}),
                );
            }
            if request.params().wait() {
                let requested = self
                    .updater
                    .request_source_update(request.params().source_id())
                    .is_ok();
                let completed = requested
                    && self.update(self.clock.now()).is_ok()
                    && self.snapshot().last_update == UpdateStatus::Succeeded;
                let generation = self.snapshot().generation.map(GenerationId::get);
                return if completed {
                    ControlResponse::success(
                        request_id,
                        generation,
                        json!({"accepted": true, "needed": true, "completed": true}),
                    )
                } else {
                    ControlResponse::failure(
                        request_id,
                        generation,
                        unavailable_control_error(ErrorDomain::Subscription, "UPDATE-FAILED"),
                    )
                };
            }
        }
        #[cfg(feature = "subscription-update")]
        if self.config.is_some()
            && matches!(
                request.method(),
                ControlMethod::ServiceStart
                    | ControlMethod::ServiceStop
                    | ControlMethod::ConfigReload
            )
        {
            let request_id = request.request_id().clone();
            let checkpoint = match self
                .config
                .as_ref()
                .expect("configuration was checked")
                .checkpoint()
            {
                Ok(checkpoint) => checkpoint,
                Err(error) => {
                    return ControlResponse::failure(
                        request_id,
                        self.snapshot().generation.map(GenerationId::get),
                        config_control_error(self.config.as_ref(), &error),
                    );
                }
            };
            let result = {
                let config = self.config.as_mut().expect("configuration was checked");
                match request.method() {
                    ControlMethod::ServiceStart => config.set_service_enabled(true),
                    ControlMethod::ServiceStop => config.set_service_enabled(false),
                    ControlMethod::ConfigReload => config.reload(),
                    _ => unreachable!(),
                }
            };
            return match result {
                Ok(change) => {
                    let changed = matches!(change, ConfigChange::Changed { .. });
                    if changed {
                        self.apply_config_change(change);
                    } else {
                        match request.method() {
                            ControlMethod::ServiceStart => {
                                self.control.queue_command(ControlCommand::Start)
                            }
                            ControlMethod::ServiceStop => {
                                self.control.queue_command(ControlCommand::Stop)
                            }
                            ControlMethod::ConfigReload => {}
                            _ => unreachable!(),
                        }
                    }
                    let completed =
                        !request.params().wait() || self.handle_commands(self.clock.now()).is_ok();
                    let generation = self.snapshot().generation.map(GenerationId::get);
                    if completed {
                        ControlResponse::success(
                            request_id,
                            generation,
                            json!({
                                "accepted": true,
                                "changed": changed,
                                "completed": request.params().wait(),
                            }),
                        )
                    } else {
                        let _ = self.rollback_config_transaction(checkpoint);
                        ControlResponse::failure(
                            request_id,
                            generation,
                            unavailable_control_error(ErrorDomain::Config, "APPLY-ROLLED-BACK"),
                        )
                    }
                }
                Err(error) => ControlResponse::failure(
                    request_id,
                    self.snapshot().generation.map(GenerationId::get),
                    config_control_error(self.config.as_ref(), &error),
                ),
            };
        }
        #[cfg(feature = "subscription-update")]
        if self.config.is_some() && request.method() == ControlMethod::ConfigApply {
            let request_id = request.request_id().clone();
            let checkpoint = match self
                .config
                .as_ref()
                .expect("configuration was checked")
                .checkpoint()
            {
                Ok(checkpoint) => checkpoint,
                Err(error) => {
                    return ControlResponse::failure(
                        request_id,
                        self.snapshot().generation.map(GenerationId::get),
                        config_control_error(self.config.as_ref(), &error),
                    );
                }
            };
            let result = {
                let config = self.config.as_mut().expect("configuration was checked");
                config.apply_document(
                    request
                        .params()
                        .expected_config_digest()
                        .unwrap_or_default(),
                    request
                        .params()
                        .document()
                        .expect("protocol validated document"),
                )
            };
            return match result {
                Ok(change) => {
                    let changed = matches!(change, ConfigChange::Changed { .. });
                    if changed {
                        self.apply_config_change(change);
                    }
                    let completed = self.handle_commands(self.clock.now()).is_ok();
                    let generation = self.snapshot().generation.map(GenerationId::get);
                    if completed {
                        ControlResponse::success(
                            request_id,
                            generation,
                            json!({"accepted": true, "changed": changed, "completed": true}),
                        )
                    } else {
                        let _ = self.rollback_config_transaction(checkpoint);
                        ControlResponse::failure(
                            request_id,
                            generation,
                            unavailable_control_error(ErrorDomain::Config, "APPLY-ROLLED-BACK"),
                        )
                    }
                }
                Err(error) => ControlResponse::failure(
                    request_id,
                    self.snapshot().generation.map(GenerationId::get),
                    config_control_error_with_document(
                        self.config.as_ref(),
                        &error,
                        request.params().document(),
                    ),
                ),
            };
        }
        #[cfg(feature = "subscription-update")]
        if self.config.is_some() && request.method() == ControlMethod::ConfigMutate {
            let request_id = request.request_id().clone();
            let checkpoint = match self
                .config
                .as_ref()
                .expect("configuration was checked")
                .checkpoint()
            {
                Ok(checkpoint) => checkpoint,
                Err(error) => {
                    return ControlResponse::failure(
                        request_id,
                        self.snapshot().generation.map(GenerationId::get),
                        config_control_error(self.config.as_ref(), &error),
                    );
                }
            };
            let result = {
                let config = self.config.as_mut().expect("configuration was checked");
                config.mutate(
                    request
                        .params()
                        .expected_config_digest()
                        .unwrap_or_default(),
                    request
                        .params()
                        .mutation_value()
                        .expect("protocol validated mutation"),
                )
            };
            return match result {
                Ok(outcome) => {
                    let changed = matches!(outcome.change(), ConfigChange::Changed { .. });
                    let source_id = outcome.source_id().map(str::to_owned);
                    if changed {
                        self.apply_config_change(outcome.into_change());
                    }
                    let completed = self.handle_commands(self.clock.now()).is_ok();
                    let generation = self.snapshot().generation.map(GenerationId::get);
                    if completed {
                        ControlResponse::success(
                            request_id,
                            generation,
                            json!({
                                "accepted": true,
                                "changed": changed,
                                "completed": true,
                                "source_id": source_id,
                                "observed_config_digest": self.config.as_ref().map(|value| value.current().digest()),
                            }),
                        )
                    } else {
                        let _ = self.rollback_config_transaction(checkpoint);
                        ControlResponse::failure(
                            request_id,
                            generation,
                            unavailable_control_error(ErrorDomain::Config, "APPLY-ROLLED-BACK"),
                        )
                    }
                }
                Err(error) => ControlResponse::failure(
                    request_id,
                    self.snapshot().generation.map(GenerationId::get),
                    config_control_error(self.config.as_ref(), &error),
                ),
            };
        }
        self.control.handle(request)
    }

    fn subscribe_events(&mut self, request: &ControlRequest) -> Option<crate::EventSubscription> {
        (request.method() == ControlMethod::EventsSubscribe)
            .then(|| {
                self.event_hub.subscribe(
                    request.request_id().clone(),
                    request.params().event_kinds().unwrap_or_default(),
                )
            })
            .and_then(Result::ok)
    }
}

#[cfg(feature = "subscription-update")]
fn import_document(document: Option<&serde_json::Value>) -> Option<(Vec<u8>, FormatHint)> {
    let document = document?.as_object()?;
    if document.len() > 2 {
        return None;
    }
    let content = document.get("content")?.as_str()?;
    if content.is_empty() || content.len() > 768 * 1024 {
        return None;
    }
    let format_hint = match document
        .get("format_hint")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("auto")
    {
        "auto" => FormatHint::Auto,
        "uri_list" => FormatHint::UriList,
        "base64_list" => FormatHint::Base64List,
        "clash_yaml" => FormatHint::ClashYaml,
        "singbox_json" => FormatHint::SingboxJson,
        "ini_profile" => FormatHint::IniProfile,
        "surfboard_ini" => FormatHint::SurfboardIni,
        _ => return None,
    };
    Some((content.as_bytes().to_vec(), format_hint))
}

fn state_wire_for_event(state: RuntimeState) -> &'static str {
    match state {
        RuntimeState::Init => "init",
        RuntimeState::Probing => "probing",
        RuntimeState::StartingCore => "starting_core",
        RuntimeState::RunningTproxy => "running_tproxy",
        RuntimeState::StartingTun => "starting_tun",
        RuntimeState::RunningTun => "running_tun",
        RuntimeState::Degraded => "degraded",
        RuntimeState::FailOpenDirect => "fail_open_direct",
        RuntimeState::Backoff => "backoff",
        RuntimeState::CircuitOpen => "circuit_open",
        RuntimeState::Stopping => "stopping",
    }
}

fn captures_traffic(state: RuntimeState) -> bool {
    matches!(
        state,
        RuntimeState::RunningTproxy | RuntimeState::RunningTun | RuntimeState::Degraded
    )
}

#[cfg(feature = "subscription-update")]
fn disruption_wire(impact: crate::ApplyImpact) -> &'static str {
    match impact {
        crate::ApplyImpact::RuntimeOnly => "none",
        crate::ApplyImpact::NetworkPlan => "sub_second",
        crate::ApplyImpact::GenerationActivation | crate::ApplyImpact::StopDataPlane => {
            "up_to_3_seconds"
        }
    }
}

#[cfg(feature = "subscription-update")]
fn config_control_error(
    config: Option<&ConfigRuntime>,
    error: &crate::ConfigRuntimeError,
) -> nethop_protocol::ControlError {
    config_control_error_with_document(config, error, None)
}

#[cfg(feature = "subscription-update")]
fn config_control_error_with_document(
    config: Option<&ConfigRuntime>,
    error: &crate::ConfigRuntimeError,
    document: Option<&serde_json::Value>,
) -> nethop_protocol::ControlError {
    let (domain, detail) = error.diagnostic();
    if !error.is_conflict() {
        return unavailable_control_error(domain, detail);
    }
    let observed = config
        .and_then(|config| config.observed_digest().ok())
        .or_else(|| config.map(|config| config.current().digest().to_owned()));
    let changed_field_ids = config
        .zip(document)
        .and_then(|(config, document)| config.preview_document(document).ok())
        .map_or_else(Vec::new, |preview| preview.changed_field_ids());
    unavailable_control_error_with_details(
        domain,
        detail,
        json!({
            "observed_config_digest": observed,
            "changed_field_ids": changed_field_ids,
            "requires_reload": true,
        }),
    )
}

#[cfg(feature = "subscription-update")]
fn capability_document(
    report: &CapabilityReport,
    probe_sequence: u64,
    observed_at: Duration,
) -> serde_json::Value {
    let report_value = serde_json::to_value(report).expect("capability report is serializable");
    let digest = nethop_subscription::Digest::sha256(
        &serde_json::to_vec(&report_value).expect("capability report is serializable"),
    )
    .hex();
    let evidence = json!({
        "probe_id": format!("probe-{probe_sequence}"),
        "observed_at_monotonic_ms": observed_at.as_millis(),
        "digest": digest,
    });
    let item = |key: &str, status: CapabilityStatus, reason: &str, effect: &str| {
        json!({
            "key": key,
            "status": manager_capability_status(status),
            "reason_code": reason,
            "requirements": {
                "android_api": null,
                "root_backend": null,
                "kernel_features": [],
            },
            "evidence": evidence,
            "apply_effect": effect,
        })
    };
    let ipv4 = if report.ipv4().supports_tproxy() {
        CapabilityStatus::Supported
    } else {
        first_family_failure(report.ipv4())
    };
    let ipv6 = if report.ipv6().supports_tproxy() {
        CapabilityStatus::Supported
    } else {
        first_family_failure(report.ipv6())
    };
    let allocation = report
        .allocations()
        .iter()
        .find(|entry| entry.status().is_supported())
        .map_or(CapabilityStatus::Conflict, |entry| entry.status());
    let interfaces = if report.interfaces().is_empty() {
        CapabilityStatus::NotPresent
    } else {
        CapabilityStatus::Supported
    };
    json!({
        "schema_version": 1,
        "probe_id": format!("probe-{probe_sequence}"),
        "observed_at_monotonic_ms": observed_at.as_millis(),
        "report_digest": digest,
        "items": [
            item("android", report.android(), capability_reason(report.android()), "admission"),
            item("root", report.root(), capability_reason(report.root()), "admission"),
            item("capture.tproxy.ipv4", ipv4, capability_reason(ipv4), "network_plan"),
            item("capture.tproxy.ipv6", ipv6, capability_reason(ipv6), "network_plan"),
            item("capture.tun", report.tun(), capability_reason(report.tun()), "core_activation"),
            item("network.active_tunnel", report.active_tunnel(), capability_reason(report.active_tunnel()), "network_plan"),
            item("network.inbound_port", report.inbound_port_status(), capability_reason(report.inbound_port_status()), "core_activation"),
            item("network.resource_candidate", allocation, capability_reason(allocation), "network_plan"),
            item("network.interfaces", interfaces, capability_reason(interfaces), "network_plan"),
        ],
    })
}

#[cfg(feature = "subscription-update")]
fn manager_capability_status(status: CapabilityStatus) -> &'static str {
    match status {
        CapabilityStatus::Supported => "supported",
        CapabilityStatus::Conflict => "conflict",
        CapabilityStatus::NotPresent => "unavailable",
        CapabilityStatus::Denied | CapabilityStatus::Unsupported => "unsupported",
    }
}

#[cfg(feature = "subscription-update")]
fn capability_reason(status: CapabilityStatus) -> &'static str {
    match status {
        CapabilityStatus::Supported => "probe_supported",
        CapabilityStatus::NotPresent => "device_state_not_present",
        CapabilityStatus::Unsupported => "kernel_feature_unsupported",
        CapabilityStatus::Denied => "probe_permission_denied",
        CapabilityStatus::Conflict => "resource_conflict",
    }
}

#[cfg(feature = "subscription-update")]
fn first_family_failure(family: &nethop_android::FamilyCapability) -> CapabilityStatus {
    [
        family.address(),
        family.netfilter(),
        family.restore(),
        family.tproxy(),
        family.mark(),
        family.conntrack(),
        family.owner(),
        family.socket(),
        family.policy_routing(),
        family.chain_namespace(),
    ]
    .into_iter()
    .find(|status| !status.is_supported())
    .unwrap_or(CapabilityStatus::Unsupported)
}

#[cfg(feature = "subscription-update")]
fn config_schema_document() -> serde_json::Value {
    let mut fields = vec![
        schema_field(
            "schema_version",
            "integer",
            json!(1),
            "system",
            0,
            false,
            false,
            "runtime_only",
            "normal",
            1,
        ),
        schema_field(
            "service.enabled",
            "boolean",
            json!(true),
            "service",
            10,
            false,
            false,
            "stop_data_plane",
            "disruptive",
            1,
        ),
        schema_field(
            "subscriptions.auto_update",
            "boolean",
            json!(true),
            "subscriptions",
            20,
            false,
            false,
            "runtime_only",
            "normal",
            2,
        ),
        ranged_schema_field(
            "subscriptions.update_interval_hours",
            "integer",
            json!(24),
            "subscriptions",
            21,
            false,
            false,
            "runtime_only",
            "normal",
            2,
            1,
            168,
        ),
        collection_schema_field(
            "subscriptions.sources",
            "source_array",
            "subscriptions",
            22,
            false,
            true,
            "generation_activation",
            "disruptive",
            1,
            1,
            16,
        ),
        ranged_schema_field(
            "subscriptions.sources[].name",
            "string",
            json!(null),
            "subscriptions",
            23,
            false,
            false,
            "generation_activation",
            "normal",
            1,
            1,
            128,
        ),
        schema_field(
            "subscriptions.sources[].enabled",
            "boolean",
            json!(true),
            "subscriptions",
            24,
            false,
            false,
            "generation_activation",
            "normal",
            1,
        ),
        schema_field(
            "subscriptions.sources[].url",
            "string",
            json!(""),
            "subscriptions",
            25,
            false,
            true,
            "generation_activation",
            "disruptive",
            1,
        ),
        enum_schema_field(
            "subscriptions.sources[].request_profile",
            json!("sing_box_android"),
            "subscriptions",
            26,
            true,
            false,
            "generation_activation",
            "normal",
            2,
            &[
                "generic",
                "mihomo",
                "clash_standard",
                "surfboard",
                "sing_box",
                "sing_box_android",
            ],
        ),
        enum_schema_field(
            "subscriptions.sources[].format_hint",
            json!("auto"),
            "subscriptions",
            27,
            true,
            false,
            "generation_activation",
            "normal",
            2,
            &[
                "auto",
                "uri_list",
                "base64_list",
                "clash_yaml",
                "singbox_json",
            ],
        ),
        collection_schema_field(
            "subscriptions.sources[].mirrors",
            "string_array",
            "subscriptions",
            28,
            true,
            true,
            "generation_activation",
            "normal",
            2,
            0,
            3,
        ),
        collection_schema_field(
            "subscriptions.sources[].filter.include_names",
            "string_array",
            "subscriptions",
            29,
            true,
            false,
            "generation_activation",
            "normal",
            2,
            0,
            32,
        ),
        collection_schema_field(
            "subscriptions.sources[].filter.exclude_names",
            "string_array",
            "subscriptions",
            30,
            true,
            false,
            "generation_activation",
            "normal",
            2,
            0,
            32,
        ),
        collection_schema_field(
            "subscriptions.sources[].filter.protocols",
            "proxy_protocol_array",
            "subscriptions",
            31,
            true,
            false,
            "generation_activation",
            "normal",
            2,
            0,
            9,
        ),
        enum_schema_field(
            "proxy.outbound_mode",
            json!("rule"),
            "proxy",
            30,
            false,
            false,
            "generation_activation",
            "normal",
            2,
            &["rule", "global", "direct"],
        ),
        enum_schema_field(
            "proxy.selector_mode",
            json!("urltest"),
            "proxy",
            31,
            false,
            false,
            "generation_activation",
            "normal",
            2,
            &["urltest", "manual"],
        ),
        ranged_schema_field(
            "proxy.urltest.interval_minutes",
            "integer",
            json!(10),
            "proxy",
            32,
            true,
            false,
            "generation_activation",
            "normal",
            2,
            5,
            1440,
        ),
        ranged_schema_field(
            "proxy.urltest.tolerance_ms",
            "integer",
            json!(50),
            "proxy",
            33,
            true,
            false,
            "generation_activation",
            "normal",
            2,
            0,
            1000,
        ),
        ranged_schema_field(
            "proxy.urltest.max_candidates",
            "integer",
            json!(64),
            "proxy",
            34,
            true,
            false,
            "generation_activation",
            "normal",
            2,
            1,
            256,
        ),
        ranged_schema_field(
            "proxy.urltest.concurrency",
            "integer",
            json!(10),
            "proxy",
            35,
            true,
            false,
            "generation_activation",
            "normal",
            2,
            10,
            10,
        ),
        enum_schema_field(
            "applications.mode",
            json!("all"),
            "applications",
            40,
            false,
            false,
            "network_plan",
            "normal",
            2,
            &["all", "blacklist", "whitelist"],
        ),
        collection_schema_field(
            "applications.targets",
            "application_target_array",
            "applications",
            41,
            true,
            false,
            "network_plan",
            "disruptive",
            2,
            0,
            2000,
        ),
        enum_schema_field(
            "network.capture_mode",
            json!("auto"),
            "network",
            50,
            false,
            false,
            "network_plan",
            "disruptive",
            2,
            &["auto", "tproxy", "tun"],
        ),
        schema_field(
            "network.proxy_tcp",
            "boolean",
            json!(true),
            "network",
            51,
            true,
            false,
            "network_plan",
            "disruptive",
            2,
        ),
        schema_field(
            "network.proxy_udp",
            "boolean",
            json!(true),
            "network",
            52,
            true,
            false,
            "network_plan",
            "disruptive",
            2,
        ),
        enum_schema_field(
            "network.ipv6_mode",
            json!("auto"),
            "network",
            53,
            true,
            false,
            "network_plan",
            "disruptive",
            2,
            &["auto", "proxy", "block"],
        ),
        enum_schema_field(
            "network.dns_mode",
            json!("auto"),
            "network",
            54,
            true,
            false,
            "network_plan",
            "disruptive",
            2,
            &["auto", "proxy", "system"],
        ),
        enum_schema_field(
            "network.tun_stack",
            json!("gvisor"),
            "network",
            55,
            true,
            false,
            "generation_activation",
            "disruptive",
            2,
            &["system", "gvisor"],
        ),
        schema_field(
            "network.interfaces.mobile",
            "boolean",
            json!(true),
            "network",
            56,
            true,
            false,
            "network_plan",
            "normal",
            2,
        ),
        schema_field(
            "network.interfaces.wifi",
            "boolean",
            json!(true),
            "network",
            57,
            true,
            false,
            "network_plan",
            "normal",
            2,
        ),
        schema_field(
            "network.interfaces.hotspot",
            "boolean",
            json!(false),
            "network",
            58,
            true,
            false,
            "network_plan",
            "experimental",
            2,
        ),
        schema_field(
            "network.interfaces.usb",
            "boolean",
            json!(false),
            "network",
            59,
            true,
            false,
            "network_plan",
            "experimental",
            2,
        ),
        collection_schema_field(
            "network.interfaces.include",
            "string_array",
            "network",
            60,
            true,
            false,
            "network_plan",
            "normal",
            2,
            0,
            64,
        ),
        collection_schema_field(
            "network.interfaces.exclude",
            "string_array",
            "network",
            61,
            true,
            false,
            "network_plan",
            "normal",
            2,
            0,
            64,
        ),
        schema_field(
            "network.wifi_scenes.enabled",
            "boolean",
            json!(false),
            "network",
            62,
            true,
            false,
            "network_plan",
            "normal",
            2,
        ),
        ranged_schema_field(
            "network.wifi_scenes.probe_interval_seconds",
            "integer",
            json!(30),
            "network",
            63,
            true,
            false,
            "runtime_only",
            "normal",
            2,
            15,
            3600,
        ),
        collection_schema_field(
            "network.wifi_scenes.rules",
            "wifi_scene_rule_array",
            "network",
            64,
            true,
            true,
            "network_plan",
            "sensitive",
            2,
            0,
            64,
        ),
        schema_field(
            "routing.bypass_private",
            "boolean",
            json!(true),
            "routing",
            70,
            false,
            false,
            "generation_activation",
            "normal",
            2,
        ),
        schema_field(
            "routing.bypass_cn",
            "boolean",
            json!(false),
            "routing",
            71,
            false,
            false,
            "generation_activation",
            "experimental",
            2,
        ),
        schema_field(
            "routing.block_quic",
            "boolean",
            json!(false),
            "routing",
            72,
            true,
            false,
            "generation_activation",
            "experimental",
            2,
        ),
        collection_schema_field(
            "routing.force_proxy_cidrs",
            "cidr_array",
            "routing",
            73,
            true,
            false,
            "generation_activation",
            "normal",
            2,
            0,
            512,
        ),
        collection_schema_field(
            "routing.bypass_cidrs",
            "cidr_array",
            "routing",
            74,
            true,
            false,
            "generation_activation",
            "normal",
            2,
            0,
            512,
        ),
        collection_schema_field(
            "routing.force_proxy_domains",
            "domain_suffix_array",
            "routing",
            75,
            true,
            false,
            "generation_activation",
            "normal",
            2,
            0,
            512,
        ),
        collection_schema_field(
            "routing.bypass_domains",
            "domain_suffix_array",
            "routing",
            76,
            true,
            false,
            "generation_activation",
            "normal",
            2,
            0,
            512,
        ),
        collection_schema_field(
            "routing.block_domains",
            "domain_suffix_array",
            "routing",
            77,
            true,
            false,
            "generation_activation",
            "normal",
            2,
            0,
            512,
        ),
        enum_schema_field(
            "logging.level",
            json!("info"),
            "logging",
            80,
            false,
            false,
            "runtime_only",
            "normal",
            2,
            &["error", "warn", "info", "debug", "trace"],
        ),
        ranged_schema_field(
            "logging.retention_days",
            "integer",
            json!(7),
            "logging",
            81,
            true,
            false,
            "runtime_only",
            "normal",
            2,
            1,
            30,
        ),
        ranged_schema_field(
            "advanced.inbound_port",
            "integer",
            json!(7893),
            "advanced",
            90,
            true,
            false,
            "generation_activation",
            "disruptive",
            2,
            1,
            65535,
        ),
        ranged_schema_field(
            "advanced.bypass_mark",
            "integer",
            json!(131072),
            "advanced",
            91,
            true,
            false,
            "network_plan",
            "disruptive",
            2,
            1,
            u32::MAX as u64,
        ),
        schema_field(
            "advanced.ipv6_guard",
            "boolean",
            json!(true),
            "advanced",
            92,
            true,
            false,
            "network_plan",
            "destructive",
            2,
        ),
        schema_field(
            "advanced.dry_run",
            "boolean",
            json!(false),
            "advanced",
            93,
            true,
            false,
            "network_plan",
            "normal",
            2,
        ),
        ranged_schema_field(
            "advanced.health_timeout_seconds",
            "integer",
            json!(3),
            "advanced",
            94,
            true,
            false,
            "generation_activation",
            "normal",
            2,
            1,
            30,
        ),
        ranged_schema_field(
            "advanced.reconcile_interval_seconds",
            "integer",
            json!(60),
            "advanced",
            95,
            true,
            false,
            "runtime_only",
            "normal",
            2,
            60,
            3600,
        ),
        collection_schema_field(
            "advanced.resource_candidates",
            "resource_candidate_array",
            "advanced",
            96,
            true,
            false,
            "network_plan",
            "disruptive",
            2,
            1,
            16,
        ),
        ranged_schema_field(
            "advanced.resource_candidates[].mark",
            "integer",
            json!(null),
            "advanced",
            97,
            true,
            false,
            "network_plan",
            "disruptive",
            2,
            1,
            u32::MAX as u64,
        ),
        ranged_schema_field(
            "advanced.resource_candidates[].mask",
            "integer",
            json!(null),
            "advanced",
            98,
            true,
            false,
            "network_plan",
            "disruptive",
            2,
            1,
            u32::MAX as u64,
        ),
        ranged_schema_field(
            "advanced.resource_candidates[].route_table",
            "integer",
            json!(null),
            "advanced",
            99,
            true,
            false,
            "network_plan",
            "disruptive",
            2,
            1,
            u32::MAX as u64,
        ),
        ranged_schema_field(
            "advanced.resource_candidates[].rule_priority",
            "integer",
            json!(null),
            "advanced",
            100,
            true,
            false,
            "network_plan",
            "disruptive",
            2,
            1,
            u32::MAX as u64,
        ),
    ];
    for field in &mut fields {
        let id = field["field_id"].as_str().unwrap_or_default().to_owned();
        field["capability_key"] = match id.as_str() {
            "network.capture_mode" => json!("capture.mode"),
            "network.ipv6_mode" | "advanced.ipv6_guard" => json!("capture.tproxy.ipv6"),
            "network.tun_stack" => json!("capture.tun"),
            value if value.starts_with("network.interfaces.") => json!("network.interfaces"),
            value if value.starts_with("advanced.resource_candidates") => {
                json!("network.resource_candidate")
            }
            _ => serde_json::Value::Null,
        };
        field["experimental"] = json!(matches!(
            id.as_str(),
            "network.interfaces.hotspot"
                | "network.interfaces.usb"
                | "routing.bypass_cn"
                | "routing.block_quic"
        ));
    }
    json!({
        "schema_version": 1,
        "fields": fields,
    })
}

#[cfg(feature = "subscription-update")]
#[allow(clippy::too_many_arguments)]
fn schema_field(
    field_id: &str,
    value_type: &str,
    default: serde_json::Value,
    group: &str,
    order: u16,
    advanced: bool,
    sensitive: bool,
    apply_impact: &str,
    risk_level: &str,
    stage: u8,
) -> serde_json::Value {
    json!({
        "field_id": field_id,
        "path": field_id,
        "value_type": value_type,
        "default": default,
        "enum": null,
        "range": null,
        "min_items": null,
        "max_items": null,
        "title_key": format!("config.{field_id}.title"),
        "description_key": format!("config.{field_id}.description"),
        "group": group,
        "order": order,
        "advanced": advanced,
        "experimental": false,
        "deprecated": false,
        "sensitive": sensitive,
        "read_only": field_id == "schema_version",
        "write_only": false,
        "apply_impact": apply_impact,
        "risk_level": risk_level,
        "confirmation_key": if matches!(risk_level, "disruptive" | "destructive") { json!(format!("config.{field_id}.confirm")) } else { serde_json::Value::Null },
        "capability_key": null,
        "stage": stage,
    })
}

#[cfg(feature = "subscription-update")]
#[allow(clippy::too_many_arguments)]
fn enum_schema_field(
    field_id: &str,
    default: serde_json::Value,
    group: &str,
    order: u16,
    advanced: bool,
    sensitive: bool,
    apply_impact: &str,
    risk_level: &str,
    stage: u8,
    variants: &[&str],
) -> serde_json::Value {
    let mut value = schema_field(
        field_id,
        "enum",
        default,
        group,
        order,
        advanced,
        sensitive,
        apply_impact,
        risk_level,
        stage,
    );
    value["enum_values"] = json!(variants);
    value
}

#[cfg(feature = "subscription-update")]
#[allow(clippy::too_many_arguments)]
fn ranged_schema_field(
    field_id: &str,
    value_type: &str,
    default: serde_json::Value,
    group: &str,
    order: u16,
    advanced: bool,
    sensitive: bool,
    apply_impact: &str,
    risk_level: &str,
    stage: u8,
    minimum: u64,
    maximum: u64,
) -> serde_json::Value {
    let mut value = schema_field(
        field_id,
        value_type,
        default,
        group,
        order,
        advanced,
        sensitive,
        apply_impact,
        risk_level,
        stage,
    );
    value["range"] = json!({"minimum": minimum, "maximum": maximum});
    value
}

#[cfg(feature = "subscription-update")]
#[allow(clippy::too_many_arguments)]
fn collection_schema_field(
    field_id: &str,
    value_type: &str,
    group: &str,
    order: u16,
    advanced: bool,
    sensitive: bool,
    apply_impact: &str,
    risk_level: &str,
    stage: u8,
    minimum: u64,
    maximum: u64,
) -> serde_json::Value {
    let mut value = schema_field(
        field_id,
        value_type,
        json!([]),
        group,
        order,
        advanced,
        sensitive,
        apply_impact,
        risk_level,
        stage,
    );
    value["min_items"] = json!(minimum);
    value["max_items"] = json!(maximum);
    value
}

impl<S, N, V, C, U> WorkerServiceTasks for WorkerApplication<S, N, V, C, U>
where
    N: NetworkController,
    S: RuntimeRecoverySource<N>,
    S::Process: CandidateProcess,
    V: RuntimeHealthVerifier,
    C: WorkerClock,
    U: RuntimeUpdateSource,
{
    fn next_wakeup_in(&self) -> Duration {
        let now = self.clock.now();
        if self.start_pending {
            return Duration::ZERO;
        }
        let runtime = self
            .runtime
            .as_ref()
            .map(|runtime| runtime.next_wakeup_in(now));
        let restart = self.restart_at.map(|deadline| deadline.saturating_sub(now));
        #[cfg(feature = "subscription-update")]
        let scheduled = self.update_schedule.next_wakeup_in();
        #[cfg(not(feature = "subscription-update"))]
        let scheduled = None;
        #[cfg(feature = "subscription-update")]
        let core_version_check = self.core_version_schedule_retry_at.map_or_else(
            || self.core_version_schedule.next_wakeup_in(),
            |deadline| Some(deadline.saturating_sub(now)),
        );
        #[cfg(not(feature = "subscription-update"))]
        let core_version_check = None;
        #[cfg(feature = "subscription-update")]
        let rule_set_update = self.rule_set_schedule_retry_at.map_or_else(
            || self.rule_set_schedule.next_wakeup_in(),
            |deadline| Some(deadline.saturating_sub(now)),
        );
        #[cfg(not(feature = "subscription-update"))]
        let rule_set_update = None;
        #[cfg(feature = "subscription-update")]
        let log_cleanup = self.log_retention.next_wakeup_in(now);
        #[cfg(not(feature = "subscription-update"))]
        let log_cleanup = None;
        #[cfg(feature = "subscription-update")]
        let wifi_scene = self
            .config
            .as_ref()
            .filter(|config| {
                config
                    .current()
                    .effective()
                    .network()
                    .wifi_scenes()
                    .enabled()
                    && self.wifi_facts.is_some()
            })
            .map(|_| self.wifi_scene_next_probe.saturating_sub(now));
        #[cfg(not(feature = "subscription-update"))]
        let wifi_scene = None;
        let traffic = (self.event_hub.traffic_subscribers() > 0)
            .then(|| self.next_traffic_sample.saturating_sub(now));
        let payload_cleanup = self
            .webui_payload_store
            .as_ref()
            .map(|_| self.next_payload_cleanup.saturating_sub(now));
        runtime
            .into_iter()
            .chain(restart)
            .chain(scheduled)
            .chain(core_version_check)
            .chain(rule_set_update)
            .chain(log_cleanup)
            .chain(wifi_scene)
            .chain(traffic)
            .chain(payload_cleanup)
            .min()
            .unwrap_or(IDLE_WAKEUP)
    }

    fn run_ready(&mut self) -> Result<(), WorkerServiceError> {
        #[cfg(feature = "subscription-update")]
        self.observe_config_watch_health();
        #[cfg(feature = "subscription-update")]
        self.reconcile_watched_config();
        #[cfg(feature = "subscription-update")]
        if self
            .update_schedule
            .take_due()
            .map_err(|_| WorkerServiceError::TaskFailed)?
        {
            self.control.queue_command(ControlCommand::Update);
        }
        let now = self.clock.now();
        self.sample_traffic_if_due(now);
        if now >= self.next_payload_cleanup {
            if let Some(store) = self.webui_payload_store.as_ref() {
                if store
                    .cleanup_expired(SystemTime::now(), crate::PAYLOAD_TTL)
                    .is_err()
                {
                    self.event_hub.publish(
                        EventKind::Runtime,
                        json!({"kind":"webui_payload","state":"cleanup_degraded"}),
                    );
                }
            }
            self.next_payload_cleanup = now.saturating_add(Duration::from_secs(5 * 60));
        }
        #[cfg(feature = "subscription-update")]
        self.run_scheduled_core_version_check(now);
        #[cfg(feature = "subscription-update")]
        self.run_scheduled_rule_set_update(now);
        #[cfg(feature = "subscription-update")]
        self.reconcile_wifi_scene(now);
        #[cfg(feature = "subscription-update")]
        if self.log_retention.run_due(now).is_err() {
            self.event_hub.publish(
                EventKind::Runtime,
                json!({"kind":"logging","state":"retention_degraded"}),
            );
        }
        self.handle_commands(now)?;
        self.tick_runtime(now)
    }

    fn shutdown(&mut self) -> Result<(), WorkerServiceError> {
        self.stop()
    }
}
