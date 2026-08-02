use serde::{Deserialize, Serialize};
use thiserror::Error;

const MAX_UIDS: usize = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureMode {
    Tproxy,
    Tun,
    Direct,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CapturePolicyError {
    #[error("TPROXY requires a non-zero mark")]
    MissingTproxyMark,
    #[error("TPROXY requires a valid inbound port")]
    MissingTproxyPort,
    #[error("UID policy exceeds the bounded limit")]
    TooManyUids,
    #[error("a UID cannot be included and excluded at the same time")]
    OverlappingUidPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapturePolicy {
    mode: CaptureMode,
    ipv6_guard: bool,
    inbound_port: Option<u16>,
    bypass_mark: Option<u32>,
    include_uids: Vec<u32>,
    exclude_uids: Vec<u32>,
}

impl CapturePolicy {
    pub fn new(
        mode: CaptureMode,
        ipv6_guard: bool,
        inbound_port: Option<u16>,
        bypass_mark: Option<u32>,
        mut include_uids: Vec<u32>,
        mut exclude_uids: Vec<u32>,
    ) -> Result<Self, CapturePolicyError> {
        if mode == CaptureMode::Tproxy {
            if bypass_mark.unwrap_or(0) == 0 {
                return Err(CapturePolicyError::MissingTproxyMark);
            }
            if inbound_port.unwrap_or(0) == 0 {
                return Err(CapturePolicyError::MissingTproxyPort);
            }
        }
        if include_uids.len() > MAX_UIDS || exclude_uids.len() > MAX_UIDS {
            return Err(CapturePolicyError::TooManyUids);
        }
        include_uids.sort_unstable();
        include_uids.dedup();
        exclude_uids.sort_unstable();
        exclude_uids.dedup();
        if include_uids
            .iter()
            .any(|uid| exclude_uids.binary_search(uid).is_ok())
        {
            return Err(CapturePolicyError::OverlappingUidPolicy);
        }
        Ok(Self {
            mode,
            ipv6_guard,
            inbound_port,
            bypass_mark,
            include_uids,
            exclude_uids,
        })
    }

    pub fn captures_uid(&self, uid: u32) -> bool {
        let included =
            self.include_uids.is_empty() || self.include_uids.binary_search(&uid).is_ok();
        included && self.exclude_uids.binary_search(&uid).is_err()
    }

    pub const fn mode(&self) -> CaptureMode {
        self.mode
    }

    pub const fn ipv6_guard(&self) -> bool {
        self.ipv6_guard
    }

    pub const fn inbound_port(&self) -> Option<u16> {
        self.inbound_port
    }

    pub const fn bypass_mark(&self) -> Option<u32> {
        self.bypass_mark
    }

    pub fn include_uids(&self) -> &[u32] {
        &self.include_uids
    }

    pub fn exclude_uids(&self) -> &[u32] {
        &self.exclude_uids
    }
}
