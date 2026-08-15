use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::Read,
    path::{Path, PathBuf},
};

use flate2::read::GzDecoder;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
struct SourceManifest {
    schema: String,
    sources: Vec<SourceEntry>,
}
#[derive(Deserialize)]
struct SourceEntry {
    id: String,
    version: String,
    url: String,
    path: String,
    sha256: String,
    license: String,
}
#[derive(Deserialize)]
struct RecognitionFile {
    schema: String,
    territories: Vec<Recognition>,
}
#[derive(Deserialize)]
struct Recognition {
    code: String,
    #[serde(default)]
    code_aliases: Vec<String>,
    #[serde(default)]
    english_aliases: Vec<String>,
    #[serde(default)]
    chinese_aliases: Vec<String>,
}
#[derive(Deserialize)]
struct LocationFile {
    schema: String,
    locations: Vec<Location>,
}
#[derive(Deserialize)]
struct Location {
    territory_code: String,
    city_names: Vec<String>,
    airport_codes: Vec<String>,
    metropolitan_codes: Vec<String>,
}
#[derive(Deserialize)]
struct SupplementFile {
    schema: String,
    territories: Vec<Supplement>,
}
#[derive(Deserialize)]
struct Supplement {
    code: String,
    alpha3: String,
    source_id: String,
}
#[derive(Deserialize)]
struct M49Row {
    #[serde(rename = "Country or Area")]
    country: String,
    #[serde(rename = "ISO-alpha2 Code")]
    alpha2: String,
    #[serde(rename = "ISO-alpha3 Code")]
    alpha3: String,
}
struct Territory {
    alpha2: String,
    alpha3: String,
    english: String,
    chinese: String,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("territory generator failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut arguments = env::args().skip(1);
    let root = arguments
        .next()
        .map(PathBuf::from)
        .unwrap_or(env::current_dir().map_err(err)?);
    let output_root = match arguments.next() {
        Some(flag) if flag == "--output-root" => arguments
            .next()
            .map(PathBuf::from)
            .ok_or("missing --output-root value")?,
        Some(_) => {
            return Err("usage: territory-generator [workspace-root] [--output-root path]".into());
        }
        None => root.clone(),
    };
    if arguments.next().is_some() {
        return Err("unexpected territory-generator argument".into());
    }
    let data = root.join("data/territories");
    let manifest: SourceManifest = read_json(&data.join("source-versions.json"))?;
    if manifest.schema != "nethop-territory-sources-v1" || manifest.sources.len() != 5 {
        return Err("unsupported source manifest".into());
    }
    for source in &manifest.sources {
        if source.id.is_empty()
            || source.version.is_empty()
            || source.license.is_empty()
            || !source.url.starts_with("https://")
        {
            return Err("incomplete source provenance".into());
        }
        let path = root.join(&source.path);
        if !path.starts_with(&root) || sha256(&fs::read(path).map_err(err)?) != source.sha256 {
            return Err(format!("source digest mismatch: {}", source.id));
        }
    }
    let en = cldr_names(&data.join("upstream/cldr-48.2.0-territories-en.json"), "en")?;
    let zh = cldr_names(
        &data.join("upstream/cldr-48.2.0-territories-zh-Hans.json"),
        "zh-Hans",
    )?;
    let mut territories = m49(&data.join("upstream/un-m49-country-area-en.csv"), &en, &zh)?;
    let supplements: SupplementFile =
        toml::from_str(&fs::read_to_string(data.join("identity-supplements.toml")).map_err(err)?)
            .map_err(err)?;
    apply_supplements(
        &mut territories,
        &supplements,
        &data.join("upstream/cldr-48.2.0-codeMappings.json"),
    )?;
    let recognition: RecognitionFile =
        toml::from_str(&fs::read_to_string(data.join("recognition.toml")).map_err(err)?)
            .map_err(err)?;
    let locations: LocationFile =
        toml::from_str(&fs::read_to_string(data.join("locations.toml")).map_err(err)?)
            .map_err(err)?;
    validate_manual(&territories, &recognition, &locations)?;
    write_registry(
        &output_root.join("crates/nethop-core/src/generated/territory_registry.rs"),
        &territories,
    )?;
    write_recognition(
        &output_root.join("crates/nethop-subscription/src/generated/territory_recognition.rs"),
        &territories,
        &recognition,
        &locations,
    )?;
    write_web_manifest(
        &output_root.join("webui/src/generated/territories.ts"),
        &territories,
    )?;
    extract_flags(
        &data.join("upstream/country-flag-icons-1.6.20.tgz"),
        &output_root.join("webui/src/assets/flags"),
        &territories,
    )?;
    Ok(())
}

fn m49(
    path: &Path,
    en: &BTreeMap<String, String>,
    zh: &BTreeMap<String, String>,
) -> Result<Vec<Territory>, String> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(b';')
        .from_path(path)
        .map_err(err)?;
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    for row in reader.deserialize::<M49Row>() {
        let row = row.map_err(err)?;
        if row.alpha2.len() != 2 || row.alpha3.len() != 3 || !seen.insert(row.alpha2.clone()) {
            return Err(format!("invalid M49 identity: {}", row.country));
        }
        rows.push(Territory {
            english: en
                .get(&row.alpha2)
                .cloned()
                .ok_or_else(|| format!("missing CLDR en: {}", row.alpha2))?,
            chinese: zh
                .get(&row.alpha2)
                .cloned()
                .ok_or_else(|| format!("missing CLDR zh: {}", row.alpha2))?,
            alpha2: row.alpha2,
            alpha3: row.alpha3,
        });
    }
    rows.sort_by(|a, b| a.alpha2.cmp(&b.alpha2));
    if rows.len() < 240 {
        return Err("M49 identity set is incomplete".into());
    }
    Ok(rows)
}

