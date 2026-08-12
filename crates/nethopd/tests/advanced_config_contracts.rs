use std::fs;

use nethop_core::{CaptureMode, TunStack};
use nethop_subscription::RequestProfile;
use nethopd::{
    ApplicationMode, ApplyImpact, CaptureIntent, ChangeKind, ConfigError, ConfigStore, DnsMode,
    Ipv6Mode, LogLevel, OutboundMode, SourceFormatHint, TunStackIntent,
};
use tempfile::tempdir;

fn complete_config() -> &'static str {
    r#"schema_version = 3

[service]
enabled = true

[subscriptions]
mode = "single"
auto_update = true
update_interval_hours = 24

[[subscriptions.sources]]
name = "Primary"
enabled = true
url = "https://subscription.example/primary"
request_profile = "sing_box_android"
format_hint = "auto"
mirrors = ["https://mirror.example/primary"]
filter = { include_names = ["alpha"], exclude_names = ["backup"], protocols = ["vless"] }

[[subscriptions.sources]]
name = "Backup"
enabled = false
url = ""
request_profile = "mihomo"
format_hint = "clash_yaml"
mirrors = []

[proxy]
outbound_mode = "rule"

[proxy.urltest]
interval_minutes = 10
tolerance_ms = 50
max_candidates = 64
concurrency = 10

[applications]
mode = "whitelist"
targets = [
  { kind = "package", android_user_id = 0, package = "com.example.alpha" },
  { kind = "package", android_user_id = 10, package = "com.example.beta" },
  { kind = "uid", uid = 10123 },
  { kind = "uid", uid = 10124 },
]

[network]
capture_mode = "auto"
proxy_tcp = true
proxy_udp = true
ipv6_mode = "auto"
dns_mode = "auto"
tun_stack = "system"

[network.interfaces]
mobile = true
wifi = true
hotspot = false
usb = false
include = ["wlan*"]
exclude = ["wlan-test"]

[routing]
bypass_private = true
bypass_cn = true
block_quic = false
force_proxy_cidrs = ["203.0.113.7/24"]
bypass_cidrs = ["192.0.2.0/24"]
force_proxy_domains = ["Video.Example"]
bypass_domains = ["direct.example"]
block_domains = ["ads.example"]

[logging]
level = "info"
retention_days = 7

[advanced]
inbound_port = 7893
bypass_mark = 131072
ipv6_guard = true
dry_run = false
health_timeout_seconds = 3
reconcile_interval_seconds = 60

[[advanced.resource_candidates]]
mark = 1313407232
mask = 4294967295
route_table = 100
rule_priority = 12000
"#
}

fn write_private(path: &std::path::Path, contents: &str) {
    fs::write(path, contents).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
}

#[test]
fn complete_v3_schema_builds_typed_effective_sections() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nethop.toml");
    write_private(&path, complete_config());

    let snapshot = ConfigStore::new(&path).unwrap().load().unwrap();
    let config = snapshot.effective();
    assert!(config.subscriptions().auto_update());
    assert_eq!(config.subscriptions().update_interval_hours(), 24);
    assert_eq!(
        config.sources()[0].request_profile(),
        RequestProfile::SingBoxAndroid
    );
    assert_eq!(config.sources()[0].format_hint(), SourceFormatHint::Auto);
    assert_eq!(config.sources()[0].mirrors().len(), 1);
    assert_eq!(config.sources()[0].filter().include_names(), ["alpha"]);
    assert_eq!(config.sources()[0].filter().exclude_names(), ["backup"]);
    assert_eq!(config.proxy().outbound_mode(), OutboundMode::Rule);
    assert_eq!(config.proxy().urltest().max_candidates(), 64);
    assert_eq!(config.applications().mode(), ApplicationMode::Whitelist);
    assert_eq!(config.applications().targets().len(), 4);
    assert_eq!(config.capture().include_uids(), [10123, 10124]);
    assert_eq!(config.capture().exclude_uids(), [0]);
    assert_eq!(config.network().capture_mode(), CaptureIntent::Auto);
    assert_eq!(config.network().ipv6_mode(), Ipv6Mode::Auto);
    assert_eq!(config.network().dns_mode(), DnsMode::Auto);
    assert_eq!(config.network().tun_stack(), TunStackIntent::System);
    assert!(config.network().interfaces().mobile());
    assert!(config.network().interfaces().wifi());
    assert_eq!(config.network().interfaces().include(), ["wlan*"]);
    assert_eq!(config.network().interfaces().exclude(), ["wlan-test"]);
    assert_eq!(
        config.routing().force_proxy_cidrs()[0].as_str(),
        "203.0.113.0/24"
    );
    assert!(config.routing().bypass_cn());
    assert_eq!(config.routing().force_proxy_domains(), ["video.example"]);
    assert_eq!(config.routing().bypass_domains(), ["direct.example"]);
    assert_eq!(config.routing().block_domains(), ["ads.example"]);
    assert_eq!(config.logging().level(), LogLevel::Info);
    assert_eq!(config.advanced().health_timeout_seconds(), 3);
    assert_eq!(config.allocations().len(), 1);

    let debug = format!("{snapshot:?} {config:?}");
    assert!(!debug.contains("subscription.example"));
    assert!(!debug.contains("mirror.example"));
}

