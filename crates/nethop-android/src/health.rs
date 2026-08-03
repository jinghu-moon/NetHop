use thiserror::Error;

use crate::{
    CapabilityError, IpFamily, NetworkPlan, ProbeBackend, ProbeCommand, ResourceCandidate,
};

pub trait NetworkHealthVerifier {
    fn verify(&mut self, plan: &NetworkPlan) -> Result<(), NetworkHealthError>;
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
        self.verify_routing(plan, IpFamily::Ipv4)?;
        if plan.ipv6_captured() {
            self.verify_owner(plan, IpFamily::Ipv6)?;
            self.verify_routing(plan, IpFamily::Ipv6)?;
        } else if plan.ipv6_guarded() {
            self.verify_owner(plan, IpFamily::Ipv6)?;
        }
        Ok(())
    }
}

impl<B: ProbeBackend> NetworkPlanVerifier<B> {
    fn verify_owner(
        &mut self,
        plan: &NetworkPlan,
        family: IpFamily,
    ) -> Result<(), NetworkHealthError> {
        let snapshot = self.run(ProbeCommand::NetfilterSnapshot(family))?;
        if !snapshot.contains(&plan.owner_marker()) || !snapshot.contains(&plan.entry_chain(family))
        {
            return Err(NetworkHealthError::OwnerMarkerMissing);
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
    PolicyRuleMissing,
    RouteMissing,
}

impl NetworkHealthDiagnosticCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidPolicy => "network_health_invalid_policy",
            Self::ProbeFailed => "network_health_probe_failed",
            Self::InboundPortMissing => "network_health_inbound_port_missing",
            Self::OwnerMarkerMissing => "network_health_owner_marker_missing",
            Self::PolicyRuleMissing => "network_health_policy_rule_missing",
            Self::RouteMissing => "network_health_route_missing",
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
    #[error("candidate policy rule is missing")]
    PolicyRuleMissing,
    #[error("candidate local route is missing")]
    RouteMissing,
}

impl NetworkHealthError {
    pub const fn code(self) -> NetworkHealthDiagnosticCode {
        match self {
            Self::InvalidPolicy => NetworkHealthDiagnosticCode::InvalidPolicy,
            Self::ProbeFailed => NetworkHealthDiagnosticCode::ProbeFailed,
            Self::InboundPortMissing => NetworkHealthDiagnosticCode::InboundPortMissing,
            Self::OwnerMarkerMissing => NetworkHealthDiagnosticCode::OwnerMarkerMissing,
            Self::PolicyRuleMissing => NetworkHealthDiagnosticCode::PolicyRuleMissing,
            Self::RouteMissing => NetworkHealthDiagnosticCode::RouteMissing,
        }
    }
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
