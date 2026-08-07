use thiserror::Error;

use nethop_core::{CapturePolicy, GenerationId};

use crate::plan::{NetworkOperation, NetworkOperationKind, NetworkPlan, PlanSlot, restore_step};
use crate::{IpFamily, NetfilterTable, ProbeBackend, ProbeCommand, ResourceCandidate};

const MAX_FORWARD_INTERFACES: usize = 8;
const MAX_INTERFACE_NAME_BYTES: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardingPlan {
    generation: GenerationId,
    slot: PlanSlot,
    allocation: ResourceCandidate,
    interfaces: Vec<String>,
    steps: Vec<crate::plan::PlanStep>,
}

impl ForwardingPlan {
    pub const fn generation(&self) -> GenerationId {
        self.generation
    }

    pub const fn slot(&self) -> PlanSlot {
        self.slot
    }

    pub const fn allocation(&self) -> ResourceCandidate {
        self.allocation
    }

    pub fn interfaces(&self) -> &[String] {
        &self.interfaces
    }

    pub fn owner_marker(&self) -> String {
        format!("nethop:fwd:g={}", self.generation.get())
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

    pub(crate) fn steps(&self) -> &[crate::plan::PlanStep] {
        &self.steps
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ForwardingPlanner;

impl ForwardingPlanner {
    pub fn build(
        &self,
        generation: GenerationId,
        slot: PlanSlot,
        base: &NetworkPlan,
        policy: &CapturePolicy,
        interfaces: Vec<String>,
    ) -> Result<ForwardingPlan, ForwardingPlanError> {
        if generation != base.generation() || slot != base.slot() || generation.get() == 0 {
            return Err(ForwardingPlanError::GenerationMismatch);
        }
        if interfaces.is_empty() || interfaces.len() > MAX_FORWARD_INTERFACES {
            return Err(ForwardingPlanError::InvalidInterfaceSet);
        }
        let original_interface_count = interfaces.len();
        let mut interfaces = interfaces;
        interfaces.sort();
        interfaces.dedup();
        if interfaces.len() != original_interface_count
            || interfaces.is_empty()
            || interfaces.iter().any(|name| {
                name.is_empty()
                    || name.len() > MAX_INTERFACE_NAME_BYTES
                    || name == "lo"
                    || !name.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'+')
                    })
            })
        {
            return Err(ForwardingPlanError::InvalidInterfaceSet);
        }
        let chain = format!(
            "NH_FWD_{}",
            match slot {
                PlanSlot::A => "A",
                PlanSlot::B => "B",
            }
        );
        let owner = format!("nethop:fwd:g={}", generation.get());
        let mark = base.allocation().mark();
        let mask = base.allocation().mask();
        let inbound_port = policy
            .inbound_port()
            .ok_or(ForwardingPlanError::InboundPortMissing)?;
        let mut apply = vec!["*mangle".to_owned(), format!("-N {chain}")];
        for interface in &interfaces {
            apply.push(format!(
                "-A PREROUTING -i {interface} -m comment --comment {owner} -j {chain}"
            ));
        }
        apply.push(format!("-A {chain} -m conntrack --ctdir REPLY -j ACCEPT"));
        apply.push(format!(
            "-A {chain} -m mark --mark 0x{mark:x}/0x{mask:x} -j RETURN"
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
        if policy.proxy_tcp() {
            apply.push(format!(
                "-A {chain} -p tcp -j TPROXY --on-port {inbound_port} --tproxy-mark 0x{mark:x}/0x{mask:x}"
            ));
        }
        if policy.proxy_udp() {
            apply.push(format!(
                "-A {chain} -p udp -j TPROXY --on-port {inbound_port} --tproxy-mark 0x{mark:x}/0x{mask:x}"
            ));
        }
        apply.push("COMMIT".to_owned());
        let mut rollback = vec!["*mangle".to_owned()];
        for interface in &interfaces {
            rollback.push(format!(
                "-D PREROUTING -i {interface} -m comment --comment {owner} -j {chain}"
            ));
        }
        rollback.extend([
            format!("-F {chain}"),
            format!("-X {chain}"),
            "COMMIT".to_owned(),
        ]);
        Ok(ForwardingPlan {
            generation,
            slot,
            allocation: base.allocation(),
            interfaces,
            steps: vec![restore_step(
                NetworkOperationKind::ForwardingRestore,
                IpFamily::Ipv4,
                apply.join("\n") + "\n",
                rollback.join("\n") + "\n",
            )],
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ForwardingPlanError {
    #[error("forwarding plan generation does not match the local capture plan")]
    GenerationMismatch,
    #[error("forwarding interface set is invalid or empty")]
    InvalidInterfaceSet,
    #[error("forwarding capture inbound port is missing")]
    InboundPortMissing,
}

#[derive(Debug)]
pub struct ForwardingPlanVerifier<B> {
    backend: B,
    inbound_port: u16,
}

impl<B: ProbeBackend> ForwardingPlanVerifier<B> {
    pub fn new(backend: B, inbound_port: u16) -> Result<Self, ForwardingHealthError> {
        if inbound_port == 0 {
            return Err(ForwardingHealthError::InvalidPolicy);
        }
        Ok(Self {
            backend,
            inbound_port,
        })
    }

    pub fn verify(&mut self, plan: &ForwardingPlan) -> Result<(), ForwardingHealthError> {
        let socket = self
            .backend
            .run(ProbeCommand::ListeningSockets)
            .map_err(|_| ForwardingHealthError::ProbeFailed)?;
        if !socket.success()
            || !socket
                .stdout()
                .split_ascii_whitespace()
                .any(|value| value.ends_with(&format!(":{}", self.inbound_port)))
        {
            return Err(ForwardingHealthError::InboundPortMissing);
        }
        let snapshot = self
            .backend
            .run(ProbeCommand::NetfilterSnapshot(
                IpFamily::Ipv4,
                NetfilterTable::Filter,
            ))
            .map_err(|_| ForwardingHealthError::ProbeFailed)?;
        if !snapshot.success() {
            return Err(ForwardingHealthError::ProbeFailed);
        }
        let chain = format!(
            "NH_FWD_{}",
            match plan.slot() {
                PlanSlot::A => "A",
                PlanSlot::B => "B",
            }
        );
        let owner = plan.owner_marker();
        if !snapshot.stdout().contains(&format!("-N {chain}")) {
            return Err(ForwardingHealthError::ChainMissing);
        }
        for interface in plan.interfaces() {
            let jump =
                format!("-A PREROUTING -i {interface} -m comment --comment {owner} -j {chain}");
            if !snapshot.stdout().lines().any(|line| line == jump) {
                return Err(ForwardingHealthError::InterfaceJumpMissing);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ForwardingHealthError {
    #[error("forwarding health policy is invalid")]
    InvalidPolicy,
    #[error("forwarding health probe failed")]
    ProbeFailed,
    #[error("forwarding inbound port is not listening")]
    InboundPortMissing,
    #[error("forwarding chain is missing")]
    ChainMissing,
    #[error("forwarding interface jump is missing")]
    InterfaceJumpMissing,
}
