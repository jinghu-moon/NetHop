use std::{borrow::Cow, fmt};

use base64::{Engine, engine::general_purpose};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    detect::{DetectionError, DetectionResult},
    diagnostics::DiagnosticCode,
    limits::ParserLimits,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Base64Variant {
    Standard,
    UrlSafe,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Base64ContainerError {
    #[error("Base64 nesting depth is greater than one")]
    Nested,
    #[error("Base64 alphabet or padding is invalid")]
    Invalid,
    #[error("Base64 input exceeds {limit} bytes")]
    InputTooLarge { limit: usize },
    #[error("decoded Base64 output exceeds {limit} bytes")]
    OutputTooLarge { limit: usize },
    #[error("decoded Base64 output is not valid UTF-8")]
    InvalidUtf8,
    #[error("decoded payload format detection failed")]
    Detection(#[from] DetectionError),
}

impl Base64ContainerError {
    pub const fn code(&self) -> DiagnosticCode {
        match self {
            Self::Nested => DiagnosticCode::Base64NestingExceeded,
            Self::Invalid => DiagnosticCode::InvalidBase64,
            Self::InputTooLarge { .. } => DiagnosticCode::InputTooLarge,
            Self::OutputTooLarge { .. } => DiagnosticCode::InputTooLarge,
            Self::InvalidUtf8 => DiagnosticCode::InvalidUtf8,
            Self::Detection(error) => error.code(),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct DecodedSubscription {
    bytes: Vec<u8>,
    variant: Base64Variant,
    depth: u8,
    detected: Option<DetectionResult>,
}

impl fmt::Debug for DecodedSubscription {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DecodedSubscription")
            .field("bytes_len", &self.bytes.len())
            .field("variant", &self.variant)
            .field("depth", &self.depth)
            .field(
                "detected_format",
                &self.detected.as_ref().map(DetectionResult::format),
            )
            .finish()
    }
}

impl DecodedSubscription {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub const fn variant(&self) -> Base64Variant {
        self.variant
    }

    pub const fn depth(&self) -> u8 {
        self.depth
    }

    pub fn detected_format(&self) -> crate::payload::FormatHint {
        self.detected
            .as_ref()
            .map(DetectionResult::format)
            .unwrap_or(crate::payload::FormatHint::Auto)
    }

    pub fn detection(&self) -> Option<&DetectionResult> {
        self.detected.as_ref()
    }
}

pub fn decode_base64(
    input: &[u8],
    limits: &ParserLimits,
) -> Result<DecodedSubscription, Base64ContainerError> {
    decode_base64_at_depth(input, limits, 0)
}

pub fn decode_base64_at_depth(
    input: &[u8],
    limits: &ParserLimits,
    depth: u8,
) -> Result<DecodedSubscription, Base64ContainerError> {
    if depth > 0 {
        return Err(Base64ContainerError::Nested);
    }
    if input.len() > limits.max_body_bytes() {
        return Err(Base64ContainerError::InputTooLarge {
            limit: limits.max_body_bytes(),
        });
    }
    let (compact, variant) = compact_and_classify(input)?;
    let bytes = decode_compact(&compact, variant, limits.max_body_bytes())?;
    Ok(DecodedSubscription {
        bytes,
        variant,
        depth: depth.saturating_add(1),
        detected: None,
    })
}

pub fn decode_base64_and_detect(
    input: &[u8],
    limits: &ParserLimits,
) -> Result<DecodedSubscription, Base64ContainerError> {
    let mut decoded = decode_base64(input, limits)?;
    let detected = crate::detect_bytes(&decoded.bytes, crate::payload::FormatHint::Auto, limits)?;
    if detected.format() == crate::payload::FormatHint::Base64List {
        return Err(Base64ContainerError::Nested);
    }
    decoded.detected = Some(detected);
    Ok(decoded)
}

pub(crate) fn decode_base64_with_limit(
    input: &[u8],
    limit: usize,
) -> Result<DecodedBytes, Base64ContainerError> {
    let (compact, variant) = compact_and_classify(input)?;
    let bytes = decode_compact(&compact, variant, limit)?;
    Ok(DecodedBytes { bytes })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DecodedBytes {
    bytes: Vec<u8>,
}

impl DecodedBytes {
    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

fn compact_and_classify(
    input: &[u8],
) -> Result<(Cow<'_, [u8]>, Base64Variant), Base64ContainerError> {
    let mut compact = input
        .iter()
        .any(|byte| matches!(byte, b'\r' | b'\n'))
        .then(|| Vec::with_capacity(input.len()));
    let mut standard_specific = false;
    let mut url_specific = false;
    for byte in input.iter().copied() {
        if matches!(byte, b'\r' | b'\n') {
            continue;
        }
        match byte {
            b'+' | b'/' => standard_specific = true,
            b'-' | b'_' => url_specific = true,
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'=' => {}
            _ => return Err(Base64ContainerError::Invalid),
        }
        if standard_specific && url_specific {
            return Err(Base64ContainerError::Invalid);
        }
        if let Some(compact) = &mut compact {
            compact.push(byte);
        }
    }
    let compact = compact.map_or_else(|| Cow::Borrowed(input), Cow::Owned);
    if compact.is_empty() {
        return Err(Base64ContainerError::Invalid);
    }
    Ok((
        compact,
        if url_specific {
            Base64Variant::UrlSafe
        } else {
            Base64Variant::Standard
        },
    ))
}

fn decode_compact(
    compact: &[u8],
    variant: Base64Variant,
    limit: usize,
) -> Result<Vec<u8>, Base64ContainerError> {
    if compact.len() % 4 == 1 {
        return Err(Base64ContainerError::Invalid);
    }
    let padding = compact
        .iter()
        .rev()
        .take_while(|byte| **byte == b'=')
        .count();
    if padding > 2 || (padding > 0 && compact.len() % 4 != 0) {
        return Err(Base64ContainerError::Invalid);
    }
    let remainder = compact.len() % 4;
    let estimate = compact
        .len()
        .checked_div(4)
        .and_then(|groups| groups.checked_mul(3))
        .and_then(|value| {
            value.checked_add(match remainder {
                2 => 1,
                3 => 2,
                _ => 0,
            })
        })
        .and_then(|value| value.checked_sub(padding))
        .ok_or(Base64ContainerError::Invalid)?;
    if estimate > limit {
        return Err(Base64ContainerError::OutputTooLarge { limit });
    }
    let mut output = vec![0u8; estimate];
    let written = match variant {
        Base64Variant::Standard => {
            if padding == 0 {
                general_purpose::STANDARD_NO_PAD
                    .decode_slice(compact, &mut output)
                    .map_err(|_| Base64ContainerError::Invalid)?
            } else {
                general_purpose::STANDARD
                    .decode_slice(compact, &mut output)
                    .map_err(|_| Base64ContainerError::Invalid)?
            }
        }
        Base64Variant::UrlSafe => {
            if padding == 0 {
                general_purpose::URL_SAFE_NO_PAD
                    .decode_slice(compact, &mut output)
                    .map_err(|_| Base64ContainerError::Invalid)?
            } else {
                general_purpose::URL_SAFE
                    .decode_slice(compact, &mut output)
                    .map_err(|_| Base64ContainerError::Invalid)?
            }
        }
    };
    output.truncate(written);
    Ok(output)
}
