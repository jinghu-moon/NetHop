#![cfg(feature = "fetch")]

use nethop_subscription::{
    CandidateAcceptance, CapabilityMatrix, Digest, FetchClient, FetchPolicy, FetchRequest,
    FormatHint, ParserLimits, RequestProfile, SourceId, SourceInput, convert_stable_sources,
    detect_bytes,
};

struct RealSource {
    label: &'static str,
    env_name: &'static str,
    profile: RequestProfile,
    stable_format: Option<FormatHint>,
}

#[test]
#[ignore = "requires explicitly authorized real subscription URLs in environment variables"]
fn authorized_real_sources_are_downloaded_without_logging_secrets() {
    let sources = [
        RealSource {
            label: "mihomo",
            env_name: "NETHOP_TEST_MIHOMO_URL",
            profile: RequestProfile::Mihomo,
            stable_format: Some(FormatHint::ClashYaml),
        },
        RealSource {
            label: "surfboard",
            env_name: "NETHOP_TEST_SURFBOARD_URL",
            profile: RequestProfile::Surfboard,
            stable_format: Some(FormatHint::SurfboardIni),
        },
        RealSource {
            label: "clash_standard",
            env_name: "NETHOP_TEST_CLASH_URL",
            profile: RequestProfile::ClashStandard,
            stable_format: Some(FormatHint::ClashYaml),
        },
        RealSource {
            label: "singbox_android",
            env_name: "NETHOP_TEST_SFA_URL",
            profile: RequestProfile::SingBoxAndroid,
            stable_format: Some(FormatHint::SingboxJson),
        },
    ];
    let limits = ParserLimits::default();
    let policy = FetchPolicy::default();
    let client = FetchClient::new(policy.clone(), limits);

    for source in sources {
        let url = std::env::var(source.env_name)
            .unwrap_or_else(|_| panic!("missing environment variable {}", source.env_name));
        let source_id = SourceId::new(format!("real-{}", source.label)).unwrap();
        let request = FetchRequest::new(
            source_id.clone(),
            &url,
            std::iter::empty::<&str>(),
            source.profile,
            &policy,
        )
        .unwrap();
        let outcome = client
            .fetch(&request, &Default::default(), |_| {
                CandidateAcceptance::Accepted
            })
            .unwrap_or_else(|error| panic!("{} fetch failed: {}", source.label, error.code()));
        let digest = Digest::sha256(outcome.body()).hex();
        let detected = detect_bytes(outcome.body(), FormatHint::Auto, &limits).ok();

        if let Some(expected) = source.stable_format {
            let detection = detected
                .as_ref()
                .unwrap_or_else(|| panic!("{} format detection failed", source.label));
            assert!(
                detection.format() == expected
                    || (expected == FormatHint::SurfboardIni
                        && detection.format() == FormatHint::IniProfile),
                "{} format: detected {:?}, expected {:?}",
                source.label,
                detection.format(),
                expected
            );
            let conversion = convert_stable_sources(
                vec![SourceInput {
                    source_id,
                    format_hint: expected,
                    bytes: outcome.body().to_vec(),
                }],
                &limits,
                &CapabilityMatrix::default(),
            );
            let summary = &conversion.report.summary;
            assert!(
                summary.accepted + summary.rejected + summary.duplicate > 0,
                "{} contained no terminal node candidates",
                source.label
            );
            println!(
                "source={} bytes={} digest={} format={:?} accepted={} rejected={} diagnostics={:?}",
                source.label,
                outcome.body().len(),
                &digest[..12],
                expected,
                summary.accepted,
                summary.rejected,
                conversion.report.diagnostic_counts,
            );
        } else {
            println!(
                "source={} bytes={} digest={} format={:?} parser=experimental",
                source.label,
                outcome.body().len(),
                &digest[..12],
                detected.as_ref().map(|result| result.format()),
            );
        }
    }
}
