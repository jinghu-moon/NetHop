use serde::{Deserialize, Serialize, de::IgnoredAny};
use thiserror::Error;

use crate::{
    diagnostics::DiagnosticCode,
    limits::ParserLimits,
    normalize::{NormalizationError, NormalizedPayload, normalize_bytes},
    payload::{FormatHint, ImportPayload},
};

const URI_SCHEMES: &[&str] = &[
    "vless://",
    "vmess://",
    "ss://",
    "trojan://",
    "hysteria2://",
    "hy2://",
    "tuic://",
    "anytls://",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EvidenceStrength {
    Strong,
    Weak,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Base64Alphabet {
    Standard,
    UrlSafe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Base64Padding {
    Present,
    Missing,
    NotRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Base64Details {
    pub alphabet: Base64Alphabet,
    pub padding: Base64Padding,
    pub compact_len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormatEvidence {
    pub format: FormatHint,
    pub strength: EvidenceStrength,
    pub base64: Option<Base64Details>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectionResult {
    format: FormatHint,
    strength: EvidenceStrength,
    evidence: Vec<FormatEvidence>,
}

impl DetectionResult {
    pub const fn format(&self) -> FormatHint {
        self.format
    }

    pub const fn strength(&self) -> EvidenceStrength {
        self.strength
    }

    pub fn evidence(&self) -> &[FormatEvidence] {
        &self.evidence
    }

    pub fn base64_details(&self) -> Option<&Base64Details> {
        self.evidence
            .iter()
            .find(|candidate| candidate.format == FormatHint::Base64List)
            .and_then(|candidate| candidate.base64.as_ref())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DetectionError {
    #[error(transparent)]
    Normalize(#[from] NormalizationError),
    #[error("payload is empty after normalization")]
    EmptyInput,
    #[error("payload format is unknown")]
    UnknownFormat,
    #[error("multiple strong format candidates remain: {candidates:?}")]
    AmbiguousFormat { candidates: Vec<FormatHint> },
    #[error("expected format {expected:?} does not match detected evidence")]
    FormatHintMismatch {
        expected: FormatHint,
        candidates: Vec<FormatHint>,
    },
    #[error("JSON structure is invalid")]
    InvalidJson,
    #[error("YAML structure is invalid")]
    InvalidYaml,
}

impl DetectionError {
    pub const fn code(&self) -> DiagnosticCode {
        match self {
            Self::Normalize(error) => error.code(),
            Self::EmptyInput => DiagnosticCode::EmptyInput,
            Self::UnknownFormat => DiagnosticCode::UnknownFormat,
            Self::AmbiguousFormat { .. } => DiagnosticCode::AmbiguousFormat,
            Self::FormatHintMismatch { .. } => DiagnosticCode::FormatHintMismatch,
            Self::InvalidJson => DiagnosticCode::InvalidJson,
            Self::InvalidYaml => DiagnosticCode::InvalidYaml,
        }
    }

    pub fn candidates(&self) -> &[FormatHint] {
        match self {
            Self::AmbiguousFormat { candidates } | Self::FormatHintMismatch { candidates, .. } => {
                candidates
            }
            _ => &[],
        }
    }

    pub const fn terminal_format(&self) -> Option<FormatHint> {
        match self {
            Self::InvalidJson => Some(FormatHint::SingboxJson),
            Self::InvalidYaml => Some(FormatHint::ClashYaml),
            _ => None,
        }
    }
}

pub fn detect_format(
    payload: &ImportPayload,
    limits: &ParserLimits,
) -> Result<DetectionResult, DetectionError> {
    detect_bytes(payload.bytes(), payload.expected_format(), limits)
}

pub fn detect_bytes(
    bytes: &[u8],
    expected: FormatHint,
    limits: &ParserLimits,
) -> Result<DetectionResult, DetectionError> {
    let normalized = normalize_bytes(bytes, limits)?;
    detect_normalized(&normalized, expected)
}

pub fn detect_normalized(
    payload: &NormalizedPayload<'_>,
    expected: FormatHint,
) -> Result<DetectionResult, DetectionError> {
    if payload.is_empty() {
        return Err(DetectionError::EmptyInput);
    }

    let mut evidence = Vec::with_capacity(4);
    collect_profile_evidence(payload, &mut evidence);
    let has_profile_evidence = evidence
        .iter()
        .any(|candidate| candidate.format == FormatHint::IniProfile);
    let first = payload.as_str().as_bytes()[0];
    if matches!(first, b'{' | b'[') && !has_profile_evidence {
        validate_json(payload.as_str())?;
        if supported_json_shape(payload.as_str()) {
            evidence.push(strong(FormatHint::SingboxJson));
        }
    }

    let yaml = yaml_evidence(payload);
    if yaml.strong {
        if !yaml.valid {
            return Err(DetectionError::InvalidYaml);
        }
        evidence.push(strong(FormatHint::ClashYaml));
    }

    if has_uri_evidence(payload) {
        evidence.push(strong(FormatHint::UriList));
    }
    if let Some(details) = base64_evidence(payload.as_str()) {
        evidence.push(FormatEvidence {
            format: FormatHint::Base64List,
            strength: EvidenceStrength::Weak,
            base64: Some(details),
        });
    }

    choose_evidence(evidence, expected)
}

fn strong(format: FormatHint) -> FormatEvidence {
    FormatEvidence {
        format,
        strength: EvidenceStrength::Strong,
        base64: None,
    }
}

fn choose_evidence(
    evidence: Vec<FormatEvidence>,
    expected: FormatHint,
) -> Result<DetectionResult, DetectionError> {
    let strong: Vec<_> = evidence
        .iter()
        .filter(|candidate| candidate.strength == EvidenceStrength::Strong)
        .map(|candidate| candidate.format)
        .collect();

    if expected != FormatHint::Auto {
        if let Some(candidate) = evidence
            .iter()
            .find(|candidate| formats_match(expected, candidate.format))
        {
            return Ok(DetectionResult {
                format: expected,
                strength: candidate.strength,
                evidence,
            });
        }
        return Err(DetectionError::FormatHintMismatch {
            expected,
            candidates: evidence.iter().map(|candidate| candidate.format).collect(),
        });
    }

    match strong.as_slice() {
        [format] => Ok(DetectionResult {
            format: *format,
            strength: EvidenceStrength::Strong,
            evidence,
        }),
        [_, _, ..] => Err(DetectionError::AmbiguousFormat { candidates: strong }),
        [] => match evidence.as_slice() {
            [candidate] => Ok(DetectionResult {
                format: candidate.format,
                strength: candidate.strength,
                evidence,
            }),
            [] => Err(DetectionError::UnknownFormat),
            _ => Err(DetectionError::AmbiguousFormat {
                candidates: evidence.iter().map(|candidate| candidate.format).collect(),
            }),
        },
    }
}

fn formats_match(expected: FormatHint, actual: FormatHint) -> bool {
    expected == actual
        || matches!(
            (expected, actual),
            (FormatHint::SurfboardIni, FormatHint::IniProfile)
        )
}

fn validate_json(text: &str) -> Result<(), DetectionError> {
    let mut deserializer = serde_json::Deserializer::from_str(text);
    IgnoredAny::deserialize(&mut deserializer).map_err(|_| DetectionError::InvalidJson)?;
    deserializer.end().map_err(|_| DetectionError::InvalidJson)
}

fn supported_json_shape(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes[0] == b'{' {
        top_level_object_has_array_key(bytes, b"outbounds")
    } else {
        top_level_array_has_outbound_marker(bytes)
    }
}

fn top_level_object_has_array_key(bytes: &[u8], key: &[u8]) -> bool {
    let mut index = 1;
    let mut depth = 1usize;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                let Some((value, next)) = json_string(bytes, index) else {
                    return false;
                };
                if depth == 1 && value == key {
                    let mut cursor = skip_ascii_space(bytes, next);
                    if bytes.get(cursor) == Some(&b':') {
                        cursor = skip_ascii_space(bytes, cursor + 1);
                        if bytes.get(cursor) == Some(&b'[') {
                            return true;
                        }
                    }
                }
                index = next;
            }
            b'{' | b'[' => {
                depth += 1;
                index += 1;
            }
            b'}' | b']' => {
                depth = depth.saturating_sub(1);
                index += 1;
            }
            _ => index += 1,
        }
    }
    false
}

fn top_level_array_has_outbound_marker(bytes: &[u8]) -> bool {
    let mut index = 1;
    let mut depth = 1usize;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                let Some((value, next)) = json_string(bytes, index) else {
                    return false;
                };
                if depth == 2 && matches!(value, b"type" | b"tag") {
                    return true;
                }
                index = next;
            }
            b'{' | b'[' => {
                depth += 1;
                index += 1;
            }
            b'}' | b']' => {
                depth = depth.saturating_sub(1);
                index += 1;
            }
            _ => index += 1,
        }
    }
    false
}

fn json_string(bytes: &[u8], start: usize) -> Option<(&[u8], usize)> {
    let mut index = start + 1;
    let content_start = index;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            return Some((&bytes[content_start..index], index + 1));
        }
        index += 1;
    }
    None
}

fn skip_ascii_space(bytes: &[u8], mut index: usize) -> usize {
    while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
        index += 1;
    }
    index
}