#[test]
fn explicit_tun_mode_and_stack_are_admitted_into_the_runtime_policy() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nethop.toml");
    let document = complete_config()
        .replace("capture_mode = \"auto\"", "capture_mode = \"tun\"")
        .replace("tun_stack = \"system\"", "tun_stack = \"gvisor\"")
        .replace("include = [\"wlan*\"]", "include = []")
        .replace("exclude = [\"wlan-test\"]", "exclude = []");
    write_private(&path, &document);

    let snapshot = ConfigStore::new(&path).unwrap().load().unwrap();
    assert_eq!(snapshot.effective().capture().mode(), CaptureMode::Tun);
    assert_eq!(snapshot.effective().managed_tun_stack(), TunStack::Gvisor);

    let baseline_path = directory.path().join("baseline.toml");
    write_private(&baseline_path, complete_config());
    let baseline = ConfigStore::new(baseline_path).unwrap().load().unwrap();
    assert_eq!(
        baseline
            .effective()
            .change_plan(snapshot.effective())
            .impact(),
        ApplyImpact::GenerationActivation
    );
}

#[test]
fn omitted_tun_stack_defaults_to_the_android_verified_gvisor_stack() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nethop.toml");
    let document = complete_config().replace("tun_stack = \"system\"\n", "");
    write_private(&path, &document);

    let snapshot = ConfigStore::new(path).unwrap().load().unwrap();
    assert_eq!(
        snapshot.effective().network().tun_stack(),
        TunStackIntent::Gvisor
    );
    assert_eq!(snapshot.effective().managed_tun_stack(), TunStack::Gvisor);
}

#[test]
fn inactive_gvisor_stack_is_valid_before_switching_capture_mode() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nethop.toml");
    let document = complete_config().replace("tun_stack = \"system\"", "tun_stack = \"gvisor\"");
    write_private(&path, &document);

    let snapshot = ConfigStore::new(path).unwrap().load().unwrap();
    assert_eq!(
        snapshot.effective().network().capture_mode(),
        CaptureIntent::Auto
    );
    assert_eq!(
        snapshot.effective().network().tun_stack(),
        TunStackIntent::Gvisor
    );
}

#[test]
fn tun_mode_rejects_capture_controls_that_the_native_inbound_cannot_honor() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nethop.toml");
    let document = complete_config().replace("capture_mode = \"auto\"", "capture_mode = \"tun\"");
    write_private(&path, &document);

    assert_eq!(
        ConfigStore::new(path).unwrap().load().unwrap_err(),
        ConfigError::UnsupportedNetwork
    );
}

#[test]
fn surfboard_format_hint_is_an_explicit_android_import_choice() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nethop.toml");
    write_private(
        &path,
        &complete_config().replacen(
            "format_hint = \"auto\"",
            "format_hint = \"surfboard_ini\"",
            1,
        ),
    );
    let snapshot = ConfigStore::new(path).unwrap().load().unwrap();
    assert_eq!(
        snapshot.effective().sources()[0].format_hint(),
        SourceFormatHint::SurfboardIni
    );
}