fn cldr_names(path: &Path, locale: &str) -> Result<BTreeMap<String, String>, String> {
    let value: Value = read_json(path)?;
    let object = value
        .pointer(&format!("/main/{locale}/localeDisplayNames/territories"))
        .and_then(Value::as_object)
        .ok_or("invalid CLDR territories")?;
    Ok(object
        .iter()
        .filter(|(code, _)| code.len() == 2 && code.bytes().all(|b| b.is_ascii_uppercase()))
        .filter(|(code, _)| !matches!(code.as_str(), "EU" | "EZ" | "UN" | "XA" | "XB"))
        .filter_map(|(code, value)| value.as_str().map(|name| (code.clone(), name.to_owned())))
        .collect())
}

fn apply_supplements(
    territories: &mut Vec<Territory>,
    supplements: &SupplementFile,
    mappings_path: &Path,
) -> Result<(), String> {
    if supplements.schema != "nethop-territory-identity-supplements-v1" {
        return Err("unsupported identity supplement schema".into());
    }
    let mappings: Value = read_json(mappings_path)?;
    for supplement in &supplements.territories {
        let mapped = mappings
            .pointer(&format!(
                "/supplemental/codeMappings/{}/_alpha3",
                supplement.code
            ))
            .and_then(Value::as_str);
        if supplement.source_id != "cldr-code-mappings"
            || mapped != Some(supplement.alpha3.as_str())
            || territories
                .iter()
                .any(|item| item.alpha2 == supplement.code)
        {
            return Err(format!("invalid identity supplement: {}", supplement.code));
        }
        let english = cldr_names(
            &mappings_path.with_file_name("cldr-48.2.0-territories-en.json"),
            "en",
        )?
        .remove(&supplement.code)
        .ok_or("supplement has no English name")?;
        let chinese = cldr_names(
            &mappings_path.with_file_name("cldr-48.2.0-territories-zh-Hans.json"),
            "zh-Hans",
        )?
        .remove(&supplement.code)
        .ok_or("supplement has no Chinese name")?;
        territories.push(Territory {
            alpha2: supplement.code.clone(),
            alpha3: supplement.alpha3.clone(),
            english,
            chinese,
        });
    }
    territories.sort_by(|a, b| a.alpha2.cmp(&b.alpha2));
    Ok(())
}

