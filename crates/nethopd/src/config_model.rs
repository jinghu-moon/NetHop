use std::{
    collections::HashSet,
    fmt,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    str::FromStr,
};

use nethop_android::{
    AppCatalog, AppSelectionMode, ResourceCandidate, WifiSceneAction, WifiSceneMatcher,
    WifiSceneRule,
};
use nethop_core::{
    CaptureMode, CapturePolicy, ForwardingPolicy, InterfacePolicy, ManagedLogLevel, ManagedOptions,
    ManagedOutboundMode, TunStack,
};
use nethop_protocol::ApplicationTarget;
#[cfg(feature = "subscription-update")]
use nethop_subscription::FormatHint;
use nethop_subscription::{NodeFilter, ProxyProtocol, RequestProfile};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::worker_config::{CONFIG_SCHEMA_VERSION, ConfigError, MAX_AUTO_CANDIDATES, MAX_SOURCES};

const MAX_SOURCE_NAME_BYTES: usize = 128;
const MAX_SOURCE_NAME_CHARS: usize = 64;
const MAX_MIRRORS: usize = 3;
const MAX_APPLICATION_TARGETS: usize = 2_000;
const MAX_CIDRS: usize = 512;
const MAX_DOMAINS: usize = 512;
const MAX_DOMAIN_BYTES: usize = 253;
const MAX_INTERFACES: usize = 64;
const MAX_INTERFACE_PATTERN_BYTES: usize = 64;
const DEFAULT_INBOUND_PORT: u16 = 7893;
const DEFAULT_BYPASS_MARK: u32 = 131_072;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct UserConfigWire {
    pub(crate) schema_version: u32,
    pub(crate) service: ServiceWire,
    pub(crate) subscriptions: SubscriptionsWire,
    #[serde(default)]
    pub(crate) proxy: ProxyWire,
    #[serde(default)]
    pub(crate) applications: ApplicationsWire,
    #[serde(default)]
    pub(crate) network: NetworkWire,
    #[serde(default)]
    pub(crate) routing: RoutingWire,
    #[serde(default)]
    pub(crate) logging: LoggingWire,
    #[serde(default)]
    pub(crate) advanced: AdvancedWire,
}

