use nethopd::{ApiSecretError, ApiSecretStore, SecretEntropy};
use tempfile::tempdir;

struct FixedEntropy {
    byte: u8,
    calls: usize,
}

impl SecretEntropy for FixedEntropy {
    fn fill(&mut self, output: &mut [u8]) -> Result<(), ApiSecretError> {
        self.calls += 1;
        output.fill(self.byte);
        Ok(())
    }
}

#[test]
fn secret_is_created_once_as_lowercase_hex_and_is_redacted() {
    let directory = tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let path = root.join("api.secret");
    let store = ApiSecretStore::new(&path).unwrap();
    let mut first_entropy = FixedEntropy {
        byte: 0xab,
        calls: 0,
    };
    let first = store.load_or_create_with(&mut first_entropy).unwrap();
    assert_eq!(first.expose_for_composer(), "ab".repeat(32));
    assert_eq!(first_entropy.calls, 1);
    assert!(!format!("{first:?}").contains("abab"));

    let mut second_entropy = FixedEntropy {
        byte: 0xcd,
        calls: 0,
    };
    let second = store.load_or_create_with(&mut second_entropy).unwrap();
    assert_eq!(second, first);
    assert_eq!(second_entropy.calls, 0);
    assert_eq!(std::fs::read_to_string(path).unwrap(), "ab".repeat(32));
}

#[test]
fn malformed_or_symlink_secret_is_rejected_without_regeneration() {
    let directory = tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let path = root.join("api.secret");
    std::fs::write(&path, "not-a-secret").unwrap();
    let store = ApiSecretStore::new(&path).unwrap();
    let mut entropy = FixedEntropy { byte: 1, calls: 0 };
    assert_eq!(
        store.load_or_create_with(&mut entropy).unwrap_err(),
        ApiSecretError::InvalidSecret
    );
    assert_eq!(entropy.calls, 0);

    #[cfg(unix)]
    {
        std::fs::remove_file(&path).unwrap();
        let target = root.join("target");
        std::fs::write(&target, "aa".repeat(32)).unwrap();
        std::os::unix::fs::symlink(target, &path).unwrap();
        assert_eq!(
            store.load_or_create_with(&mut entropy).unwrap_err(),
            ApiSecretError::InvalidSecret
        );
    }
}

#[test]
fn relative_or_non_real_parent_is_rejected() {
    assert!(ApiSecretStore::new("state/api.secret").is_err());
    let directory = tempdir().unwrap();
    let missing = directory.path().join("missing/api.secret");
    assert!(ApiSecretStore::new(missing).is_err());
}
