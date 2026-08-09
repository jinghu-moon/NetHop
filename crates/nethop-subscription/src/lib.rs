#![doc = "NetHop's bounded, nodes-only subscription parser foundation."]

pub mod adapter;
pub mod base64_container;
pub mod capability;
#[cfg(feature = "format-clash-yaml")]
pub mod clash_yaml;
pub mod core_adapter;
pub mod detect;
pub mod diagnostics;
#[cfg(feature = "fetch")]
pub mod fetch;
pub mod ipc;
pub mod limits;
pub mod normalize;
pub mod payload;
pub mod pipeline;
pub mod protocol;
pub mod secret;
pub mod semantic;
#[cfg(feature = "format-singbox-json")]
pub mod singbox_json;
#[cfg(feature = "source-url")]
pub mod source_url;
#[cfg(feature = "format-surfboard")]
pub mod surfboard;
pub mod uri;

pub use adapter::{AdapterNodeResult, AdapterOutput};
pub use base64_container::{
    Base64ContainerError, Base64Variant, DecodedSubscription, decode_base64,
    decode_base64_and_detect, decode_base64_at_depth,
};
pub use capability::{
    CapabilityEntry, CapabilityEvidence, CapabilityMatrix, CapabilityQuery, PINNED_SING_BOX_VERSION,
};
#[cfg(feature = "format-clash-yaml")]
pub use clash_yaml::{ClashYamlError, parse_clash_yaml, yaml_options};
pub use core_adapter::{
    TerminalOutboundAdapterError, adapt_terminal_outbound, adapt_terminal_outbounds,
};
pub use detect::{
    Base64Alphabet, Base64Details, Base64Padding, DetectionError, DetectionResult,
    EvidenceStrength, FormatEvidence, detect_bytes, detect_format, detect_normalized,
};
pub use diagnostics::{
    DiagnosticCode, DiagnosticParameter, NodeDiagnostic, Severity, SourceLocation,
};
#[cfg(feature = "fetch")]
pub use fetch::{
    CandidateAcceptance, ContentEncoding, FetchAgentConfig, FetchClient, FetchDiagnosticCode,
    FetchEndpoint, FetchEndpointKind, FetchError, FetchOutcome, FetchPolicy, FetchPolicyError,
    FetchRequest, FetchTimeouts, SourceCache, SubscriptionUserInfo, UREQ_SECURITY_ADAPTER_VERSION,
    decode_response_body, is_denied_ssrf_address, next_redirect, parse_subscription_userinfo,
    validate_peer_address, validate_peer_in_approved_set, validate_resolved_addresses,
    validate_response_limits,
};
pub use ipc::{
    ACTIVE_OUTBOUND_BASELINE, CONVERSION_NODE_LIMIT, CandidateStatus, IpcPayloadOrigin,
    MANAGED_ACTIVE_OUTBOUND_LIMIT, MAX_PARSER_IPC_FRAME_BYTES, PARSER_IPC_SCHEMA_VERSION,
    ParserIpcRequest, ParserIpcRequestError, ParserIpcResponse, ParserIpcResponseError,
    RequestProfile,
};
pub use limits::ParserLimits;
pub use normalize::{
    NormalizationError, NormalizedLine, NormalizedLines, NormalizedPayload, normalize_bytes,
};
pub use payload::{
    Digest, FetchMetadata, FormatHint, HttpScheme, ImportPayload, PayloadOrigin, PayloadOriginKind,
    ReceivedAt, SourceId, SourceMetadata,
};
pub use pipeline::{
    CURRENT_FINGERPRINT_SCHEMA, CURRENT_REPORT_SCHEMA_VERSION, CompactItemReport, CompactStatus,
    ConversionReport, ConversionSummary, DedupedNode, FilteredSourceInput, NodeDisplayId,
    NodeFilter, NodeFilterError, NodeFingerprint, ReportCompatibility, ReportReadError,
    SourceBatch, SourceInput, SourceOutcome, StableConversion, VersionedReport,
    canonical_node_bytes, compose_outbound, compose_outbounds_json, convert_filtered_sources,
    convert_stable_sources, dedupe_sources, fingerprint_node, read_versioned_report,
    report_from_adapter, write_versioned_report,
};
pub use protocol::{
    AnyTlsOptions, BoundedText, Capabilities, Credentials, DisplayName, Endpoint, HttpOptions,
    Hysteria2Obfs, Hysteria2Options, PluginSpec, ProtocolOptions, ProxyNode, ProxyProtocol,
    RealityOptions, SocksOptions, SourceRef, TlsOptions, TransportKind, TransportOptions,
    TuicOptions, UdpOverTcpOptions, UnvalidatedNode, UuidValue,
};
pub use secret::SecretString;
pub use semantic::{
    NodeSpec, SemanticError, SemanticOutcome, node_spec_from_uri, semantic_diagnostic,
    validate_node_spec,
};
#[cfg(feature = "format-singbox-json")]
pub use singbox_json::{SingboxJsonError, parse_singbox_json};
#[cfg(feature = "source-url")]
pub use source_url::{SourceUrlError, validate_source_url};
#[cfg(feature = "format-surfboard")]
pub use surfboard::{SurfboardIniError, parse_surfboard_ini};
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
