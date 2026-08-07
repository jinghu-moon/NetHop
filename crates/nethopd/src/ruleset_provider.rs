use std::{collections::BTreeSet, sync::OnceLock};

use nethop_subscription::PINNED_SING_BOX_VERSION;
use serde::Deserialize;
use thiserror::Error;
use url::Url;

const MANIFEST_BYTES: &str = include_str!("../manifests/ruleset-providers-v1.json");
const MANIFEST_SCHEMA: &str = "nethop-ruleset-providers-v1";
const MAX_PROVIDERS: usize = 2;
const MAX_RULE_SET_BYTES: usize = 5 * 1024 * 1024;
const REFRESH_INTERVAL_SECONDS: u64 = 24 * 60 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleSetPurpose {
    CnDomainDirect,
    CnIpDirect,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireManifest {
    schema: String,
    providers: Vec<WireProvider>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireProvider {
    id: String,
    purpose: RuleSetPurpose,
    source_url: String,
    license_spdx: String,
    license_url: String,
    format: String,
    min_sing_box: String,
    max_bytes: usize,
    expected_content_types: Vec<String>,
    refresh_interval_seconds: u64,
    baseline_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleSetProviderManifest {
    schema: String,
    providers: Vec<RuleSetProvider>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleSetProvider {
    id: String,
    purpose: RuleSetPurpose,
    source_url: String,
    license_spdx: String,
    license_url: String,
    format: String,
    min_sing_box: String,
    max_bytes: usize,
    expected_content_types: Vec<String>,
    refresh_interval_seconds: u64,
    baseline_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum RuleSetManifestError {
    #[error("rule-set provider manifest is invalid")]
    Invalid,
}

impl RuleSetProviderManifest {
    pub fn bundled() -> Result<&'static Self, RuleSetManifestError> {
        static MANIFEST: OnceLock<Result<RuleSetProviderManifest, RuleSetManifestError>> =
            OnceLock::new();
        MANIFEST
            .get_or_init(|| Self::parse(MANIFEST_BYTES))
            .as_ref()
            .map_err(|error| *error)
    }

    fn parse(bytes: &str) -> Result<Self, RuleSetManifestError> {
        let wire: WireManifest =
            serde_json::from_str(bytes).map_err(|_| RuleSetManifestError::Invalid)?;
        if wire.schema != MANIFEST_SCHEMA
            || wire.providers.len() != MAX_PROVIDERS
            || wire
                .providers
                .iter()
                .map(|provider| provider.id.as_str())
                .collect::<BTreeSet<_>>()
                .len()
                != MAX_PROVIDERS
        {
            return Err(RuleSetManifestError::Invalid);
        }
        let providers = wire
            .providers
            .into_iter()
            .map(RuleSetProvider::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        let purposes = providers
            .iter()
            .map(|provider| provider.purpose)
            .collect::<BTreeSet<_>>();
        if purposes != BTreeSet::from([RuleSetPurpose::CnDomainDirect, RuleSetPurpose::CnIpDirect])
            || providers.iter().any(|provider| {
                !matches!(
                    (provider.id.as_str(), provider.purpose),
                    ("cn-domain", RuleSetPurpose::CnDomainDirect)
                        | ("cn-ip", RuleSetPurpose::CnIpDirect)
                )
            })
        {
            return Err(RuleSetManifestError::Invalid);
        }
        Ok(Self {
            schema: MANIFEST_SCHEMA.to_owned(),
            providers,
        })
    }

    pub fn schema(&self) -> &str {
        &self.schema
    }

    pub fn providers(&self) -> &[RuleSetProvider] {
        &self.providers
    }
}

impl TryFrom<WireProvider> for RuleSetProvider {
    type Error = RuleSetManifestError;

    fn try_from(provider: WireProvider) -> Result<Self, Self::Error> {
        let url = Url::parse(&provider.source_url).map_err(|_| RuleSetManifestError::Invalid)?;
        let license_url =
            Url::parse(&provider.license_url).map_err(|_| RuleSetManifestError::Invalid)?;
        let valid_digest = provider.baseline_sha256.len() == 64
            && provider
                .baseline_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase());
        if !matches!(url.scheme(), "https")
            || url.host_str() != Some("raw.githubusercontent.com")
            || url.username() != ""
            || url.password().is_some()
            || url.fragment().is_some()
            || !matches!(license_url.scheme(), "https")
            || license_url.host_str() != Some("github.com")
            || license_url.username() != ""
            || license_url.password().is_some()
            || license_url.fragment().is_some()
            || provider.id.is_empty()
            || provider.license_spdx != "GPL-3.0"
            || provider.format != "binary"
            || provider.min_sing_box != PINNED_SING_BOX_VERSION
            || provider.max_bytes != MAX_RULE_SET_BYTES
            || provider.expected_content_types != ["application/octet-stream"]
            || provider.refresh_interval_seconds != REFRESH_INTERVAL_SECONDS
            || !valid_digest
        {
            return Err(RuleSetManifestError::Invalid);
        }
        Ok(Self {
            id: provider.id,
            purpose: provider.purpose,
            source_url: provider.source_url,
            license_spdx: provider.license_spdx,
            license_url: provider.license_url,
            format: provider.format,
            min_sing_box: provider.min_sing_box,
            max_bytes: provider.max_bytes,
            expected_content_types: provider.expected_content_types,
            refresh_interval_seconds: provider.refresh_interval_seconds,
            baseline_sha256: provider.baseline_sha256,
        })
    }
}

impl RuleSetProvider {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub const fn purpose(&self) -> RuleSetPurpose {
        self.purpose
    }
    pub fn source_url(&self) -> &str {
        &self.source_url
    }
    pub fn license_spdx(&self) -> &str {
        &self.license_spdx
    }
    pub fn license_url(&self) -> &str {
        &self.license_url
    }
    pub fn format(&self) -> &str {
        &self.format
    }
    pub fn min_sing_box(&self) -> &str {
        &self.min_sing_box
    }
    pub const fn max_bytes(&self) -> usize {
        self.max_bytes
    }
    pub fn expected_content_types(&self) -> &[String] {
        &self.expected_content_types
    }
    pub const fn refresh_interval_seconds(&self) -> u64 {
        self.refresh_interval_seconds
    }
    pub fn baseline_sha256(&self) -> &str {
        &self.baseline_sha256
    }
}

#[cfg(test)]
mod tests {
    use super::{MANIFEST_BYTES, RuleSetManifestError, RuleSetProviderManifest};

    #[test]
    fn manifest_rejects_unknown_fields_and_id_purpose_swaps() {
        let mut unknown: serde_json::Value = serde_json::from_str(MANIFEST_BYTES).unwrap();
        unknown["unexpected"] = serde_json::json!(true);
        assert_eq!(
            RuleSetProviderManifest::parse(&unknown.to_string()),
            Err(RuleSetManifestError::Invalid)
        );

        let mut swapped: serde_json::Value = serde_json::from_str(MANIFEST_BYTES).unwrap();
        swapped["providers"][0]["purpose"] = serde_json::json!("cn_ip_direct");
        swapped["providers"][1]["purpose"] = serde_json::json!("cn_domain_direct");
        assert_eq!(
            RuleSetProviderManifest::parse(&swapped.to_string()),
            Err(RuleSetManifestError::Invalid)
        );
    }
}
