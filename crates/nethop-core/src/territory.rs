use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

mod generated {
    include!("generated/territory_registry.rs");
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DisplayTerritoryCode([u8; 2]);

impl DisplayTerritoryCode {
    pub fn new(value: &str) -> Option<Self> {
        let bytes = value.as_bytes();
        if bytes.len() != 2
            || !bytes.iter().all(u8::is_ascii_uppercase)
            || territory_by_alpha2(value).is_none()
        {
            return None;
        }
        Some(Self([bytes[0], bytes[1]]))
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).expect("validated ASCII territory")
    }
}

impl fmt::Debug for DisplayTerritoryCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl fmt::Display for DisplayTerritoryCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for DisplayTerritoryCode {
    type Err = InvalidTerritoryCode;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value).ok_or(InvalidTerritoryCode)
    }
}

impl Serialize for DisplayTerritoryCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for DisplayTerritoryCode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(&value).ok_or_else(|| de::Error::custom("invalid territory code"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidTerritoryCode;

impl fmt::Display for InvalidTerritoryCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid territory code")
    }
}

impl std::error::Error for InvalidTerritoryCode {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerritoryRecord {
    pub code: DisplayTerritoryCode,
    pub alpha3: &'static str,
    pub english_name: &'static str,
    pub chinese_name: &'static str,
}

pub fn territories() -> impl ExactSizeIterator<Item = TerritoryRecord> {
    generated::TERRITORY_ROWS.iter().map(record)
}

pub fn territory_by_alpha2(value: &str) -> Option<TerritoryRecord> {
    generated::TERRITORY_ROWS
        .binary_search_by_key(&value, |row| row.0)
        .ok()
        .map(|index| record(&generated::TERRITORY_ROWS[index]))
}

pub fn territory_by_alpha3(value: &str) -> Option<TerritoryRecord> {
    generated::TERRITORY_ROWS
        .iter()
        .find(|row| row.1 == value)
        .map(record)
}

fn record(row: &(&'static str, &'static str, &'static str, &'static str)) -> TerritoryRecord {
    TerritoryRecord {
        code: DisplayTerritoryCode([row.0.as_bytes()[0], row.0.as_bytes()[1]]),
        alpha3: row.1,
        english_name: row.2,
        chinese_name: row.3,
    }
}
