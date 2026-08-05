use serde::{Deserialize, Serialize};
use thiserror::Error;

const MAX_UIDS: usize = 2048;
const MAX_INTERFACE_PATTERNS: usize = 64;
const MAX_INTERFACE_PATTERN_BYTES: usize = 64;

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
    #[error("non-direct capture must enable TCP or UDP")]
    MissingTransport,
    #[error("interface capture policy is invalid")]
    InvalidInterfacePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterfacePolicy {
    mobile: bool,
    wifi: bool,
    include: Vec<String>,
    exclude: Vec<String>,
}

impl InterfacePolicy {
    pub fn new(
        mobile: bool,
        wifi: bool,
        include: Vec<String>,
        exclude: Vec<String>,
    ) -> Result<Self, CapturePolicyError> {
        if (!mobile && !wifi && include.is_empty())
            || include.len() > MAX_INTERFACE_PATTERNS
            || exclude.len() > MAX_INTERFACE_PATTERNS
            || !valid_patterns(&include)
            || !valid_patterns(&exclude)
            || include.iter().any(|value| exclude.contains(value))
        {
            return Err(CapturePolicyError::InvalidInterfacePolicy);
        }
        Ok(Self {
            mobile,
            wifi,
            include,
            exclude,
        })
    }

    pub const fn mobile(&self) -> bool {
        self.mobile
    }

    pub const fn wifi(&self) -> bool {
        self.wifi
    }

    pub fn include(&self) -> &[String] {
        &self.include
    }

    pub fn exclude(&self) -> &[String] {
        &self.exclude
    }

    pub fn is_unrestricted(&self) -> bool {
        self.mobile && self.wifi && self.include.is_empty() && self.exclude.is_empty()
    }
}

impl Default for InterfacePolicy {
    fn default() -> Self {
        Self {
            mobile: true,
            wifi: true,
            include: Vec::new(),
            exclude: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapturePolicy {
    mode: CaptureMode,
    proxy_tcp: bool,
    proxy_udp: bool,
    ipv6_guard: bool,
    inbound_port: Option<u16>,
    bypass_mark: Option<u32>,
    include_uids: Vec<u32>,
    exclude_uids: Vec<u32>,
    interface_policy: InterfacePolicy,
}

impl CapturePolicy {
    pub fn new(
        mode: CaptureMode,
        ipv6_guard: bool,
        inbound_port: Option<u16>,
        bypass_mark: Option<u32>,
        include_uids: Vec<u32>,
        exclude_uids: Vec<u32>,
    ) -> Result<Self, CapturePolicyError> {
        Self::new_with_protocols(
            mode,
            true,
            true,
            ipv6_guard,
            inbound_port,
            bypass_mark,
            include_uids,
            exclude_uids,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_protocols(
        mode: CaptureMode,
        proxy_tcp: bool,
        proxy_udp: bool,
        ipv6_guard: bool,
        inbound_port: Option<u16>,
        bypass_mark: Option<u32>,
        mut include_uids: Vec<u32>,
        mut exclude_uids: Vec<u32>,
    ) -> Result<Self, CapturePolicyError> {
        if mode != CaptureMode::Direct && !proxy_tcp && !proxy_udp {
            return Err(CapturePolicyError::MissingTransport);
        }
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
            proxy_tcp,
            proxy_udp,
            ipv6_guard,
            inbound_port,
            bypass_mark,
            include_uids,
            exclude_uids,
            interface_policy: InterfacePolicy::default(),
        })
    }

    pub fn with_interface_policy(
        mut self,
        interface_policy: InterfacePolicy,
    ) -> Result<Self, CapturePolicyError> {
        if self.mode == CaptureMode::Direct && !interface_policy.is_unrestricted() {
            return Err(CapturePolicyError::InvalidInterfacePolicy);
        }
        self.interface_policy = interface_policy;
        Ok(self)
    }

    pub fn captures_uid(&self, uid: u32) -> bool {
        let included =
            self.include_uids.is_empty() || self.include_uids.binary_search(&uid).is_ok();
        included && self.exclude_uids.binary_search(&uid).is_err()
    }

    pub const fn mode(&self) -> CaptureMode {
        self.mode
    }

    pub const fn proxy_tcp(&self) -> bool {
        self.proxy_tcp
    }

    pub const fn proxy_udp(&self) -> bool {
        self.proxy_udp
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

    pub const fn interface_policy(&self) -> &InterfacePolicy {
        &self.interface_policy
    }
}

fn valid_patterns(patterns: &[String]) -> bool {
    patterns.iter().all(|pattern| {
        !pattern.is_empty()
            && pattern.len() <= MAX_INTERFACE_PATTERN_BYTES
            && pattern.bytes().all(|byte| {
                byte.is_ascii_alphanumeric()
                    || matches!(byte, b'_' | b'-' | b'.' | b'+' | b'*' | b'?')
            })
    })
}
