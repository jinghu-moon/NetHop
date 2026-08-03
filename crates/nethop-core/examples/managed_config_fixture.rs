use std::collections::BTreeMap;

use nethop_core::{
    CaptureMode, CapturePolicy, ClashApi, ManagedConfig, ManagedProfile, TerminalOutbound, TunStack,
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = match std::env::args().nth(1).as_deref() {
        Some("tun") => CaptureMode::Tun,
        Some("tproxy") | None => CaptureMode::Tproxy,
        Some(_) => return Err("expected tproxy or tun".into()),
    };
    let policy = match mode {
        CaptureMode::Tproxy => {
            CapturePolicy::new(mode, true, Some(7893), Some(0x4e48), vec![], vec![])?
        }
        CaptureMode::Tun => CapturePolicy::new(mode, true, None, None, vec![], vec![])?,
        CaptureMode::Direct => unreachable!(),
    };
    let node = TerminalOutbound::new(
        "fixture-node",
        "vless",
        BTreeMap::from([
            ("server".to_owned(), json!("127.0.0.1")),
            ("server_port".to_owned(), json!(443)),
            (
                "uuid".to_owned(),
                json!("00000000-0000-4000-8000-000000000001"),
            ),
        ]),
    )?;
    let profile = ManagedProfile::new(
        policy,
        vec![node],
        ClashApi::new("127.0.0.1:9090", "fixture-secret-32-bytes-long-000")?,
    )?
    .with_tun_stack(TunStack::System);
    let config = ManagedConfig::from_profile(profile)?;
    println!("{}", String::from_utf8_lossy(config.bytes()));
    Ok(())
}
