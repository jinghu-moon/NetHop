use std::fmt;

use thiserror::Error;

use crate::{CapabilityError, ProbeBackend, ProbeCommand};

const MAX_SCENES: usize = 64;
const MAX_SCENE_ID_BYTES: usize = 64;
const MAX_SSID_BYTES: usize = 32;

#[derive(Clone, PartialEq, Eq)]
pub struct WifiNetworkFacts {
    ssid: Option<String>,
    bssid: Option<[u8; 6]>,
}

impl WifiNetworkFacts {
    pub fn new(ssid: Option<String>, bssid: Option<String>) -> Result<Self, WifiSceneError> {
        let ssid = ssid
            .filter(|value| value != "<unknown ssid>")
            .map(validate_ssid)
            .transpose()?;
        let bssid = bssid.map(|value| parse_bssid(&value)).transpose()?;
        if ssid.is_none() && bssid.is_none() {
            return Err(WifiSceneError::NetworkUnavailable);
        }
        Ok(Self { ssid, bssid })
    }

    pub fn from_android_status(output: &str) -> Result<Self, WifiSceneError> {
        if output.len() > 64 * 1024 {
            return Err(WifiSceneError::InvalidStatus);
        }
        let mut ssid = None;
        let mut bssid = None;
        for field in output
            .lines()
            .flat_map(|line| line.split(','))
            .map(str::trim)
        {
            if let Some(value) = field.strip_prefix("BSSID:") {
                let value = value.trim();
                if !value.is_empty() {
                    bssid = Some(value.to_owned());
                }
            } else if let Some((prefix, value)) = field.split_once("SSID:")
                && !prefix.ends_with('B')
            {
                let value = value.trim().trim_matches('"');
                if !value.is_empty() {
                    ssid = Some(value.to_owned());
                }
            }
        }
        Self::new(ssid, bssid)
    }

    fn ssid(&self) -> Option<&str> {
        self.ssid.as_deref()
    }

    fn bssid(&self) -> Option<[u8; 6]> {
        self.bssid
    }
}

pub trait WifiFactsSource {
    fn current(&mut self) -> Result<WifiNetworkFacts, WifiSceneError>;
}

#[derive(Debug)]
pub struct CommandWifiFactsSource<B> {
    backend: B,
}

impl<B> CommandWifiFactsSource<B> {
    pub const fn new(backend: B) -> Self {
        Self { backend }
    }
}

impl<B: ProbeBackend> WifiFactsSource for CommandWifiFactsSource<B> {
    fn current(&mut self) -> Result<WifiNetworkFacts, WifiSceneError> {
        let output = self
            .backend
            .run(ProbeCommand::WifiStatus)
            .map_err(|_error: CapabilityError| WifiSceneError::ProbeFailed)?;
        if !output.success() {
            return Err(WifiSceneError::ProbeFailed);
        }
        WifiNetworkFacts::from_android_status(output.stdout())
    }
}

