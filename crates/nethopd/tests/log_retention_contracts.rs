use std::{
    fs::{self, File, FileTimes},
    time::{Duration, SystemTime},
};

use nethopd::{FileLogRetention, LogRetentionError, RuntimeLogRetention};
use tempfile::tempdir;

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

#[test]
fn cleanup_removes_only_expired_regular_log_files() {
    let directory = tempdir().unwrap();
    let expired = directory.path().join("expired.log");
    let fresh = directory.path().join("fresh.log");
    let unrelated = directory.path().join("expired.txt");
    fs::write(&expired, b"expired").unwrap();
    fs::write(&fresh, b"fresh").unwrap();
    fs::write(&unrelated, b"unrelated").unwrap();
    set_modified(&expired, SystemTime::now() - 8 * DAY);
    set_modified(&unrelated, SystemTime::now() - 8 * DAY);

    let mut retention = FileLogRetention::new(directory.path()).unwrap();
    retention.configure(7, Duration::from_secs(10)).unwrap();

    assert!(!expired.exists());
    assert!(fresh.exists());
    assert!(unrelated.exists());
    assert_eq!(retention.next_wakeup_in(Duration::from_secs(10)), Some(DAY));
}

#[test]
fn invalid_policy_and_relative_directory_fail_closed() {
    assert_eq!(
        FileLogRetention::new("relative/logs").unwrap_err(),
        LogRetentionError::InvalidDirectory
    );

    let directory = tempdir().unwrap();
    let mut retention = FileLogRetention::new(directory.path()).unwrap();
    assert_eq!(
        retention.configure(0, Duration::ZERO).unwrap_err(),
        LogRetentionError::InvalidPolicy
    );
    assert_eq!(
        retention.configure(31, Duration::ZERO).unwrap_err(),
        LogRetentionError::InvalidPolicy
    );
}

#[test]
fn cleanup_runs_only_when_the_bounded_deadline_is_due() {
    let directory = tempdir().unwrap();
    let expired = directory.path().join("later.log");
    fs::write(&expired, b"expired later").unwrap();

    let mut retention = FileLogRetention::new(directory.path()).unwrap();
    retention.configure(7, Duration::from_secs(5)).unwrap();
    set_modified(&expired, SystemTime::now() - 8 * DAY);

    retention
        .run_due(Duration::from_secs(5) + DAY - Duration::from_secs(1))
        .unwrap();
    assert!(expired.exists());
    retention.run_due(Duration::from_secs(5) + DAY).unwrap();
    assert!(!expired.exists());
}

#[cfg(unix)]
#[test]
fn cleanup_does_not_follow_log_symlinks() {
    use std::os::unix::fs::symlink;

    let directory = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let target = outside.path().join("target.log");
    fs::write(&target, b"outside").unwrap();
    set_modified(&target, SystemTime::now() - 8 * DAY);
    symlink(&target, directory.path().join("linked.log")).unwrap();

    let mut retention = FileLogRetention::new(directory.path()).unwrap();
    retention.configure(7, Duration::ZERO).unwrap();

    assert!(target.exists());
    assert!(directory.path().join("linked.log").exists());
}

#[cfg(windows)]
#[test]
fn cleanup_does_not_follow_log_symlinks() {
    use std::os::windows::fs::symlink_file;

    let directory = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let target = outside.path().join("target.log");
    fs::write(&target, b"outside").unwrap();
    set_modified(&target, SystemTime::now() - 8 * DAY);
    if symlink_file(&target, directory.path().join("linked.log")).is_err() {
        return;
    }

    let mut retention = FileLogRetention::new(directory.path()).unwrap();
    retention.configure(7, Duration::ZERO).unwrap();

    assert!(target.exists());
    assert!(directory.path().join("linked.log").exists());
}

fn set_modified(path: &std::path::Path, modified: SystemTime) {
    File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_times(FileTimes::new().set_modified(modified))
        .unwrap();
}
