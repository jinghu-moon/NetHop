use std::{cell::RefCell, rc::Rc};

use nethop_android::{
    AllocationCapability, CapabilityError, CapabilityProbe, CapabilityReport, CapabilityStatus,
    CommandFailure, CommandInvocation, CommandOutput, ExecutionError, FamilyCapability, IpFamily,
    NetfilterBackend, NetworkCommandBackend, NetworkExecutor, NetworkOperationKind, NetworkPlanner,
    NetworkProgram, PlanSlot, ProbeBackend, ProbeCommand, ProbeOutput, ResourceCandidate,
};
use nethop_core::{CaptureMode, CapturePolicy, GenerationId};

const PORT: u16 = 7893;

fn candidate(mark: u32, table: u32, priority: u32) -> ResourceCandidate {
    ResourceCandidate::new(mark, 0xff00, table, priority).unwrap()
}

fn family(family: IpFamily, tproxy: CapabilityStatus) -> FamilyCapability {
    FamilyCapability::new(
        family,
        CapabilityStatus::Supported,
        CapabilityStatus::Supported,
        CapabilityStatus::Supported,
        tproxy,
        CapabilityStatus::Supported,
        CapabilityStatus::Supported,
        CapabilityStatus::Supported,
        CapabilityStatus::Supported,
        CapabilityStatus::Supported,
    )
}

fn report(
    ipv6_tproxy: CapabilityStatus,
    allocations: Vec<AllocationCapability>,
) -> CapabilityReport {
    CapabilityReport::new(
        CapabilityStatus::Supported,
        "arm64-v8a",
        CapabilityStatus::Supported,
        true,
        NetfilterBackend::NftWrapper,
        family(IpFamily::Ipv4, CapabilityStatus::Supported),
        family(IpFamily::Ipv6, ipv6_tproxy),
        CapabilityStatus::Supported,
        CapabilityStatus::Supported,
        PORT,
        CapabilityStatus::Supported,
        allocations,
    )
    .unwrap()
}

fn policy(ipv6_guard: bool) -> CapturePolicy {
    CapturePolicy::new(
        CaptureMode::Tproxy,
        ipv6_guard,
        Some(PORT),
        Some(0x200),
        vec![10_001, 10_002],
        vec![10_003],
    )
    .unwrap()
}

fn full_plan() -> nethop_android::NetworkPlan {
    let allocation = candidate(0x100, 100, 10_000);
    NetworkPlanner
        .build_tproxy(
            GenerationId::new(7).unwrap(),
            PlanSlot::A,
            &policy(true),
            &report(
                CapabilityStatus::Supported,
                vec![AllocationCapability::new(
                    allocation,
                    CapabilityStatus::Supported,
                )],
            ),
        )
        .unwrap()
}

#[derive(Debug)]
struct ReadOnlyProbe {
    seen: Rc<RefCell<Vec<ProbeCommand>>>,
    chain_conflict: bool,
}

impl ProbeBackend for ReadOnlyProbe {
    fn run(&mut self, command: ProbeCommand) -> Result<ProbeOutput, CapabilityError> {
        self.seen.borrow_mut().push(command);
        let output = match command {
            ProbeCommand::AndroidRelease => ProbeOutput::new(true, "14", ""),
            ProbeCommand::AndroidAbi => ProbeOutput::new(true, "arm64-v8a", ""),
            ProbeCommand::EffectiveUid => ProbeOutput::new(true, "0", ""),
            ProbeCommand::SelinuxMode => ProbeOutput::new(true, "Enforcing", ""),
            ProbeCommand::NetfilterVersion(_) => {
                ProbeOutput::new(true, "iptables v1.8.9 (nf_tables)", "")
            }
            ProbeCommand::NetfilterSnapshot(_) if self.chain_conflict => {
                ProbeOutput::new(true, ":NH_OUT_A - [0:0]", "")
            }
            ProbeCommand::NetfilterSnapshot(_) => ProbeOutput::new(true, "*mangle\nCOMMIT", ""),
            ProbeCommand::Addresses(IpFamily::Ipv4) => {
                ProbeOutput::new(true, "inet 192.0.2.2/24", "")
            }
            ProbeCommand::Addresses(IpFamily::Ipv6) => {
                ProbeOutput::new(true, "inet6 2001:db8::2/64", "")
            }
            ProbeCommand::Links => ProbeOutput::new(
                true,
                "1: lo: <LOOPBACK,UP> mtu 65536\n4: ip_vti0@NONE: <NOARP> mtu 1480",
                "",
            ),
            ProbeCommand::PolicyRules(_) => ProbeOutput::new(true, "0: from all lookup local", ""),
            ProbeCommand::RouteTable(_, _) | ProbeCommand::ListeningSockets => {
                ProbeOutput::new(true, "", "")
            }
            ProbeCommand::TunDevice
            | ProbeCommand::NetfilterRestoreHelp(_)
            | ProbeCommand::TproxyHelp(_)
            | ProbeCommand::MarkHelp(_)
            | ProbeCommand::OwnerHelp(_)
            | ProbeCommand::SocketHelp(_) => ProbeOutput::new(true, "supported", ""),
        };
        Ok(output)
    }
}

