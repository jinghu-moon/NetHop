pub use nethop_core::{
    DisplayTerritoryCode, InvalidTerritoryCode, TerritoryRecord, territories, territory_by_alpha2,
    territory_by_alpha3,
};
use std::cmp::Ordering;

mod generated {
    include!("generated/territory_recognition.rs");
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Strength {
    Emoji,
    Name,
    Code,
    LocationCode,
    City,
}

#[derive(Default)]
struct Evidence {
    best: Option<Strength>,
    code: Option<DisplayTerritoryCode>,
    conflict: bool,
}

impl Evidence {
    fn add(&mut self, strength: Strength, code: DisplayTerritoryCode) {
        match self.best {
            None => {
                self.best = Some(strength);
                self.code = Some(code);
                self.conflict = false;
            }
            Some(best) if strength < best => {
                self.best = Some(strength);
                self.code = Some(code);
                self.conflict = false;
            }
            Some(best) if strength == best && self.code != Some(code) => self.conflict = true,
            Some(_) => {}
        }
    }

    fn resolve(self) -> Option<DisplayTerritoryCode> {
        (!self.conflict).then_some(self.code).flatten()
    }
}

pub fn infer_display_territory<'a>(
    names: impl IntoIterator<Item = &'a str>,
) -> Option<DisplayTerritoryCode> {
    let mut evidence = Evidence::default();
    for name in names {
        collect_name(name, &mut evidence);
    }
    evidence.resolve()
}

fn collect_name(name: &str, evidence: &mut Evidence) {
    collect_flags(name, evidence);
    collect_indexed_phrases(name, evidence);
    collect_codes(name, evidence);
}

fn collect_indexed_phrases(name: &str, evidence: &mut Evidence) {
    const MAX_BOUNDARIES: usize = 130;
    let mut boundaries = [0usize; MAX_BOUNDARIES];
    let mut count = 1usize;
    for (index, _) in name.char_indices().skip(1) {
        if count == MAX_BOUNDARIES - 1 {
            break;
        }
        if logical_boundary(name, index) {
            boundaries[count] = index;
            count += 1;
        }
    }
    boundaries[count] = name.len();
    count += 1;

    for start_index in 0..count - 1 {
        let start = boundaries[start_index];
        for &end in boundaries
            .iter()
            .take(usize::min(start_index + 4, count))
            .skip(start_index + 1)
        {
            if start == end || end - start > 64 {
                continue;
            }
            if let Some((code, strength)) =
                indexed_lookup(generated::PHRASE_ROWS, &name[start..end])
            {
                evidence.add(strength, code);
            }
        }
    }
}

fn collect_codes(name: &str, evidence: &mut Evidence) {
    for (start, token) in uppercase_runs(name) {
        if is_data_size_unit(name, start, token) {
            continue;
        }
        if let Some((code, strength)) = indexed_lookup(generated::CODE_ROWS, token) {
            evidence.add(strength, code);
        }
    }
}

fn uppercase_runs(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let bytes = text.as_bytes();
    (0..bytes.len()).filter_map(move |start| {
        if !bytes[start].is_ascii_uppercase() || start > 0 && bytes[start - 1].is_ascii_uppercase()
        {
            return None;
        }
        let end = (start..bytes.len())
            .find(|index| !bytes[*index].is_ascii_uppercase())
            .unwrap_or(bytes.len());
        Some((start, &text[start..end]))
    })
}

fn indexed_lookup(
    rows: &[(&str, &str, u8)],
    candidate: &str,
) -> Option<(DisplayTerritoryCode, Strength)> {
    rows.binary_search_by(|row| ascii_case_cmp(row.0, candidate))
        .ok()
        .map(|index| {
            (
                generated_code(rows[index].1),
                generated_strength(rows[index].2),
            )
        })
}

fn generated_strength(value: u8) -> Strength {
    match value {
        1 => Strength::Name,
        2 => Strength::Code,
        3 => Strength::LocationCode,
        4 => Strength::City,
        _ => panic!("generated territory strength"),
    }
}

fn ascii_case_cmp(left: &str, right: &str) -> Ordering {
    left.bytes()
        .map(|byte| byte.to_ascii_lowercase())
        .cmp(right.bytes().map(|byte| byte.to_ascii_lowercase()))
}

fn logical_boundary(text: &str, index: usize) -> bool {
    if index == 0 || index == text.len() {
        return true;
    }
    let left = text[..index].chars().next_back().expect("left character");
    let right = text[index..].chars().next().expect("right character");
    !left.is_alphanumeric()
        || !right.is_alphanumeric()
        || left.is_ascii_alphanumeric() != right.is_ascii_alphanumeric()
}

fn collect_flags(name: &str, evidence: &mut Evidence) {
    let mut chars = name.chars();
    while let Some(first) = chars.next() {
        let first = first as u32;
        if !(0x1f1e6..=0x1f1ff).contains(&first) {
            continue;
        }
        let Some(second) = chars.next() else {
            break;
        };
        let second = second as u32;
        if !(0x1f1e6..=0x1f1ff).contains(&second) {
            continue;
        }
        let bytes = [
            b'A' + (first - 0x1f1e6) as u8,
            b'A' + (second - 0x1f1e6) as u8,
        ];
        if let Ok(text) = std::str::from_utf8(&bytes)
            && let Some(code) = DisplayTerritoryCode::new(text)
        {
            evidence.add(Strength::Emoji, code);
        }
    }
}

fn generated_code(value: &str) -> DisplayTerritoryCode {
    DisplayTerritoryCode::new(value).expect("generated territory reference")
}

fn is_data_size_unit(text: &str, start: usize, token: &str) -> bool {
    token == "GB"
        && text[..start]
            .trim_end()
            .chars()
            .next_back()
            .is_some_and(|character| character.is_ascii_digit())
}
