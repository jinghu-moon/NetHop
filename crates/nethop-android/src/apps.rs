use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

const MAX_APPS: usize = 20_000;
const MAX_PACKAGES_PER_UID: usize = 128;
const MAX_PACKAGE_BYTES: usize = 255;
const PER_USER_RANGE: u32 = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppClass {
    System,
    User,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppIdentity {
    android_user_id: u32,
    uid: u32,
    package_name: String,
    class: AppClass,
}

impl AppIdentity {
    pub const fn android_user_id(&self) -> u32 {
        self.android_user_id
    }

    pub const fn uid(&self) -> u32 {
        self.uid
    }

    pub fn package_name(&self) -> &str {
        &self.package_name
    }

    pub const fn class(&self) -> AppClass {
        self.class
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UidGroup {
    android_user_id: u32,
    uid: u32,
    package_names: Vec<String>,
    class: AppClass,
}

impl UidGroup {
    pub const fn android_user_id(&self) -> u32 {
        self.android_user_id
    }

    pub const fn uid(&self) -> u32 {
        self.uid
    }

    pub fn package_names(&self) -> &[String] {
        &self.package_names
    }

    pub const fn class(&self) -> AppClass {
        self.class
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PackageSnapshot<'a> {
    android_user_id: u32,
    all: &'a str,
    system: &'a str,
    user: &'a str,
}

impl<'a> PackageSnapshot<'a> {
    pub const fn new(android_user_id: u32, all: &'a str, system: &'a str, user: &'a str) -> Self {
        Self {
            android_user_id,
            all,
            system,
            user,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppCatalog {
    apps: Vec<AppIdentity>,
    groups: Vec<UidGroup>,
}

impl AppCatalog {
    pub fn from_snapshots<'a>(
        snapshots: impl IntoIterator<Item = PackageSnapshot<'a>>,
    ) -> Result<Self, AppCatalogError> {
        let mut apps = Vec::new();
        let mut users = BTreeSet::new();
        for snapshot in snapshots {
            if !users.insert(snapshot.android_user_id) {
                return Err(AppCatalogError::DuplicateUser);
            }
            let all = parse_packages(snapshot.all)?;
            let system = parse_packages(snapshot.system)?;
            let user = parse_packages(snapshot.user)?;
            for (package, uid) in &all {
                validate_uid_user(*uid, snapshot.android_user_id)?;
                let is_system = system.get(package).copied() == Some(*uid);
                let is_user = user.get(package).copied() == Some(*uid);
                let class = match (is_system, is_user) {
                    (true, false) => AppClass::System,
                    (false, true) => AppClass::User,
                    (false, false) => return Err(AppCatalogError::UnclassifiedPackage),
                    (true, true) => return Err(AppCatalogError::AmbiguousClassification),
                };
                apps.push(AppIdentity {
                    android_user_id: snapshot.android_user_id,
                    uid: *uid,
                    package_name: package.clone(),
                    class,
                });
            }
            if system
                .keys()
                .chain(user.keys())
                .any(|package| !all.contains_key(package))
            {
                return Err(AppCatalogError::ClassificationOutsideFullSet);
            }
            if apps.len() > MAX_APPS {
                return Err(AppCatalogError::TooManyApps);
            }
        }
        apps.sort_by(|left, right| {
            (left.android_user_id, left.uid, &left.package_name).cmp(&(
                right.android_user_id,
                right.uid,
                &right.package_name,
            ))
        });
        let groups = build_groups(&apps)?;
        Ok(Self { apps, groups })
    }

    pub fn apps(&self) -> &[AppIdentity] {
        &self.apps
    }

    pub fn groups(&self) -> &[UidGroup] {
        &self.groups
    }

    pub fn app(&self, android_user_id: u32, package_name: &str) -> Option<&AppIdentity> {
        self.apps
            .iter()
            .find(|app| app.android_user_id == android_user_id && app.package_name == package_name)
    }

    pub fn group(&self, android_user_id: u32, uid: u32) -> Option<&UidGroup> {
        self.groups
            .iter()
            .find(|group| group.android_user_id == android_user_id && group.uid == uid)
    }

    pub fn compile_selection<'a>(
        &self,
        mode: AppSelectionMode,
        selected: impl IntoIterator<Item = (u32, &'a str)>,
    ) -> Result<CompiledAppSelection, AppCatalogError> {
        let mut selected_uids = BTreeSet::new();
        let mut selected_packages = BTreeSet::new();
        for (android_user_id, package_name) in selected {
            let app = self
                .app(android_user_id, package_name)
                .ok_or(AppCatalogError::UnknownPackage)?;
            selected_uids.insert((app.android_user_id, app.uid));
            selected_packages.insert((app.android_user_id, app.package_name.as_str()));
        }
        let mut expansions = Vec::new();
        for (android_user_id, uid) in &selected_uids {
            let group = self
                .group(*android_user_id, *uid)
                .ok_or(AppCatalogError::UnknownPackage)?;
            if group.package_names.len() > 1 {
                let selected_in_group = group
                    .package_names
                    .iter()
                    .filter(|package| {
                        selected_packages.contains(&(*android_user_id, package.as_str()))
                    })
                    .count();
                if selected_in_group < group.package_names.len() {
                    expansions.push(SharedUidExpansion {
                        android_user_id: *android_user_id,
                        uid: *uid,
                        affected_packages: group.package_names.clone(),
                    });
                }
            }
        }
        let uids = selected_uids.into_iter().map(|(_, uid)| uid).collect();
        let (include_uids, exclude_uids) = match mode {
            AppSelectionMode::Whitelist => (uids, Vec::new()),
            AppSelectionMode::Blacklist => (Vec::new(), uids),
        };
        Ok(CompiledAppSelection {
            include_uids,
            exclude_uids,
            expansions,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppSelectionMode {
    Blacklist,
    Whitelist,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedUidExpansion {
    android_user_id: u32,
    uid: u32,
    affected_packages: Vec<String>,
}

impl SharedUidExpansion {
    pub const fn android_user_id(&self) -> u32 {
        self.android_user_id
    }

    pub const fn uid(&self) -> u32 {
        self.uid
    }

    pub fn affected_packages(&self) -> &[String] {
        &self.affected_packages
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledAppSelection {
    include_uids: Vec<u32>,
    exclude_uids: Vec<u32>,
    expansions: Vec<SharedUidExpansion>,
}

impl CompiledAppSelection {
    pub fn include_uids(&self) -> &[u32] {
        &self.include_uids
    }

    pub fn exclude_uids(&self) -> &[u32] {
        &self.exclude_uids
    }

    pub fn expansions(&self) -> &[SharedUidExpansion] {
        &self.expansions
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum AppCatalogError {
    #[error("package catalog contains an invalid line")]
    InvalidLine,
    #[error("package name is invalid or too long")]
    InvalidPackage,
    #[error("package UID does not belong to the declared Android user")]
    UserUidMismatch,
    #[error("package occurs more than once with inconsistent UID")]
    DuplicatePackage,
    #[error("Android user snapshot occurs more than once")]
    DuplicateUser,
    #[error("package is missing from both classification sets")]
    UnclassifiedPackage,
    #[error("package occurs in both system and user classification sets")]
    AmbiguousClassification,
    #[error("classification contains a package absent from the full set")]
    ClassificationOutsideFullSet,
    #[error("application catalog exceeds the bounded limit")]
    TooManyApps,
    #[error("shared UID contains too many packages")]
    SharedUidTooLarge,
    #[error("selected package is absent from the current catalog")]
    UnknownPackage,
}

fn parse_packages(input: &str) -> Result<BTreeMap<String, u32>, AppCatalogError> {
    let mut packages = BTreeMap::new();
    for line in input.lines().filter(|line| !line.trim().is_empty()) {
        let mut fields = line.split_ascii_whitespace();
        let package = fields
            .next()
            .and_then(|field| field.strip_prefix("package:"))
            .ok_or(AppCatalogError::InvalidLine)?;
        let uid = fields
            .next()
            .and_then(|field| field.strip_prefix("uid:"))
            .and_then(|value| value.parse::<u32>().ok())
            .ok_or(AppCatalogError::InvalidLine)?;
        if fields.next().is_some() || !valid_package_name(package) {
            return Err(AppCatalogError::InvalidPackage);
        }
        if packages.insert(package.to_owned(), uid).is_some() {
            return Err(AppCatalogError::DuplicatePackage);
        }
    }
    Ok(packages)
}

fn valid_package_name(package: &str) -> bool {
    !package.is_empty()
        && package.len() <= MAX_PACKAGE_BYTES
        && package
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        && !package.starts_with('.')
        && !package.ends_with('.')
        && !package.contains("..")
}

fn validate_uid_user(uid: u32, android_user_id: u32) -> Result<(), AppCatalogError> {
    (uid / PER_USER_RANGE == android_user_id)
        .then_some(())
        .ok_or(AppCatalogError::UserUidMismatch)
}

fn build_groups(apps: &[AppIdentity]) -> Result<Vec<UidGroup>, AppCatalogError> {
    let mut grouped: BTreeMap<(u32, u32), Vec<&AppIdentity>> = BTreeMap::new();
    for app in apps {
        grouped
            .entry((app.android_user_id, app.uid))
            .or_default()
            .push(app);
    }
    grouped
        .into_iter()
        .map(|((android_user_id, uid), apps)| {
            if apps.len() > MAX_PACKAGES_PER_UID {
                return Err(AppCatalogError::SharedUidTooLarge);
            }
            let class = if apps.iter().any(|app| app.class == AppClass::System) {
                AppClass::System
            } else {
                AppClass::User
            };
            Ok(UidGroup {
                android_user_id,
                uid,
                package_names: apps.iter().map(|app| app.package_name.clone()).collect(),
                class,
            })
        })
        .collect()
}
