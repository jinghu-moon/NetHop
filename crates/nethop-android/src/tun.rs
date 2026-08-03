use nethop_core::TunStack;
use thiserror::Error;

use crate::{
    CapabilityError, CapabilityReport, CapabilityStatus, IpFamily, ProbeBackend, ProbeCommand,
};

const DEFAULT_INTERFACE: &str = "nethop0";
const MAX_INTERFACE_BYTES: usize = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TunCandidate {
    stack: TunStack,
    mtu: u16,
}

impl TunCandidate {
    pub const fn stack(self) -> TunStack {
        self.stack
    }

    pub const fn mtu(self) -> u16 {
        self.mtu
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunFallbackPlan {
    candidates: Vec<TunCandidate>,
}

impl TunFallbackPlan {
    pub fn candidates(&self) -> &[TunCandidate] {
        &self.candidates
    }

    pub const fn uses_gso(&self) -> bool {
        false
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct TunFallbackPlanner;

impl TunFallbackPlanner {
    pub fn build(
        &self,
        capabilities: &CapabilityReport,
    ) -> Result<TunFallbackPlan, TunFallbackError> {
        if !capabilities.android().is_supported() || !capabilities.root().is_supported() {
            return Err(TunFallbackError::PlatformUnavailable);
        }
        if capabilities.tun() != CapabilityStatus::Supported {
            return Err(TunFallbackError::DeviceUnavailable);
        }
        if capabilities.active_tunnel() != CapabilityStatus::Supported {
            return Err(TunFallbackError::ExistingTunnelConflict);
        }
        Ok(TunFallbackPlan {
            candidates: vec![
                TunCandidate {
                    stack: TunStack::System,
                    mtu: 9000,
                },
                TunCandidate {
                    stack: TunStack::System,
                    mtu: 1500,
                },
                TunCandidate {
                    stack: TunStack::Gvisor,
                    mtu: 9000,
                },
                TunCandidate {
                    stack: TunStack::Gvisor,
                    mtu: 1500,
                },
            ],
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TunFallbackError {
    #[error("Android root platform capability is unavailable")]
    PlatformUnavailable,
    #[error("TUN device capability is unavailable")]
    DeviceUnavailable,
    #[error("an active VPN or TUN interface conflicts with fallback")]
    ExistingTunnelConflict,
}

impl TunFallbackError {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PlatformUnavailable => "tun_fallback_platform_unavailable",
            Self::DeviceUnavailable => "tun_fallback_device_unavailable",
            Self::ExistingTunnelConflict => "tun_fallback_existing_tunnel_conflict",
        }
    }
}

#[derive(Debug)]
pub struct TunHealthVerifier<B> {
    backend: B,
    interface: String,
}

impl<B> TunHealthVerifier<B> {
    pub fn new(backend: B, interface: impl Into<String>) -> Result<Self, TunHealthError> {
        let interface = interface.into();
        if interface.is_empty()
            || interface.len() > MAX_INTERFACE_BYTES
            || !interface
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err(TunHealthError::InvalidInterface);
        }
        Ok(Self { backend, interface })
    }

    pub fn into_backend(self) -> B {
        self.backend
    }
}

impl<B: ProbeBackend> TunHealthVerifier<B> {
    pub fn verify(&mut self) -> Result<(), TunHealthError> {
        let links = self.run(ProbeCommand::Links)?;
        if !links
            .lines()
            .any(|line| interface_is_up(line, &self.interface))
        {
            return Err(TunHealthError::InterfaceMissing);
        }
        let ipv4 = self.run(ProbeCommand::Addresses(IpFamily::Ipv4))?;
        if !address_belongs_to(&ipv4, &self.interface, "inet") {
            return Err(TunHealthError::Ipv4AddressMissing);
        }
        let ipv6 = self.run(ProbeCommand::Addresses(IpFamily::Ipv6))?;
        if !address_belongs_to(&ipv6, &self.interface, "inet6") {
            return Err(TunHealthError::Ipv6AddressMissing);
        }
        Ok(())
    }

    fn run(&mut self, command: ProbeCommand) -> Result<String, TunHealthError> {
        let output = self
            .backend
            .run(command)
            .map_err(|_error: CapabilityError| TunHealthError::ProbeFailed)?;
        if !output.success() {
            return Err(TunHealthError::ProbeFailed);
        }
        Ok(output.stdout().to_owned())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TunHealthError {
    #[error("TUN interface name is invalid")]
    InvalidInterface,
    #[error("TUN state could not be observed")]
    ProbeFailed,
    #[error("owned TUN interface is absent or down")]
    InterfaceMissing,
    #[error("owned TUN interface has no IPv4 address")]
    Ipv4AddressMissing,
    #[error("owned TUN interface has no IPv6 address")]
    Ipv6AddressMissing,
}

impl TunHealthError {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidInterface => "tun_health_invalid_interface",
            Self::ProbeFailed => "tun_health_probe_failed",
            Self::InterfaceMissing => "tun_health_interface_missing",
            Self::Ipv4AddressMissing => "tun_health_ipv4_address_missing",
            Self::Ipv6AddressMissing => "tun_health_ipv6_address_missing",
        }
    }
}

fn interface_is_up(line: &str, interface: &str) -> bool {
    let Some((_, remainder)) = line.split_once(':') else {
        return false;
    };
    let Some((name, flags)) = remainder.split_once(':') else {
        return false;
    };
    name.trim() == interface
        && flags
            .split_once('<')
            .and_then(|(_, flags)| flags.split_once('>'))
            .is_some_and(|(flags, _)| flags.split(',').any(|flag| flag == "UP"))
}

fn address_belongs_to(output: &str, interface: &str, family_token: &str) -> bool {
    output.lines().any(|line| {
        let tokens = line.split_ascii_whitespace().collect::<Vec<_>>();
        tokens.contains(&family_token) && tokens.contains(&interface)
    })
}

pub const fn default_tun_interface() -> &'static str {
    DEFAULT_INTERFACE
}