#[derive(Debug, Clone, Copy)]
struct YamlEvidence {
    strong: bool,
    valid: bool,
}

fn yaml_evidence(payload: &NormalizedPayload<'_>) -> YamlEvidence {
    if matches!(payload.as_bytes().first(), Some(b'{' | b'[')) {
        return YamlEvidence {
            strong: false,
            valid: true,
        };
    }
    let mut lines = payload.lines();
    while let Some(line) = lines.next() {
        let raw = line.text();
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed == "---" {
            continue;
        }
        if raw.len() != raw.trim_start().len() || !trimmed.starts_with("proxies:") {
            continue;
        }
        let remainder = trimmed["proxies:".len()..].trim();
        let inline_sequence = remainder.starts_with('[');
        let block_sequence = remainder.is_empty()
            && lines.clone().any(|candidate| {
                let candidate = candidate.text();
                !candidate.trim().is_empty()
                    && candidate.len() != candidate.trim_start().len()
                    && candidate.trim_start().starts_with('-')
            });
        if inline_sequence || block_sequence {
            return YamlEvidence {
                strong: true,
                valid: balanced_yaml_flow(payload.as_str()),
            };
        }
    }
    YamlEvidence {
        strong: false,
        valid: true,
    }
}

fn balanced_yaml_flow(text: &str) -> bool {
    let mut square = 0usize;
    let mut curly = 0usize;
    let mut single = false;
    let mut double = false;
    let mut escaped = false;
    for byte in text.bytes() {
        if double && escaped {
            escaped = false;
            continue;
        }
        if double && byte == b'\\' {
            escaped = true;
            continue;
        }
        match byte {
            b'\'' if !double => single = !single,
            b'"' if !single => double = !double,
            b'[' if !single && !double => square += 1,
            b']' if !single && !double => {
                if square == 0 {
                    return false;
                }
                square -= 1;
            }
            b'{' if !single && !double => curly += 1,
            b'}' if !single && !double => {
                if curly == 0 {
                    return false;
                }
                curly -= 1;
            }
            _ => {}
        }
    }
    square == 0 && curly == 0 && !single && !double && !escaped
}

