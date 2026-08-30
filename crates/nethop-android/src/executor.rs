use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use nethop_core::GenerationId;
use thiserror::Error;

use crate::{
    capability::IpFamily,
    forwarding::ForwardingPlan,
    plan::{MutationAction, NetworkOperation, NetworkPlan, PlanSlot, PlanStep},
};

const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_OUTPUT_LIMIT: usize = 64 * 1024;
const MAX_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_OUTPUT_LIMIT: usize = 1024 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkProgram {
    IptablesRestore,
    Ip6tablesRestore,
    Ip,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandInvocation {
    program: NetworkProgram,
    arguments: Vec<String>,
    stdin: Option<String>,
}

impl CommandInvocation {
    pub const fn program(&self) -> NetworkProgram {
        self.program
    }

    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }

    pub fn stdin(&self) -> Option<&str> {
        self.stdin.as_deref()
    }

    fn from_operation(operation: &NetworkOperation) -> Self {
        match operation {
            NetworkOperation::Restore { family, payload } => Self {
                program: match family {
                    IpFamily::Ipv4 => NetworkProgram::IptablesRestore,
                    IpFamily::Ipv6 => NetworkProgram::Ip6tablesRestore,
                },
                arguments: vec!["--noflush".to_owned()],
                stdin: Some(payload.clone()),
            },
            NetworkOperation::PolicyRoute {
                action,
                family,
                table,
                device,
                local,
            } => {
                let destination = match family {
                    IpFamily::Ipv4 => "0.0.0.0/0",
                    IpFamily::Ipv6 => "::/0",
                };
                let mut arguments = vec![
                    family_flag(*family).to_owned(),
                    "route".to_owned(),
                    action_name(*action).to_owned(),
                ];
                if *local {
                    arguments.push("local".to_owned());
                }
                arguments.extend([
                    destination.to_owned(),
                    "dev".to_owned(),
                    device.clone(),
                    "table".to_owned(),
                    table.to_string(),
                ]);
                Self {
                    program: NetworkProgram::Ip,
                    arguments,
                    stdin: None,
                }
            }
            NetworkOperation::PolicyRule {
                action,
                family,
                mark,
                mask,
                table,
                priority,
            } => Self {
                program: NetworkProgram::Ip,
                arguments: vec![
                    family_flag(*family).to_owned(),
                    "rule".to_owned(),
                    action_name(*action).to_owned(),
                    "priority".to_owned(),
                    priority.to_string(),
                    "fwmark".to_owned(),
                    format!("0x{mark:x}/0x{mask:x}"),
                    "lookup".to_owned(),
                    table.to_string(),
                ],
                stdin: None,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandOutput {
    success: bool,
}

impl CommandOutput {
    pub const fn success() -> Self {
        Self { success: true }
    }

    pub const fn rejected() -> Self {
        Self { success: false }
    }

    pub const fn is_success(self) -> bool {
        self.success
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum CommandFailure {
    #[error("network command could not be started")]
    Spawn,
    #[error("network command timed out")]
    Timeout,
    #[error("network command output could not be handled")]
    Output,
    #[error("network command target was already absent")]
    Absent,
}

pub trait NetworkCommandBackend {
    fn execute(&mut self, invocation: &CommandInvocation) -> Result<CommandOutput, CommandFailure>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemCommandLimits {
    command_timeout: Duration,
    output_bytes_per_stream: usize,
}

impl SystemCommandLimits {
    pub fn new(
        command_timeout: Duration,
        output_bytes_per_stream: usize,
    ) -> Result<Self, ExecutionError> {
        if command_timeout.is_zero()
            || command_timeout > MAX_COMMAND_TIMEOUT
            || output_bytes_per_stream == 0
            || output_bytes_per_stream > MAX_OUTPUT_LIMIT
        {
            return Err(ExecutionError::InvalidBackend);
        }
        Ok(Self {
            command_timeout,
            output_bytes_per_stream,
        })
    }
}

impl Default for SystemCommandLimits {
    fn default() -> Self {
        Self {
            command_timeout: DEFAULT_COMMAND_TIMEOUT,
            output_bytes_per_stream: DEFAULT_OUTPUT_LIMIT,
        }
    }
}

#[derive(Debug)]
pub struct SystemCommandBackend {
    iptables_restore: PathBuf,
    ip6tables_restore: PathBuf,
    ip: PathBuf,
    limits: SystemCommandLimits,
}

impl SystemCommandBackend {
    pub fn from_system(limits: SystemCommandLimits) -> Result<Self, ExecutionError> {
        Ok(Self {
            iptables_restore: resolve_tool("/system/bin/iptables-restore")?,
            ip6tables_restore: resolve_tool("/system/bin/ip6tables-restore")?,
            ip: resolve_tool("/system/bin/ip")?,
            limits,
        })
    }

    fn program(&self, program: NetworkProgram) -> &Path {
        match program {
            NetworkProgram::IptablesRestore => &self.iptables_restore,
            NetworkProgram::Ip6tablesRestore => &self.ip6tables_restore,
            NetworkProgram::Ip => &self.ip,
        }
    }
}

impl NetworkCommandBackend for SystemCommandBackend {
    fn execute(&mut self, invocation: &CommandInvocation) -> Result<CommandOutput, CommandFailure> {
        run_bounded(self.program(invocation.program), invocation, self.limits)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyReceipt {
    generation: GenerationId,
    slot: PlanSlot,
    completed_steps: usize,
}

impl ApplyReceipt {
    pub const fn generation(&self) -> GenerationId {
        self.generation
    }

    pub const fn slot(&self) -> PlanSlot {
        self.slot
    }

    pub const fn completed_steps(&self) -> usize {
        self.completed_steps
    }
}

#[derive(Debug)]
pub struct NetworkExecutor<B> {
    backend: B,
}

impl<B: NetworkCommandBackend> NetworkExecutor<B> {
    pub const fn new(backend: B) -> Self {
        Self { backend }
    }

    pub fn into_backend(self) -> B {
        self.backend
    }

    pub fn apply(&mut self, plan: &NetworkPlan) -> Result<ApplyReceipt, ExecutionError> {
        self.apply_steps(plan.generation(), plan.slot(), plan.steps())
    }

    pub fn apply_forwarding(
        &mut self,
        plan: &ForwardingPlan,
    ) -> Result<ApplyReceipt, ExecutionError> {
        self.apply_steps(plan.generation(), plan.slot(), plan.steps())
    }

    fn apply_steps(
        &mut self,
        generation: GenerationId,
        slot: PlanSlot,
        steps: &[PlanStep],
    ) -> Result<ApplyReceipt, ExecutionError> {
        let mut completed = 0;
        for (index, step) in steps.iter().enumerate() {
            let invocation = CommandInvocation::from_operation(&step.apply);
            let result = self.backend.execute(&invocation);
            if !matches!(result, Ok(output) if output.is_success()) {
                let rollback_failed = self.rollback_range(steps, index + 1).err();
                return Err(match rollback_failed {
                    Some(rollback_step) => ExecutionError::ApplyRollbackFailed {
                        apply_step: index,
                        rollback_step,
                    },
                    None => ExecutionError::ApplyFailed { step: index },
                });
            }
            completed += 1;
        }
        Ok(ApplyReceipt {
            generation,
            slot,
            completed_steps: completed,
        })
    }

    pub fn rollback(
        &mut self,
        plan: &NetworkPlan,
        receipt: &mut ApplyReceipt,
    ) -> Result<(), ExecutionError> {
        self.rollback_steps(plan.generation(), plan.slot(), plan.steps(), receipt)
    }

    pub fn rollback_forwarding(
        &mut self,
        plan: &ForwardingPlan,
        receipt: &mut ApplyReceipt,
    ) -> Result<(), ExecutionError> {
        self.rollback_steps(plan.generation(), plan.slot(), plan.steps(), receipt)
    }

    fn rollback_steps(
        &mut self,
        generation: GenerationId,
        slot: PlanSlot,
        steps: &[PlanStep],
        receipt: &mut ApplyReceipt,
    ) -> Result<(), ExecutionError> {
        if receipt.generation != generation || receipt.slot != slot {
            return Err(ExecutionError::ReceiptMismatch);
        }
        while receipt.completed_steps > 0 {
            let index = receipt.completed_steps - 1;
            if !self.execute_rollback(steps, index) {
                return Err(ExecutionError::RollbackFailed { step: index });
            }
            receipt.completed_steps -= 1;
        }
        Ok(())
    }

    fn rollback_range(&mut self, steps: &[PlanStep], end_exclusive: usize) -> Result<(), usize> {
        let mut first_failure = None;
        for index in (0..end_exclusive).rev() {
            if !self.execute_rollback(steps, index) && first_failure.is_none() {
                first_failure = Some(index);
            }
        }
        first_failure.map_or(Ok(()), Err)
    }

    fn execute_rollback(&mut self, steps: &[PlanStep], index: usize) -> bool {
        let invocation = CommandInvocation::from_operation(&steps[index].rollback);
        match self.backend.execute(&invocation) {
            Ok(output) => output.is_success(),
            Err(CommandFailure::Absent) => true,
            Err(_) => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionDiagnosticCode {
    InvalidBackend,
    ApplyFailed,
    ApplyRollbackFailed,
    RollbackFailed,
    ReceiptMismatch,
}

impl ExecutionDiagnosticCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidBackend => "network_executor_invalid_backend",
            Self::ApplyFailed => "network_executor_apply_failed",
            Self::ApplyRollbackFailed => "network_executor_apply_rollback_failed",
            Self::RollbackFailed => "network_executor_rollback_failed",
            Self::ReceiptMismatch => "network_executor_receipt_mismatch",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ExecutionError {
    #[error("system network backend is invalid")]
    InvalidBackend,
    #[error("network plan failed at apply step {step}")]
    ApplyFailed { step: usize },
    #[error("network plan failed at apply step {apply_step} and rollback step {rollback_step}")]
    ApplyRollbackFailed {
        apply_step: usize,
        rollback_step: usize,
    },
    #[error("network rollback failed at step {step}")]
    RollbackFailed { step: usize },
    #[error("network rollback receipt does not belong to the supplied plan")]
    ReceiptMismatch,
}

impl ExecutionError {
    pub const fn code(self) -> ExecutionDiagnosticCode {
        match self {
            Self::InvalidBackend => ExecutionDiagnosticCode::InvalidBackend,
            Self::ApplyFailed { .. } => ExecutionDiagnosticCode::ApplyFailed,
            Self::ApplyRollbackFailed { .. } => ExecutionDiagnosticCode::ApplyRollbackFailed,
            Self::RollbackFailed { .. } => ExecutionDiagnosticCode::RollbackFailed,
            Self::ReceiptMismatch => ExecutionDiagnosticCode::ReceiptMismatch,
        }
    }
}

fn family_flag(family: IpFamily) -> &'static str {
    match family {
        IpFamily::Ipv4 => "-4",
        IpFamily::Ipv6 => "-6",
    }
}

fn action_name(action: MutationAction) -> &'static str {
    match action {
        MutationAction::Add => "add",
        MutationAction::Delete => "del",
    }
}

fn resolve_tool(path: &str) -> Result<PathBuf, ExecutionError> {
    let path = Path::new(path);
    if !path.is_absolute() {
        return Err(ExecutionError::InvalidBackend);
    }
    let target = fs::canonicalize(path).map_err(|_| ExecutionError::InvalidBackend)?;
    target
        .is_file()
        // Keep the applet symlink name: iptables dispatches restore mode from argv[0].
        .then(|| path.to_path_buf())
        .ok_or(ExecutionError::InvalidBackend)
}

fn run_bounded(
    program: &Path,
    invocation: &CommandInvocation,
    limits: SystemCommandLimits,
) -> Result<CommandOutput, CommandFailure> {
    let mut child = Command::new(program)
        .args(&invocation.arguments)
        .stdin(if invocation.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| CommandFailure::Spawn)?;
    let stdout = child.stdout.take().ok_or(CommandFailure::Output)?;
    let stderr = child.stderr.take().ok_or(CommandFailure::Output)?;
    let stdout_reader = thread::spawn(move || read_bounded(stdout, limits.output_bytes_per_stream));
    let stderr_reader = thread::spawn(move || read_bounded(stderr, limits.output_bytes_per_stream));
    if let Some(payload) = &invocation.stdin {
        child
            .stdin
            .take()
            .ok_or(CommandFailure::Output)?
            .write_all(payload.as_bytes())
            .map_err(|_| CommandFailure::Output)?;
    }
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|_| CommandFailure::Output)? {
            break status;
        }
        if started.elapsed() >= limits.command_timeout {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(CommandFailure::Timeout);
        }
        thread::sleep(POLL_INTERVAL);
    };
    let _stdout = stdout_reader.join().map_err(|_| CommandFailure::Output)??;
    let stderr = stderr_reader.join().map_err(|_| CommandFailure::Output)??;
    if status.success() {
        Ok(CommandOutput::success())
    } else if indicates_absent(&stderr) {
        Err(CommandFailure::Absent)
    } else {
        Ok(CommandOutput::rejected())
    }
}

fn read_bounded(mut reader: impl Read, limit: usize) -> Result<Vec<u8>, CommandFailure> {
    let mut output = Vec::with_capacity(limit.min(4096));
    let mut buffer = [0_u8; 4096];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|_| CommandFailure::Output)?;
        if read == 0 {
            return Ok(output);
        }
        let remaining = limit.saturating_sub(output.len());
        output.extend_from_slice(&buffer[..read.min(remaining)]);
    }
}

fn indicates_absent(stderr: &[u8]) -> bool {
    let stderr = String::from_utf8_lossy(stderr).to_ascii_lowercase();
    stderr.contains("no such file or directory")
        || stderr.contains("does not exist")
        || stderr.contains("bad rule")
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{ExecutionError, SystemCommandLimits, indicates_absent};

    #[test]
    fn command_limits_are_bounded() {
        assert_eq!(
            SystemCommandLimits::new(Duration::ZERO, 1),
            Err(ExecutionError::InvalidBackend)
        );
    }

    #[test]
    fn known_absence_diagnostics_are_classified_without_becoming_public_messages() {
        assert!(indicates_absent(
            b"RTNETLINK answers: No such file or directory"
        ));
        assert!(indicates_absent(b"iptables: Bad rule"));
    }
}