fn validate_manual(
    territories: &[Territory],
    recognition: &RecognitionFile,
    locations: &LocationFile,
) -> Result<(), String> {
    if recognition.schema != "nethop-territory-recognition-v1"
        || locations.schema != "nethop-territory-locations-v1"
    {
        return Err("unsupported manual data schema".into());
    }
    let codes: BTreeSet<_> = territories.iter().map(|r| r.alpha2.as_str()).collect();
    let mut aliases = BTreeSet::new();
    for item in &recognition.territories {
        if !codes.contains(item.code.as_str()) {
            return Err(format!("unknown recognition territory: {}", item.code));
        }
        for alias in item
            .code_aliases
            .iter()
            .chain(&item.english_aliases)
            .chain(&item.chinese_aliases)
        {
            if alias.is_empty() || !aliases.insert(alias) {
                return Err(format!("duplicate recognition alias: {alias}"));
            }
        }
    }
    let mut location_codes = BTreeSet::new();
    for item in &locations.locations {
        if !codes.contains(item.territory_code.as_str()) {
            return Err(format!(
                "unknown location territory: {}",
                item.territory_code
            ));
        }
        for code in item.airport_codes.iter().chain(&item.metropolitan_codes) {
            if code.len() != 3
                || !code.bytes().all(|b| b.is_ascii_uppercase())
                || !location_codes.insert(code)
            {
                return Err(format!("invalid location code: {code}"));
            }
        }
    }
    Ok(())
}

fn write_registry(path: &Path, territories: &[Territory]) -> Result<(), String> {
    let mut out = String::from("// @generated by territory-generator; do not edit.\n");
    out.push_str("pub const TERRITORY_ROWS: &[(&str, &str, &str, &str)] = &[\n");
    for r in territories {
        out.push_str(&format!(
            "    ({:?}, {:?}, {:?}, {:?}),\n",
            r.alpha2, r.alpha3, r.english, r.chinese
        ));
    }
    out.push_str("];\n");
    write(path, &out)
}
fn write_recognition(
    path: &Path,
    territories: &[Territory],
    recognition: &RecognitionFile,
    locations: &LocationFile,
) -> Result<(), String> {
    let mut out = String::from("// @generated by territory-generator; do not edit.\n");
    let mut phrases = BTreeMap::new();
    for territory in territories {
        insert_index(&mut phrases, &territory.english, &territory.alpha2, 1)?;
        insert_index(&mut phrases, &territory.chinese, &territory.alpha2, 1)?;
    }
    for territory in &recognition.territories {
        for alias in territory
            .english_aliases
            .iter()
            .chain(&territory.chinese_aliases)
        {
            insert_index(&mut phrases, alias, &territory.code, 1)?;
        }
    }
    for location in &locations.locations {
        for city in &location.city_names {
            insert_index(&mut phrases, city, &location.territory_code, 4)?;
        }
    }
    out.push_str("pub const PHRASE_ROWS: &[(&str, &str, u8)] = &[\n");
    for (_, (name, code, strength)) in phrases {
        out.push_str(&format!("    ({name:?}, {code:?}, {strength}),\n"));
    }
    out.push_str("];\n");

    let mut codes = BTreeMap::new();
    for territory in territories {
        insert_index(&mut codes, &territory.alpha2, &territory.alpha2, 2)?;
        insert_index(&mut codes, &territory.alpha3, &territory.alpha2, 2)?;
    }
    for territory in &recognition.territories {
        for alias in &territory.code_aliases {
            insert_index(&mut codes, alias, &territory.code, 2)?;
        }
    }
    for location in &locations.locations {
        for code in location
            .airport_codes
            .iter()
            .chain(&location.metropolitan_codes)
        {
            insert_index(&mut codes, code, &location.territory_code, 3)?;
        }
    }
    out.push_str("pub const CODE_ROWS: &[(&str, &str, u8)] = &[\n");
    for (_, (name, code, strength)) in codes {
        out.push_str(&format!("    ({name:?}, {code:?}, {strength}),\n"));
    }
    out.push_str("];\n");
    write(path, &out)
}