fn has_uri_evidence(payload: &NormalizedPayload<'_>) -> bool {
    payload.lines().any(|line| {
        let line = line.text().trim_start();
        !line.starts_with('#')
            && URI_SCHEMES.iter().any(|scheme| {
                line.strip_prefix(scheme)
                    .is_some_and(|rest| !rest.is_empty())
            })
    })
}

fn collect_profile_evidence(payload: &NormalizedPayload<'_>, evidence: &mut Vec<FormatEvidence>) {
    for line in payload.lines() {
        let line = line.text().trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if matches!(line, "[Proxy]" | "[General]" | "[Proxy Group]") {
            evidence.push(strong(FormatHint::IniProfile));
        }
        return;
    }
}

fn base64_evidence(text: &str) -> Option<Base64Details> {
    let compact_len = text
        .bytes()
        .filter(|byte| !matches!(byte, b'\r' | b'\n'))
        .count();
    if compact_len < 4 || compact_len % 4 == 1 {
        return None;
    }
    let mut standard_specific = false;
    let mut url_specific = false;
    let mut padding_index = None;
    for (compact_index, byte) in text
        .bytes()
        .filter(|byte| !matches!(byte, b'\r' | b'\n'))
        .enumerate()
    {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' => {
                if padding_index.is_some() {
                    return None;
                }
            }
            b'+' | b'/' => {
                if padding_index.is_some() {
                    return None;
                }
                standard_specific = true;
            }
            b'-' | b'_' => {
                if padding_index.is_some() {
                    return None;
                }
                url_specific = true;
            }
            b'=' => {
                padding_index.get_or_insert(compact_index);
            }
            _ => return None,
        };
    }
    if standard_specific && url_specific {
        return None;
    }
    if let Some(index) = padding_index {
        let padding = compact_len - index;
        if padding > 2 || compact_len % 4 != 0 {
            return None;
        }
    }
    let alphabet = if url_specific {
        Base64Alphabet::UrlSafe
    } else {
        Base64Alphabet::Standard
    };
    let padding = if padding_index.is_some() {
        Base64Padding::Present
    } else if compact_len % 4 == 0 {
        Base64Padding::NotRequired
    } else {
        Base64Padding::Missing
    };
    Some(Base64Details {
        alphabet,
        padding,
        compact_len,
    })
}
