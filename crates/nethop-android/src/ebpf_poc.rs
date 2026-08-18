#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EbpfPocScope {
    Local,
    SharedNetwork,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EbpfPocFacts {
    bpf_available: bool,
    cgroup_v2_available: bool,
    cgroup_socket_attach_available: bool,
    tc_attach_available: bool,
    core_supported: bool,
}

impl EbpfPocFacts {
    pub const fn new(
        bpf_available: bool,
        cgroup_v2_available: bool,
        cgroup_socket_attach_available: bool,
        tc_attach_available: bool,
        core_supported: bool,
    ) -> Self {
        Self {
            bpf_available,
            cgroup_v2_available,
            cgroup_socket_attach_available,
            tc_attach_available,
            core_supported,
        }
    }

    pub const fn evaluate(self, scope: EbpfPocScope) -> EbpfPocDiagnostic {
        if !self.bpf_available {
            return EbpfPocDiagnostic::BpfUnavailable;
        }
        if !self.cgroup_v2_available {
            return EbpfPocDiagnostic::CgroupV2Unavailable;
        }
        if !self.cgroup_socket_attach_available {
            return EbpfPocDiagnostic::CgroupSocketAttachUnavailable;
        }
        if matches!(scope, EbpfPocScope::SharedNetwork) && !self.tc_attach_available {
            return EbpfPocDiagnostic::TcAttachUnavailable;
        }
        if !self.core_supported {
            return EbpfPocDiagnostic::CoreUnsupported;
        }
        EbpfPocDiagnostic::Eligible
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EbpfPocDiagnostic {
    Eligible,
    BpfUnavailable,
    CgroupV2Unavailable,
    CgroupSocketAttachUnavailable,
    TcAttachUnavailable,
    CoreUnsupported,
}

impl EbpfPocDiagnostic {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Eligible => "ebpf_poc_eligible",
            Self::BpfUnavailable => "ebpf_bpf_unavailable",
            Self::CgroupV2Unavailable => "ebpf_cgroup_v2_unavailable",
            Self::CgroupSocketAttachUnavailable => "ebpf_cgroup_socket_attach_unavailable",
            Self::TcAttachUnavailable => "ebpf_tc_attach_unavailable",
            Self::CoreUnsupported => "ebpf_core_unsupported",
        }
    }
}