fn insert_index(
    rows: &mut BTreeMap<String, (String, String, u8)>,
    name: &str,
    code: &str,
    strength: u8,
) -> Result<(), String> {
    let key = name.to_ascii_lowercase();
    if let Some((_, existing, existing_strength)) = rows.get(&key) {
        if existing == code {
            if strength < *existing_strength {
                rows.insert(key, (name.to_owned(), code.to_owned(), strength));
            }
            return Ok(());
        }
        if strength == *existing_strength {
            return Err(format!("ambiguous territory name index: {name}"));
        }
        if strength < *existing_strength {
            rows.insert(key, (name.to_owned(), code.to_owned(), strength));
        }
        return Ok(());
    }
    rows.insert(key, (name.to_owned(), code.to_owned(), strength));
    Ok(())
}

fn write_web_manifest(path: &Path, territories: &[Territory]) -> Result<(), String> {
    let rows: Vec<_> = territories
        .iter()
        .map(|record| {
            json!({
                "code": record.alpha2,
                "alpha3": record.alpha3,
                "englishName": record.english,
                "chineseName": record.chinese,
            })
        })
        .collect();
    let serialized = serde_json::to_string_pretty(&rows).map_err(err)?;
    write(
        path,
        &format!(
            "// @generated by territory-generator; do not edit.\nexport const territories = {serialized} as const;\nexport type TerritoryCode = typeof territories[number][\"code\"];\nexport const territoryCodes = territories.map((territory) => territory.code) as readonly TerritoryCode[];\n"
        ),
    )
}

fn extract_flags(
    archive_path: &Path,
    output: &Path,
    territories: &[Territory],
) -> Result<(), String> {
    fs::create_dir_all(output).map_err(err)?;
    let wanted: BTreeSet<_> = territories
        .iter()
        .map(|r| format!("package/3x2/{}.svg", r.alpha2))
        .collect();
    let file = fs::File::open(archive_path).map_err(err)?;
    let mut archive = tar::Archive::new(GzDecoder::new(file));
    let mut found = BTreeSet::new();
    for entry in archive.entries().map_err(err)? {
        let mut entry = entry.map_err(err)?;
        let name = entry
            .path()
            .map_err(err)?
            .to_string_lossy()
            .replace('\\', "/");
        if !wanted.contains(&name) {
            continue;
        }
        let mut svg = String::new();
        entry.read_to_string(&mut svg).map_err(err)?;
        let normalized = svg.to_ascii_lowercase();
        if svg.len() > 16 * 1024
            || !svg.starts_with("<svg")
            || normalized.contains("<script")
            || normalized.contains("<foreignobject")
            || normalized.contains(" onload=")
            || normalized.contains(" onclick=")
            || normalized.contains(" href=")
            || normalized.contains("xlink:href")
            || normalized.contains("url(http")
            || normalized.contains(" src=")
        {
            return Err(format!("unsafe flag SVG: {name}"));
        }
        let code = &name[12..14];
        write(&output.join(format!("{code}.svg")), &svg)?;
        found.insert(name);
    }
    if found != wanted {
        return Err("flag coverage is incomplete".into());
    }
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    serde_json::from_slice(&fs::read(path).map_err(err)?).map_err(err)
}
fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn write(path: &Path, value: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(err)?;
    }
    fs::write(path, value.as_bytes()).map_err(err)
}
fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