#[test]
fn capability_probe_is_read_only_versioned_and_detects_nft_wrapper() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let report = CapabilityProbe::new(
        ReadOnlyProbe {
            seen: Rc::clone(&seen),
            chain_conflict: false,
        },
        vec![candidate(0x100, 100, 10_000)],
        PORT,
    )
    .unwrap()
    .probe()
    .unwrap();

    assert_eq!(report.schema_version(), 1);
    assert_eq!(report.abi(), "arm64-v8a");
    assert_eq!(report.backend(), NetfilterBackend::NftWrapper);
    assert!(report.selinux_enforcing());
    assert!(report.ipv4().supports_tproxy());
    assert!(report.ipv6().supports_tproxy());
    assert_eq!(
        report.allocations()[0].status(),
        CapabilityStatus::Supported
    );
    assert!(
        seen.borrow()
            .iter()
            .all(|command| !matches!(command, ProbeCommand::RouteTable(_, 0)))
    );
}

#[test]
fn capability_probe_rejects_preexisting_nethop_chain_namespace() {
    let report = CapabilityProbe::new(
        ReadOnlyProbe {
            seen: Rc::new(RefCell::new(Vec::new())),
            chain_conflict: true,
        },
        vec![candidate(0x100, 100, 10_000)],
        PORT,
    )
    .unwrap()
    .probe()
    .unwrap();

    assert_eq!(report.ipv4().chain_namespace(), CapabilityStatus::Conflict);
    assert!(!report.ipv4().supports_tproxy());
}

#[test]
fn pure_plan_is_deterministic_owned_and_never_flushes_system_tables() {
    let first = full_plan();
    let second = full_plan();
    assert_eq!(first, second);
    assert_eq!(first.operation_kinds().len(), 6);
    assert!(!first.ipv6_guarded());

    for (_, payload) in first.restore_payloads() {
        assert!(payload.contains("nethop:g=7"));
        assert!(payload.contains("NH_"));
        assert!(!payload.contains("-F OUTPUT"));
        assert!(!payload.contains("-F PREROUTING"));
        assert!(!payload.contains("--flush"));
        assert!(!payload.contains("iptables"));
    }
}

#[test]
fn ipv6_guard_precedes_ipv4_capture_when_ipv6_tproxy_is_unavailable() {
    let allocation = candidate(0x100, 100, 10_000);
    let plan = NetworkPlanner
        .build_tproxy(
            GenerationId::new(8).unwrap(),
            PlanSlot::B,
            &policy(true),
            &report(
                CapabilityStatus::Unsupported,
                vec![AllocationCapability::new(
                    allocation,
                    CapabilityStatus::Supported,
                )],
            ),
        )
        .unwrap();
    assert!(plan.ipv6_guarded());
    assert_eq!(
        plan.operation_kinds().next(),
        Some(NetworkOperationKind::Ipv6GuardRestore)
    );
    let payload = plan.restore_payloads().next().unwrap().1;
    assert!(payload.contains("*filter"));
    assert!(payload.contains("-j DROP"));
    assert!(payload.contains("--uid-owner 10001"));
}

#[test]
fn ipv6_without_proxy_or_guard_is_rejected_before_mutation() {
    let allocation = candidate(0x100, 100, 10_000);
    let error = NetworkPlanner
        .build_tproxy(
            GenerationId::new(9).unwrap(),
            PlanSlot::A,
            &policy(false),
            &report(
                CapabilityStatus::Unsupported,
                vec![AllocationCapability::new(
                    allocation,
                    CapabilityStatus::Supported,
                )],
            ),
        )
        .unwrap_err();
    assert_eq!(error.code().as_str(), "network_plan_ipv6_leak_risk");
}

