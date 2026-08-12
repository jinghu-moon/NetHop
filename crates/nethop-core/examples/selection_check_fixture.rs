use std::{collections::BTreeMap, fs, path::PathBuf};

use nethop_core::{
    CaptureMode, CapturePolicy, ClashApi, ManagedConfig, ManagedLogLevel, ManagedOptions,
    ManagedOutboundMode, ManagedProfile, TerminalOutbound,
};
use serde_json::json;

fn outbound(tag: &str, port: u16) -> Result<TerminalOutbound, nethop_core::ComposerError> {
    TerminalOutbound::new(
        tag,
        "vless",
        BTreeMap::from([
            ("server".into(), json!("example.com")),
            ("server_port".into(), json!(port)),
            ("uuid".into(), json!("00000000-0000-4000-8000-000000000001")),
        ]),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "single".into());
    let auto_pool = match mode.as_str() {
        "single" => vec!["nh1s-1111111111111111".into()],
        "merge" => vec![
            "nh1s-1111111111111111".into(),
            "nh1s-2222222222222222".into(),
        ],
        _ => return Err("expected single or merge".into()),
    };
    let profile = ManagedProfile::new(
        CapturePolicy::new(CaptureMode::Direct, false, None, None, vec![], vec![])?,
        vec![
            outbound("nh1s-1111111111111111", 443)?,
            outbound("nh1s-2222222222222222", 8443)?,
        ],
        auto_pool,
        ClashApi::new("127.0.0.1:19090", "fixture-secret-32-bytes-long-000")?,
    )?
    .with_options(ManagedOptions::new(
        ManagedOutboundMode::Direct,
        10,
        50,
        64,
        ManagedLogLevel::Warn,
        true,
        false,
        vec![],
        vec![],
    )?);
    let config = ManagedConfig::from_profile(profile)?;
    if let Some(path) = std::env::args().nth(2).map(PathBuf::from) {
        fs::write(path, config.bytes())?;
    } else {
        print!("{}", String::from_utf8_lossy(config.bytes()));
    }
    Ok(())
}
