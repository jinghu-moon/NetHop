use thiserror::Error;

use crate::{
    CapabilityError, IpFamily, NetfilterTable, NetworkPlan, ProbeBackend, ProbeCommand,
    ResourceCandidate,
};

pub trait NetworkHealthVerifier {
    fn verify(&mut self, plan: &NetworkPlan) -> Result<(), NetworkHealthError>;

    fn replace_inbound_port(&mut self, _inbound_port: u16) -> Result<(), NetworkHealthError> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct NetworkPlanVerifier<B> {
    backend: B,
    inbound_port: u16,
}

impl<B> NetworkPlanVerifier<B> {
    pub fn new(backend: B, inbound_port: u16) -> Result<Self, NetworkHealthError> {
        if inbound_port == 0 {
            return Err(NetworkHealthError::InvalidPolicy);
        }
        Ok(Self {
            backend,
            inbound_port,
        })
    }

    pub fn into_backend(self) -> B {
        self.backend
    }
}

impl<B: ProbeBackend> NetworkHealthVerifier for NetworkPlanVerifier<B> {
    fn verify(&mut self, plan: &NetworkPlan) -> Result<(), NetworkHealthError> {
        let sockets = self.run(ProbeCommand::ListeningSockets)?;
        if !socket_output_contains_port(&sockets, self.inbound_port) {
            return Err(NetworkHealthError::InboundPortMissing);
        }

        self.verify_owner(plan, IpFamily::Ipv4)?;
        self.verify_dns_guard(plan, IpFamily::Ipv4)?;
        self.verify_routing(plan, IpFamily::Ipv4)?;
        if !plan.forwarding_interfaces().is_empty() {
            self.verify_forwarding(plan)?;
            self.verify_forwarding_ipv6_guard(plan)?;
        }
        if plan.ipv6_captured() {
            self.verify_owner(plan, IpFamily::Ipv6)?;
            self.verify_dns_guard(plan, IpFamily::Ipv6)?;
            self.verify_routing(plan, IpFamily::Ipv6)?;
        } else if plan.ipv6_guarded() {
            self.verify_owner(plan, IpFamily::Ipv6)?;
        }
        Ok(())
    }

    fn replace_inbound_port(&mut self, inbound_port: u16) -> Result<(), NetworkHealthError> {
        if inbound_port == 0 {
            return Err(NetworkHealthError::InvalidPolicy);
        }
        self.inbound_port = inbound_port;
        Ok(())
    }
}

impl<B: ProbeBackend> NetworkPlanVerifier<B> {
    fn verify_owner(
        &mut self,
        plan: &NetworkPlan,
        family: IpFamily,
    ) -> Result<(), NetworkHealthError> {
        let table = if family == IpFamily::Ipv6 && !plan.ipv6_captured() {
            NetfilterTable::Filter
        } else {
            NetfilterTable::Mangle
        };
        let snapshot = self.run(ProbeCommand::NetfilterSnapshot(family, table))?;
        if !snapshot.contains(&plan.owner_marker()) || !snapshot.contains(&plan.entry_chain(family))
        {
            return Err(NetworkHealthError::OwnerMarkerMissing);
        }
        if (family == IpFamily::Ipv4 || plan.ipv6_captured())
            && !reply_bypass_precedes_capture(&snapshot, plan)
        {
            return Err(NetworkHealthError::ReplyBypassMissing);
        }
        if (family == IpFamily::Ipv4 || plan.ipv6_captured())
            && !loopback_bypass_precedes_capture(&snapshot, plan)
        {
            return Err(NetworkHealthError::LoopbackBypassMissing);
        }
        if (family == IpFamily::Ipv4 || plan.ipv6_captured())
            && !dns_capture_present(&snapshot, plan)
        {
            return Err(NetworkHealthError::DnsCaptureMissing);
        }
        Ok(())
    }

    fn verify_dns_guard(
        &mut self,
        plan: &NetworkPlan,
        family: IpFamily,
    ) -> Result<(), NetworkHealthError> {
        if !plan.dns_guarded() {
            return Ok(());
        }
        let snapshot = self.run(ProbeCommand::NetfilterSnapshot(
            family,
            NetfilterTable::Filter,
        ))?;
        if !dns_guard_present(&snapshot, plan) {
            return Err(NetworkHealthError::DnsGuardMissing);
        }
        Ok(())
    }

