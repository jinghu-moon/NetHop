use nethop_android::{
    CapabilityError, CommandPrivateDnsFactsSource, DnsSplitStatus, PrivateDnsFactsSource,
    PrivateDnsMode, ProbeBackend, ProbeCommand, ProbeOutput,
};

struct FixedProbe(ProbeOutput);

impl ProbeBackend for FixedProbe {
    fn run(&mut self, command: ProbeCommand) -> Result<ProbeOutput, CapabilityError> {
        assert_eq!(command, ProbeCommand::PrivateDnsMode);
        Ok(self.0.clone())
    }
}

#[test]
fn only_disabled_private_dns_claims_healthy_split_dns() {
    for (raw, mode, split) in [
        ("off\n", PrivateDnsMode::Off, DnsSplitStatus::Healthy),
        (
            "opportunistic\n",
            PrivateDnsMode::Opportunistic,
            DnsSplitStatus::DegradedPrivateDns,
        ),
        (
            "hostname\n",
            PrivateDnsMode::Strict,
            DnsSplitStatus::DegradedPrivateDns,
        ),
        ("null\n", PrivateDnsMode::Unknown, DnsSplitStatus::Unknown),
    ] {
        let mut source =
            CommandPrivateDnsFactsSource::new(FixedProbe(ProbeOutput::new(true, raw, "")));
        let status = source.current().unwrap();
        assert_eq!(status.mode(), mode);
        assert_eq!(status.dns_split(), split);
    }
}

#[test]
fn private_dns_probe_failure_is_explicit_and_never_claims_healthy() {
    let mut source = CommandPrivateDnsFactsSource::new(FixedProbe(ProbeOutput::new(
        false,
        "",
        "permission denied",
    )));
    assert!(source.current().is_err());
}
