use std::{collections::BTreeSet, fs, path::PathBuf};

use nethop_subscription::Digest;
use nethopd::{RuleSetProviderManifest, RuleSetPurpose};

#[test]
fn bundled_provider_manifest_is_strict_bounded_and_license_bound() {
    let manifest = RuleSetProviderManifest::bundled().unwrap();
    assert_eq!(manifest.schema(), "nethop-ruleset-providers-v1");
    assert_eq!(manifest.providers().len(), 2);
    assert_eq!(
        manifest
            .providers()
            .iter()
            .map(|provider| provider.id())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["cn-domain", "cn-ip"])
    );
    for provider in manifest.providers() {
        assert!(
            provider
                .source_url()
                .starts_with("https://raw.githubusercontent.com/")
        );
        assert_eq!(provider.license_spdx(), "GPL-3.0");
        assert!(provider.license_url().starts_with("https://github.com/"));
        assert_eq!(provider.format(), "binary");
        assert_eq!(provider.min_sing_box(), "1.13.15");
        assert_eq!(provider.max_bytes(), 5 * 1024 * 1024);
        assert_eq!(
            provider.expected_content_types(),
            ["application/octet-stream"]
        );
        assert_eq!(provider.refresh_interval_seconds(), 24 * 60 * 60);
        assert_eq!(provider.baseline_sha256().len(), 64);
    }
    assert_eq!(
        manifest.providers()[0].purpose(),
        RuleSetPurpose::CnDomainDirect
    );
    assert_eq!(
        manifest.providers()[1].purpose(),
        RuleSetPurpose::CnIpDirect
    );
}

#[test]
fn provider_baseline_digests_match_the_shipped_rule_sets() {
    let manifest = RuleSetProviderManifest::bundled().unwrap();
    let module = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../module/rulesets");
    for provider in manifest.providers() {
        let file = match provider.purpose() {
            RuleSetPurpose::CnDomainDirect => "cn-domain.srs",
            RuleSetPurpose::CnIpDirect => "cn-ip.srs",
        };
        let bytes = fs::read(module.join(file)).unwrap();
        assert_eq!(Digest::sha256(&bytes).hex(), provider.baseline_sha256());
    }
}
