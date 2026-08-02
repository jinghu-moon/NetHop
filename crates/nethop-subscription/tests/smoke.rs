use nethop_subscription::{CRATE_NAME, FOUNDATION_VERSION};

mod common;

#[test]
fn public_library_smoke_compiles() {
    assert_eq!(CRATE_NAME, "nethop-subscription");
    assert_eq!(FOUNDATION_VERSION, "workspace-foundation-v1");
    assert!(common::workspace_root().join("Cargo.toml").is_file());
}
