use thiserror::Error;

use crate::{diagnostics::DiagnosticCode, limits::ParserLimits};

const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NormalizationError {
    #[error("payload exceeds the configured body limit")]
    InputTooLarge { actual: usize, limit: usize },
    #[error("payload is not valid UTF-8")]
    InvalidUtf8,
    #[error("payload contains a NUL byte")]
    NulByte,
}

impl NormalizationError {
    pub const fn code(&self) -> DiagnosticCode {
        match self {
            Self::InputTooLarge { .. } => DiagnosticCode::InputTooLarge,
            Self::InvalidUtf8 => DiagnosticCode::InvalidUtf8,
            Self::NulByte => DiagnosticCode::NulByte,
        }
    }
}

/// A validated, trimmed view over the original payload bytes.
///
/// Line endings are normalized by [`NormalizedPayload::lines`] without copying
/// the complete body. The underlying text and credentials are never rewritten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NormalizedPayload<'a> {
    text: &'a str,
}

impl<'a> NormalizedPayload<'a> {
    pub fn new(bytes: &'a [u8], limits: &ParserLimits) -> Result<Self, NormalizationError> {
        normalize_bytes(bytes, limits)
    }

    pub const fn as_str(&self) -> &'a str {
        self.text
    }

    pub const fn as_bytes(&self) -> &'a [u8] {
        self.text.as_bytes()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn lines(&self) -> NormalizedLines<'a> {
        NormalizedLines {
            remaining: self.text,
            next_number: 1,
        }
    }
}

pub fn normalize_bytes<'a>(
    bytes: &'a [u8],
    limits: &ParserLimits,
) -> Result<NormalizedPayload<'a>, NormalizationError> {
    if bytes.len() > limits.max_body_bytes() {
        return Err(NormalizationError::InputTooLarge {
            actual: bytes.len(),
            limit: limits.max_body_bytes(),
        });
    }
    let bytes = bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes);
    if bytes.contains(&0) {
        return Err(NormalizationError::NulByte);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| NormalizationError::InvalidUtf8)?;
    Ok(NormalizedPayload { text: text.trim() })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NormalizedLine<'a> {
    number: u32,
    text: &'a str,
}

impl<'a> NormalizedLine<'a> {
    pub const fn number(&self) -> u32 {
        self.number
    }

    pub const fn text(&self) -> &'a str {
        self.text
    }
}

#[derive(Debug, Clone)]
pub struct NormalizedLines<'a> {
    remaining: &'a str,
    next_number: u32,
}

impl<'a> Iterator for NormalizedLines<'a> {
    type Item = NormalizedLine<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining.is_empty() {
            return None;
        }
        let bytes = self.remaining.as_bytes();
        let separator = bytes.iter().position(|byte| matches!(byte, b'\r' | b'\n'));
        let (line, consumed) = match separator {
            Some(index) => {
                let width =
                    usize::from(bytes[index] == b'\r' && bytes.get(index + 1) == Some(&b'\n')) + 1;
                (&self.remaining[..index], index + width)
            }
            None => (self.remaining, self.remaining.len()),
        };
        self.remaining = &self.remaining[consumed..];
        let number = self.next_number;
        self.next_number = self.next_number.saturating_add(1);
        Some(NormalizedLine { number, text: line })
    }
}
