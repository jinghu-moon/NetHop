use nethop_android::{
    AndroidToolPaths, CapabilityProbe, CommandProbeBackend, ProbeLimits, ResourceCandidate,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("{}", error.code().as_str());
        std::process::exit(1);
    }
}

fn run() -> Result<(), nethop_android::CapabilityError> {
    let candidates = [
        ResourceCandidate::new(0x4e49_0100, u32::MAX, 100, 12_000),
        ResourceCandidate::new(0x4e49_0200, u32::MAX, 101, 12_010),
        ResourceCandidate::new(0x4e49_0300, u32::MAX, 102, 12_020),
    ]
    .into_iter()
    .collect::<Option<Vec<_>>>()
    .ok_or(nethop_android::CapabilityError::InvalidPolicy)?;
    let backend =
        CommandProbeBackend::new(AndroidToolPaths::from_system()?, ProbeLimits::default());
    let report = CapabilityProbe::new(backend, candidates, 7893)?.probe()?;
    let json = serde_json::to_string_pretty(&report)
        .map_err(|_| nethop_android::CapabilityError::CommandOutputFailed)?;
    println!("{json}");
    Ok(())
}