impl fmt::Debug for WifiNetworkFacts {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WifiNetworkFacts")
            .field("ssid", &self.ssid.as_ref().map(|_| "[REDACTED]"))
            .field("bssid", &self.bssid.map(|_| "[REDACTED]"))
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WifiSceneAction {
    EnableProxy,
    DisableProxy,
}

impl WifiSceneAction {
    pub const fn service_enabled(self) -> bool {
        matches!(self, Self::EnableProxy)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct WifiSceneRule {
    id: String,
    ssid: Option<String>,
    bssid: Option<[u8; 6]>,
    action: WifiSceneAction,
}

impl WifiSceneRule {
    pub fn new(
        id: impl Into<String>,
        ssid: Option<String>,
        bssid: Option<String>,
        action: WifiSceneAction,
    ) -> Result<Self, WifiSceneError> {
        let id = id.into();
        if id.is_empty()
            || id.len() > MAX_SCENE_ID_BYTES
            || !id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(WifiSceneError::InvalidRule);
        }
        let ssid = ssid.map(validate_ssid).transpose()?;
        let bssid = bssid.map(|value| parse_bssid(&value)).transpose()?;
        if ssid.is_none() && bssid.is_none() {
            return Err(WifiSceneError::InvalidRule);
        }
        Ok(Self {
            id,
            ssid,
            bssid,
            action,
        })
    }

    fn matches(&self, facts: &WifiNetworkFacts) -> bool {
        self.ssid
            .as_deref()
            .is_none_or(|ssid| facts.ssid() == Some(ssid))
            && self.bssid.is_none_or(|bssid| facts.bssid() == Some(bssid))
    }

    fn specificity(&self) -> u8 {
        u8::from(self.ssid.is_some()) + 2 * u8::from(self.bssid.is_some())
    }
}

impl fmt::Debug for WifiSceneRule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WifiSceneRule")
            .field("id", &self.id)
            .field("ssid", &self.ssid.as_ref().map(|_| "[REDACTED]"))
            .field("bssid", &self.bssid.map(|_| "[REDACTED]"))
            .field("action", &self.action)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiSceneDecision {
    scene_id: String,
    action: WifiSceneAction,
}

impl WifiSceneDecision {
    pub fn scene_id(&self) -> &str {
        &self.scene_id
    }

    pub const fn action(&self) -> WifiSceneAction {
        self.action
    }

    pub const fn requires_reconcile(&self) -> bool {
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiSceneMatcher {
    rules: Vec<WifiSceneRule>,
}

impl WifiSceneMatcher {
    pub fn new(rules: Vec<WifiSceneRule>) -> Result<Self, WifiSceneError> {
        if rules.len() > MAX_SCENES {
            return Err(WifiSceneError::TooManyRules);
        }
        for (index, rule) in rules.iter().enumerate() {
            if rules[..index].iter().any(|previous| {
                previous.id == rule.id
                    || (previous.ssid == rule.ssid && previous.bssid == rule.bssid)
            }) {
                return Err(WifiSceneError::DuplicateRule);
            }
        }
        Ok(Self { rules })
    }

    pub fn evaluate(&self, facts: &WifiNetworkFacts) -> Option<WifiSceneDecision> {
        let mut selected: Option<&WifiSceneRule> = None;
        for rule in self.rules.iter().filter(|rule| rule.matches(facts)) {
            match selected {
                None => selected = Some(rule),
                Some(current) if rule.specificity() > current.specificity() => {
                    selected = Some(rule);
                }
                _ => {}
            }
        }
        selected.map(|rule| WifiSceneDecision {
            scene_id: rule.id.clone(),
            action: rule.action,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum WifiSceneError {
    #[error("current Wi-Fi network is unavailable")]
    NetworkUnavailable,
    #[error("Wi-Fi scene rule is invalid")]
    InvalidRule,
    #[error("too many Wi-Fi scene rules")]
    TooManyRules,
    #[error("Wi-Fi scene rule is duplicated")]
    DuplicateRule,
    #[error("Wi-Fi status probe failed")]
    ProbeFailed,
    #[error("Wi-Fi status output is invalid or exceeds its bound")]
    InvalidStatus,
}

fn validate_ssid(value: String) -> Result<String, WifiSceneError> {
    if value.is_empty() || value.len() > MAX_SSID_BYTES || value.chars().any(char::is_control) {
        return Err(WifiSceneError::InvalidRule);
    }
    Ok(value)
}

fn parse_bssid(value: &str) -> Result<[u8; 6], WifiSceneError> {
    let parts = value.split(':').collect::<Vec<_>>();
    if parts.len() != 6 || parts.iter().any(|part| part.len() != 2) {
        return Err(WifiSceneError::InvalidRule);
    }
    let mut output = [0_u8; 6];
    for (index, part) in parts.into_iter().enumerate() {
        output[index] = u8::from_str_radix(part, 16).map_err(|_| WifiSceneError::InvalidRule)?;
    }
    Ok(output)
}