    fn verify_routing(
        &mut self,
        plan: &NetworkPlan,
        family: IpFamily,
    ) -> Result<(), NetworkHealthError> {
        let allocation = plan.allocation();
        let rules = self.run(ProbeCommand::PolicyRules(family))?;
        if !rules
            .lines()
            .any(|line| policy_rule_matches(line, allocation))
        {
            return Err(NetworkHealthError::PolicyRuleMissing);
        }
        let routes = self.run(ProbeCommand::RouteTable(family, allocation.route_table()))?;
        if !routes
            .lines()
            .any(|line| line.contains("local") && line.contains("dev lo"))
        {
            return Err(NetworkHealthError::RouteMissing);
        }
        Ok(())
    }

    fn verify_forwarding(&mut self, plan: &NetworkPlan) -> Result<(), NetworkHealthError> {
        let snapshot = self.run(ProbeCommand::NetfilterSnapshot(
            IpFamily::Ipv4,
            NetfilterTable::Filter,
        ))?;
        let chain = format!("NH_FWD_{}", plan.slot_suffix());
        if !snapshot.lines().any(|line| line == format!("-N {chain}")) {
            return Err(NetworkHealthError::ForwardingChainMissing);
        }
        let owner = plan.forwarding_owner_marker();
        for interface in plan.forwarding_interfaces() {
            let jump =
                format!("-A PREROUTING -i {interface} -m comment --comment {owner} -j {chain}");
            if !snapshot.lines().any(|line| line == jump) {
                return Err(NetworkHealthError::ForwardingInterfaceJumpMissing);
            }
        }
        Ok(())
    }

    fn verify_forwarding_ipv6_guard(
        &mut self,
        plan: &NetworkPlan,
    ) -> Result<(), NetworkHealthError> {
        let snapshot = self.run(ProbeCommand::NetfilterSnapshot(
            IpFamily::Ipv6,
            NetfilterTable::Filter,
        ))?;
        let chain = format!("NH_FWD6_{}", plan.slot_suffix());
        if !snapshot.lines().any(|line| line == format!("-N {chain}")) {
            return Err(NetworkHealthError::ForwardingIpv6GuardMissing);
        }
        let owner = plan.forwarding_owner_marker();
        for interface in plan.forwarding_interfaces() {
            let jump = format!("-A FORWARD -i {interface} -m comment --comment {owner} -j {chain}");
            if !snapshot.lines().any(|line| line == jump) {
                return Err(NetworkHealthError::ForwardingIpv6GuardMissing);
            }
        }
        if !snapshot
            .lines()
            .any(|line| line == format!("-A {chain} -j DROP"))
        {
            return Err(NetworkHealthError::ForwardingIpv6GuardMissing);
        }
        Ok(())
    }

