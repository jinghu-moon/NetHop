#![doc = "NetHop's bounded, nodes-only subscription parser foundation."]

pub mod base64_container;
pub mod capability;
pub mod detect;
pub mod diagnostics;
pub mod limits;
pub mod normalize;
pub mod payload;
pub mod protocol;
pub mod secret;
pub mod semantic;
pub mod uri;

pub use base64_container::{
    Base64ContainerError, Base64Variant, DecodedSubscription, decode_base64,
    decode_base64_and_detect, decode_base64_at_depth,
};
pub use capability::{CapabilityEntry, CapabilityEvidence, CapabilityMatrix, CapabilityQuery};
pub use detect::{
    Base64Alphabet, Base64Details, Base64Padding, DetectionError, DetectionResult,
    EvidenceStrength, FormatEvidence, detect_bytes, detect_format, detect_normalized,
};
pub use diagnostics::{
    DiagnosticCode, DiagnosticParameter, NodeDiagnostic, Severity, SourceLocation,
};
pub use limits::ParserLimits;
pub use normalize::{
    NormalizationError, NormalizedLine, NormalizedLines, NormalizedPayload, normalize_bytes,
};
pub use payload::{
    Digest, FetchMetadata, FormatHint, HttpScheme, ImportPayload, PayloadOrigin, PayloadOriginKind,
    ReceivedAt, SourceId, SourceMetadata,
};
pub use protocol::{
    BoundedText, Capabilities, Credentials, DisplayName, Endpoint, PluginSpec, ProtocolOptions,
    ProxyNode, ProxyProtocol, RealityOptions, SourceRef, TlsOptions, TransportKind,
    TransportOptions, UnvalidatedNode, UuidValue,
};
pub use secret::SecretString;
pub use semantic::{
    NodeSpec, SemanticError, SemanticOutcome, node_spec_from_uri, semantic_diagnostic,
    validate_node_spec,
};
pub use uri::{
    UriContainerError, UriNodeCandidate, UriNodeResult, UriQueryParameter, UriScheme,
    decode_vmess_inner_json, parse_uri_line, parse_uri_list, percent_decode_field,
};

/// The package identity used by workspace and integration smoke contracts.
pub const CRATE_NAME: &str = "nethop-subscription";

/// The parser foundation is intentionally empty until the public model phase.
pub const FOUNDATION_VERSION: &str = "workspace-foundation-v1";

#[cfg(test)]
mod tests {
    use super::{CRATE_NAME, FOUNDATION_VERSION};

    #[test]
    fn foundation_constants_are_stable() {
        assert_eq!(CRATE_NAME, "nethop-subscription");
        assert_eq!(FOUNDATION_VERSION, "workspace-foundation-v1");
    }
}