#[test]
fn minimal_phase_one_document_receives_frozen_phase_two_defaults() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nethop.toml");
    write_private(
        &path,
        "schema_version = 3\n[service]\nenabled = true\n[subscriptions]\n[[subscriptions.sources]]\nname = \"Primary\"\nurl = \"\"\n",
    );

    let snapshot = ConfigStore::new(path).unwrap().load().unwrap();
    let config = snapshot.effective();
    assert!(config.subscriptions().auto_update());
    assert_eq!(config.subscriptions().update_interval_hours(), 24);
    assert_eq!(config.proxy().outbound_mode(), OutboundMode::Rule);
    assert_eq!(config.applications().mode(), ApplicationMode::All);
    assert_eq!(config.network().capture_mode(), CaptureIntent::Auto);
    assert!(config.routing().bypass_cn());
    assert_eq!(config.logging().retention_days(), 7);
    assert_eq!(config.allocations().len(), 3);
}

#[test]
fn advanced_ranges_and_collections_fail_closed() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nethop.toml");
    let cases = [
        (
            "update_interval_hours = 24",
            "update_interval_hours = 0",
            ConfigError::InvalidUpdateSchedule,
        ),
        (
            "interval_minutes = 10",
            "interval_minutes = 1",
            ConfigError::InvalidProxy,
        ),
        (
            "concurrency = 10",
            "concurrency = 17",
            ConfigError::InvalidProxy,
        ),
        (
            "retention_days = 7",
            "retention_days = 31",
            ConfigError::InvalidLogging,
        ),
        (
            "health_timeout_seconds = 3",
            "health_timeout_seconds = 0",
            ConfigError::InvalidAdvanced,
        ),
        (
            "reconcile_interval_seconds = 60",
            "reconcile_interval_seconds = 10",
            ConfigError::InvalidAdvanced,
        ),
        (
            "mark = 1313407232",
            "mark = 0",
            ConfigError::InvalidAdvanced,
        ),
    ];
    for (from, to, expected) in cases {
        write_private(&path, &complete_config().replace(from, to));
        assert_eq!(
            ConfigStore::new(&path).unwrap().load().unwrap_err(),
            expected
        );
    }
}

#[test]
fn duplicate_or_conflicting_advanced_values_are_rejected() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nethop.toml");
    let cases = [
        (
            complete_config().replace(
                "{ kind = \"package\", android_user_id = 10, package = \"com.example.beta\" }",
                "{ kind = \"package\", android_user_id = 0, package = \"com.example.alpha\" }",
            ),
            ConfigError::InvalidApplications,
        ),
        (
            complete_config().replace(
                "{ kind = \"uid\", uid = 10123 }",
                "{ kind = \"uid\", uid = 0 }",
            ),
            ConfigError::InvalidApplications,
        ),
        (
            complete_config().replace(
                "bypass_cidrs = [\"192.0.2.0/24\"]",
                "bypass_cidrs = [\"203.0.113.0/24\"]",
            ),
            ConfigError::InvalidRouting,
        ),
        (
            complete_config().replace(
                "bypass_domains = [\"direct.example\"]",
                "bypass_domains = [\"sub.video.example\"]",
            ),
            ConfigError::InvalidRouting,
        ),
        (
            complete_config().replace(
                "block_domains = [\"ads.example\"]",
                "block_domains = [\"bad domain\"]",
            ),
            ConfigError::InvalidRouting,
        ),
    ];
    for (contents, expected) in cases {
        write_private(&path, &contents);
        assert_eq!(
            ConfigStore::new(&path).unwrap().load().unwrap_err(),
            expected
        );
    }
}

#[test]
fn hotspot_and_usb_are_admitted_into_the_capture_policy() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nethop.toml");
    let contents = complete_config()
        .replace("hotspot = false", "hotspot = true")
        .replace("usb = false", "usb = true");
    write_private(&path, &contents);

    let snapshot = ConfigStore::new(&path).unwrap().load().unwrap();
    let forwarding = snapshot.effective().capture().forwarding_policy();
    assert!(snapshot.effective().network().interfaces().hotspot());
    assert!(snapshot.effective().network().interfaces().usb());
    assert!(forwarding.hotspot());
    assert!(forwarding.usb());
}