    fn run(&mut self, command: ProbeCommand) -> Result<String, NetworkHealthError> {
        let output = self
            .backend
            .run(command)
            .map_err(|_error: CapabilityError| NetworkHealthError::ProbeFailed)?;
        if !output.success() {
            return Err(NetworkHealthError::ProbeFailed);
        }
        Ok(output.stdout().to_owned())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkHealthDiagnosticCode {
    InvalidPolicy,
    ProbeFailed,
    InboundPortMissing,
    OwnerMarkerMissing,
    ReplyBypassMissing,
    LoopbackBypassMissing,
    DnsCaptureMissing,
    DnsGuardMissing,
    PolicyRuleMissing,
    RouteMissing,
    ForwardingChainMissing,
    ForwardingInterfaceJumpMissing,
    ForwardingIpv6GuardMissing,
}

impl NetworkHealthDiagnosticCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidPolicy => "network_health_invalid_policy",
            Self::ProbeFailed => "network_health_probe_failed",
            Self::InboundPortMissing => "network_health_inbound_port_missing",
            Self::OwnerMarkerMissing => "network_health_owner_marker_missing",
            Self::ReplyBypassMissing => "network_health_reply_bypass_missing",
            Self::LoopbackBypassMissing => "network_health_loopback_bypass_missing",
            Self::DnsCaptureMissing => "network_health_dns_capture_missing",
            Self::DnsGuardMissing => "network_health_dns_guard_missing",
            Self::PolicyRuleMissing => "network_health_policy_rule_missing",
            Self::RouteMissing => "network_health_route_missing",
            Self::ForwardingChainMissing => "network_health_forwarding_chain_missing",
            Self::ForwardingInterfaceJumpMissing => {
                "network_health_forwarding_interface_jump_missing"
            }
            Self::ForwardingIpv6GuardMissing => "network_health_forwarding_ipv6_guard_missing",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum NetworkHealthError {
    #[error("network health policy is invalid")]
    InvalidPolicy,
    #[error("network health state could not be observed")]
    ProbeFailed,
    #[error("candidate inbound port is not listening")]
    InboundPortMissing,
    #[error("candidate network owner marker is missing")]
    OwnerMarkerMissing,
    #[error("candidate TPROXY reply-direction bypass is missing or ordered after capture")]
    ReplyBypassMissing,
    #[error("candidate TPROXY loopback bypass is missing or ordered after capture")]
    LoopbackBypassMissing,
    #[error("candidate DNS capture rules are missing")]
    DnsCaptureMissing,
    #[error("candidate DNS leak guard rules are missing")]
    DnsGuardMissing,
    #[error("candidate policy rule is missing")]
    PolicyRuleMissing,
    #[error("candidate local route is missing")]
    RouteMissing,
    #[error("candidate forwarding chain is missing")]
    ForwardingChainMissing,
    #[error("candidate forwarding interface jump is missing")]
    ForwardingInterfaceJumpMissing,
    #[error("forwarding IPv6 fail-closed guard is missing")]
    ForwardingIpv6GuardMissing,
}

impl NetworkHealthError {
    pub const fn code(self) -> NetworkHealthDiagnosticCode {
        match self {
            Self::InvalidPolicy => NetworkHealthDiagnosticCode::InvalidPolicy,
            Self::ProbeFailed => NetworkHealthDiagnosticCode::ProbeFailed,
            Self::InboundPortMissing => NetworkHealthDiagnosticCode::InboundPortMissing,
            Self::OwnerMarkerMissing => NetworkHealthDiagnosticCode::OwnerMarkerMissing,
            Self::ReplyBypassMissing => NetworkHealthDiagnosticCode::ReplyBypassMissing,
            Self::LoopbackBypassMissing => NetworkHealthDiagnosticCode::LoopbackBypassMissing,
            Self::DnsCaptureMissing => NetworkHealthDiagnosticCode::DnsCaptureMissing,
            Self::DnsGuardMissing => NetworkHealthDiagnosticCode::DnsGuardMissing,
            Self::PolicyRuleMissing => NetworkHealthDiagnosticCode::PolicyRuleMissing,
            Self::RouteMissing => NetworkHealthDiagnosticCode::RouteMissing,
            Self::ForwardingChainMissing => NetworkHealthDiagnosticCode::ForwardingChainMissing,
            Self::ForwardingInterfaceJumpMissing => {
                NetworkHealthDiagnosticCode::ForwardingInterfaceJumpMissing
            }
            Self::ForwardingIpv6GuardMissing => {
                NetworkHealthDiagnosticCode::ForwardingIpv6GuardMissing
            }
        }
    }
}

fn dns_capture_present(snapshot: &str, plan: &NetworkPlan) -> bool {
    let output_chain = plan.entry_chain(IpFamily::Ipv4);
    let prerouting_chain = plan.prerouting_chain();
    ["tcp", "udp"].iter().all(|protocol| {
        snapshot.lines().any(|line| {
            line.starts_with(&format!("-A {output_chain} "))
                && line.contains(&format!("-p {protocol} "))
                && line.contains("--dport 53 ")
                && line.contains(" -j MARK ")
        }) && snapshot.lines().any(|line| {
            line.starts_with(&format!("-A {prerouting_chain} "))
                && line.contains(&format!("-p {protocol} "))
                && line.contains("--dport 53 ")
                && line.contains(" -j TPROXY ")
        })
    })
}

fn dns_guard_present(snapshot: &str, plan: &NetworkPlan) -> bool {
    let chain = plan.dns_guard_chain();
    let allocation = plan.allocation();
    if !snapshot
        .lines()
        .any(|line| comment_jump_matches(line, "OUTPUT", &plan.owner_marker(), &chain))
    {
        return false;
    }
    let lines = snapshot.lines().collect::<Vec<_>>();
    let return_before_drop = |predicate: &dyn Fn(&str) -> bool| {
        let return_index = lines.iter().position(|line| {
            line.starts_with(&format!("-A {chain} "))
                && line.ends_with(" -j RETURN")
                && predicate(line)
        });
        let first_drop = lines.iter().position(|line| {
            line.starts_with(&format!("-A {chain} ")) && line.ends_with(" -j DROP")
        });
        matches!((return_index, first_drop), (Some(return_index), Some(drop_index)) if return_index < drop_index)
    };
    if !return_before_drop(&|line| line.contains("-o lo "))
        || !return_before_drop(&|line| {
            mark_argument_matches(line, allocation.mark(), allocation.mask())
        })
        || !return_before_drop(&|line| mark_argument_matches(line, plan.bypass_mark(), u32::MAX))
    {
        return false;
    }
    [("udp", 53_u16), ("tcp", 53), ("tcp", 853)]
        .iter()
        .all(|(protocol, port)| {
            lines.iter().any(|line| {
                line.starts_with(&format!("-A {chain} "))
                    && line.contains(&format!("-p {protocol} "))
                    && line.contains(&format!("--dport {port} "))
                    && line.contains(" -j DROP")
            })
        })
}

fn comment_jump_matches(line: &str, source: &str, marker: &str, target: &str) -> bool {
    let tokens = line.split_ascii_whitespace().collect::<Vec<_>>();
    tokens.get(0..2) == Some(&["-A", source])
        && tokens.windows(2).any(|pair| pair == ["-m", "comment"])
        && tokens
            .windows(2)
            .any(|pair| pair[0] == "--comment" && pair[1].trim_matches('"') == marker)
        && tokens
            .windows(2)
            .any(|pair| pair[0] == "-j" && pair[1] == target)
}

fn mark_argument_matches(line: &str, mark: u32, mask: u32) -> bool {
    line.split_ascii_whitespace()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|pair| pair[0] == "--mark")
        .and_then(|pair| parse_mark_mask(pair[1]))
        == Some((mark, mask))
}

fn reply_bypass_precedes_capture(snapshot: &str, plan: &NetworkPlan) -> bool {
    let output_chain = plan.entry_chain(IpFamily::Ipv4);
    let prerouting_chain = plan.prerouting_chain();
    let output_reply = format!("-A {output_chain} -m conntrack --ctdir REPLY -j ACCEPT");
    let prerouting_reply = format!("-A {prerouting_chain} -m conntrack --ctdir REPLY -j ACCEPT");

    let Some(output_reply_index) = snapshot.find(&output_reply) else {
        return false;
    };
    let Some(prerouting_reply_index) = snapshot.find(&prerouting_reply) else {
        return false;
    };
    let output_capture_index = snapshot.lines().position(|line| {
        line.starts_with(&format!("-A {output_chain} ")) && line.contains(" -j MARK ")
    });
    let prerouting_capture_index = snapshot.lines().position(|line| {
        line.starts_with(&format!("-A {prerouting_chain} "))
            && (line.contains(" -m socket ") || line.contains(" -j TPROXY "))
    });
    let output_reply_line = snapshot[..output_reply_index].lines().count();
    let prerouting_reply_line = snapshot[..prerouting_reply_index].lines().count();

    output_capture_index.is_some_and(|index| output_reply_line < index)
        && prerouting_capture_index.is_some_and(|index| prerouting_reply_line < index)
}

fn loopback_bypass_precedes_capture(snapshot: &str, plan: &NetworkPlan) -> bool {
    let prerouting_chain = plan.prerouting_chain();
    let allocation = plan.allocation();
    let Some(loopback_bypass_line) = snapshot
        .lines()
        .position(|line| loopback_bypass_matches(line, &prerouting_chain, allocation))
    else {
        return false;
    };
    let socket_index = snapshot.lines().position(|line| {
        line.starts_with(&format!("-A {prerouting_chain} ")) && line.contains(" -m socket ")
    });
    let tproxy_index = snapshot.lines().position(|line| {
        line.starts_with(&format!("-A {prerouting_chain} ")) && line.contains(" -j TPROXY ")
    });
    socket_index.is_some_and(|index| index < loopback_bypass_line)
        && tproxy_index.is_some_and(|index| loopback_bypass_line < index)
}

fn loopback_bypass_matches(
    line: &str,
    prerouting_chain: &str,
    allocation: ResourceCandidate,
) -> bool {
    let tokens = line.split_ascii_whitespace().collect::<Vec<_>>();
    let chain_matches = tokens.get(0..2) == Some(&["-A", prerouting_chain]);
    let loopback_matches = tokens.windows(2).any(|pair| pair == ["-i", "lo"]);
    let return_matches = tokens.windows(2).any(|pair| pair == ["-j", "RETURN"]);
    let mark_matches = tokens
        .windows(3)
        .find(|window| window[0] == "!" && window[1] == "--mark")
        .and_then(|window| parse_mark_mask(window[2]))
        == Some((allocation.mark(), allocation.mask()));
    let mark_module = tokens.windows(2).any(|pair| pair == ["-m", "mark"]);
    chain_matches && loopback_matches && return_matches && mark_module && mark_matches
}

fn socket_output_contains_port(output: &str, port: u16) -> bool {
    let needle = format!(":{port}");
    output
        .split_ascii_whitespace()
        .any(|token| token.ends_with(&needle) || token.contains(&format!("{needle} ")))
}

fn policy_rule_matches(line: &str, candidate: ResourceCandidate) -> bool {
    let tokens = line.split_ascii_whitespace().collect::<Vec<_>>();
    let priority_matches = tokens
        .first()
        .and_then(|token| token.strip_suffix(':'))
        .and_then(|value| value.parse::<u32>().ok())
        == Some(candidate.rule_priority());
    let mark_matches = tokens
        .windows(2)
        .find(|pair| pair[0] == "fwmark")
        .and_then(|pair| parse_mark_mask(pair[1]))
        == Some((candidate.mark(), candidate.mask()));
    let table_matches = tokens.windows(2).any(|pair| {
        matches!(pair[0], "lookup" | "table")
            && pair[1].parse::<u32>().ok() == Some(candidate.route_table())
    });
    priority_matches && mark_matches && table_matches
}

fn parse_mark_mask(value: &str) -> Option<(u32, u32)> {
    let (mark, mask) = value.split_once('/').unwrap_or((value, "0xffffffff"));
    Some((parse_u32(mark)?, parse_u32(mask)?))
}

fn parse_u32(value: &str) -> Option<u32> {
    value.strip_prefix("0x").map_or_else(
        || value.parse().ok(),
        |hex| u32::from_str_radix(hex, 16).ok(),
    )
}

#[cfg(test)]
mod tests {
    use super::{comment_jump_matches, loopback_bypass_matches, mark_argument_matches};
    use crate::ResourceCandidate;

