use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

const CAPABILITY_SCHEMA_VERSION: u16 = 1;
const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(2);
const DEFAULT_OUTPUT_LIMIT: usize = 64 * 1024;
const MAX_COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_OUTPUT_LIMIT: usize = 1024 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IpFamily {
    Ipv4,
    Ipv6,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityStatus {
    Supported,
    NotPresent,
    Unsupported,
    Denied,
    Conflict,
}

impl CapabilityStatus {
    pub const fn is_supported(self) -> bool {
        matches!(self, Self::Supported)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetfilterBackend {
    Legacy,
    NftWrapper,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ResourceCandidate {
    mark: u32,
    mask: u32,
    route_table: u32,
    rule_priority: u32,
}

impl ResourceCandidate {
    pub const fn new(mark: u32, mask: u32, route_table: u32, rule_priority: u32) -> Option<Self> {
        if mark == 0 || mask == 0 || mark & !mask != 0 || route_table == 0 || rule_priority == 0 {
            return None;
        }
        Some(Self {
            mark,
            mask,
            route_table,
            rule_priority,
        })
    }

    pub const fn mark(self) -> u32 {
        self.mark
    }

    pub const fn mask(self) -> u32 {
        self.mask
    }

    pub const fn route_table(self) -> u32 {
        self.route_table
    }

    pub const fn rule_priority(self) -> u32 {
        self.rule_priority
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllocationCapability {
    candidate: ResourceCandidate,
    status: CapabilityStatus,
}

impl AllocationCapability {
    pub const fn new(candidate: ResourceCandidate, status: CapabilityStatus) -> Self {
        Self { candidate, status }
    }

    pub const fn candidate(self) -> ResourceCandidate {
        self.candidate
    }

    pub const fn status(self) -> CapabilityStatus {
        self.status
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyCapability {
    family: IpFamily,
    address: CapabilityStatus,
    netfilter: CapabilityStatus,
    restore: CapabilityStatus,
    tproxy: CapabilityStatus,
    mark: CapabilityStatus,
    owner: CapabilityStatus,
    socket: CapabilityStatus,
    policy_routing: CapabilityStatus,
    chain_namespace: CapabilityStatus,
}

impl FamilyCapability {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        family: IpFamily,
        address: CapabilityStatus,
        netfilter: CapabilityStatus,
        restore: CapabilityStatus,
        tproxy: CapabilityStatus,
        mark: CapabilityStatus,
        owner: CapabilityStatus,
        socket: CapabilityStatus,
        policy_routing: CapabilityStatus,
        chain_namespace: CapabilityStatus,
    ) -> Self {
        Self {
            family,
            address,
            netfilter,
            restore,
            tproxy,
            mark,
            owner,
            socket,
            policy_routing,
            chain_namespace,
        }
    }

    pub const fn family(&self) -> IpFamily {
        self.family
    }

    pub const fn address(&self) -> CapabilityStatus {
        self.address
    }

    pub const fn netfilter(&self) -> CapabilityStatus {
        self.netfilter
    }

    pub const fn restore(&self) -> CapabilityStatus {
        self.restore
    }

    pub const fn tproxy(&self) -> CapabilityStatus {
        self.tproxy
    }

    pub const fn mark(&self) -> CapabilityStatus {
        self.mark
    }

    pub const fn owner(&self) -> CapabilityStatus {
        self.owner
    }

    pub const fn socket(&self) -> CapabilityStatus {
        self.socket
    }

    pub const fn policy_routing(&self) -> CapabilityStatus {
        self.policy_routing
    }

    pub const fn chain_namespace(&self) -> CapabilityStatus {
        self.chain_namespace
    }

    pub const fn supports_tproxy(&self) -> bool {
        self.address.is_supported()
            && self.netfilter.is_supported()
            && self.restore.is_supported()
            && self.tproxy.is_supported()
            && self.mark.is_supported()
            && self.owner.is_supported()
            && self.socket.is_supported()
            && self.policy_routing.is_supported()
            && self.chain_namespace.is_supported()
    }

    pub const fn supports_guard(&self) -> bool {
        self.address.is_supported()
            && self.netfilter.is_supported()
            && self.restore.is_supported()
            && self.owner.is_supported()
            && self.chain_namespace.is_supported()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityReport {
    schema_version: u16,
    android: CapabilityStatus,
    abi: String,
    root: CapabilityStatus,
    selinux_enforcing: bool,
    backend: NetfilterBackend,
    ipv4: FamilyCapability,
    ipv6: FamilyCapability,
    tun: CapabilityStatus,
    active_tunnel: CapabilityStatus,
    inbound_port: u16,
    inbound_port_status: CapabilityStatus,
    allocations: Vec<AllocationCapability>,
}

impl CapabilityReport {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        android: CapabilityStatus,
        abi: impl Into<String>,
        root: CapabilityStatus,
        selinux_enforcing: bool,
        backend: NetfilterBackend,
        ipv4: FamilyCapability,
        ipv6: FamilyCapability,
        tun: CapabilityStatus,
        active_tunnel: CapabilityStatus,
        inbound_port: u16,
        inbound_port_status: CapabilityStatus,
        allocations: Vec<AllocationCapability>,
    ) -> Result<Self, CapabilityError> {
        if inbound_port == 0
            || ipv4.family() != IpFamily::Ipv4
            || ipv6.family() != IpFamily::Ipv6
            || allocations.is_empty()
        {
            return Err(CapabilityError::InvalidPolicy);
        }
        Ok(Self {
            schema_version: CAPABILITY_SCHEMA_VERSION,
            android,
            abi: abi.into(),
            root,
            selinux_enforcing,
            backend,
            ipv4,
            ipv6,
            tun,
            active_tunnel,
            inbound_port,
            inbound_port_status,
            allocations,
        })
    }

    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }

    pub const fn android(&self) -> CapabilityStatus {
        self.android
    }

    pub fn abi(&self) -> &str {
        &self.abi
    }

    pub const fn root(&self) -> CapabilityStatus {
        self.root
    }

    pub const fn selinux_enforcing(&self) -> bool {
        self.selinux_enforcing
    }

    pub const fn backend(&self) -> NetfilterBackend {
        self.backend
    }

    pub const fn ipv4(&self) -> &FamilyCapability {
        &self.ipv4
    }

    pub const fn ipv6(&self) -> &FamilyCapability {
        &self.ipv6
    }

    pub const fn tun(&self) -> CapabilityStatus {
        self.tun
    }

    pub const fn active_tunnel(&self) -> CapabilityStatus {
        self.active_tunnel
    }

    pub const fn inbound_port(&self) -> u16 {
        self.inbound_port
    }

    pub const fn inbound_port_status(&self) -> CapabilityStatus {
        self.inbound_port_status
    }

    pub fn allocations(&self) -> &[AllocationCapability] {
        &self.allocations
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeCommand {
    AndroidRelease,
    AndroidAbi,
    EffectiveUid,
    SelinuxMode,
    NetfilterVersion(IpFamily),
    NetfilterSnapshot(IpFamily),
    NetfilterRestoreHelp(IpFamily),
    TproxyHelp(IpFamily),
    MarkHelp(IpFamily),
    OwnerHelp(IpFamily),
    SocketHelp(IpFamily),
    PolicyRules(IpFamily),
    RouteTable(IpFamily, u32),
    Addresses(IpFamily),
    Links,
    ListeningSockets,
    TunDevice,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeOutput {
    success: bool,
    stdout: String,
    stderr: String,
}

impl ProbeOutput {
    pub fn new(success: bool, stdout: impl Into<String>, stderr: impl Into<String>) -> Self {
        Self {
            success,
            stdout: stdout.into(),
            stderr: stderr.into(),
        }
    }

    pub const fn success(&self) -> bool {
        self.success
    }

    pub fn stdout(&self) -> &str {
        &self.stdout
    }

    pub fn stderr(&self) -> &str {
        &self.stderr
    }
}

pub trait ProbeBackend {
    fn run(&mut self, command: ProbeCommand) -> Result<ProbeOutput, CapabilityError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeLimits {
    command_timeout: Duration,
    output_bytes_per_stream: usize,
}

impl ProbeLimits {
    pub fn new(
        command_timeout: Duration,
        output_bytes_per_stream: usize,
    ) -> Result<Self, CapabilityError> {
        if command_timeout.is_zero()
            || command_timeout > MAX_COMMAND_TIMEOUT
            || output_bytes_per_stream == 0
            || output_bytes_per_stream > MAX_OUTPUT_LIMIT
        {
            return Err(CapabilityError::InvalidPolicy);
        }
        Ok(Self {
            command_timeout,
            output_bytes_per_stream,
        })
    }
}

impl Default for ProbeLimits {
    fn default() -> Self {
        Self {
            command_timeout: DEFAULT_COMMAND_TIMEOUT,
            output_bytes_per_stream: DEFAULT_OUTPUT_LIMIT,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AndroidToolPaths {
    getprop: PathBuf,
    id: PathBuf,
    getenforce: PathBuf,
    iptables: PathBuf,
    ip6tables: PathBuf,
    iptables_save: PathBuf,
    ip6tables_save: PathBuf,
    iptables_restore: PathBuf,
    ip6tables_restore: PathBuf,
    ip: PathBuf,
    ss: PathBuf,
    tun: PathBuf,
}

impl AndroidToolPaths {
    pub fn from_system() -> Result<Self, CapabilityError> {
        Ok(Self {
            getprop: resolve_tool("/system/bin/getprop")?,
            id: resolve_tool("/system/bin/id")?,
            getenforce: resolve_tool("/system/bin/getenforce")?,
            iptables: resolve_tool("/system/bin/iptables")?,
            ip6tables: resolve_tool("/system/bin/ip6tables")?,
            iptables_save: resolve_tool("/system/bin/iptables-save")?,
            ip6tables_save: resolve_tool("/system/bin/ip6tables-save")?,
            iptables_restore: resolve_tool("/system/bin/iptables-restore")?,
            ip6tables_restore: resolve_tool("/system/bin/ip6tables-restore")?,
            ip: resolve_tool("/system/bin/ip")?,
            ss: resolve_tool("/system/bin/ss")?,
            tun: PathBuf::from("/dev/net/tun"),
        })
    }
}

#[derive(Debug)]
pub struct CommandProbeBackend {
    tools: AndroidToolPaths,
    limits: ProbeLimits,
}

impl CommandProbeBackend {
    pub const fn new(tools: AndroidToolPaths, limits: ProbeLimits) -> Self {
        Self { tools, limits }
    }
}

impl ProbeBackend for CommandProbeBackend {
    fn run(&mut self, command: ProbeCommand) -> Result<ProbeOutput, CapabilityError> {
        if command == ProbeCommand::TunDevice {
            return Ok(match fs::metadata(&self.tools.tun) {
                Ok(metadata) if !metadata.is_dir() => ProbeOutput::new(true, "present", ""),
                _ => ProbeOutput::new(false, "", "unavailable"),
            });
        }
        let (program, args) = self.command_spec(command);
        run_bounded(program, &args, self.limits)
    }
}

impl CommandProbeBackend {
    fn command_spec(&self, command: ProbeCommand) -> (&Path, Vec<String>) {
        match command {
            ProbeCommand::AndroidRelease => {
                (&self.tools.getprop, vec!["ro.build.version.release".into()])
            }
            ProbeCommand::AndroidAbi => (&self.tools.getprop, vec!["ro.product.cpu.abi".into()]),
            ProbeCommand::EffectiveUid => (&self.tools.id, vec!["-u".into()]),
            ProbeCommand::SelinuxMode => (&self.tools.getenforce, Vec::new()),
            ProbeCommand::NetfilterVersion(family) => {
                (self.netfilter(family), vec!["--version".into()])
            }
            ProbeCommand::NetfilterSnapshot(family) => (self.netfilter_save(family), Vec::new()),
            ProbeCommand::NetfilterRestoreHelp(family) => {
                (self.netfilter_restore(family), vec!["--help".into()])
            }
            ProbeCommand::TproxyHelp(family) => (
                self.netfilter(family),
                vec!["-j".into(), "TPROXY".into(), "-h".into()],
            ),
            ProbeCommand::MarkHelp(family) => (
                self.netfilter(family),
                vec!["-j".into(), "MARK".into(), "-h".into()],
            ),
            ProbeCommand::OwnerHelp(family) => (
                self.netfilter(family),
                vec!["-m".into(), "owner".into(), "-h".into()],
            ),
            ProbeCommand::SocketHelp(family) => (
                self.netfilter(family),
                vec!["-m".into(), "socket".into(), "-h".into()],
            ),
            ProbeCommand::PolicyRules(family) => (
                &self.tools.ip,
                vec![family_flag(family).into(), "rule".into(), "show".into()],
            ),
            ProbeCommand::RouteTable(family, table) => (
                &self.tools.ip,
                vec![
                    family_flag(family).into(),
                    "route".into(),
                    "show".into(),
                    "table".into(),
                    table.to_string(),
                ],
            ),
            ProbeCommand::Addresses(family) => (
                &self.tools.ip,
                vec![family_flag(family).into(), "address".into(), "show".into()],
            ),
            ProbeCommand::Links => (&self.tools.ip, vec!["link".into(), "show".into()]),
            ProbeCommand::ListeningSockets => (&self.tools.ss, vec!["-H".into(), "-lntu".into()]),
            ProbeCommand::TunDevice => unreachable!("handled without a process"),
        }
    }

    fn netfilter(&self, family: IpFamily) -> &Path {
        match family {
            IpFamily::Ipv4 => &self.tools.iptables,
            IpFamily::Ipv6 => &self.tools.ip6tables,
        }
    }

    fn netfilter_save(&self, family: IpFamily) -> &Path {
        match family {
            IpFamily::Ipv4 => &self.tools.iptables_save,
            IpFamily::Ipv6 => &self.tools.ip6tables_save,
        }
    }

    fn netfilter_restore(&self, family: IpFamily) -> &Path {
        match family {
            IpFamily::Ipv4 => &self.tools.iptables_restore,
            IpFamily::Ipv6 => &self.tools.ip6tables_restore,
        }
    }
}

#[derive(Debug)]
pub struct CapabilityProbe<B> {
    backend: B,
    candidates: Vec<ResourceCandidate>,
    inbound_port: u16,
}

impl<B: ProbeBackend> CapabilityProbe<B> {
    pub fn new(
        backend: B,
        candidates: Vec<ResourceCandidate>,
        inbound_port: u16,
    ) -> Result<Self, CapabilityError> {
        if candidates.is_empty() || candidates.len() > 16 || inbound_port == 0 {
            return Err(CapabilityError::InvalidPolicy);
        }
        let unique = candidates.iter().copied().collect::<BTreeSet<_>>();
        if unique.len() != candidates.len() {
            return Err(CapabilityError::InvalidPolicy);
        }
        Ok(Self {
            backend,
            candidates,
            inbound_port,
        })
    }

    pub fn probe(mut self) -> Result<CapabilityReport, CapabilityError> {
        let release = self.backend.run(ProbeCommand::AndroidRelease)?;
        let abi = self.backend.run(ProbeCommand::AndroidAbi)?;
        let uid = self.backend.run(ProbeCommand::EffectiveUid)?;
        let selinux = self.backend.run(ProbeCommand::SelinuxMode)?;
        let version4 = self
            .backend
            .run(ProbeCommand::NetfilterVersion(IpFamily::Ipv4))?;
        let version6 = self
            .backend
            .run(ProbeCommand::NetfilterVersion(IpFamily::Ipv6))?;
        let ipv4 = self.probe_family(IpFamily::Ipv4, version4.clone())?;
        let ipv6 = self.probe_family(IpFamily::Ipv6, version6.clone())?;
        let sockets = self.backend.run(ProbeCommand::ListeningSockets)?;
        let links = self.backend.run(ProbeCommand::Links)?;
        let tun = self.backend.run(ProbeCommand::TunDevice)?;
        let allocations = self.probe_allocations()?;
        let versions = format!("{} {}", version4.stdout(), version6.stdout()).to_ascii_lowercase();
        let backend = if versions.contains("nf_tables") || versions.contains("nft") {
            NetfilterBackend::NftWrapper
        } else if versions.contains("legacy") {
            NetfilterBackend::Legacy
        } else {
            NetfilterBackend::Unknown
        };

        CapabilityReport::new(
            success_status(&release),
            bounded_label(abi.stdout()),
            if uid.success() && uid.stdout().trim() == "0" {
                CapabilityStatus::Supported
            } else {
                classify_failure(&uid)
            },
            selinux.stdout().trim().eq_ignore_ascii_case("enforcing"),
            backend,
            ipv4,
            ipv6,
            success_status(&tun),
            if links.success() && links_output_has_active_tunnel(links.stdout()) {
                CapabilityStatus::Conflict
            } else {
                success_status(&links)
            },
            self.inbound_port,
            if socket_output_contains_port(sockets.stdout(), self.inbound_port) {
                CapabilityStatus::Conflict
            } else {
                success_status(&sockets)
            },
            allocations,
        )
    }

    fn probe_family(
        &mut self,
        family: IpFamily,
        version: ProbeOutput,
    ) -> Result<FamilyCapability, CapabilityError> {
        let address = self.backend.run(ProbeCommand::Addresses(family))?;
        let snapshot = self.backend.run(ProbeCommand::NetfilterSnapshot(family))?;
        let restore = self
            .backend
            .run(ProbeCommand::NetfilterRestoreHelp(family))?;
        let tproxy = self.backend.run(ProbeCommand::TproxyHelp(family))?;
        let mark = self.backend.run(ProbeCommand::MarkHelp(family))?;
        let owner = self.backend.run(ProbeCommand::OwnerHelp(family))?;
        let socket = self.backend.run(ProbeCommand::SocketHelp(family))?;
        let rules = self.backend.run(ProbeCommand::PolicyRules(family))?;
        let chain_namespace = if snapshot.success() && contains_owned_chain(snapshot.stdout()) {
            CapabilityStatus::Conflict
        } else {
            success_status(&snapshot)
        };
        Ok(FamilyCapability::new(
            family,
            address_status(&address, family),
            success_status(&version),
            success_status(&restore),
            success_status(&tproxy),
            success_status(&mark),
            success_status(&owner),
            success_status(&socket),
            success_status(&rules),
            chain_namespace,
        ))
    }

    fn probe_allocations(&mut self) -> Result<Vec<AllocationCapability>, CapabilityError> {
        let rules4 = self
            .backend
            .run(ProbeCommand::PolicyRules(IpFamily::Ipv4))?;
        let rules6 = self
            .backend
            .run(ProbeCommand::PolicyRules(IpFamily::Ipv6))?;
        let candidates = self.candidates.clone();
        candidates
            .into_iter()
            .map(|candidate| {
                let route4 = self.backend.run(ProbeCommand::RouteTable(
                    IpFamily::Ipv4,
                    candidate.route_table(),
                ))?;
                let route6 = self.backend.run(ProbeCommand::RouteTable(
                    IpFamily::Ipv6,
                    candidate.route_table(),
                ))?;
                let conflict = rules_conflict(rules4.stdout(), candidate)
                    || rules_conflict(rules6.stdout(), candidate)
                    || !route4.stdout().trim().is_empty()
                    || !route6.stdout().trim().is_empty();
                let denied = [
                    rules4.stderr(),
                    rules6.stderr(),
                    route4.stderr(),
                    route6.stderr(),
                ]
                .iter()
                .any(|value| is_denied(value));
                let status = if denied {
                    CapabilityStatus::Denied
                } else if conflict {
                    CapabilityStatus::Conflict
                } else if rules4.success()
                    && rules6.success()
                    && route4.success()
                    && route6.success()
                {
                    CapabilityStatus::Supported
                } else {
                    CapabilityStatus::Unsupported
                };
                Ok(AllocationCapability::new(candidate, status))
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityDiagnosticCode {
    InvalidPolicy,
    InvalidTool,
    CommandSpawnFailed,
    CommandTimedOut,
    CommandOutputFailed,
}

impl CapabilityDiagnosticCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidPolicy => "android_capability_invalid_policy",
            Self::InvalidTool => "android_capability_invalid_tool",
            Self::CommandSpawnFailed => "android_capability_command_spawn_failed",
            Self::CommandTimedOut => "android_capability_command_timed_out",
            Self::CommandOutputFailed => "android_capability_command_output_failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CapabilityError {
    #[error("capability probe policy is invalid")]
    InvalidPolicy,
    #[error("capability probe tool path is invalid")]
    InvalidTool,
    #[error("capability probe command could not be started")]
    CommandSpawnFailed,
    #[error("capability probe command timed out")]
    CommandTimedOut,
    #[error("capability probe output could not be read")]
    CommandOutputFailed,
}

impl CapabilityError {
    pub const fn code(&self) -> CapabilityDiagnosticCode {
        match self {
            Self::InvalidPolicy => CapabilityDiagnosticCode::InvalidPolicy,
            Self::InvalidTool => CapabilityDiagnosticCode::InvalidTool,
            Self::CommandSpawnFailed => CapabilityDiagnosticCode::CommandSpawnFailed,
            Self::CommandTimedOut => CapabilityDiagnosticCode::CommandTimedOut,
            Self::CommandOutputFailed => CapabilityDiagnosticCode::CommandOutputFailed,
        }
    }
}

fn resolve_tool(path: &str) -> Result<PathBuf, CapabilityError> {
    let path = Path::new(path);
    if !path.is_absolute() {
        return Err(CapabilityError::InvalidTool);
    }
    let target = path
        .canonicalize()
        .map_err(|_| CapabilityError::InvalidTool)?;
    target
        .is_file()
        // Android toolbox/toybox and iptables select applets from argv[0].
        .then(|| path.to_path_buf())
        .ok_or(CapabilityError::InvalidTool)
}

fn family_flag(family: IpFamily) -> &'static str {
    match family {
        IpFamily::Ipv4 => "-4",
        IpFamily::Ipv6 => "-6",
    }
}

fn run_bounded(
    program: &Path,
    args: &[String],
    limits: ProbeLimits,
) -> Result<ProbeOutput, CapabilityError> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| CapabilityError::CommandSpawnFailed)?;
    let stdout = child
        .stdout
        .take()
        .ok_or(CapabilityError::CommandOutputFailed)?;
    let stderr = child
        .stderr
        .take()
        .ok_or(CapabilityError::CommandOutputFailed)?;
    let stdout_reader = thread::spawn(move || read_bounded(stdout, limits.output_bytes_per_stream));
    let stderr_reader = thread::spawn(move || read_bounded(stderr, limits.output_bytes_per_stream));
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|_| CapabilityError::CommandOutputFailed)?
        {
            break status;
        }
        if started.elapsed() >= limits.command_timeout {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(CapabilityError::CommandTimedOut);
        }
        thread::sleep(POLL_INTERVAL);
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| CapabilityError::CommandOutputFailed)??;
    let stderr = stderr_reader
        .join()
        .map_err(|_| CapabilityError::CommandOutputFailed)??;
    Ok(ProbeOutput::new(
        status.success(),
        String::from_utf8_lossy(&stdout),
        String::from_utf8_lossy(&stderr),
    ))
}

fn read_bounded(mut reader: impl Read, limit: usize) -> Result<Vec<u8>, CapabilityError> {
    let mut output = Vec::with_capacity(limit.min(4096));
    let mut buffer = [0_u8; 4096];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|_| CapabilityError::CommandOutputFailed)?;
        if read == 0 {
            return Ok(output);
        }
        let remaining = limit.saturating_sub(output.len());
        output.extend_from_slice(&buffer[..read.min(remaining)]);
    }
}

fn success_status(output: &ProbeOutput) -> CapabilityStatus {
    if output.success() {
        CapabilityStatus::Supported
    } else {
        classify_failure(output)
    }
}

fn address_status(output: &ProbeOutput, family: IpFamily) -> CapabilityStatus {
    if !output.success() {
        return classify_failure(output);
    }
    let marker = match family {
        IpFamily::Ipv4 => "inet ",
        IpFamily::Ipv6 => "inet6 ",
    };
    if output.stdout().lines().any(|line| line.contains(marker)) {
        CapabilityStatus::Supported
    } else {
        CapabilityStatus::NotPresent
    }
}

fn classify_failure(output: &ProbeOutput) -> CapabilityStatus {
    if is_denied(output.stderr()) {
        CapabilityStatus::Denied
    } else {
        CapabilityStatus::Unsupported
    }
}

fn is_denied(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.contains("permission denied")
        || value.contains("operation not permitted")
        || value.contains("avc: denied")
}

fn bounded_label(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        .take(64)
        .collect()
}

fn contains_owned_chain(snapshot: &str) -> bool {
    snapshot.lines().any(|line| {
        line.split_ascii_whitespace()
            .any(|token| token.trim_start_matches(':').starts_with("NH_"))
    })
}

fn socket_output_contains_port(output: &str, port: u16) -> bool {
    let needle = format!(":{port}");
    output
        .split_ascii_whitespace()
        .any(|token| token.ends_with(&needle) || token.contains(&format!("{needle} ")))
}

fn rules_conflict(rules: &str, candidate: ResourceCandidate) -> bool {
    rules.lines().any(|line| rule_conflicts(line, candidate))
}

fn rule_conflicts(line: &str, candidate: ResourceCandidate) -> bool {
    let tokens = line.split_ascii_whitespace().collect::<Vec<_>>();
    let priority_conflict = tokens
        .first()
        .and_then(|token| token.strip_suffix(':'))
        .and_then(|value| value.parse::<u32>().ok())
        == Some(candidate.rule_priority());
    let table_conflict = tokens.windows(2).any(|pair| {
        matches!(pair[0], "lookup" | "table")
            && pair[1].parse::<u32>().ok() == Some(candidate.route_table())
    });
    let mark_conflict = tokens
        .windows(2)
        .find(|pair| pair[0] == "fwmark")
        .and_then(|pair| parse_mark_mask(pair[1]))
        .is_some_and(|(mark, mask)| {
            let common = mask & candidate.mask();
            mark & common == candidate.mark() & common
        });
    priority_conflict || table_conflict || mark_conflict
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

fn links_output_has_active_tunnel(output: &str) -> bool {
    output.lines().any(|line| {
        let mut fields = line.splitn(3, ':');
        let _index = fields.next();
        let name = fields.next().map(str::trim).unwrap_or_default();
        let state = fields.next().unwrap_or_default();
        (name.starts_with("tun") || name.starts_with("wg") || name.starts_with("ppp"))
            && state
                .split_once('<')
                .and_then(|(_, flags)| flags.split_once('>'))
                .is_some_and(|(flags, _)| flags.split(',').any(|flag| flag == "UP"))
    })
}

#[cfg(test)]
mod tests {
    use super::{
        CapabilityStatus, ProbeLimits, ProbeOutput, ResourceCandidate, contains_owned_chain,
        links_output_has_active_tunnel, rules_conflict, socket_output_contains_port,
    };
    use std::time::Duration;

    #[test]
    fn resource_candidate_rejects_unmasked_bits() {
        assert!(ResourceCandidate::new(0x100, 0xff, 100, 10000).is_none());
        assert!(ResourceCandidate::new(0x100, 0xff00, 100, 10000).is_some());
    }

    #[test]
    fn limits_are_bounded() {
        assert!(ProbeLimits::new(Duration::ZERO, 1).is_err());
        assert!(ProbeLimits::new(Duration::from_secs(1), 0).is_err());
    }

    #[test]
    fn ownership_and_resource_conflicts_are_exact_enough_for_rejection() {
        let candidate = ResourceCandidate::new(0x100, 0xff00, 100, 10000).unwrap();
        assert!(contains_owned_chain(":NH_OUT - [0:0]"));
        assert!(rules_conflict(
            "10000: from all fwmark 0x100 lookup 100",
            candidate
        ));
        assert!(!rules_conflict("10001: from all lookup 101", candidate));
        assert!(!rules_conflict(
            "16000: from all fwmark 0x10063/0x1ffff lookup 97",
            candidate
        ));
        assert!(rules_conflict(
            "16000: from all fwmark 0x10163/0x1ffff lookup 97",
            candidate
        ));
    }

    #[test]
    fn listening_port_detection_handles_android_ss_output() {
        assert!(socket_output_contains_port(
            "udp UNCONN 0 0 127.0.0.1:7893 0.0.0.0:*",
            7893
        ));
        assert!(!socket_output_contains_port(
            "tcp LISTEN 0 128 *:443 *:*",
            7893
        ));
    }

    #[test]
    fn denied_output_maps_without_exposing_details() {
        let output = ProbeOutput::new(false, "", "avc: denied secret");
        assert_eq!(super::success_status(&output), CapabilityStatus::Denied);
    }

    #[test]
    fn successful_address_query_distinguishes_absent_family() {
        let output = ProbeOutput::new(true, "1: lo: <LOOPBACK>", "");
        assert_eq!(
            super::address_status(&output, super::IpFamily::Ipv6),
            CapabilityStatus::NotPresent
        );
    }

    #[test]
    fn only_active_tunnel_like_interfaces_are_conflicts() {
        assert!(links_output_has_active_tunnel(
            "24: tun0: <POINTOPOINT,UP,LOWER_UP> mtu 1500"
        ));
        assert!(!links_output_has_active_tunnel(
            "4: ip_vti0@NONE: <NOARP> mtu 1480 state DOWN"
        ));
    }
}
