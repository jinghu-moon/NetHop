#![doc = "NetHop's bounded, nodes-only subscription parser foundation."]

pub mod capability;
pub mod diagnostics;
pub mod limits;
pub mod payload;
pub mod protocol;
pub mod secret;

pub use capability::{CapabilityEntry, CapabilityEvidence, CapabilityMatrix, CapabilityQuery};
pub use diagnostics::{
    DiagnosticCode, DiagnosticParameter, NodeDiagnostic, Severity, SourceLocation,
};
pub use limits::ParserLimits;
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
