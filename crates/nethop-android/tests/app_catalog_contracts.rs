use nethop_android::{
    AppCatalog, AppCatalogError, AppClass, AppSelectionMode, CapabilityError, PackageListKind,
    PackageSnapshot, ProbeBackend, ProbeCommand, ProbeOutput, resolve_selection,
};

struct PackageBackend;

impl ProbeBackend for PackageBackend {
    fn run(&mut self, command: ProbeCommand) -> Result<ProbeOutput, CapabilityError> {
        let output = match command {
            ProbeCommand::UserList => {
                "Users:\n  UserInfo{0:Owner:13} running\n  UserInfo{10:Work:30} running\n  UserInfo{11:Stopped:30}\n"
            }
            ProbeCommand::PackageList {
                kind: PackageListKind::All,
                android_user_id: 0,
            } => "package:android uid:1000\npackage:com.example.user uid:10123\n",
            ProbeCommand::PackageList {
                kind: PackageListKind::System,
                android_user_id: 0,
            } => "package:android uid:1000\n",
            ProbeCommand::PackageList {
                kind: PackageListKind::User,
                android_user_id: 0,
            } => "package:com.example.user uid:10123\n",
            ProbeCommand::PackageList {
                kind: PackageListKind::All | PackageListKind::User,
                android_user_id: 10,
            } => "package:com.example.work uid:1010123\n",
            ProbeCommand::PackageList {
                kind: PackageListKind::System,
                android_user_id: 10,
            } => "",
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
fn started_user_catalog_queries_each_running_user_and_skips_stopped_profiles() {
    let catalog = AppCatalog::load_started_users(&mut PackageBackend).unwrap();
    assert_eq!(catalog.app(0, "android").unwrap().uid(), 1000);
    assert_eq!(catalog.app(0, "com.example.user").unwrap().uid(), 10_123);
    assert_eq!(
        catalog.app(10, "com.example.work").unwrap().uid(),
        1_010_123
    );
    assert!(catalog.app(11, "com.example.work").is_none());
}

#[test]
fn runtime_resolver_uses_only_full_package_listing_and_reports_unresolved() {
    let resolved = resolve_selection(
        &mut PackageBackend,
        AppSelectionMode::Whitelist,
        [(0, "com.example.user"), (0, "com.missing")],
    )
    .unwrap();
    assert_eq!(resolved.include_uids(), [10123]);
    assert_eq!(
        resolved.unresolved_packages(),
        &[(0, "com.missing".to_owned())]
    );
}

#[test]
fn runtime_resolver_expands_shared_uid_without_classification_queries() {
    let resolved = resolve_selection(
        &mut PackageBackend,
        AppSelectionMode::Blacklist,
        [(0, "android")],
    )
    .unwrap();
    assert_eq!(resolved.exclude_uids(), [1000]);
    assert!(resolved.expansions().is_empty());
}