impl UserConfigWire {
    pub(crate) fn canonicalized(&self) -> Result<Self, ConfigError> {
        let mut wire = self.clone();
        // Validate before normalization so canonical output cannot hide invalid input.
        let _ = EffectiveConfig::from_wire(wire.clone())?;
        wire.applications.targets.sort();
        for source in &mut wire.subscriptions.sources {
            source.filter.include_names.sort();
            source.filter.exclude_names.sort();
            source.filter.excluded_node_ids.sort();
            source.filter.protocols.sort();
        }
        wire.network.interfaces.include.sort();
        wire.network.interfaces.exclude.sort();
        wire.network
            .wifi_scenes
            .rules
            .sort_by(|left, right| left.id.cmp(&right.id));
        wire.routing.force_proxy_cidrs = parse_cidrs(wire.routing.force_proxy_cidrs)?
            .into_iter()
            .map(|cidr| cidr.text)
            .collect();
        wire.routing.bypass_cidrs = parse_cidrs(wire.routing.bypass_cidrs)?
            .into_iter()
            .map(|cidr| cidr.text)
            .collect();
        wire.routing.force_proxy_domains = parse_domains(wire.routing.force_proxy_domains)?;
        wire.routing.bypass_domains = parse_domains(wire.routing.bypass_domains)?;
        wire.routing.block_domains = parse_domains(wire.routing.block_domains)?;
        Ok(wire)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ServiceWire {
    pub(crate) enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct SubscriptionsWire {
    #[serde(default)]
    mode: SubscriptionMode,
    #[serde(default = "default_true")]
    auto_update: bool,
    #[serde(default = "default_update_interval_hours")]
    update_interval_hours: u16,
    pub(crate) sources: Vec<SubscriptionSourceWire>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct SubscriptionSourceWire {
    name: String,
    #[serde(default = "default_true")]
    enabled: bool,
    url: String,
    #[serde(default)]
    request_profile: SourceRequestProfile,
    #[serde(default)]
    format_hint: SourceFormatHint,
    #[serde(default)]
    mirrors: Vec<String>,
    #[serde(default)]
    filter: SourceFilterWire,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct SourceFilterWire {
    #[serde(default)]
    include_names: Vec<String>,
    #[serde(default)]
    exclude_names: Vec<String>,
    #[serde(default)]
    excluded_node_ids: Vec<String>,
    #[serde(default)]
    protocols: Vec<ProxyProtocol>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum SourceRequestProfile {
    Generic,
    Mihomo,
    ClashStandard,
    Surfboard,
    SingBox,
    #[default]
    SingBoxAndroid,
}

impl SourceRequestProfile {
    const fn effective(self) -> RequestProfile {
        match self {
            Self::Generic => RequestProfile::NetHopGeneric,
            Self::Mihomo => RequestProfile::Mihomo,
            Self::ClashStandard => RequestProfile::ClashStandard,
            Self::Surfboard => RequestProfile::Surfboard,
            Self::SingBox => RequestProfile::SingBox,
            Self::SingBoxAndroid => RequestProfile::SingBoxAndroid,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceFormatHint {
    #[default]
    Auto,
    UriList,
    Base64List,
    ClashYaml,
    SingboxJson,
    SurfboardIni,
}

impl SourceFormatHint {
    #[cfg(feature = "subscription-update")]
    pub(crate) const fn parser_hint(self) -> FormatHint {
        match self {
            Self::Auto => FormatHint::Auto,
            Self::UriList => FormatHint::UriList,
            Self::Base64List => FormatHint::Base64List,
            Self::ClashYaml => FormatHint::ClashYaml,
            Self::SingboxJson => FormatHint::SingboxJson,
            Self::SurfboardIni => FormatHint::SurfboardIni,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProxyWire {
    #[serde(default)]
    outbound_mode: OutboundMode,
    #[serde(default)]
    urltest: UrltestWire,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutboundMode {
    #[default]
    Rule,
    Global,
    Direct,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct UrltestWire {
    #[serde(default = "default_urltest_interval")]
    interval_minutes: u16,
    #[serde(default = "default_urltest_tolerance")]
    tolerance_ms: u16,
    #[serde(default = "default_urltest_candidates")]
    max_candidates: u16,
}

impl Default for UrltestWire {
    fn default() -> Self {
        Self {
            interval_minutes: default_urltest_interval(),
            tolerance_ms: default_urltest_tolerance(),
            max_candidates: default_urltest_candidates(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ApplicationsWire {
    #[serde(default)]
    mode: ApplicationMode,
    #[serde(default)]
    targets: Vec<ApplicationTarget>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApplicationMode {
    #[default]
    All,
    Blacklist,
    Whitelist,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct NetworkWire {
    #[serde(default)]
    capture_mode: CaptureIntent,
    #[serde(default = "default_true")]
    proxy_tcp: bool,
    #[serde(default = "default_true")]
    proxy_udp: bool,
    #[serde(default)]
    ipv6_mode: Ipv6Mode,
    #[serde(default)]
    dns_mode: DnsMode,
    #[serde(default)]
    tun_stack: TunStackIntent,
    #[serde(default)]
    interfaces: InterfacesWire,
    #[serde(default)]
    wifi_scenes: WifiScenesWire,
}

impl Default for NetworkWire {
    fn default() -> Self {
        Self {
            capture_mode: CaptureIntent::default(),
            proxy_tcp: true,
            proxy_udp: true,
            ipv6_mode: Ipv6Mode::default(),
            dns_mode: DnsMode::default(),
            tun_stack: TunStackIntent::default(),
            interfaces: InterfacesWire::default(),
            wifi_scenes: WifiScenesWire::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CaptureIntent {
    #[default]
    Auto,
    Tproxy,
    Tun,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Ipv6Mode {
    #[default]
    Auto,
    Proxy,
    Block,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DnsMode {
    #[default]
    Auto,
    Proxy,
    System,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TunStackIntent {
    System,
    #[default]
    Gvisor,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct InterfacesWire {
    #[serde(default = "default_true")]
    mobile: bool,
    #[serde(default = "default_true")]
    wifi: bool,
    #[serde(default)]
    hotspot: bool,
    #[serde(default)]
    usb: bool,
    #[serde(default)]
    include: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
}

impl Default for InterfacesWire {
    fn default() -> Self {
        Self {
            mobile: true,
            wifi: true,
            hotspot: false,
            usb: false,
            include: Vec::new(),
            exclude: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct WifiScenesWire {
    #[serde(default)]
    enabled: bool,
    #[serde(default = "default_wifi_scene_probe_interval")]
    probe_interval_seconds: u16,
    #[serde(default)]
    rules: Vec<WifiSceneRuleWire>,
}

impl Default for WifiScenesWire {
    fn default() -> Self {
        Self {
            enabled: false,
            probe_interval_seconds: default_wifi_scene_probe_interval(),
            rules: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct WifiSceneRuleWire {
    id: String,
    #[serde(default)]
    ssid: Option<String>,
    #[serde(default)]
    bssid: Option<String>,
    action: WifiSceneActionWire,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum WifiSceneActionWire {
    EnableProxy,
    DisableProxy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct RoutingWire {
    #[serde(default = "default_true")]
    bypass_private: bool,
    #[serde(default = "default_true")]
    bypass_cn: bool,
    #[serde(default)]
    block_quic: bool,
    #[serde(default)]
    force_proxy_cidrs: Vec<String>,
    #[serde(default)]
    bypass_cidrs: Vec<String>,
    #[serde(default)]
    force_proxy_domains: Vec<String>,
    #[serde(default)]
    bypass_domains: Vec<String>,
    #[serde(default)]
    block_domains: Vec<String>,
}

impl Default for RoutingWire {
    fn default() -> Self {
        Self {
            bypass_private: true,
            bypass_cn: true,
            block_quic: false,
            force_proxy_cidrs: Vec::new(),
            bypass_cidrs: Vec::new(),
            force_proxy_domains: Vec::new(),
            bypass_domains: Vec::new(),
            block_domains: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct LoggingWire {
    #[serde(default)]
    level: LogLevel,
    #[serde(default = "default_retention_days")]
    retention_days: u8,
}

impl Default for LoggingWire {
    fn default() -> Self {
        Self {
            level: LogLevel::default(),
            retention_days: default_retention_days(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdvancedWire {
    #[serde(default = "default_inbound_port")]
    inbound_port: u16,
    #[serde(default = "default_bypass_mark")]
    bypass_mark: u32,
    #[serde(default = "default_true")]
    ipv6_guard: bool,
    #[serde(default)]
    dry_run: bool,
    #[serde(default = "default_health_timeout")]
    health_timeout_seconds: u8,
    #[serde(default = "default_reconcile_interval")]
    reconcile_interval_seconds: u16,
    #[serde(default = "default_resource_candidates")]
    resource_candidates: Vec<ResourceCandidateWire>,
}

impl Default for AdvancedWire {
    fn default() -> Self {
        Self {
            inbound_port: default_inbound_port(),
            bypass_mark: default_bypass_mark(),
            ipv6_guard: true,
            dry_run: false,
            health_timeout_seconds: default_health_timeout(),
            reconcile_interval_seconds: default_reconcile_interval(),
            resource_candidates: default_resource_candidates(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResourceCandidateWire {
    mark: u32,
    mask: u32,
    route_table: u32,
    rule_priority: u32,
}

#[derive(Clone, PartialEq, Eq)]
pub struct EffectiveConfig {
    service_enabled: bool,
    subscriptions: SubscriptionSettings,
    sources: Vec<UserSource>,
    proxy: ProxySettings,
    applications: ApplicationSettings,
    network: NetworkSettings,
    routing: RoutingSettings,
    logging: LoggingSettings,
    advanced: AdvancedSettings,
    capture: CapturePolicy,
    allocations: Vec<ResourceCandidate>,
}

impl EffectiveConfig {
    pub(crate) fn from_wire(wire: UserConfigWire) -> Result<Self, ConfigError> {
        if wire.schema_version != CONFIG_SCHEMA_VERSION {
            return Err(ConfigError::UnsupportedSchema);
        }
        let sources = validate_sources(&wire.subscriptions.sources)?;
        let subscriptions = validate_subscription_settings(&wire.subscriptions, &sources)?;
        let proxy = validate_proxy(wire.proxy)?;
        let applications = validate_applications(wire.applications)?;
        let network = validate_network(wire.network)?;
        let routing = validate_routing(wire.routing)?;
        let logging = validate_logging(wire.logging)?;
        let (advanced, allocations) = validate_advanced(wire.advanced)?;
        let capture_mode = match network.capture_mode {
            CaptureIntent::Auto | CaptureIntent::Tproxy => CaptureMode::Tproxy,
            CaptureIntent::Tun => CaptureMode::Tun,
        };
        let capture = build_capture(
            capture_mode,
            &network,
            &advanced,
            applications.initial_include_uids(),
            applications.base_exclude_uids(),
        )?;
        Ok(Self {
            service_enabled: wire.service.enabled,
            subscriptions,
            sources,
            proxy,
            applications,
            network,
            routing,
            logging,
            advanced,
            capture,
            allocations,
        })
    }

    pub const fn service_enabled(&self) -> bool {
        self.service_enabled
    }

    pub const fn subscriptions(&self) -> &SubscriptionSettings {
        &self.subscriptions
    }

    pub fn sources(&self) -> &[UserSource] {
        &self.sources
    }

    pub const fn proxy(&self) -> &ProxySettings {
        &self.proxy
    }

    pub const fn applications(&self) -> &ApplicationSettings {
        &self.applications
    }

    pub const fn network(&self) -> &NetworkSettings {
        &self.network
    }

    pub const fn routing(&self) -> &RoutingSettings {
        &self.routing
    }

    pub const fn logging(&self) -> &LoggingSettings {
        &self.logging
    }

    pub const fn advanced(&self) -> &AdvancedSettings {
        &self.advanced
    }

    pub const fn capture(&self) -> &CapturePolicy {
        &self.capture
    }

    pub fn admitted_capture(
        &self,
        catalog: Option<&AppCatalog>,
    ) -> Result<CapturePolicy, ConfigError> {
        let packages = self.applications.packages().collect::<Vec<_>>();
        if packages.is_empty() {
            return Ok(self.capture.clone());
        }
        let mode = match self.applications.mode {
            ApplicationMode::All => return Err(ConfigError::InvalidApplications),
            ApplicationMode::Blacklist => AppSelectionMode::Blacklist,
            ApplicationMode::Whitelist => AppSelectionMode::Whitelist,
        };
        let selection = catalog
            .ok_or(ConfigError::ApplicationCatalogUnavailable)?
            .compile_selection(mode, packages)
            .map_err(|_| ConfigError::InvalidApplications)?;
        self.capture_with_application_uids(selection.include_uids(), selection.exclude_uids())
    }

    pub fn capture_with_application_uids(
        &self,
        include_uids: &[u32],
        exclude_uids: &[u32],
    ) -> Result<CapturePolicy, ConfigError> {
        let mut include = self.applications.base_include_uids();
        include.retain(|uid| *uid != APPLICATION_RESOLUTION_SENTINEL);
        include.extend_from_slice(include_uids);
        include.sort_unstable();
        include.dedup();
        let mut exclude = self.applications.base_exclude_uids();
        exclude.extend_from_slice(exclude_uids);
        exclude.sort_unstable();
        exclude.dedup();
        if include.iter().any(|uid| exclude.contains(uid)) {
            return Err(ConfigError::InvalidApplications);
        }
        let capture_mode = match self.network.capture_mode {
            CaptureIntent::Auto | CaptureIntent::Tproxy => CaptureMode::Tproxy,
            CaptureIntent::Tun => CaptureMode::Tun,
        };
        build_capture(
            capture_mode,
            &self.network,
            &self.advanced,
            include,
            exclude,
        )
    }

    pub fn allocations(&self) -> &[ResourceCandidate] {
        &self.allocations
    }

    pub fn managed_options(&self) -> Result<ManagedOptions, ConfigError> {
        ManagedOptions::new(
            match self.proxy.outbound_mode {
                OutboundMode::Rule => ManagedOutboundMode::Rule,
                OutboundMode::Global => ManagedOutboundMode::Global,
                OutboundMode::Direct => ManagedOutboundMode::Direct,
            },
            self.proxy.urltest.interval_minutes,
            self.proxy.urltest.tolerance_ms,
            usize::from(self.proxy.urltest.max_candidates),
            match self.logging.level {
                LogLevel::Error => ManagedLogLevel::Error,
                LogLevel::Warn => ManagedLogLevel::Warn,
                LogLevel::Info => ManagedLogLevel::Info,
                LogLevel::Debug => ManagedLogLevel::Debug,
                LogLevel::Trace => ManagedLogLevel::Trace,
            },
            self.routing.bypass_private,
            self.routing.bypass_cn,
            self.routing
                .force_proxy_cidrs
                .iter()
                .map(|cidr| cidr.text.clone())
                .collect(),
            self.routing
                .bypass_cidrs
                .iter()
                .map(|cidr| cidr.text.clone())
                .collect(),
        )
        .and_then(|options| {
            options.with_domain_rules(
                self.routing.force_proxy_domains.clone(),
                self.routing.bypass_domains.clone(),
                self.routing.block_domains.clone(),
            )
        })
        .map_err(|_| ConfigError::InvalidProxy)
    }

    pub const fn managed_tun_stack(&self) -> TunStack {
        match self.network.tun_stack {
            TunStackIntent::System => TunStack::System,
            TunStackIntent::Gvisor => TunStack::Gvisor,
        }
    }

    pub fn change_plan(&self, candidate: &Self) -> ChangePlan {
        let mut changes = Vec::with_capacity(8);
        let mut impact = ApplyImpact::RuntimeOnly;
        if self.service_enabled != candidate.service_enabled {
            changes.push(ChangeKind::Service);
            impact = if candidate.service_enabled {
                ApplyImpact::GenerationActivation
            } else {
                ApplyImpact::StopDataPlane
            };
        }
        if self.subscriptions != candidate.subscriptions {
            changes.push(ChangeKind::SubscriptionSchedule);
        }
        if self.sources != candidate.sources {
            changes.push(ChangeKind::Sources);
            impact = impact.max(ApplyImpact::GenerationActivation);
        }
        if self.proxy != candidate.proxy {
            changes.push(ChangeKind::Proxy);
            impact = impact.max(ApplyImpact::GenerationActivation);
        }
        if self.applications != candidate.applications {
            changes.push(ChangeKind::Applications);
            let tun_topology = self.network.capture_mode == CaptureIntent::Tun
                || candidate.network.capture_mode == CaptureIntent::Tun;
            impact = impact.max(if tun_topology {
                ApplyImpact::GenerationActivation
            } else {
                ApplyImpact::NetworkPlan
            });
        }
        if self.network != candidate.network {
            changes.push(ChangeKind::Network);
            let core_topology_changed = self.network.capture_mode != candidate.network.capture_mode
                || self.network.tun_stack != candidate.network.tun_stack;
            impact = impact.max(if core_topology_changed {
                ApplyImpact::GenerationActivation
            } else {
                ApplyImpact::NetworkPlan
            });
        }
        if self.routing != candidate.routing {
            changes.push(ChangeKind::Routing);
            impact = impact.max(ApplyImpact::GenerationActivation);
        }
        if self.logging != candidate.logging {
            changes.push(ChangeKind::Logging);
        }
        if self.advanced != candidate.advanced || self.allocations != candidate.allocations {
            changes.push(ChangeKind::Advanced);
            impact = impact.max(ApplyImpact::NetworkPlan);
        }
        ChangePlan { changes, impact }
    }
}

fn build_capture(
    capture_mode: CaptureMode,
    network: &NetworkSettings,
    advanced: &AdvancedSettings,
    include_uids: Vec<u32>,
    exclude_uids: Vec<u32>,
) -> Result<CapturePolicy, ConfigError> {
    let capture = CapturePolicy::new_with_protocols(
        capture_mode,
        network.proxy_tcp,
        network.proxy_udp,
        advanced.ipv6_guard,
        Some(advanced.inbound_port),
        Some(advanced.bypass_mark),
        include_uids,
        exclude_uids,
    )
    .map_err(|_| ConfigError::InvalidApplications)?;
    let interfaces = InterfacePolicy::new(
        network.interfaces.mobile,
        network.interfaces.wifi,
        network.interfaces.include.clone(),
        network.interfaces.exclude.clone(),
    )
    .map_err(|_| ConfigError::InvalidNetwork)?;
    capture
        .with_interface_policy(interfaces)
        .and_then(|capture| {
            capture.with_forwarding_policy(ForwardingPolicy::new(
                network.interfaces.hotspot,
                network.interfaces.usb,
            ))
        })
        .map_err(|_| ConfigError::InvalidNetwork)
}

/// UID 1 is never an application UID on Android. It prevents a whitelist
/// from temporarily degrading to "capture everything" while PackageManager
/// is unavailable; it is removed once runtime resolution succeeds.
pub(crate) const APPLICATION_RESOLUTION_SENTINEL: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyImpact {
    RuntimeOnly,
    NetworkPlan,
    GenerationActivation,
    StopDataPlane,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Service,
    SubscriptionSchedule,
    Sources,
    Proxy,
    Applications,
    Network,
    Routing,
    Logging,
    Advanced,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangePlan {
    changes: Vec<ChangeKind>,
    impact: ApplyImpact,
}

impl ChangePlan {
    pub fn changes(&self) -> &[ChangeKind] {
        &self.changes
    }

    pub const fn impact(&self) -> ApplyImpact {
        self.impact
    }
}

impl fmt::Debug for EffectiveConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EffectiveConfig")
            .field("service_enabled", &self.service_enabled)
            .field("source_count", &self.sources.len())
            .field("proxy", &self.proxy)
            .field("applications", &self.applications)
            .field("network", &self.network)
            .field("routing", &self.routing)
            .field("logging", &self.logging)
            .field("advanced", &self.advanced)
            .field("allocation_count", &self.allocations.len())
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionSettings {
    mode: SubscriptionMode,
    auto_update: bool,
    update_interval_hours: u16,
}

impl SubscriptionSettings {
    pub const fn mode(&self) -> SubscriptionMode {
        self.mode
    }

    pub const fn auto_update(&self) -> bool {
        self.auto_update
    }

    pub const fn update_interval_hours(&self) -> u16 {
        self.update_interval_hours
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionMode {
    #[default]
    Single,
    Merge,
}

#[derive(Clone, PartialEq, Eq)]
pub struct UserSource {
    name: SourceName,
    enabled: bool,
    url: String,
    request_profile: RequestProfile,
    format_hint: SourceFormatHint,
    mirrors: Vec<String>,
    filter: NodeFilter,
}

impl UserSource {
    pub fn name(&self) -> &SourceName {
        &self.name
    }

    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub const fn request_profile(&self) -> RequestProfile {
        self.request_profile
    }

    pub const fn format_hint(&self) -> SourceFormatHint {
        self.format_hint
    }

    pub fn mirrors(&self) -> &[String] {
        &self.mirrors
    }

    pub const fn filter(&self) -> &NodeFilter {
        &self.filter
    }
}

impl fmt::Debug for UserSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UserSource")
            .field("name", &self.name)
            .field("enabled", &self.enabled)
            .field("url", &"[REDACTED]")
            .field("request_profile", &self.request_profile)
            .field("format_hint", &self.format_hint)
            .field("mirror_count", &self.mirrors.len())
            .field(
                "filter_rule_count",
                &(self.filter.include_names().len()
                    + self.filter.exclude_names().len()
                    + self.filter.excluded_node_ids().len()
                    + self.filter.protocols().len()),
            )
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SourceName(String);

impl SourceName {
    pub fn new(value: String) -> Result<Self, ConfigError> {
        if value.is_empty()
            || value.len() > MAX_SOURCE_NAME_BYTES
            || value.chars().count() > MAX_SOURCE_NAME_CHARS
            || value.trim() != value
            || value.chars().any(char::is_control)
        {
            return Err(ConfigError::InvalidSourceName);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SourceName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("SourceName").field(&self.0).finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxySettings {
    outbound_mode: OutboundMode,
    urltest: UrltestSettings,
}

impl ProxySettings {
    pub const fn outbound_mode(&self) -> OutboundMode {
        self.outbound_mode
    }

    pub const fn urltest(&self) -> &UrltestSettings {
        &self.urltest
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrltestSettings {
    interval_minutes: u16,
    tolerance_ms: u16,
    max_candidates: u16,
}

impl UrltestSettings {
    pub const fn interval_minutes(&self) -> u16 {
        self.interval_minutes
    }

    pub const fn tolerance_ms(&self) -> u16 {
        self.tolerance_ms
    }

    pub const fn max_candidates(&self) -> u16 {
        self.max_candidates
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationSettings {
    mode: ApplicationMode,
    targets: Vec<ApplicationTarget>,
}

impl ApplicationSettings {
    pub const fn mode(&self) -> ApplicationMode {
        self.mode
    }

    pub fn targets(&self) -> &[ApplicationTarget] {
        &self.targets
    }

    fn packages(&self) -> impl Iterator<Item = (u32, &str)> {
        self.targets.iter().filter_map(|target| match target {
            ApplicationTarget::Package {
                android_user_id,
                package,
            } => Some((*android_user_id, package.as_str())),
            ApplicationTarget::Uid { .. } => None,
        })
    }

    fn direct_uids(&self) -> impl Iterator<Item = u32> + '_ {
        self.targets.iter().filter_map(|target| match target {
            ApplicationTarget::Uid { uid } => Some(*uid),
            ApplicationTarget::Package { .. } => None,
        })
    }

    fn base_include_uids(&self) -> Vec<u32> {
        match self.mode {
            ApplicationMode::Whitelist => self.direct_uids().collect(),
            ApplicationMode::All | ApplicationMode::Blacklist => Vec::new(),
        }
    }

    fn initial_include_uids(&self) -> Vec<u32> {
        let mut include = self.base_include_uids();
        if self.mode == ApplicationMode::Whitelist && !self.targets.is_empty() && include.is_empty()
        {
            include.push(APPLICATION_RESOLUTION_SENTINEL);
        }
        include
    }

    fn base_exclude_uids(&self) -> Vec<u32> {
        let mut excluded = vec![0];
        if self.mode == ApplicationMode::Blacklist {
            excluded.extend(self.direct_uids());
        }
        excluded
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkSettings {
    capture_mode: CaptureIntent,
    proxy_tcp: bool,
    proxy_udp: bool,
    ipv6_mode: Ipv6Mode,
    dns_mode: DnsMode,
    tun_stack: TunStackIntent,
    interfaces: InterfaceSettings,
    wifi_scenes: WifiSceneSettings,
}

impl NetworkSettings {
    pub const fn capture_mode(&self) -> CaptureIntent {
        self.capture_mode
    }

    pub const fn proxy_tcp(&self) -> bool {
        self.proxy_tcp
    }

    pub const fn proxy_udp(&self) -> bool {
        self.proxy_udp
    }

    pub const fn ipv6_mode(&self) -> Ipv6Mode {
        self.ipv6_mode
    }

    pub const fn dns_mode(&self) -> DnsMode {
        self.dns_mode
    }

    pub const fn tun_stack(&self) -> TunStackIntent {
        self.tun_stack
    }

    pub const fn interfaces(&self) -> &InterfaceSettings {
        &self.interfaces
    }

    pub const fn wifi_scenes(&self) -> &WifiSceneSettings {
        &self.wifi_scenes
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiSceneSettings {
    enabled: bool,
    probe_interval_seconds: u16,
    matcher: WifiSceneMatcher,
}

impl WifiSceneSettings {
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    pub const fn probe_interval_seconds(&self) -> u16 {
        self.probe_interval_seconds
    }

    pub const fn matcher(&self) -> &WifiSceneMatcher {
        &self.matcher
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceSettings {
    mobile: bool,
    wifi: bool,
    hotspot: bool,
    usb: bool,
    include: Vec<String>,
    exclude: Vec<String>,
}

impl InterfaceSettings {
    pub const fn mobile(&self) -> bool {
        self.mobile
    }

    pub const fn wifi(&self) -> bool {
        self.wifi
    }

    pub const fn hotspot(&self) -> bool {
        self.hotspot
    }

    pub const fn usb(&self) -> bool {
        self.usb
    }

    pub fn include(&self) -> &[String] {
        &self.include
    }

    pub fn exclude(&self) -> &[String] {
        &self.exclude
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutingSettings {
    bypass_private: bool,
    bypass_cn: bool,
    block_quic: bool,
    force_proxy_cidrs: Vec<CanonicalCidr>,
    bypass_cidrs: Vec<CanonicalCidr>,
    force_proxy_domains: Vec<String>,
    bypass_domains: Vec<String>,
    block_domains: Vec<String>,
}

impl RoutingSettings {
    pub const fn bypass_private(&self) -> bool {
        self.bypass_private
    }

    pub const fn bypass_cn(&self) -> bool {
        self.bypass_cn
    }

    pub const fn block_quic(&self) -> bool {
        self.block_quic
    }

    pub fn force_proxy_cidrs(&self) -> &[CanonicalCidr] {
        &self.force_proxy_cidrs
    }

    pub fn bypass_cidrs(&self) -> &[CanonicalCidr] {
        &self.bypass_cidrs
    }

    pub fn force_proxy_domains(&self) -> &[String] {
        &self.force_proxy_domains
    }

    pub fn bypass_domains(&self) -> &[String] {
        &self.bypass_domains
    }

    pub fn block_domains(&self) -> &[String] {
        &self.block_domains
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CanonicalCidr {
    text: String,
    network: u128,
    prefix: u8,
    ipv6: bool,
}

impl CanonicalCidr {
    pub fn as_str(&self) -> &str {
        &self.text
    }

    fn overlaps(&self, other: &Self) -> bool {
        if self.ipv6 != other.ipv6 {
            return false;
        }
        let bits = if self.ipv6 { 128 } else { 32 };
        let prefix = self.prefix.min(other.prefix);
        let mask = prefix_mask(bits, prefix);
        self.network & mask == other.network & mask
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoggingSettings {
    level: LogLevel,
    retention_days: u8,
}

impl LoggingSettings {
    pub const fn level(&self) -> LogLevel {
        self.level
    }

    pub const fn retention_days(&self) -> u8 {
        self.retention_days
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvancedSettings {
    inbound_port: u16,
    bypass_mark: u32,
    ipv6_guard: bool,
    dry_run: bool,
    health_timeout_seconds: u8,
    reconcile_interval_seconds: u16,
}

impl AdvancedSettings {
    pub const fn inbound_port(&self) -> u16 {
        self.inbound_port
    }

    pub const fn bypass_mark(&self) -> u32 {
        self.bypass_mark
    }

    pub const fn ipv6_guard(&self) -> bool {
        self.ipv6_guard
    }

    pub const fn dry_run(&self) -> bool {
        self.dry_run
    }

    pub const fn health_timeout_seconds(&self) -> u8 {
        self.health_timeout_seconds
    }

    pub const fn reconcile_interval_seconds(&self) -> u16 {
        self.reconcile_interval_seconds
    }
}

fn validate_subscription_settings(
    wire: &SubscriptionsWire,
    sources: &[UserSource],
) -> Result<SubscriptionSettings, ConfigError> {
    if !(1..=168).contains(&wire.update_interval_hours) {
        return Err(ConfigError::InvalidUpdateSchedule);
    }
    let configured = sources
        .iter()
        .filter(|source| !source.url.is_empty())
        .count();
    let enabled = sources.iter().filter(|source| source.enabled).count();
    let active = sources
        .iter()
        .filter(|source| source.enabled && !source.url.is_empty())
        .count();
    match wire.mode {
        SubscriptionMode::Single if configured == 0 && enabled <= 1 => {}
        SubscriptionMode::Single if enabled > 1 => {
            return Err(ConfigError::SingleSourceNotUnique);
        }
        SubscriptionMode::Single if active != 1 => {
            return Err(ConfigError::NoActiveSource);
        }
        SubscriptionMode::Single => {}
        SubscriptionMode::Merge if active == 0 => {
            return Err(ConfigError::NoActiveSource);
        }
        SubscriptionMode::Merge => {}
    }
    Ok(SubscriptionSettings {
        mode: wire.mode,
        auto_update: wire.auto_update,
        update_interval_hours: wire.update_interval_hours,
    })
}

fn validate_sources(wire: &[SubscriptionSourceWire]) -> Result<Vec<UserSource>, ConfigError> {
    if wire.is_empty() || wire.len() > MAX_SOURCES {
        return Err(ConfigError::InvalidSourceCount);
    }
    let mut names = HashSet::with_capacity(wire.len());
    let mut urls = HashSet::with_capacity(wire.len());
    let mut sources = Vec::with_capacity(wire.len());
    for source in wire {
        let name = SourceName::new(source.name.clone())?;
        if !names.insert(name.as_str().to_owned()) {
            return Err(ConfigError::DuplicateSourceName);
        }
        validate_optional_url(&source.url)?;
        if !source.url.is_empty() && !urls.insert(source.url.clone()) {
            return Err(ConfigError::DuplicateSourceUrl);
        }
        if source.mirrors.len() > MAX_MIRRORS
            || (source.url.is_empty() && !source.mirrors.is_empty())
        {
            return Err(ConfigError::InvalidSourceOptions);
        }
        let mut mirror_set = HashSet::with_capacity(source.mirrors.len());
        for mirror in &source.mirrors {
            validate_optional_url(mirror)?;
            if mirror.is_empty() || mirror == &source.url || !mirror_set.insert(mirror.clone()) {
                return Err(ConfigError::InvalidSourceOptions);
            }
        }
        sources.push(UserSource {
            name,
            enabled: source.enabled,
            url: source.url.clone(),
            request_profile: source.request_profile.effective(),
            format_hint: source.format_hint,
            mirrors: source.mirrors.clone(),
            filter: NodeFilter::new_with_node_ids(
                source.filter.include_names.clone(),
                source.filter.exclude_names.clone(),
                source.filter.excluded_node_ids.clone(),
                source.filter.protocols.clone(),
            )
            .map_err(|_| ConfigError::InvalidSourceOptions)?,
        });
    }
    Ok(sources)
}

fn validate_optional_url(url: &str) -> Result<(), ConfigError> {
    if url.is_empty() {
        return Ok(());
    }
    let parsed = Url::parse(url).map_err(|_| ConfigError::InvalidSourceUrl)?;
    if parsed.scheme() != "https" {
        return Err(ConfigError::SourceUrlNonHttps);
    }
    if !parsed.username().is_empty() || parsed.password().is_some() || parsed.host_str().is_none() {
        return Err(ConfigError::InvalidSourceUrl);
    }
    Ok(())
}

fn validate_proxy(wire: ProxyWire) -> Result<ProxySettings, ConfigError> {
    if !(5..=1440).contains(&wire.urltest.interval_minutes)
        || wire.urltest.tolerance_ms > 1000
        || !(1..=MAX_AUTO_CANDIDATES).contains(&wire.urltest.max_candidates)
    {
        return Err(ConfigError::InvalidProxy);
    }
    Ok(ProxySettings {
        outbound_mode: wire.outbound_mode,
        urltest: UrltestSettings {
            interval_minutes: wire.urltest.interval_minutes,
            tolerance_ms: wire.urltest.tolerance_ms,
            max_candidates: wire.urltest.max_candidates,
        },
    })
}

fn validate_applications(wire: ApplicationsWire) -> Result<ApplicationSettings, ConfigError> {
    if wire.targets.len() > MAX_APPLICATION_TARGETS
        || !all_unique(&wire.targets)
        || matches!(wire.mode, ApplicationMode::All) != wire.targets.is_empty()
        || wire.targets.iter().any(|target| match target {
            ApplicationTarget::Package {
                android_user_id,
                package,
            } => *android_user_id > 21_474 || !valid_package(package),
            ApplicationTarget::Uid { uid } => *uid == 0,
        })
    {
        return Err(ConfigError::InvalidApplications);
    }
    let mut targets = wire.targets;
    targets.sort();
    Ok(ApplicationSettings {
        mode: wire.mode,
        targets,
    })
}

fn validate_network(wire: NetworkWire) -> Result<NetworkSettings, ConfigError> {
    if !wire.proxy_tcp && !wire.proxy_udp {
        return Err(ConfigError::InvalidNetwork);
    }
    if wire.ipv6_mode != Ipv6Mode::Auto || wire.dns_mode != DnsMode::Auto {
        return Err(ConfigError::UnsupportedNetwork);
    }
    if wire.capture_mode == CaptureIntent::Tun
        && (!wire.proxy_tcp
            || !wire.proxy_udp
            || !wire.interfaces.mobile
            || !wire.interfaces.wifi
            || wire.interfaces.hotspot
            || wire.interfaces.usb
            || !wire.interfaces.include.is_empty()
            || !wire.interfaces.exclude.is_empty())
    {
        return Err(ConfigError::UnsupportedNetwork);
    }
    validate_patterns(&wire.interfaces.include)?;
    validate_patterns(&wire.interfaces.exclude)?;
    if !wire.interfaces.mobile && !wire.interfaces.wifi && wire.interfaces.include.is_empty() {
        return Err(ConfigError::InvalidNetwork);
    }
    if wire
        .interfaces
        .include
        .iter()
        .any(|value| wire.interfaces.exclude.contains(value))
    {
        return Err(ConfigError::InvalidNetwork);
    }
    if !(15..=3600).contains(&wire.wifi_scenes.probe_interval_seconds)
        || (wire.wifi_scenes.enabled && wire.wifi_scenes.rules.is_empty())
    {
        return Err(ConfigError::InvalidNetwork);
    }
    let mut wifi_rules = Vec::with_capacity(wire.wifi_scenes.rules.len());
    for rule in wire.wifi_scenes.rules {
        wifi_rules.push(
            WifiSceneRule::new(
                rule.id,
                rule.ssid,
                rule.bssid,
                match rule.action {
                    WifiSceneActionWire::EnableProxy => WifiSceneAction::EnableProxy,
                    WifiSceneActionWire::DisableProxy => WifiSceneAction::DisableProxy,
                },
            )
            .map_err(|_| ConfigError::InvalidNetwork)?,
        );
    }
    let wifi_scenes = WifiSceneSettings {
        enabled: wire.wifi_scenes.enabled,
        probe_interval_seconds: wire.wifi_scenes.probe_interval_seconds,
        matcher: WifiSceneMatcher::new(wifi_rules).map_err(|_| ConfigError::InvalidNetwork)?,
    };
    Ok(NetworkSettings {
        capture_mode: wire.capture_mode,
        proxy_tcp: wire.proxy_tcp,
        proxy_udp: wire.proxy_udp,
        ipv6_mode: wire.ipv6_mode,
        dns_mode: wire.dns_mode,
        tun_stack: wire.tun_stack,
        interfaces: InterfaceSettings {
            mobile: wire.interfaces.mobile,
            wifi: wire.interfaces.wifi,
            hotspot: wire.interfaces.hotspot,
            usb: wire.interfaces.usb,
            include: wire.interfaces.include,
            exclude: wire.interfaces.exclude,
        },
        wifi_scenes,
    })
}

fn validate_patterns(patterns: &[String]) -> Result<(), ConfigError> {
    if patterns.len() > MAX_INTERFACES || !all_unique(patterns) {
        return Err(ConfigError::InvalidNetwork);
    }
    if patterns.iter().any(|pattern| {
        pattern.is_empty()
            || pattern.len() > MAX_INTERFACE_PATTERN_BYTES
            || !pattern.bytes().all(|byte| {
                byte.is_ascii_alphanumeric()
                    || matches!(byte, b'_' | b'-' | b'.' | b'+' | b'*' | b'?')
            })
    }) {
        return Err(ConfigError::InvalidNetwork);
    }
    Ok(())
}

fn validate_routing(wire: RoutingWire) -> Result<RoutingSettings, ConfigError> {
    if wire.block_quic {
        return Err(ConfigError::UnsupportedRouting);
    }
    if wire.force_proxy_cidrs.len() > MAX_CIDRS || wire.bypass_cidrs.len() > MAX_CIDRS {
        return Err(ConfigError::InvalidRouting);
    }
    let force_proxy_cidrs = parse_cidrs(wire.force_proxy_cidrs)?;
    let bypass_cidrs = parse_cidrs(wire.bypass_cidrs)?;
    if force_proxy_cidrs
        .iter()
        .any(|force| bypass_cidrs.iter().any(|bypass| force.overlaps(bypass)))
    {
        return Err(ConfigError::InvalidRouting);
    }
    if wire.force_proxy_domains.len() > MAX_DOMAINS
        || wire.bypass_domains.len() > MAX_DOMAINS
        || wire.block_domains.len() > MAX_DOMAINS
    {
        return Err(ConfigError::InvalidRouting);
    }
    let force_proxy_domains = parse_domains(wire.force_proxy_domains)?;
    let bypass_domains = parse_domains(wire.bypass_domains)?;
    let block_domains = parse_domains(wire.block_domains)?;
    if domain_sets_overlap(&force_proxy_domains, &bypass_domains)
        || domain_sets_overlap(&force_proxy_domains, &block_domains)
        || domain_sets_overlap(&bypass_domains, &block_domains)
    {
        return Err(ConfigError::InvalidRouting);
    }
    Ok(RoutingSettings {
        bypass_private: wire.bypass_private,
        bypass_cn: wire.bypass_cn,
        block_quic: wire.block_quic,
        force_proxy_cidrs,
        bypass_cidrs,
        force_proxy_domains,
        bypass_domains,
        block_domains,
    })
}

fn parse_domains(values: Vec<String>) -> Result<Vec<String>, ConfigError> {
    let mut parsed = Vec::with_capacity(values.len());
    let mut seen = HashSet::with_capacity(values.len());
    for value in values {
        if value.is_empty()
            || value.len() > MAX_DOMAIN_BYTES
            || value != value.trim()
            || IpAddr::from_str(&value).is_ok()
        {
            return Err(ConfigError::InvalidRouting);
        }
        let value = value.to_ascii_lowercase();
        if !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        }) || !value.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .bytes()
                    .next()
                    .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
                && label
                    .bytes()
                    .last()
                    .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        }) || !seen.insert(value.clone())
        {
            return Err(ConfigError::InvalidRouting);
        }
        parsed.push(value);
    }
    parsed.sort();
    Ok(parsed)
}

fn domain_sets_overlap(left: &[String], right: &[String]) -> bool {
    left.iter().any(|left| {
        right
            .iter()
            .any(|right| domain_is_suffix_of(left, right) || domain_is_suffix_of(right, left))
    })
}

fn domain_is_suffix_of(value: &str, suffix: &str) -> bool {
    value == suffix
        || value
            .strip_suffix(suffix)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

fn parse_cidrs(values: Vec<String>) -> Result<Vec<CanonicalCidr>, ConfigError> {
    let mut parsed = Vec::with_capacity(values.len());
    let mut seen = HashSet::with_capacity(values.len());
    for value in values {
        let cidr = parse_cidr(&value)?;
        if !seen.insert(cidr.clone()) {
            return Err(ConfigError::InvalidRouting);
        }
        parsed.push(cidr);
    }
    parsed.sort_by(|left, right| left.text.cmp(&right.text));
    Ok(parsed)
}

fn parse_cidr(value: &str) -> Result<CanonicalCidr, ConfigError> {
    let (address, prefix) = value.split_once('/').ok_or(ConfigError::InvalidRouting)?;
    if address.is_empty() || prefix.is_empty() || prefix.contains('/') {
        return Err(ConfigError::InvalidRouting);
    }
    let address = IpAddr::from_str(address).map_err(|_| ConfigError::InvalidRouting)?;
    let prefix = prefix
        .parse::<u8>()
        .map_err(|_| ConfigError::InvalidRouting)?;
    match address {
        IpAddr::V4(address) if prefix <= 32 => {
            let network = u32::from(address) & prefix_mask(32, prefix) as u32;
            Ok(CanonicalCidr {
                text: format!("{}/{}", Ipv4Addr::from(network), prefix),
                network: u128::from(network),
                prefix,
                ipv6: false,
            })
        }
        IpAddr::V6(address) if prefix <= 128 => {
            let network = u128::from(address) & prefix_mask(128, prefix);
            Ok(CanonicalCidr {
                text: format!("{}/{}", Ipv6Addr::from(network), prefix),
                network,
                prefix,
                ipv6: true,
            })
        }
        _ => Err(ConfigError::InvalidRouting),
    }
}

fn validate_logging(wire: LoggingWire) -> Result<LoggingSettings, ConfigError> {
    if !(1..=30).contains(&wire.retention_days) {
        return Err(ConfigError::InvalidLogging);
    }
    Ok(LoggingSettings {
        level: wire.level,
        retention_days: wire.retention_days,
    })
}

fn validate_advanced(
    wire: AdvancedWire,
) -> Result<(AdvancedSettings, Vec<ResourceCandidate>), ConfigError> {
    if wire.inbound_port == 0
        || wire.bypass_mark == 0
        || !(1..=30).contains(&wire.health_timeout_seconds)
        || !(60..=3600).contains(&wire.reconcile_interval_seconds)
        || wire.resource_candidates.is_empty()
        || wire.resource_candidates.len() > 16
        || !all_unique(&wire.resource_candidates)
    {
        return Err(ConfigError::InvalidAdvanced);
    }
    let mut allocations = Vec::with_capacity(wire.resource_candidates.len());
    for candidate in &wire.resource_candidates {
        let allocation = ResourceCandidate::new(
            candidate.mark,
            candidate.mask,
            candidate.route_table,
            candidate.rule_priority,
        )
        .ok_or(ConfigError::InvalidAdvanced)?;
        if allocation.mark() & wire.bypass_mark != 0 {
            return Err(ConfigError::InvalidAdvanced);
        }
        allocations.push(allocation);
    }
    Ok((
        AdvancedSettings {
            inbound_port: wire.inbound_port,
            bypass_mark: wire.bypass_mark,
            ipv6_guard: wire.ipv6_guard,
            dry_run: wire.dry_run,
            health_timeout_seconds: wire.health_timeout_seconds,
            reconcile_interval_seconds: wire.reconcile_interval_seconds,
        },
        allocations,
    ))
}

fn valid_package(value: &str) -> bool {
    value.len() <= 255
        && value.contains('.')
        && value.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
}

fn all_unique<T: Eq + std::hash::Hash>(values: &[T]) -> bool {
    let mut seen = HashSet::with_capacity(values.len());
    values.iter().all(|value| seen.insert(value))
}

const fn prefix_mask(bits: u8, prefix: u8) -> u128 {
    if prefix == 0 {
        0
    } else if bits == 128 {
        u128::MAX << (128 - prefix)
    } else {
        (u32::MAX << (32 - prefix)) as u128
    }
}

const fn default_true() -> bool {
    true
}
const fn default_update_interval_hours() -> u16 {
    24
}
const fn default_urltest_interval() -> u16 {
    10
}
const fn default_urltest_tolerance() -> u16 {
    50
}
const fn default_urltest_candidates() -> u16 {
    64
}
const fn default_retention_days() -> u8 {
    7
}
const fn default_inbound_port() -> u16 {
    DEFAULT_INBOUND_PORT
}
const fn default_bypass_mark() -> u32 {
    DEFAULT_BYPASS_MARK
}
const fn default_health_timeout() -> u8 {
    3
}
const fn default_reconcile_interval() -> u16 {
    60
}

const fn default_wifi_scene_probe_interval() -> u16 {
    30
}

fn default_resource_candidates() -> Vec<ResourceCandidateWire> {
    [
        (1_313_407_232, 0xffff_ffff, 100, 12_000),
        (1_313_407_488, 0xffff_ffff, 101, 12_010),
        (1_313_407_744, 0xffff_ffff, 102, 12_020),
    ]
    .into_iter()
    .map(
        |(mark, mask, route_table, rule_priority)| ResourceCandidateWire {
            mark,
            mask,
            route_table,
            rule_priority,
        },
    )
    .collect()
}
