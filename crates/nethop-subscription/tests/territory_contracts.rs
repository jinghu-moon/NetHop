use nethop_subscription::{
    CapabilityMatrix, NodeSpec, ParserLimits, SourceBatch, SourceId, dedupe_sources,
    validate_node_spec,
};
use nethop_subscription::{
    DisplayTerritoryCode, infer_display_territory, territories, territory_by_alpha3,
};
use serde::Deserialize;

fn infer(names: &[&str]) -> Option<String> {
    infer_display_territory(names.iter().copied()).map(|code| code.as_str().to_owned())
}

#[test]
fn registry_and_value_type_are_strict() {
    assert_eq!(territories().len(), 249);
    assert!(DisplayTerritoryCode::new("JP").is_some());
    assert!(DisplayTerritoryCode::new("jp").is_none());
    assert!(DisplayTerritoryCode::new("EU").is_none());
    assert_eq!(territory_by_alpha3("JPN").unwrap().code.as_str(), "JP");
}

#[test]
fn evidence_levels_and_boundaries_are_deterministic() {
    for (name, expected) in [
        ("🇯🇵 东京", "JP"),
        ("日本-US", "JP"),
        ("香港HKT-A", "HK"),
        ("HKT-A", "TH"),
        ("JPN Premium", "JP"),
        ("UK X5", "GB"),
        ("SG Premium", "SG"),
        ("TW_01", "TW"),
        ("RO Bucharest 1", "RO"),
        ("ID Jakarta 1", "ID"),
        ("RUSSIA", "RU"),
        ("SINGAPORE", "SG"),
    ] {
        assert_eq!(infer(&[name]).as_deref(), Some(expected), "{name}");
    }
    for name in [
        "STATUS",
        "SINGLE",
        "Fast",
        "Balancer",
        "EU Premium",
        "jp lower",
        "剩余流量：388.64 GB",
    ] {
        assert_eq!(infer(&[name]), None, "{name}");
    }
    assert_eq!(infer(&["日本-美国"]), None);
    assert_eq!(infer(&["Fast-B2", "Japan-Tokyo"]).as_deref(), Some("JP"));
    assert_eq!(infer(&["Japan", "美国"]), None);
    assert_eq!(infer(&["Japan", "Japan", "Fast"]).as_deref(), Some("JP"));
    assert_eq!(infer(&["Fast", "Japan"]), infer(&["Japan", "Fast"]));
}

#[test]
fn serde_rejects_unknown_and_lowercase_codes() {
    assert_eq!(
        serde_json::to_string(&DisplayTerritoryCode::new("HK").unwrap()).unwrap(),
        "\"HK\""
    );
    assert!(serde_json::from_str::<DisplayTerritoryCode>("\"hk\"").is_err());
    assert!(serde_json::from_str::<DisplayTerritoryCode>("\"EU\"").is_err());
}

#[test]
fn dedupe_infers_once_from_all_aliases_without_changing_identity() {
    let node = |name: &str| {
        let mut spec = NodeSpec::minimal("vless", "node.example", 443);
        spec.display_name = Some(name.to_owned());
        spec.uuid = Some("550e8400-e29b-41d4-a716-446655440000".to_owned());
        spec.tls = true;
        validate_node_spec(spec, &CapabilityMatrix::default())
            .unwrap()
            .node
    };
    let first = node("Fast-B2");
    let second = node("Japan-Tokyo");
    let fingerprint = nethop_subscription::fingerprint_node(&first);
    let (nodes, _) = dedupe_sources(
        vec![
            SourceBatch {
                source_id: SourceId::new("src_11111111111111111111111111111111").unwrap(),
                nodes: vec![first],
                rejected: 0,
                warnings: 0,
            },
            SourceBatch {
                source_id: SourceId::new("src_22222222222222222222222222222222").unwrap(),
                nodes: vec![second],
                rejected: 0,
                warnings: 0,
            },
        ],
        &ParserLimits::default(),
    );
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].fingerprint, fingerprint);
    assert_eq!(nodes[0].display_territory_code.unwrap().as_str(), "JP");
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NameFixture {
    schema: String,
    sample_id: String,
    source_sha256: String,
    format: String,
    nodes: Vec<NameFixtureNode>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NameFixtureNode {
    name: String,
    expected_territory_code: Option<String>,
    information_node: bool,
}

fn verify_fixture(bytes: &[u8], expected_id: &str, expected_count: usize) -> Vec<Option<String>> {
    let fixture: NameFixture = serde_json::from_slice(bytes).unwrap();
    assert_eq!(fixture.schema, "nethop-territory-name-fixture-v1");
    assert_eq!(fixture.sample_id, expected_id);
    assert_eq!(fixture.format, "clash_yaml");
    assert_eq!(fixture.source_sha256.len(), 64);
    assert_eq!(fixture.nodes.len(), expected_count);
    fixture
        .nodes
        .iter()
        .map(|node| {
            let actual = infer(&[&node.name]);
            assert_eq!(actual, node.expected_territory_code, "{}", node.name);
            assert_eq!(
                node.information_node,
                expected_id == "magic-ring" && actual.is_none()
            );
            actual
        })
        .collect()
}

#[test]
fn sanitized_real_name_fixtures_cover_expected_territories() {
    let glados = verify_fixture(
        include_bytes!("fixtures/territory/glados.json"),
        "glados",
        56,
    );
    assert_eq!(glados.iter().filter(|code| code.is_some()).count(), 44);
    assert_eq!(glados.iter().filter(|code| code.is_none()).count(), 12);

    let magic_ring = verify_fixture(
        include_bytes!("fixtures/territory/magic-ring.json"),
        "magic-ring",
        44,
    );
    assert_eq!(magic_ring.iter().filter(|code| code.is_some()).count(), 42);
    assert_eq!(magic_ring.iter().filter(|code| code.is_none()).count(), 2);

    let fsllist = verify_fixture(
        include_bytes!("fixtures/territory/fsllist.json"),
        "fsllist",
        51,
    );
    let histogram = fsllist.into_iter().flatten().fold(
        std::collections::BTreeMap::new(),
        |mut counts, code| {
            *counts.entry(code).or_insert(0usize) += 1;
            counts
        },
    );
    assert_eq!(
        histogram,
        std::collections::BTreeMap::from([
            ("FR".to_owned(), 1),
            ("GB".to_owned(), 4),
            ("HK".to_owned(), 3),
            ("ID".to_owned(), 1),
            ("IN".to_owned(), 1),
            ("JP".to_owned(), 4),
            ("NL".to_owned(), 4),
            ("RO".to_owned(), 12),
            ("SG".to_owned(), 2),
            ("US".to_owned(), 19),
        ])
    );
}
