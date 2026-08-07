use nethop_android::{
    AllocationCapability, CapabilityError, CapabilityReport, CapabilityStatus, CommandFailure,
    CommandInvocation, CommandOutput, FamilyCapability, ForwardingHealthError,
    ForwardingPlanVerifier, ForwardingPlanner, IpFamily, NetfilterBackend, NetfilterTable,
    NetworkCommandBackend, NetworkExecutor, NetworkPlanner, PlanSlot, ProbeBackend, ProbeCommand,
    ProbeOutput, ResourceCandidate,
};
use nethop_core::{CaptureMode, CapturePolicy, GenerationId};

const PORT: u16 = 7893;

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

fn fixture() -> (CapturePolicy, nethop_android::NetworkPlan) {
    let allocation = ResourceCandidate::new(0x100, 0xff00, 100, 10_000).unwrap();
    let capabilities = CapabilityReport::new(
        CapabilityStatus::Supported,
        "arm64-v8a",
        CapabilityStatus::Supported,
        true,
        NetfilterBackend::Legacy,
        family(IpFamily::Ipv4),
        family(IpFamily::Ipv6),
        CapabilityStatus::Supported,
        CapabilityStatus::Supported,
        PORT,
        CapabilityStatus::Supported,
        vec![AllocationCapability::new(
            allocation,
            CapabilityStatus::Supported,
        )],
    )
    .unwrap();
    let policy = CapturePolicy::new(
        CaptureMode::Tproxy,
        true,
        Some(PORT),
        Some(0x20_000),
        Vec::new(),
        vec![0],
    )
    .unwrap();
    let plan = NetworkPlanner
        .build_tproxy(
            GenerationId::new(7).unwrap(),
            PlanSlot::A,
            &policy,
            &capabilities,
        )
        .unwrap();
    (policy, plan)
}

#[test]
fn hotspot_and_usb_forwarding_are_isolated_from_local_output_and_uid_capture() {
    let (policy, local) = fixture();
    let plan = ForwardingPlanner
        .build(
            GenerationId::new(7).unwrap(),
            PlanSlot::A,
            &local,
            &policy,
            vec!["rndis0".into(), "wlan1".into()],
        )
        .unwrap();
    let payload = plan.restore_payloads().next().unwrap().1;
    assert!(
        payload.contains("-A PREROUTING -i rndis0 -m comment --comment nethop:fwd:g=7 -j NH_FWD_A")
    );
    assert!(
        payload.contains("-A PREROUTING -i wlan1 -m comment --comment nethop:fwd:g=7 -j NH_FWD_A")
    );
    assert!(payload.contains("-A NH_FWD_A -p tcp -j TPROXY"));
    assert!(payload.contains("-A NH_FWD_A -p udp -j TPROXY"));
    assert!(!payload.contains("-A OUTPUT"));
    assert!(!payload.contains("--uid-owner"));
    assert!(!payload.contains("NH_OUT_A"));
}

#[derive(Default)]
struct RecordingBackend(Vec<CommandInvocation>);

impl NetworkCommandBackend for RecordingBackend {
    fn execute(&mut self, invocation: &CommandInvocation) -> Result<CommandOutput, CommandFailure> {
        self.0.push(invocation.clone());
        Ok(CommandOutput::success())
    }
}

#[test]
fn forwarding_apply_and_rollback_use_the_shared_bounded_executor() {
    let (policy, local) = fixture();
    let plan = ForwardingPlanner
        .build(
            GenerationId::new(7).unwrap(),
            PlanSlot::A,
            &local,
            &policy,
            vec!["wlan1".into()],
        )
        .unwrap();
    let mut executor = NetworkExecutor::new(RecordingBackend::default());
    let mut receipt = executor.apply_forwarding(&plan).unwrap();
    executor.rollback_forwarding(&plan, &mut receipt).unwrap();
    let calls = executor.into_backend().0;
    assert_eq!(calls.len(), 2);
    let rollback = calls[1].stdin().unwrap();
    assert!(rollback.contains("-D PREROUTING -i wlan1"));
    assert!(rollback.contains("-F NH_FWD_A"));
    assert!(!rollback.contains("-F PREROUTING"));
    assert!(!rollback.contains("NH_OUT_A"));
}

struct ForwardProbe {
    include_jump: bool,
}

impl ProbeBackend for ForwardProbe {
    fn run(&mut self, command: ProbeCommand) -> Result<ProbeOutput, CapabilityError> {
        Ok(match command {
            ProbeCommand::ListeningSockets => {
                ProbeOutput::new(true, "tcp LISTEN 0 128 127.0.0.1:7893", "")
            }
            ProbeCommand::NetfilterSnapshot(IpFamily::Ipv4, NetfilterTable::Filter) => {
                ProbeOutput::new(
                    true,
                    if self.include_jump {
                        "-N NH_FWD_A\n-A PREROUTING -i wlan1 -m comment --comment nethop:fwd:g=7 -j NH_FWD_A"
                    } else {
                        "-N NH_FWD_A"
                    },
                    "",
                )
            }
            _ => panic!("forwarding health must not inspect unrelated state"),
        })
    }
}

#[test]
fn forwarding_health_checks_listener_owner_chain_and_each_interface() {
    let (policy, local) = fixture();
    let plan = ForwardingPlanner
        .build(
            GenerationId::new(7).unwrap(),
            PlanSlot::A,
            &local,
            &policy,
            vec!["wlan1".into()],
        )
        .unwrap();
    ForwardingPlanVerifier::new(ForwardProbe { include_jump: true }, PORT)
        .unwrap()
        .verify(&plan)
        .unwrap();
    assert_eq!(
        ForwardingPlanVerifier::new(
            ForwardProbe {
                include_jump: false,
            },
            PORT,
        )
        .unwrap()
        .verify(&plan)
        .unwrap_err(),
        ForwardingHealthError::InterfaceJumpMissing
    );
}

#[test]
fn forwarding_rejects_loopback_duplicates_and_generation_mismatch() {
    let (policy, local) = fixture();
    for interfaces in [vec!["lo".into()], vec!["wlan1".into(), "wlan1".into()]] {
        assert!(
            ForwardingPlanner
                .build(
                    GenerationId::new(7).unwrap(),
                    PlanSlot::A,
                    &local,
                    &policy,
                    interfaces,
                )
                .is_err()
        );
    }
    assert!(
        ForwardingPlanner
            .build(
                GenerationId::new(8).unwrap(),
                PlanSlot::A,
                &local,
                &policy,
                vec!["wlan1".into()],
            )
            .is_err()
    );
}