#[test]
fn wifi_scene_rules_are_typed_bounded_and_part_of_network_diff() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nethop.toml");
    let scene = "\n[network.wifi_scenes]\nenabled = true\nprobe_interval_seconds = 30\n\n[[network.wifi_scenes.rules]]\nid = \"trusted-home\"\nssid = \"Private Home\"\nbssid = \"aa:bb:cc:dd:ee:ff\"\naction = \"disable_proxy\"\n";
    let contents = complete_config().replace("\n[routing]", &format!("{scene}\n[routing]"));
    write_private(&path, &contents);

    let snapshot = ConfigStore::new(&path).unwrap().load().unwrap();
    let settings = snapshot.effective().network().wifi_scenes();
    assert!(settings.enabled());
    assert_eq!(settings.probe_interval_seconds(), 30);

    let invalid = contents.replace("probe_interval_seconds = 30", "probe_interval_seconds = 5");
    write_private(&path, &invalid);
    assert_eq!(
        ConfigStore::new(&path).unwrap().load().unwrap_err(),
        ConfigError::InvalidNetwork
    );
}

#[test]
fn canonical_write_preserves_the_complete_typed_document() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nethop.toml");
    let unsorted = complete_config()
        .replace(
            "{ kind = \"package\", android_user_id = 0, package = \"com.example.alpha\" },\n  { kind = \"package\", android_user_id = 10, package = \"com.example.beta\" },",
            "{ kind = \"package\", android_user_id = 10, package = \"com.example.beta\" },\n  { kind = \"package\", android_user_id = 0, package = \"com.example.alpha\" },",
        )
        .replace(
            "force_proxy_cidrs = [\"203.0.113.7/24\"]",
            "force_proxy_cidrs = [\"203.0.114.7/24\", \"203.0.113.7/24\"]",
        );
    write_private(&path, &unsorted);
    let store = ConfigStore::new(&path).unwrap();
    let before = store.load().unwrap();

    let after = store.set_service_enabled(before.digest(), false).unwrap();
    assert!(!after.effective().service_enabled());
    let text = fs::read_to_string(&path).unwrap();
    for section in [
        "[proxy]",
        "[applications]",
        "[network]",
        "[routing]",
        "[logging]",
        "[advanced]",
        "[[advanced.resource_candidates]]",
    ] {
        assert!(text.contains(section), "canonical TOML dropped {section}");
    }
    assert!(text.contains("# Persistent proxy switch."));
    assert!(text.contains("# User-visible name and HTTPS subscription URL."));
    assert!(text.contains("# sing-box 1.13.15 uses a fixed internal URL-test concurrency of 10."));
    assert!(text.find("com.example.alpha").unwrap() < text.find("com.example.beta").unwrap());
    assert!(text.find("10123").unwrap() < text.find("10124").unwrap());
    assert!(text.find("203.0.113.0/24").unwrap() < text.find("203.0.114.0/24").unwrap());
    assert!(text.contains("force_proxy_domains = [\"video.example\"]"));
    assert_eq!(after.effective().sources()[0].mirrors().len(), 1);
}

#[test]
fn typed_diff_returns_the_minimal_bounded_apply_impact() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nethop.toml");
    write_private(&path, complete_config());
    let before = ConfigStore::new(&path).unwrap().load().unwrap();

    write_private(
        &path,
        &complete_config().replace("level = \"info\"", "level = \"debug\""),
    );
    let logging = ConfigStore::new(&path).unwrap().load().unwrap();
    let plan = before.effective().change_plan(logging.effective());
    assert_eq!(plan.impact(), ApplyImpact::RuntimeOnly);
    assert_eq!(plan.changes(), &[ChangeKind::Logging]);

    write_private(
        &path,
        &complete_config().replace("proxy_udp = true", "proxy_udp = false"),
    );
    let network = ConfigStore::new(&path).unwrap().load().unwrap();
    let plan = before.effective().change_plan(network.effective());
    assert_eq!(plan.impact(), ApplyImpact::NetworkPlan);
    assert!(plan.changes().contains(&ChangeKind::Network));

    write_private(
        &path,
        &complete_config().replace("tolerance_ms = 50", "tolerance_ms = 75"),
    );
    let proxy = ConfigStore::new(path).unwrap().load().unwrap();
    let plan = before.effective().change_plan(proxy.effective());
    assert_eq!(plan.impact(), ApplyImpact::GenerationActivation);
    assert_eq!(plan.changes(), &[ChangeKind::Proxy]);
}