#[test]
fn planner_selects_first_conflict_free_resource_candidate() {
    let conflict = candidate(0x100, 100, 10_000);
    let available = candidate(0x300, 101, 10_001);
    let plan = NetworkPlanner
        .build_tproxy(
            GenerationId::new(10).unwrap(),
            PlanSlot::A,
            &policy(true),
            &report(
                CapabilityStatus::Supported,
                vec![
                    AllocationCapability::new(conflict, CapabilityStatus::Conflict),
                    AllocationCapability::new(available, CapabilityStatus::Supported),
                ],
            ),
        )
        .unwrap();
    assert_eq!(plan.allocation(), available);
}

#[test]
fn planner_rejects_active_or_unobservable_tunnel_state() {
    let allocation = candidate(0x100, 100, 10_000);
    for tunnel_status in [CapabilityStatus::Conflict, CapabilityStatus::Denied] {
        let capabilities = CapabilityReport::new(
            CapabilityStatus::Supported,
            "arm64-v8a",
            CapabilityStatus::Supported,
            true,
            NetfilterBackend::Legacy,
            family(IpFamily::Ipv4, CapabilityStatus::Supported),
            family(IpFamily::Ipv6, CapabilityStatus::Supported),
            CapabilityStatus::Supported,
            tunnel_status,
            PORT,
            CapabilityStatus::Supported,
            vec![AllocationCapability::new(
                allocation,
                CapabilityStatus::Supported,
            )],
        )
        .unwrap();
        let error = NetworkPlanner
            .build_tproxy(
                GenerationId::new(11).unwrap(),
                PlanSlot::A,
                &policy(true),
                &capabilities,
            )
            .unwrap_err();
        assert_eq!(
            error.code().as_str(),
            "network_plan_existing_tunnel_conflict"
        );
    }
}

#[derive(Debug, Default)]
struct RecordingBackend {
    calls: Vec<CommandInvocation>,
    fail_at: Vec<usize>,
}

impl RecordingBackend {
    fn failing_at(index: usize) -> Self {
        Self {
            calls: Vec::new(),
            fail_at: vec![index],
        }
    }

    fn failing_at_many(indices: &[usize]) -> Self {
        Self {
            calls: Vec::new(),
            fail_at: indices.to_vec(),
        }
    }
}

impl NetworkCommandBackend for RecordingBackend {
    fn execute(&mut self, invocation: &CommandInvocation) -> Result<CommandOutput, CommandFailure> {
        let index = self.calls.len();
        self.calls.push(invocation.clone());
        if self.fail_at.contains(&index) {
            Ok(CommandOutput::rejected())
        } else {
            Ok(CommandOutput::success())
        }
    }
}

#[test]
fn executor_uses_only_typed_programs_and_rolls_back_in_reverse_order() {
    let plan = full_plan();
    let mut executor = NetworkExecutor::new(RecordingBackend::default());
    let mut receipt = executor.apply(&plan).unwrap();
    let apply_count = receipt.completed_steps();
    executor.rollback(&plan, &mut receipt).unwrap();
    assert_eq!(receipt.completed_steps(), 0);
    executor.rollback(&plan, &mut receipt).unwrap();

    let backend = executor.into_backend();
    assert_eq!(backend.calls.len(), apply_count * 2);
    assert_eq!(
        backend.calls[apply_count].program(),
        NetworkProgram::Ip6tablesRestore
    );
    assert_eq!(backend.calls.last().unwrap().program(), NetworkProgram::Ip);
    assert!(backend.calls.iter().all(|call| matches!(
        call.program(),
        NetworkProgram::Ip | NetworkProgram::IptablesRestore | NetworkProgram::Ip6tablesRestore
    )));
    assert!(
        backend
            .calls
            .iter()
            .filter(|call| call.stdin().is_some())
            .all(|call| call.arguments() == ["--noflush"])
    );
}

#[test]
fn every_apply_failure_triggers_reverse_rollback() {
    let plan = full_plan();
    let step_count = plan.operation_kinds().len();
    for failed_step in 0..step_count {
        let mut executor = NetworkExecutor::new(RecordingBackend::failing_at(failed_step));
        assert_eq!(
            executor.apply(&plan),
            Err(ExecutionError::ApplyFailed { step: failed_step })
        );
        let backend = executor.into_backend();
        assert_eq!(backend.calls.len(), failed_step + 1 + failed_step + 1);
    }
}

#[test]
fn rollback_failure_is_reported_after_all_possible_cleanup_is_attempted() {
    let plan = full_plan();
    let mut executor = NetworkExecutor::new(RecordingBackend::failing_at_many(&[2, 3]));
    assert_eq!(
        executor.apply(&plan),
        Err(ExecutionError::ApplyRollbackFailed {
            apply_step: 2,
            rollback_step: 2,
        })
    );
    let backend = executor.into_backend();
    assert_eq!(backend.calls.len(), 6);
}
