use nethop_android::{
    AllocationCapability, CapabilityError, CapabilityReport, CapabilityStatus, FamilyCapability,
    IpFamily, NetfilterBackend, ProbeBackend, ProbeCommand, ProbeOutput, ResourceCandidate,
    TunFallbackError, TunFallbackPlanner, TunHealthError, TunHealthVerifier,
};
use nethop_core::TunStack;

fn family(family: IpFamily) -> FamilyCapability {
    FamilyCapability::new(
        family,
        CapabilityStatus::Supported,
        CapabilityStatus::Supported,
        CapabilityStatus::Supported,
        CapabilityStatus::Supported,
        CapabilityStatus::Supported,
        CapabilityStatus::Supported,
        CapabilityStatus::Supported,
        CapabilityStatus::Supported,
        CapabilityStatus::Supported,
        CapabilityStatus::Supported,
    )
}

fn report(tun: CapabilityStatus, active_tunnel: CapabilityStatus) -> CapabilityReport {
    CapabilityReport::new(
        CapabilityStatus::Supported,
        "arm64-v8a",
        CapabilityStatus::Supported,
        true,
        NetfilterBackend::Legacy,
        family(IpFamily::Ipv4),
        family(IpFamily::Ipv6),
        tun,
        active_tunnel,
        7893,
        CapabilityStatus::Supported,
        vec![AllocationCapability::new(
            ResourceCandidate::new(0x100, 0xff00, 100, 10_000).unwrap(),
            CapabilityStatus::Supported,
        )],
    )
    .unwrap()
}

#[test]
fn tun_fallback_candidates_are_bounded_and_ordered() {
    let plan = TunFallbackPlanner
        .build(&report(
            CapabilityStatus::Supported,
            CapabilityStatus::Supported,
        ))
        .unwrap();
    let candidates = plan.candidates();
    assert_eq!(candidates.len(), 4);
    assert_eq!(
        (candidates[0].stack(), candidates[0].mtu()),
        (TunStack::System, 9000)
    );
    assert_eq!(
        (candidates[1].stack(), candidates[1].mtu()),
        (TunStack::System, 1500)
    );
    assert_eq!(
        (candidates[2].stack(), candidates[2].mtu()),
        (TunStack::Gvisor, 9000)
    );
    assert_eq!(
        (candidates[3].stack(), candidates[3].mtu()),
        (TunStack::Gvisor, 1500)
    );
    assert!(!plan.uses_gso());
}

#[test]
fn tun_fallback_rejects_missing_device_and_existing_vpn() {
    assert_eq!(
        TunFallbackPlanner
            .build(&report(
                CapabilityStatus::Unsupported,
                CapabilityStatus::Supported,
            ))
            .unwrap_err(),
        TunFallbackError::DeviceUnavailable
    );
    assert_eq!(
        TunFallbackPlanner
            .build(&report(
                CapabilityStatus::Supported,
                CapabilityStatus::Conflict,
            ))
            .unwrap_err(),
        TunFallbackError::ExistingTunnelConflict
    );
}

#[derive(Debug)]
struct TunProbe {
    links: &'static str,
    ipv4: &'static str,
    ipv6: &'static str,
}

impl ProbeBackend for TunProbe {
    fn run(&mut self, command: ProbeCommand) -> Result<ProbeOutput, CapabilityError> {
        let output = match command {
            ProbeCommand::Links => self.links,
            ProbeCommand::Addresses(IpFamily::Ipv4) => self.ipv4,
            ProbeCommand::Addresses(IpFamily::Ipv6) => self.ipv6,
            _ => return Err(CapabilityError::CommandSpawnFailed),
        };
        Ok(ProbeOutput::new(true, output, ""))
    }
}

#[test]
fn tun_health_requires_owned_up_interface_and_both_families() {
    let mut verifier = TunHealthVerifier::new(
        TunProbe {
            links: "8: nethop0: <POINTOPOINT,UP,LOWER_UP> mtu 9000",
            ipv4: "8: nethop0: <POINTOPOINT,UP> mtu 9000\n    inet 172.19.0.1/30 scope global nethop0",
            ipv6: "8: nethop0: <POINTOPOINT,UP> mtu 9000\n    inet6 fdfe:dcba:9876::1/126 scope global",
        },
        "nethop0",
    )
    .unwrap();
    verifier.verify().unwrap();

    let mut missing_ipv6 = TunHealthVerifier::new(
        TunProbe {
            links: "8: nethop0: <POINTOPOINT,UP> mtu 1500",
            ipv4: "8: nethop0: <POINTOPOINT,UP> mtu 1500\n    inet 172.19.0.1/30 scope global nethop0",
            ipv6: "",
        },
        "nethop0",
    )
    .unwrap();
    assert_eq!(
        missing_ipv6.verify().unwrap_err(),
        TunHealthError::Ipv6AddressMissing
    );
}

#[test]
fn tun_shutdown_health_requires_the_owned_interface_to_disappear() {
    let mut absent = TunHealthVerifier::new(
        TunProbe {
            links: "1: lo: <LOOPBACK,UP> mtu 65536",
            ipv4: "",
            ipv6: "",
        },
        "nethop0",
    )
    .unwrap();
    absent.verify_absent().unwrap();

    let mut leaked = TunHealthVerifier::new(
        TunProbe {
            links: "8: nethop0: <POINTOPOINT,UP> mtu 1500",
            ipv4: "",
            ipv6: "",
        },
        "nethop0",
    )
    .unwrap();
    assert_eq!(
        leaked.verify_absent().unwrap_err(),
        TunHealthError::InterfaceStillPresent
    );
}
