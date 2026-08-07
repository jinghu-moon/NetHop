use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{CapabilityError, ProbeBackend, ProbeCommand};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivateDnsMode {
    Off,
    Opportunistic,
    Strict,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DnsSplitStatus {
    Healthy,
    DegradedPrivateDns,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrivateDnsStatus {
    mode: PrivateDnsMode,
    dns_split: DnsSplitStatus,
}

impl PrivateDnsStatus {
    pub const fn mode(self) -> PrivateDnsMode {
        self.mode
    }

    pub const fn dns_split(self) -> DnsSplitStatus {
        self.dns_split
    }

    pub const fn from_mode(mode: PrivateDnsMode) -> Self {
        match mode {
            PrivateDnsMode::Off => Self {
                mode: PrivateDnsMode::Off,
                dns_split: DnsSplitStatus::Healthy,
            },
            PrivateDnsMode::Opportunistic => Self {
                mode: PrivateDnsMode::Opportunistic,
                dns_split: DnsSplitStatus::DegradedPrivateDns,
            },
            PrivateDnsMode::Strict => Self {
                mode: PrivateDnsMode::Strict,
                dns_split: DnsSplitStatus::DegradedPrivateDns,
            },
            PrivateDnsMode::Unknown => Self {
                mode: PrivateDnsMode::Unknown,
                dns_split: DnsSplitStatus::Unknown,
            },
        }
    }

    fn parse(value: &str) -> Self {
        Self::from_mode(match value.trim() {
            "off" => PrivateDnsMode::Off,
            "opportunistic" => PrivateDnsMode::Opportunistic,
            "hostname" => PrivateDnsMode::Strict,
            _ => PrivateDnsMode::Unknown,
        })
    }
}

pub trait PrivateDnsFactsSource {
    fn current(&mut self) -> Result<PrivateDnsStatus, PrivateDnsError>;
}

pub struct CommandPrivateDnsFactsSource<B> {
    backend: B,
}

impl<B> CommandPrivateDnsFactsSource<B> {
    pub const fn new(backend: B) -> Self {
        Self { backend }
    }
}

impl<B: ProbeBackend> PrivateDnsFactsSource for CommandPrivateDnsFactsSource<B> {
    fn current(&mut self) -> Result<PrivateDnsStatus, PrivateDnsError> {
        let output = self
            .backend
            .run(ProbeCommand::PrivateDnsMode)
            .map_err(|_error: CapabilityError| PrivateDnsError::ProbeFailed)?;
        if !output.success() || !output.stderr().trim().is_empty() {
            return Err(PrivateDnsError::ProbeFailed);
        }
        Ok(PrivateDnsStatus::parse(output.stdout()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum PrivateDnsError {
    #[error("private DNS state could not be queried")]
    ProbeFailed,
}
