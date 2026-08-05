#![cfg(feature = "subscription-update")]

use std::{fs, path::Path};

use nethopd::{
    ConfigRuntime, ConfigStore, PersistentUpdateSchedule, RuntimeUpdateSchedule, ScheduleStore,
    SourceIdEntropy, SourceRegistry, SourceRegistryError, StatsStore,
};
use tempfile::tempdir;

struct FixedEntropy(u8);

impl SourceIdEntropy for FixedEntropy {
    fn fill(&mut self, output: &mut [u8; 16]) -> Result<(), SourceRegistryError> {
        output.fill(self.0);
        self.0 = self.0.saturating_add(1);
        Ok(())
    }
}

fn write_config(path: &Path) {
    fs::write(
        path,
        r#"schema_version = 1
[service]
enabled = true
[subscriptions]
auto_update = true
update_interval_hours = 24
[[subscriptions.sources]]
name = "Primary"
url = "https://one.example/sub"
[[subscriptions.sources]]
name = "Backup"
url = "https://two.example/sub"
"#,
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
}

#[test]
fn persistent_schedule_uses_active_source_ids_and_survives_restart() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("nethop.toml");
    let registry_path = directory.path().join("source-registry.v1.json");
    let database_path = directory.path().join("nethop.db");
    write_config(&config_path);
    let store = ConfigStore::new(&config_path).unwrap();
    let registry = SourceRegistry::new(&registry_path).unwrap();
    let snapshot = store.load().unwrap();
    let sources = registry.reconcile(&snapshot, &mut FixedEntropy(1)).unwrap();
    let runtime = ConfigRuntime::new(store, registry, snapshot, &sources);
    let source_config = runtime.update_schedule().2.clone();

    let mut schedule =
        PersistentUpdateSchedule::load(StatsStore::open(&database_path).unwrap()).unwrap();
    schedule.configure(false, 24, &source_config).unwrap();
    assert_eq!(schedule.next_wakeup_in(), None);
    assert!(!schedule.take_due().unwrap());

    schedule.configure(true, 24, &source_config).unwrap();
    assert!(schedule.next_wakeup_in().unwrap().as_secs() <= 1);
    assert!(schedule.take_due().unwrap());
    schedule.record_result(false).unwrap();
    drop(schedule);

    let mut persisted = StatsStore::open(&database_path).unwrap();
    let records = ScheduleStore::load(&mut persisted).unwrap();
    assert_eq!(records.len(), 2);
    assert!(records.iter().all(|record| record.failure_count() == 1));
    drop(persisted);

    let mut restarted =
        PersistentUpdateSchedule::load(StatsStore::open(&database_path).unwrap()).unwrap();
    restarted.configure(true, 24, &source_config).unwrap();
    let retry = restarted.next_wakeup_in().unwrap().as_secs();
    assert!((45 * 60..=75 * 60).contains(&retry));
    assert!(!restarted.take_due().unwrap());
}
