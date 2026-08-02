use serde::Serialize;
use thiserror::Error;

pub const MAX_BODY_BYTES: usize = 5 * 1024 * 1024;
pub const MAX_NODE_COUNT: usize = 10_000;
pub const MAX_LINE_BYTES: usize = 16 * 1024;
pub const MAX_DEPTH: usize = 64;
pub const MAX_STRING_BYTES: usize = 64 * 1024;
pub const MAX_QUERY_PARAMS: usize = 64;
pub const MAX_FRAGMENT_BYTES: usize = 256;
pub const MAX_VMESS_JSON_BYTES: usize = 64 * 1024;
pub const MAX_SOURCE_REFS: usize = 64;
pub const MAX_DETAILED_DIAGNOSTICS: usize = 1_000;
pub const MAX_WARNINGS_PER_NODE: usize = 16;
pub const MAX_REPORT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_FIELD_PATH_BYTES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ParserLimits {
    max_body_bytes: usize,
    max_nodes: usize,
    max_line_bytes: usize,
    max_depth: usize,
    max_string_bytes: usize,
    max_query_params: usize,
    max_fragment_bytes: usize,
    max_vmess_json_bytes: usize,
    max_source_refs: usize,
    max_detailed_diagnostics: usize,
    max_warnings_per_node: usize,
    max_report_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum LimitError {
    #[error("{field} must be greater than zero")]
    Zero { field: &'static str },
    #[error("{field} exceeds the security ceiling")]
    ExceedsCeiling { field: &'static str },
}

impl ParserLimits {
    pub fn new(
        max_body_bytes: usize,
        max_nodes: usize,
        max_line_bytes: usize,
        max_depth: usize,
        max_string_bytes: usize,
    ) -> Result<Self, LimitError> {
        let limits = Self {
            max_body_bytes,
            max_nodes,
            max_line_bytes,
            max_depth,
            max_string_bytes,
            max_query_params: MAX_QUERY_PARAMS,
            max_fragment_bytes: MAX_FRAGMENT_BYTES,
            max_vmess_json_bytes: MAX_VMESS_JSON_BYTES,
            max_source_refs: MAX_SOURCE_REFS,
            max_detailed_diagnostics: MAX_DETAILED_DIAGNOSTICS,
            max_warnings_per_node: MAX_WARNINGS_PER_NODE,
            max_report_bytes: MAX_REPORT_BYTES,
        };
        limits.validate()?;
        Ok(limits)
    }

    pub fn validate(&self) -> Result<(), LimitError> {
        let values = [
            ("max_body_bytes", self.max_body_bytes, MAX_BODY_BYTES),
            ("max_nodes", self.max_nodes, MAX_NODE_COUNT),
            ("max_line_bytes", self.max_line_bytes, MAX_LINE_BYTES),
            ("max_depth", self.max_depth, MAX_DEPTH),
            ("max_string_bytes", self.max_string_bytes, MAX_STRING_BYTES),
            ("max_query_params", self.max_query_params, MAX_QUERY_PARAMS),
            (
                "max_fragment_bytes",
                self.max_fragment_bytes,
                MAX_FRAGMENT_BYTES,
            ),
            (
                "max_vmess_json_bytes",
                self.max_vmess_json_bytes,
                MAX_VMESS_JSON_BYTES,
            ),
            ("max_source_refs", self.max_source_refs, MAX_SOURCE_REFS),
            (
                "max_detailed_diagnostics",
                self.max_detailed_diagnostics,
                MAX_DETAILED_DIAGNOSTICS,
            ),
            (
                "max_warnings_per_node",
                self.max_warnings_per_node,
                MAX_WARNINGS_PER_NODE,
            ),
            ("max_report_bytes", self.max_report_bytes, MAX_REPORT_BYTES),
        ];
        for (field, value, ceiling) in values {
            if value == 0 {
                return Err(LimitError::Zero { field });
            }
            if value > ceiling {
                return Err(LimitError::ExceedsCeiling { field });
            }
        }
        Ok(())
    }

    pub const fn max_body_bytes(&self) -> usize {
        self.max_body_bytes
    }
    pub const fn max_nodes(&self) -> usize {
        self.max_nodes
    }
    pub const fn max_line_bytes(&self) -> usize {
        self.max_line_bytes
    }
    pub const fn max_depth(&self) -> usize {
        self.max_depth
    }
    pub const fn max_string_bytes(&self) -> usize {
        self.max_string_bytes
    }
    pub const fn max_query_params(&self) -> usize {
        self.max_query_params
    }
    pub const fn max_fragment_bytes(&self) -> usize {
        self.max_fragment_bytes
    }
    pub const fn max_vmess_json_bytes(&self) -> usize {
        self.max_vmess_json_bytes
    }
    pub const fn max_source_refs(&self) -> usize {
        self.max_source_refs
    }
    pub const fn max_detailed_diagnostics(&self) -> usize {
        self.max_detailed_diagnostics
    }
    pub const fn max_warnings_per_node(&self) -> usize {
        self.max_warnings_per_node
    }
    pub const fn max_report_bytes(&self) -> usize {
        self.max_report_bytes
    }
}

impl Default for ParserLimits {
    fn default() -> Self {
        Self::new(
            MAX_BODY_BYTES,
            MAX_NODE_COUNT,
            MAX_LINE_BYTES,
            MAX_DEPTH,
            MAX_STRING_BYTES,
        )
        .expect("frozen ParserLimits defaults must be valid")
    }
}
