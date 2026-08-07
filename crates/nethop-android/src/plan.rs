use nethop_core::{CaptureMode, CapturePolicy, ForwardingPolicy, GenerationId, InterfacePolicy};
use thiserror::Error;

use crate::capability::{CapabilityReport, CapabilityStatus, IpFamily, ResourceCandidate};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanSlot {
    A,
    B,
}

impl PlanSlot {
    const fn suffix(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkOperationKind {
    Ipv6GuardRestore,
    DnsGuardRestore,
    ForwardingRestore,
    PolicyRouteAdd,
    PolicyRuleAdd,
    NetfilterRestore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkPlan {
    generation: GenerationId,
    slot: PlanSlot,
    allocation: ResourceCandidate,
    bypass_mark: u32,
    ipv6_captured: bool,
    ipv6_guarded: bool,
    dns_guarded: bool,
    forwarding_interfaces: Vec<String>,
    steps: Vec<PlanStep>,
}

impl NetworkPlan {
    pub const fn generation(&self) -> GenerationId {
        self.generation
    }

    pub const fn slot(&self) -> PlanSlot {
        self.slot
    }

    pub const fn slot_suffix(&self) -> &'static str {
        self.slot.suffix()
    }

    pub const fn allocation(&self) -> ResourceCandidate {
        self.allocation
    }

    pub(crate) const fn bypass_mark(&self) -> u32 {
        self.bypass_mark
    }

    pub const fn ipv6_guarded(&self) -> bool {
        self.ipv6_guarded
    }

    pub const fn ipv6_captured(&self) -> bool {
        self.ipv6_captured
    }

    pub const fn dns_guarded(&self) -> bool {
        self.dns_guarded
    }

    pub fn forwarding_interfaces(&self) -> &[String] {
        &self.forwarding_interfaces
    }

    pub fn forwarding_owner_marker(&self) -> String {
        format!("nethop:fwd:g={}", self.generation.get())
    }

    pub fn owner_marker(&self) -> String {
        format!("nethop:g={}", self.generation.get())
    }

    pub fn operation_kinds(&self) -> impl ExactSizeIterator<Item = NetworkOperationKind> + '_ {
        self.steps.iter().map(|step| step.kind)
    }

    pub fn restore_payloads(&self) -> impl Iterator<Item = (IpFamily, &str)> {
        self.steps.iter().filter_map(|step| match &step.apply {
            NetworkOperation::Restore { family, payload } => Some((*family, payload.as_str())),
            _ => None,
        })
    }

    pub(crate) fn steps(&self) -> &[PlanStep] {
        &self.steps
    }

    pub(crate) fn entry_chain(&self, family: IpFamily) -> String {
        if family == IpFamily::Ipv6 && self.ipv6_guarded {
            format!("NH_V6G_{}", self.slot.suffix())
        } else {
            format!("NH_OUT_{}", self.slot.suffix())
        }
    }

    pub(crate) fn prerouting_chain(&self) -> String {
        format!("NH_PRE_{}", self.slot.suffix())
    }

    pub(crate) fn dns_guard_chain(&self) -> String {
        format!("NH_DNS_{}", self.slot.suffix())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlanStep {
    pub(crate) kind: NetworkOperationKind,
    pub(crate) apply: NetworkOperation,
    pub(crate) rollback: NetworkOperation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MutationAction {
    Add,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NetworkOperation {
    Restore {
        family: IpFamily,
        payload: String,
    },
    PolicyRoute {
        action: MutationAction,
        family: IpFamily,
        table: u32,
    },
    PolicyRule {
        action: MutationAction,
        family: IpFamily,
        mark: u32,
        mask: u32,
        table: u32,
        priority: u32,
    },
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NetworkPlanner;

impl NetworkPlanner {
    pub fn build_tproxy(
        &self,
        generation: GenerationId,
        slot: PlanSlot,
        policy: &CapturePolicy,
        capabilities: &CapabilityReport,
    ) -> Result<NetworkPlan, NetworkPlanError> {
        if generation.get() == 0 {
            return Err(NetworkPlanError::InvalidGeneration);
        }
        if policy.mode() != CaptureMode::Tproxy {
            return Err(NetworkPlanError::UnsupportedCaptureMode);
        }
        if !capabilities.android().is_supported() || !capabilities.root().is_supported() {
            return Err(NetworkPlanError::PlatformUnavailable);
        }
        if capabilities.active_tunnel() != CapabilityStatus::Supported {
            return Err(NetworkPlanError::ExistingTunnelConflict);
        }
        let inbound_port = policy
            .inbound_port()
            .filter(|port| *port == capabilities.inbound_port())
            .ok_or(NetworkPlanError::InboundPortMismatch)?;
        if capabilities.inbound_port_status() != CapabilityStatus::Supported {
            return Err(NetworkPlanError::InboundPortConflict);
        }
        if !capabilities.ipv4().supports_tproxy() {
            return Err(NetworkPlanError::Ipv4TproxyUnavailable);
        }
        let allocation = capabilities
            .allocations()
            .iter()
            .find(|allocation| allocation.status().is_supported())
            .map(|allocation| allocation.candidate())
            .ok_or(NetworkPlanError::ResourceConflict)?;
        let bypass_mark = policy
            .bypass_mark()
            .ok_or(NetworkPlanError::BypassMarkConflict)?;
        if bypass_mark & allocation.mask() == allocation.mark() {
            return Err(NetworkPlanError::BypassMarkConflict);
        }
        let interfaces = resolve_interfaces(policy.interface_policy(), capabilities.interfaces())?;
        let forwarding_interfaces =
            resolve_forwarding_interfaces(policy.forwarding_policy(), capabilities.interfaces())?;

        let ipv6_tproxy = capabilities.ipv6().supports_tproxy();
        let ipv6_present = match capabilities.ipv6().address() {
            CapabilityStatus::Supported => true,
            CapabilityStatus::NotPresent => false,
            _ => return Err(NetworkPlanError::Ipv6LeakRisk),
        };
        let ipv6_guarded = ipv6_present && !ipv6_tproxy;
        if ipv6_guarded && (!policy.ipv6_guard() || !capabilities.ipv6().supports_guard()) {
            return Err(NetworkPlanError::Ipv6LeakRisk);
        }

        let owner = format!("nethop:g={}", generation.get());
        let mut steps = Vec::with_capacity(if ipv6_tproxy { 8 } else { 5 });

        if ipv6_guarded {
            let (apply, rollback) = ipv6_guard_payloads(slot, policy, &owner);
            steps.push(restore_step(
                NetworkOperationKind::Ipv6GuardRestore,
                IpFamily::Ipv6,
                apply,
                rollback,
            ));
        }

        push_policy_steps(&mut steps, IpFamily::Ipv4, allocation);
        if ipv6_tproxy {
            push_policy_steps(&mut steps, IpFamily::Ipv6, allocation);
        }

        let (apply4, rollback4) = tproxy_payloads(
            IpFamily::Ipv4,
            slot,
            policy,
            allocation,
            inbound_port,
            &owner,
            interfaces.as_deref(),
        );
        steps.push(restore_step(
            NetworkOperationKind::NetfilterRestore,
            IpFamily::Ipv4,
            apply4,
            rollback4,
        ));
        let (dns_apply4, dns_rollback4) = dns_guard_payloads(
            IpFamily::Ipv4,
            slot,
            policy,
            allocation,
            &owner,
            interfaces.as_deref(),
        );
        steps.push(restore_step(
            NetworkOperationKind::DnsGuardRestore,
            IpFamily::Ipv4,
            dns_apply4,
            dns_rollback4,
        ));
        if !forwarding_interfaces.is_empty() {
            let (apply, rollback) = forwarding_payloads(
                slot,
                policy,
                allocation,
                &forwarding_interfaces,
                &format!("nethop:fwd:g={}", generation.get()),
            );
            steps.push(restore_step(
                NetworkOperationKind::ForwardingRestore,
                IpFamily::Ipv4,
                apply,
                rollback,
            ));
            let (apply6, rollback6) = forwarding_ipv6_guard_payloads(
                slot,
                &forwarding_interfaces,
                &format!("nethop:fwd:g={}", generation.get()),
            );
            steps.push(restore_step(
                NetworkOperationKind::ForwardingRestore,
                IpFamily::Ipv6,
                apply6,
                rollback6,
            ));
        }
        if ipv6_tproxy {
            let (apply6, rollback6) = tproxy_payloads(
                IpFamily::Ipv6,
                slot,
                policy,
                allocation,
                inbound_port,
                &owner,
                interfaces.as_deref(),
            );
            steps.push(restore_step(
                NetworkOperationKind::NetfilterRestore,
                IpFamily::Ipv6,
                apply6,
                rollback6,
            ));
            let (dns_apply6, dns_rollback6) = dns_guard_payloads(
                IpFamily::Ipv6,
                slot,
                policy,
                allocation,
                &owner,
                interfaces.as_deref(),
            );
            steps.push(restore_step(
                NetworkOperationKind::DnsGuardRestore,
                IpFamily::Ipv6,
                dns_apply6,
                dns_rollback6,
            ));
        }

        Ok(NetworkPlan {
            generation,
            slot,
            allocation,
            bypass_mark,
            ipv6_captured: ipv6_tproxy,
            ipv6_guarded,
            dns_guarded: true,
            forwarding_interfaces,
            steps,
        })
    }
}

fn dns_guard_payloads(
    _family: IpFamily,
    slot: PlanSlot,
    policy: &CapturePolicy,
    allocation: ResourceCandidate,
    owner: &str,
    interfaces: Option<&[String]>,
) -> (String, String) {
    let chain = format!("NH_DNS_{}", slot.suffix());
    let mut apply = vec![
        "*filter".to_owned(),
        format!("-N {chain}"),
        format!("-A OUTPUT -m comment --comment {owner} -j {chain}"),
        format!("-A {chain} -o lo -j RETURN"),
        format!(
            "-A {chain} -m mark --mark 0x{:x}/0x{:x} -j RETURN",
            allocation.mark(),
            allocation.mask()
        ),
        format!(
            "-A {chain} -m mark --mark 0x{:x}/0xffffffff -j RETURN",
            policy.bypass_mark().expect("validated TPROXY policy")
        ),
    ];
    let mut append_drop = |protocol: &str, port: u16| {
        if let Some(interfaces) = interfaces {
            for interface in interfaces {
                apply.push(format!(
                    "-A {chain} -o {interface} -p {protocol} --dport {port} -j DROP"
                ));
            }
        } else {
            apply.push(format!("-A {chain} -p {protocol} --dport {port} -j DROP"));
        }
    };
    append_drop("udp", 53);
    append_drop("tcp", 53);
    append_drop("tcp", 853);
    apply.extend([format!("-A {chain} -j RETURN"), "COMMIT".to_owned()]);

    let rollback = [
        "*filter".to_owned(),
        format!("-D OUTPUT -m comment --comment {owner} -j {chain}"),
        format!("-F {chain}"),
        format!("-X {chain}"),
        "COMMIT".to_owned(),
    ]
    .join("\n")
        + "\n";
    (apply.join("\n") + "\n", rollback)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanDiagnosticCode {
    InvalidGeneration,
    UnsupportedCaptureMode,
    PlatformUnavailable,
    InboundPortMismatch,
    InboundPortConflict,
    ExistingTunnelConflict,
    Ipv4TproxyUnavailable,
    Ipv6LeakRisk,
    ResourceConflict,
    BypassMarkConflict,
    InterfaceSelectionEmpty,
    ForwardingInterfaceSelectionEmpty,
}

impl PlanDiagnosticCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidGeneration => "network_plan_invalid_generation",
            Self::UnsupportedCaptureMode => "network_plan_unsupported_capture_mode",
            Self::PlatformUnavailable => "network_plan_platform_unavailable",
            Self::InboundPortMismatch => "network_plan_inbound_port_mismatch",
            Self::InboundPortConflict => "network_plan_inbound_port_conflict",
            Self::ExistingTunnelConflict => "network_plan_existing_tunnel_conflict",
            Self::Ipv4TproxyUnavailable => "network_plan_ipv4_tproxy_unavailable",
            Self::Ipv6LeakRisk => "network_plan_ipv6_leak_risk",
            Self::ResourceConflict => "network_plan_resource_conflict",
            Self::BypassMarkConflict => "network_plan_bypass_mark_conflict",
            Self::InterfaceSelectionEmpty => "network_plan_interface_selection_empty",
            Self::ForwardingInterfaceSelectionEmpty => {
                "network_plan_forwarding_interface_selection_empty"
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum NetworkPlanError {
    #[error("network plan generation must be non-zero")]
    InvalidGeneration,
    #[error("network planner only accepts TPROXY capture policy")]
    UnsupportedCaptureMode,
    #[error("Android root platform capability is unavailable")]
    PlatformUnavailable,
    #[error("capture and capability inbound ports do not match")]
    InboundPortMismatch,
    #[error("TPROXY inbound port is already occupied")]
    InboundPortConflict,
    #[error("an active TUN, WireGuard or PPP interface conflicts with transparent capture")]
    ExistingTunnelConflict,
    #[error("IPv4 TPROXY capability is unavailable")]
    Ipv4TproxyUnavailable,
    #[error("IPv6 cannot be proxied or guarded without leaking")]
    Ipv6LeakRisk,
    #[error("no conflict-free mark, route table and rule priority are available")]
    ResourceConflict,
    #[error("capture mark conflicts with the core bypass mark")]
    BypassMarkConflict,
    #[error("configured interface scope matches no safe Android interface")]
    InterfaceSelectionEmpty,
    #[error("requested hotspot or USB forwarding matches no safe Android interface")]
    ForwardingInterfaceSelectionEmpty,
}

impl NetworkPlanError {
    pub const fn code(self) -> PlanDiagnosticCode {
        match self {
            Self::InvalidGeneration => PlanDiagnosticCode::InvalidGeneration,
            Self::UnsupportedCaptureMode => PlanDiagnosticCode::UnsupportedCaptureMode,
            Self::PlatformUnavailable => PlanDiagnosticCode::PlatformUnavailable,
            Self::InboundPortMismatch => PlanDiagnosticCode::InboundPortMismatch,
            Self::InboundPortConflict => PlanDiagnosticCode::InboundPortConflict,
            Self::ExistingTunnelConflict => PlanDiagnosticCode::ExistingTunnelConflict,
            Self::Ipv4TproxyUnavailable => PlanDiagnosticCode::Ipv4TproxyUnavailable,
            Self::Ipv6LeakRisk => PlanDiagnosticCode::Ipv6LeakRisk,
            Self::ResourceConflict => PlanDiagnosticCode::ResourceConflict,
            Self::BypassMarkConflict => PlanDiagnosticCode::BypassMarkConflict,
            Self::InterfaceSelectionEmpty => PlanDiagnosticCode::InterfaceSelectionEmpty,
            Self::ForwardingInterfaceSelectionEmpty => {
                PlanDiagnosticCode::ForwardingInterfaceSelectionEmpty
            }
        }
    }
}

fn push_policy_steps(steps: &mut Vec<PlanStep>, family: IpFamily, allocation: ResourceCandidate) {
    steps.push(PlanStep {
        kind: NetworkOperationKind::PolicyRouteAdd,
        apply: NetworkOperation::PolicyRoute {
            action: MutationAction::Add,
            family,
            table: allocation.route_table(),
        },
        rollback: NetworkOperation::PolicyRoute {
            action: MutationAction::Delete,
            family,
            table: allocation.route_table(),
        },
    });
    steps.push(PlanStep {
        kind: NetworkOperationKind::PolicyRuleAdd,
        apply: NetworkOperation::PolicyRule {
            action: MutationAction::Add,
            family,
            mark: allocation.mark(),
            mask: allocation.mask(),
            table: allocation.route_table(),
            priority: allocation.rule_priority(),
        },
        rollback: NetworkOperation::PolicyRule {
            action: MutationAction::Delete,
            family,
            mark: allocation.mark(),
            mask: allocation.mask(),
            table: allocation.route_table(),
            priority: allocation.rule_priority(),
        },
    });
}

pub(crate) fn restore_step(
    kind: NetworkOperationKind,
    family: IpFamily,
    apply: String,
    rollback: String,
) -> PlanStep {
    PlanStep {
        kind,
        apply: NetworkOperation::Restore {
            family,
            payload: apply,
        },
        rollback: NetworkOperation::Restore {
            family,
            payload: rollback,
        },
    }
}

fn tproxy_payloads(
    family: IpFamily,
    slot: PlanSlot,
    policy: &CapturePolicy,
    allocation: ResourceCandidate,
    inbound_port: u16,
    owner: &str,
    interfaces: Option<&[String]>,
) -> (String, String) {
    let suffix = slot.suffix();
    let output_chain = format!("NH_OUT_{suffix}");
    let prerouting_chain = format!("NH_PRE_{suffix}");
    let divert_chain = format!("NH_DIV_{suffix}");
    let mut apply = vec![
        "*mangle".to_owned(),
        format!("-N {output_chain}"),
        format!("-N {prerouting_chain}"),
        format!("-N {divert_chain}"),
        format!("-A OUTPUT -m comment --comment {owner} -j {output_chain}"),
        format!("-A PREROUTING -m comment --comment {owner} -j {prerouting_chain}"),
        format!(
            "-A {output_chain} -m mark --mark 0x{:x}/0x{:x} -j RETURN",
            policy.bypass_mark().expect("validated TPROXY policy"),
            allocation.mask()
        ),
        format!("-A {output_chain} -m conntrack --ctdir REPLY -j ACCEPT"),
        format!("-A {prerouting_chain} -m conntrack --ctdir REPLY -j ACCEPT"),
    ];
    apply.extend([
        format!(
            "-A {output_chain} -p tcp --dport 53 -j MARK --set-xmark 0x{:x}/0x{:x}",
            allocation.mark(),
            allocation.mask()
        ),
        format!(
            "-A {output_chain} -p udp --dport 53 -j MARK --set-xmark 0x{:x}/0x{:x}",
            allocation.mark(),
            allocation.mask()
        ),
    ]);
    // Android's netd emits app DNS queries as UID 0. Capture DNS before the
    // core UID bypass; sing-box's own upstream connections use non-DNS ports.
    append_output_uid_bypasses(&mut apply, &output_chain, policy);
    for destination in reserved_destinations(family) {
        apply.push(format!("-A {output_chain} -d {destination} -j RETURN"));
    }
    append_output_capture_rules(&mut apply, &output_chain, policy, allocation, interfaces);
    apply.extend([
        format!(
            "-A {divert_chain} -j MARK --set-xmark 0x{:x}/0x{:x}",
            allocation.mark(),
            allocation.mask()
        ),
        format!("-A {divert_chain} -j ACCEPT"),
    ]);
    if policy.proxy_tcp() {
        apply.push(format!(
            "-A {prerouting_chain} -p tcp -m socket --transparent -j {divert_chain}"
        ));
    }
    apply.push(format!(
        "-A {prerouting_chain} -i lo -m mark ! --mark 0x{:x}/0x{:x} -j RETURN",
        allocation.mark(),
        allocation.mask()
    ));
    apply.push(format!(
        "-A {prerouting_chain} -p tcp --dport 53 -m mark --mark 0x{:x}/0x{:x} -j TPROXY --on-port {inbound_port} --tproxy-mark 0x{:x}/0x{:x}",
        allocation.mark(),
        allocation.mask(),
        allocation.mark(),
        allocation.mask()
    ));
    apply.push(format!(
        "-A {prerouting_chain} -p udp --dport 53 -m mark --mark 0x{:x}/0x{:x} -j TPROXY --on-port {inbound_port} --tproxy-mark 0x{:x}/0x{:x}",
        allocation.mark(),
        allocation.mask(),
        allocation.mark(),
        allocation.mask()
    ));
    if policy.proxy_tcp() {
        apply.push(format!(
            "-A {prerouting_chain} -p tcp -m mark --mark 0x{:x}/0x{:x} -j TPROXY --on-port {inbound_port} --tproxy-mark 0x{:x}/0x{:x}",
            allocation.mark(),
            allocation.mask(),
            allocation.mark(),
            allocation.mask()
        ));
    }
    if policy.proxy_udp() {
        apply.push(format!(
            "-A {prerouting_chain} -p udp -m mark --mark 0x{:x}/0x{:x} -j TPROXY --on-port {inbound_port} --tproxy-mark 0x{:x}/0x{:x}",
            allocation.mark(),
            allocation.mask(),
            allocation.mark(),
            allocation.mask()
        ));
    }
    apply.push("COMMIT".to_owned());

    let rollback = [
        "*mangle".to_owned(),
        format!("-D OUTPUT -m comment --comment {owner} -j {output_chain}"),
        format!("-D PREROUTING -m comment --comment {owner} -j {prerouting_chain}"),
        format!("-F {output_chain}"),
        format!("-F {prerouting_chain}"),
        format!("-F {divert_chain}"),
        format!("-X {output_chain}"),
        format!("-X {prerouting_chain}"),
        format!("-X {divert_chain}"),
        "COMMIT".to_owned(),
    ]
    .join("\n")
        + "\n";
    (apply.join("\n") + "\n", rollback)
}

fn append_output_uid_bypasses(rules: &mut Vec<String>, chain: &str, policy: &CapturePolicy) {
    for uid in policy.exclude_uids() {
        rules.push(format!("-A {chain} -m owner --uid-owner {uid} -j RETURN"));
    }
}

fn append_output_capture_rules(
    rules: &mut Vec<String>,
    chain: &str,
    policy: &CapturePolicy,
    allocation: ResourceCandidate,
    interfaces: Option<&[String]>,
) {
    if let Some(interfaces) = interfaces {
        for interface in interfaces {
            if policy.include_uids().is_empty() {
                rules.push(format!(
                    "-A {chain} -o {interface} -j MARK --set-xmark 0x{:x}/0x{:x}",
                    allocation.mark(),
                    allocation.mask()
                ));
            } else {
                for uid in policy.include_uids() {
                    rules.push(format!(
                        "-A {chain} -o {interface} -m owner --uid-owner {uid} -j MARK --set-xmark 0x{:x}/0x{:x}",
                        allocation.mark(),
                        allocation.mask()
                    ));
                }
            }
        }
    } else if policy.include_uids().is_empty() {
        rules.push(format!(
            "-A {chain} -j MARK --set-xmark 0x{:x}/0x{:x}",
            allocation.mark(),
            allocation.mask()
        ));
    } else {
        for uid in policy.include_uids() {
            rules.push(format!(
                "-A {chain} -m owner --uid-owner {uid} -j MARK --set-xmark 0x{:x}/0x{:x}",
                allocation.mark(),
                allocation.mask()
            ));
        }
    }
}

fn resolve_interfaces(
    policy: &InterfacePolicy,
    available: &[String],
) -> Result<Option<Vec<String>>, NetworkPlanError> {
    if policy.is_unrestricted() {
        return Ok(None);
    }
    let mut selected = available
        .iter()
        .filter(|name| name.as_str() != "lo")
        .filter(|name| {
            if policy.include().is_empty() {
                (policy.mobile() && is_mobile_interface(name))
                    || (policy.wifi() && is_wifi_interface(name))
            } else {
                policy
                    .include()
                    .iter()
                    .any(|pattern| glob_matches(pattern, name))
            }
        })
        .filter(|name| {
            !policy
                .exclude()
                .iter()
                .any(|pattern| glob_matches(pattern, name))
        })
        .cloned()
        .collect::<Vec<_>>();
    selected.sort();
    selected.dedup();
    if selected.is_empty() {
        Err(NetworkPlanError::InterfaceSelectionEmpty)
    } else {
        Ok(Some(selected))
    }
}

fn resolve_forwarding_interfaces(
    policy: ForwardingPolicy,
    available: &[String],
) -> Result<Vec<String>, NetworkPlanError> {
    if !policy.enabled() {
        return Ok(Vec::new());
    }
    let mut selected = available
        .iter()
        .filter(|name| {
            (policy.hotspot() && is_hotspot_interface(name))
                || (policy.usb() && is_usb_tether_interface(name))
        })
        .cloned()
        .collect::<Vec<_>>();
    selected.sort();
    selected.dedup();
    if selected.is_empty() || selected.len() > 8 {
        Err(NetworkPlanError::ForwardingInterfaceSelectionEmpty)
    } else {
        Ok(selected)
    }
}

fn forwarding_payloads(
    slot: PlanSlot,
    policy: &CapturePolicy,
    allocation: ResourceCandidate,
    interfaces: &[String],
    owner: &str,
) -> (String, String) {
    let chain = format!("NH_FWD_{}", slot.suffix());
    let mut apply = vec!["*mangle".to_owned(), format!("-N {chain}")];
    for interface in interfaces {
        apply.push(format!(
            "-A PREROUTING -i {interface} -m comment --comment {owner} -j {chain}"
        ));
    }
    apply.push(format!("-A {chain} -m conntrack --ctdir REPLY -j ACCEPT"));
    apply.push(format!(
        "-A {chain} -m mark --mark 0x{:x}/0x{:x} -j RETURN",
        allocation.mark(),
        allocation.mask()
    ));
    for destination in [
        "0.0.0.0/8",
        "10.0.0.0/8",
        "100.64.0.0/10",
        "127.0.0.0/8",
        "169.254.0.0/16",
        "172.16.0.0/12",
        "192.168.0.0/16",
        "224.0.0.0/4",
        "240.0.0.0/4",
    ] {
        apply.push(format!("-A {chain} -d {destination} -j RETURN"));
    }
    let inbound_port = policy
        .inbound_port()
        .expect("validated TPROXY policy has an inbound port");
    if policy.proxy_tcp() {
        apply.push(format!(
            "-A {chain} -p tcp -j TPROXY --on-port {inbound_port} --tproxy-mark 0x{:x}/0x{:x}",
            allocation.mark(),
            allocation.mask()
        ));
    }
    if policy.proxy_udp() {
        apply.push(format!(
            "-A {chain} -p udp -j TPROXY --on-port {inbound_port} --tproxy-mark 0x{:x}/0x{:x}",
            allocation.mark(),
            allocation.mask()
        ));
    }
    apply.push("COMMIT".to_owned());

    let mut rollback = vec!["*mangle".to_owned()];
    for interface in interfaces {
        rollback.push(format!(
            "-D PREROUTING -i {interface} -m comment --comment {owner} -j {chain}"
        ));
    }
    rollback.extend([
        format!("-F {chain}"),
        format!("-X {chain}"),
        "COMMIT".to_owned(),
    ]);
    (apply.join("\n") + "\n", rollback.join("\n") + "\n")
}

fn forwarding_ipv6_guard_payloads(
    slot: PlanSlot,
    interfaces: &[String],
    owner: &str,
) -> (String, String) {
    let chain = format!("NH_FWD6_{}", slot.suffix());
    let mut apply = vec!["*filter".to_owned(), format!("-N {chain}")];
    for interface in interfaces {
        apply.push(format!(
            "-A FORWARD -i {interface} -m comment --comment {owner} -j {chain}"
        ));
    }
    apply.push(format!("-A {chain} -j DROP"));
    apply.push("COMMIT".to_owned());

    let mut rollback = vec!["*filter".to_owned()];
    for interface in interfaces {
        rollback.push(format!(
            "-D FORWARD -i {interface} -m comment --comment {owner} -j {chain}"
        ));
    }
    rollback.extend([
        format!("-F {chain}"),
        format!("-X {chain}"),
        "COMMIT".to_owned(),
    ]);
    (apply.join("\n") + "\n", rollback.join("\n") + "\n")
}

fn is_wifi_interface(name: &str) -> bool {
    ["wlan", "wifi", "swlan"]
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

fn is_mobile_interface(name: &str) -> bool {
    [
        "rmnet", "ccmni", "pdp", "wwan", "v4-rmnet", "r_rmnet", "clat",
    ]
    .iter()
    .any(|prefix| name.starts_with(prefix))
}

fn is_hotspot_interface(name: &str) -> bool {
    if ["ap", "softap", "swlan"]
        .iter()
        .any(|prefix| name.starts_with(prefix))
    {
        return true;
    }
    name.strip_prefix("wlan")
        .and_then(|suffix| suffix.parse::<u8>().ok())
        .is_some_and(|index| index > 0)
}

fn is_usb_tether_interface(name: &str) -> bool {
    ["rndis", "usb"]
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

fn glob_matches(pattern: &str, value: &str) -> bool {
    let pattern = pattern.as_bytes();
    let value = value.as_bytes();
    let (mut pattern_index, mut value_index) = (0, 0);
    let (mut star, mut retry_value) = (None, 0);
    while value_index < value.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == b'?' || pattern[pattern_index] == value[value_index])
        {
            pattern_index += 1;
            value_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            star = Some(pattern_index);
            pattern_index += 1;
            retry_value = value_index;
        } else if let Some(star_index) = star {
            pattern_index = star_index + 1;
            retry_value += 1;
            value_index = retry_value;
        } else {
            return false;
        }
    }
    while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
}

fn ipv6_guard_payloads(slot: PlanSlot, policy: &CapturePolicy, owner: &str) -> (String, String) {
    let suffix = slot.suffix();
    let guard_chain = format!("NH_V6G_{suffix}");
    let block_chain = format!("NH_V6B_{suffix}");
    let mut apply = vec![
        "*filter".to_owned(),
        format!("-N {guard_chain}"),
        format!("-N {block_chain}"),
        format!("-A OUTPUT -m comment --comment {owner} -j {guard_chain}"),
    ];
    for uid in policy.exclude_uids() {
        apply.push(format!(
            "-A {guard_chain} -m owner --uid-owner {uid} -j RETURN"
        ));
    }
    if policy.include_uids().is_empty() {
        apply.push(format!("-A {guard_chain} -j {block_chain}"));
    } else {
        for uid in policy.include_uids() {
            apply.push(format!(
                "-A {guard_chain} -m owner --uid-owner {uid} -j {block_chain}"
            ));
        }
        apply.push(format!("-A {guard_chain} -j RETURN"));
    }
    for destination in ["::1/128", "fe80::/10", "fc00::/7", "ff00::/8"] {
        apply.push(format!("-A {block_chain} -d {destination} -j RETURN"));
    }
    apply.extend([format!("-A {block_chain} -j DROP"), "COMMIT".to_owned()]);

    let rollback = [
        "*filter".to_owned(),
        format!("-D OUTPUT -m comment --comment {owner} -j {guard_chain}"),
        format!("-F {guard_chain}"),
        format!("-F {block_chain}"),
        format!("-X {guard_chain}"),
        format!("-X {block_chain}"),
        "COMMIT".to_owned(),
    ]
    .join("\n")
        + "\n";
    (apply.join("\n") + "\n", rollback)
}

fn reserved_destinations(family: IpFamily) -> &'static [&'static str] {
    match family {
        IpFamily::Ipv4 => &[
            "0.0.0.0/8",
            "10.0.0.0/8",
            "100.64.0.0/10",
            "127.0.0.0/8",
            "169.254.0.0/16",
            "172.16.0.0/12",
            "192.168.0.0/16",
            "224.0.0.0/4",
            "240.0.0.0/4",
        ],
        IpFamily::Ipv6 => &["::/128", "::1/128", "fe80::/10", "fc00::/7", "ff00::/8"],
    }
}

#[cfg(test)]
mod tests {
    use super::{PlanSlot, dns_guard_payloads, reserved_destinations};
    use crate::{IpFamily, ResourceCandidate};
    use nethop_core::{CaptureMode, CapturePolicy};

    #[test]
    fn reserved_destination_sets_are_family_specific() {
        assert!(reserved_destinations(IpFamily::Ipv4).contains(&"127.0.0.0/8"));
        assert!(reserved_destinations(IpFamily::Ipv6).contains(&"::1/128"));
    }

    #[test]
    fn dns_guard_rollback_removes_only_the_owned_jump_and_chain() {
        let policy = CapturePolicy::new(
            CaptureMode::Tproxy,
            true,
            Some(7893),
            Some(0x200),
            Vec::new(),
            vec![0],
        )
        .unwrap();
        let allocation = ResourceCandidate::new(0x100, 0xff00, 100, 10_000).unwrap();
        let (_, rollback) = dns_guard_payloads(
            IpFamily::Ipv4,
            PlanSlot::A,
            &policy,
            allocation,
            "nethop:g=7",
            None,
        );
        assert!(rollback.contains("-D OUTPUT -m comment --comment nethop:g=7 -j NH_DNS_A"));
        assert!(rollback.contains("-F NH_DNS_A"));
        assert!(rollback.contains("-X NH_DNS_A"));
        assert!(!rollback.contains("-F OUTPUT"));
        assert!(!rollback.contains("--flush"));
    }
}
