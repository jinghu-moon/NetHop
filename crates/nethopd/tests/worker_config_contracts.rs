use std::fs;

use nethopd::{ConfigError, ConfigStore};
use tempfile::tempdir;

fn valid_config() -> &'static str {
    r#"schema_version = 1

[service]
enabled = true

[subscriptions]

[[subscriptions.sources]]
name = "Primary"
url = "https://subscription.example/sfa/token"

[[subscriptions.sources]]
name = "Backup"
enabled = false
url = ""
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
fn strict_toml_builds_effective_config_without_user_source_ids() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nethop.toml");
    write_private(&path, valid_config());

    let snapshot = ConfigStore::new(&path).unwrap().load().unwrap();
    let config = snapshot.effective();
    assert!(config.service_enabled());
    assert_eq!(config.sources().len(), 2);
    assert_eq!(config.sources()[0].name().as_str(), "Primary");
    assert!(config.sources()[0].enabled());
    assert_eq!(config.sources()[1].name().as_str(), "Backup");
    assert!(!config.sources()[1].enabled());
    assert_eq!(config.capture().inbound_port(), Some(7893));
    assert_eq!(config.capture().bypass_mark(), Some(131_072));
    assert_eq!(config.allocations().len(), 3);
    assert_eq!(snapshot.digest().len(), 64);

    let debug = format!("{snapshot:?} {config:?} {:?}", config.sources()[0]);
    assert!(!debug.contains("subscription.example"));
    assert!(!debug.contains("token"));
}

#[test]
fn unknown_id_duplicate_names_and_duplicate_urls_fail_closed() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nethop.toml");
    let cases = [
        (
            valid_config().replace("name = \"Primary\"", "id = \"user-id\"\nname = \"Primary\""),
            ConfigError::UnknownField,
        ),
        (
            valid_config().replace("name = \"Backup\"", "name = \"Primary\""),
            ConfigError::DuplicateSourceName,
        ),
        (
            valid_config().replace(
                "enabled = false\nurl = \"\"",
                "enabled = true\nurl = \"https://subscription.example/sfa/token\"",
            ),
            ConfigError::DuplicateSourceUrl,
        ),
        (
            valid_config().replace(
                "https://subscription.example",
                "http://subscription.example",
            ),
            ConfigError::SourceUrlNonHttps,
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
fn source_name_and_file_limits_are_bounded_before_runtime_use() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nethop.toml");
    for contents in [
        valid_config().replace("name = \"Primary\"", "name = \" Primary\""),
        valid_config().replace(
            "name = \"Primary\"",
            &format!("name = \"{}\"", "x".repeat(129)),
        ),
        "schema_version = 1\n[service]\nenabled = true\n[subscriptions]\nsources = []\n".to_owned(),
    ] {
        write_private(&path, &contents);
        assert!(ConfigStore::new(&path).unwrap().load().is_err());
    }

    write_private(&path, &" ".repeat(256 * 1024 + 1));
    assert_eq!(
        ConfigStore::new(&path).unwrap().load().unwrap_err(),
        ConfigError::TooLarge
    );
}

#[test]
fn service_enabled_is_persisted_as_canonical_toml_with_cas() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nethop.toml");
    write_private(&path, valid_config());
    let store = ConfigStore::new(&path).unwrap();
    let before = store.load().unwrap();

    let disabled = store.set_service_enabled(before.digest(), false).unwrap();
    assert!(!disabled.effective().service_enabled());
    assert_ne!(before.digest(), disabled.digest());
    assert!(
        fs::read_to_string(&path)
            .unwrap()
            .contains("enabled = false")
    );
    assert!(
        fs::read_to_string(&path)
            .unwrap()
            .contains("# NetHop user configuration")
    );
    assert_eq!(
        store
            .set_service_enabled(before.digest(), true)
            .unwrap_err(),
        ConfigError::Conflict
    );
}

#[test]
fn module_default_is_the_complete_v1_toml_schema() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../module/defaults/nethop.toml")
        .canonicalize()
        .unwrap();
    let directory = tempdir().unwrap();
    let path = directory.path().join("nethop.toml");
    write_private(&path, &fs::read_to_string(source).unwrap());
    let snapshot = ConfigStore::new(path).unwrap().load().unwrap();
    assert!(snapshot.effective().service_enabled());
    assert_eq!(snapshot.effective().sources().len(), 2);
    assert_eq!(snapshot.effective().sources()[0].name().as_str(), "Primary");
    assert!(snapshot.effective().sources()[0].enabled());
    assert!(
        snapshot.effective().sources()[0]
            .url()
            .starts_with("https://")
    );
    assert!(!snapshot.effective().sources()[1].enabled());
    assert_eq!(
        snapshot.effective().proxy().urltest().interval_minutes(),
        10
    );
    assert_eq!(snapshot.effective().allocations().len(), 3);
}

#[test]
fn utf8_bom_is_accepted_without_changing_the_exact_byte_digest() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nethop.toml");
    let contents = format!("\u{feff}{}", valid_config());
    write_private(&path, &contents);

    let snapshot = ConfigStore::new(&path).unwrap().load().unwrap();
    assert!(snapshot.effective().service_enabled());
    assert_eq!(
        snapshot.digest(),
        nethop_subscription::Digest::sha256(contents.as_bytes()).hex()
    );
}

#[cfg(unix)]
#[test]
fn configuration_parent_directory_must_be_private() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempdir().unwrap();
    let config_directory = directory.path().join("config");
    fs::create_dir(&config_directory).unwrap();
    let path = config_directory.join("nethop.toml");
    write_private(&path, valid_config());

    fs::set_permissions(&config_directory, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        ConfigStore::new(&path).unwrap().load().unwrap_err(),
        ConfigError::InvalidPath
    );

    fs::set_permissions(&config_directory, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(ConfigStore::new(&path).unwrap().load().is_ok());
}
