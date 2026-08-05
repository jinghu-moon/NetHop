use nethop_android::{
    AppCatalog, AppCatalogError, AppClass, AppSelectionMode, CapabilityError, PackageListKind,
    PackageSnapshot, ProbeBackend, ProbeCommand, ProbeOutput,
};

struct PackageBackend;

impl ProbeBackend for PackageBackend {
    fn run(&mut self, command: ProbeCommand) -> Result<ProbeOutput, CapabilityError> {
        let output = match command {
            ProbeCommand::PackageList(PackageListKind::All) => {
                "package:android uid:1000\npackage:com.example.user uid:10123\n"
            }
            ProbeCommand::PackageList(PackageListKind::System) => "package:android uid:1000\n",
            ProbeCommand::PackageList(PackageListKind::User) => {
                "package:com.example.user uid:10123\n"
            }
            _ => return Err(CapabilityError::InvalidPolicy),
        };
        Ok(ProbeOutput::new(true, output, ""))
    }
}

fn snapshot() -> PackageSnapshot<'static> {
    PackageSnapshot::new(
        0,
        "package:android uid:1000\npackage:com.vendor.updated uid:1000\npackage:com.example.user uid:10123\n",
        "package:android uid:1000\npackage:com.vendor.updated uid:1000\n",
        "package:com.example.user uid:10123\n",
    )
}

#[test]
fn catalog_groups_shared_uids_and_preserves_system_classification() {
    let catalog = AppCatalog::from_snapshots([snapshot()]).unwrap();
    assert_eq!(catalog.apps().len(), 3);
    assert_eq!(catalog.groups().len(), 2);
    let shared = catalog.group(0, 1000).unwrap();
    assert_eq!(shared.package_names(), ["android", "com.vendor.updated"]);
    assert_eq!(shared.class(), AppClass::System);
    assert_eq!(
        catalog.app(0, "com.example.user").unwrap().class(),
        AppClass::User
    );
}

#[test]
fn selecting_one_shared_uid_package_expands_the_whole_group() {
    let catalog = AppCatalog::from_snapshots([snapshot()]).unwrap();
    let compiled = catalog
        .compile_selection(AppSelectionMode::Whitelist, [(0, "android")])
        .unwrap();
    assert_eq!(compiled.include_uids(), [1000]);
    assert!(compiled.exclude_uids().is_empty());
    assert_eq!(compiled.expansions().len(), 1);
    assert_eq!(
        compiled.expansions()[0].affected_packages(),
        ["android", "com.vendor.updated"]
    );

    let blacklist = catalog
        .compile_selection(AppSelectionMode::Blacklist, [(0, "com.example.user")])
        .unwrap();
    assert_eq!(blacklist.exclude_uids(), [10123]);
    assert!(blacklist.include_uids().is_empty());
}

#[test]
fn catalog_rejects_incomplete_or_ambiguous_package_snapshots() {
    assert_eq!(
        AppCatalog::from_snapshots([PackageSnapshot::new(
            0,
            "package:com.example uid:10123\n",
            "",
            "",
        )])
        .unwrap_err(),
        AppCatalogError::UnclassifiedPackage
    );
    assert_eq!(
        AppCatalog::from_snapshots([PackageSnapshot::new(
            0,
            "package:com.example uid:10123\n",
            "package:com.example uid:10123\n",
            "package:com.example uid:10123\n",
        )])
        .unwrap_err(),
        AppCatalogError::AmbiguousClassification
    );
}

#[test]
fn catalog_uses_full_android_uid_and_rejects_unknown_selection() {
    let work = PackageSnapshot::new(
        10,
        "package:com.work uid:1010123\n",
        "",
        "package:com.work uid:1010123\n",
    );
    let catalog = AppCatalog::from_snapshots([snapshot(), work]).unwrap();
    assert_eq!(catalog.app(10, "com.work").unwrap().uid(), 1_010_123);
    assert_eq!(
        catalog
            .compile_selection(AppSelectionMode::Whitelist, [(10, "missing")])
            .unwrap_err(),
        AppCatalogError::UnknownPackage
    );
}

#[test]
fn empty_selection_remains_empty_for_both_modes() {
    let catalog = AppCatalog::from_snapshots([snapshot()]).unwrap();
    for mode in [AppSelectionMode::Whitelist, AppSelectionMode::Blacklist] {
        let compiled = catalog.compile_selection(mode, std::iter::empty()).unwrap();
        assert!(compiled.include_uids().is_empty());
        assert!(compiled.exclude_uids().is_empty());
        assert!(compiled.expansions().is_empty());
    }
}

#[test]
fn mixed_shared_uid_is_classified_as_system_and_expands_atomically() {
    let catalog = AppCatalog::from_snapshots([PackageSnapshot::new(
        0,
        "package:android uid:1000\npackage:com.vendor.shared uid:1000\n",
        "package:android uid:1000\n",
        "package:com.vendor.shared uid:1000\n",
    )])
    .unwrap();

    let group = catalog.group(0, 1000).unwrap();
    assert_eq!(group.class(), AppClass::System);
    let compiled = catalog
        .compile_selection(AppSelectionMode::Blacklist, [(0, "com.vendor.shared")])
        .unwrap();
    assert_eq!(compiled.exclude_uids(), [1000]);
    assert_eq!(compiled.expansions().len(), 1);
    assert_eq!(
        compiled.expansions()[0].affected_packages(),
        ["android", "com.vendor.shared"]
    );
}

#[test]
fn primary_user_catalog_uses_only_bounded_probe_commands() {
    let catalog = AppCatalog::load_primary_user(&mut PackageBackend).unwrap();
    assert_eq!(catalog.app(0, "android").unwrap().uid(), 1000);
    assert_eq!(catalog.app(0, "com.example.user").unwrap().uid(), 10_123);
}