    #[test]
    fn loopback_bypass_accepts_android_full_mask_normalization() {
        let allocation = ResourceCandidate::new(0x4e49_0100, u32::MAX, 100, 12_000).unwrap();
        assert!(loopback_bypass_matches(
            "-A NH_PRE_A -i lo -m mark ! --mark 0x4e490100 -j RETURN",
            "NH_PRE_A",
            allocation,
        ));
    }

    #[test]
    fn loopback_bypass_rejects_a_different_mask() {
        let allocation = ResourceCandidate::new(0x100, 0xff00, 100, 10_000).unwrap();
        assert!(!loopback_bypass_matches(
            "-A NH_PRE_A -i lo -m mark ! --mark 0x100 -j RETURN",
            "NH_PRE_A",
            allocation,
        ));
    }

    #[test]
    fn android_save_normalization_preserves_comment_and_full_mask_semantics() {
        assert!(comment_jump_matches(
            "-A OUTPUT -m comment --comment \"nethop:g=1\" -j NH_DNS_A",
            "OUTPUT",
            "nethop:g=1",
            "NH_DNS_A",
        ));
        assert!(mark_argument_matches(
            "-A NH_DNS_A -m mark --mark 0x4e490100 -j RETURN",
            0x4e49_0100,
            u32::MAX,
        ));
    }
}
