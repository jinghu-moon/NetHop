use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    time::Instant,
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as ShaDigest, Sha256};
use thiserror::Error;

use crate::{
    adapter::{AdapterNodeResult, AdapterOutput},
    capability::CapabilityMatrix,
    diagnostics::{DiagnosticCode, NodeDiagnostic, Severity, SourceLocation},
    limits::ParserLimits,
    payload::{FormatHint, SourceId},
    protocol::{
        Credentials, ProtocolOptions, ProxyNode, ProxyProtocol, SourceRef, TransportOptions,
    },
    semantic::{node_spec_from_uri, semantic_diagnostic, validate_node_spec},
};

const FINGERPRINT_DOMAIN: &[u8] = b"nethop-node-v1\0";
pub const CURRENT_REPORT_SCHEMA_VERSION: u32 = 1;
pub const CURRENT_FINGERPRINT_SCHEMA: &str = "nh-fp-sha256-v1";

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NodeFingerprint([u8; 32]);

impl NodeFingerprint {
    pub const fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
    pub fn hex(&self) -> String {
        hex(&self.0)
    }
}

impl fmt::Debug for NodeFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("NodeFingerprint")
            .field(&self.display_id().as_str())
            .finish()
    }
}

impl NodeFingerprint {
    pub fn display_id(&self) -> NodeDisplayId {
        NodeDisplayId(format!("nh1s-{}", hex(&self.0[..8])))
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NodeDisplayId(String);

impl NodeDisplayId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for NodeDisplayId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl fmt::Display for NodeDisplayId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

pub fn canonical_node_bytes(node: &ProxyNode) -> Vec<u8> {
    let mut out = Vec::with_capacity(256);
    field(&mut out, "schema", "1");
    field(&mut out, "protocol", node.protocol().as_str());
    field(&mut out, "server", node.endpoint().server());
    field(&mut out, "port", &node.endpoint().port().to_string());
    encode_credentials(&mut out, node.credentials());
    encode_tls(&mut out, node.tls());
    encode_transport(&mut out, node.transport());
    encode_protocol_options(&mut out, node.protocol_options());
    field(
        &mut out,
        "udp",
        if node.capabilities().udp { "1" } else { "0" },
    );
    out
}

pub fn fingerprint_node(node: &ProxyNode) -> NodeFingerprint {
    let mut hasher = Sha256::new();
    hasher.update(FINGERPRINT_DOMAIN);
    hasher.update(canonical_node_bytes(node));
    NodeFingerprint(hasher.finalize().into())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DedupedNode {
    pub fingerprint: NodeFingerprint,
    pub node_id: NodeDisplayId,
    pub node: ProxyNode,
    pub source_refs: Vec<SourceRef>,
    pub aliases: Vec<String>,
    pub display_territory_code: Option<crate::DisplayTerritoryCode>,
    first_source_order: usize,
    first_item_index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceOutcome {
    pub accepted: usize,
    pub duplicate: usize,
    pub rejected: usize,
    pub warnings: usize,
}

impl SourceOutcome {
    pub const fn success(&self) -> bool {
        self.accepted + self.duplicate > 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceBatch {
    pub source_id: SourceId,
    pub nodes: Vec<ProxyNode>,
    pub rejected: usize,
    pub warnings: usize,
}

pub fn dedupe_sources(
    batches: Vec<SourceBatch>,
    limits: &ParserLimits,
) -> (Vec<DedupedNode>, BTreeMap<SourceId, SourceOutcome>) {
    let mut by_fp: HashMap<NodeFingerprint, usize> = HashMap::new();
    let mut nodes = Vec::<DedupedNode>::new();
    let mut outcomes = BTreeMap::<SourceId, SourceOutcome>::new();
    for (source_order, batch) in batches.into_iter().enumerate() {
        let mut outcome = SourceOutcome {
            accepted: 0,
            duplicate: 0,
            rejected: batch.rejected,
            warnings: batch.warnings,
        };
        for node in batch.nodes {
            let fp = fingerprint_node(&node);
            let display = node.display_name().as_str().to_owned();
            if let Some(index) = by_fp.get(&fp).copied() {
                outcome.duplicate += 1;
                let existing = &mut nodes[index];
                merge_refs(
                    &mut existing.source_refs,
                    node.source_refs(),
                    limits.max_source_refs(),
                );
                if !existing.aliases.contains(&display)
                    && existing.aliases.len() < limits.max_source_refs()
                {
                    existing.aliases.push(display);
                }
            } else {
                by_fp.insert(fp, nodes.len());
                let mut source_refs = Vec::new();
                merge_refs(
                    &mut source_refs,
                    node.source_refs(),
                    limits.max_source_refs(),
                );
                let first_item_index = source_refs
                    .first()
                    .map_or(0, |reference| reference.item_index);
                nodes.push(DedupedNode {
                    fingerprint: fp,
                    node_id: fp.display_id(),
                    node,
                    source_refs,
                    aliases: vec![display],
                    display_territory_code: None,
                    first_source_order: source_order,
                    first_item_index,
                });
                outcome.accepted += 1;
            }
        }
        outcomes.insert(batch.source_id, outcome);
    }
    for node in &mut nodes {
        node.display_territory_code =
            crate::infer_display_territory(node.aliases.iter().map(String::as_str));
    }
    nodes.sort_by(|a, b| {
        (a.first_source_order, a.first_item_index, a.node_id.as_str()).cmp(&(
            b.first_source_order,
            b.first_item_index,
            b.node_id.as_str(),
        ))
    });
    (nodes, outcomes)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompactStatus {
    Accepted,
    Rejected,
    Duplicate,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactItemReport {
    pub index: u32,
    pub status: CompactStatus,
    pub protocol: Option<ProxyProtocol>,
    pub node_id: Option<String>,
    pub codes: Vec<DiagnosticCode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversionSummary {
    pub detected_format: FormatHint,
    pub accepted: usize,
    pub rejected: usize,
    pub duplicate: usize,
    pub warnings: usize,
    pub source_success: bool,
    pub truncated: bool,
}

impl Default for ConversionSummary {
    fn default() -> Self {
        Self {
            detected_format: FormatHint::Auto,
            accepted: 0,
            rejected: 0,
            duplicate: 0,
            warnings: 0,
            source_success: false,
            truncated: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversionReport {
    pub summary: ConversionSummary,
    pub items: Vec<CompactItemReport>,
    pub diagnostics: Vec<NodeDiagnostic>,
    pub diagnostic_counts: BTreeMap<DiagnosticCode, usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportCompatibility {
    Current,
    LegacyRebuildRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ReportReadError {
    #[error("report JSON is invalid")]
    InvalidJson,
    #[error("report schema version is unsupported")]
    UnsupportedSchema,
    #[error("report fingerprint schema does not match the current algorithm")]
    FingerprintSchemaMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionedReport {
    pub schema_version: u32,
    pub fingerprint_schema: String,
    pub compatibility: ReportCompatibilityWire,
    pub report: ConversionReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportCompatibilityWire {
    Current,
    LegacyRebuildRequired,
}

impl VersionedReport {
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn fingerprint_schema(&self) -> &str {
        &self.fingerprint_schema
    }

    pub const fn compatibility(&self) -> ReportCompatibility {
        match self.compatibility {
            ReportCompatibilityWire::Current => ReportCompatibility::Current,
            ReportCompatibilityWire::LegacyRebuildRequired => {
                ReportCompatibility::LegacyRebuildRequired
            }
        }
    }
}

pub fn write_versioned_report(report: &ConversionReport) -> Result<Vec<u8>, ReportReadError> {
    serde_json::to_vec(&VersionedReport {
        schema_version: CURRENT_REPORT_SCHEMA_VERSION,
        fingerprint_schema: CURRENT_FINGERPRINT_SCHEMA.to_owned(),
        compatibility: ReportCompatibilityWire::Current,
        report: report.clone(),
    })
    .map_err(|_| ReportReadError::InvalidJson)
}

pub fn read_versioned_report(input: &[u8]) -> Result<VersionedReport, ReportReadError> {
    let value: serde_json::Value =
        serde_json::from_slice(input).map_err(|_| ReportReadError::InvalidJson)?;
    if value.get("schema_version").is_none() {
        let report = serde_json::from_value(value).map_err(|_| ReportReadError::InvalidJson)?;
        return Ok(VersionedReport {
            schema_version: 0,
            fingerprint_schema: "legacy-unknown".to_owned(),
            compatibility: ReportCompatibilityWire::LegacyRebuildRequired,
            report,
        });
    }
    let report: VersionedReport =
        serde_json::from_value(value).map_err(|_| ReportReadError::InvalidJson)?;
    if report.schema_version != CURRENT_REPORT_SCHEMA_VERSION {
        return Err(ReportReadError::UnsupportedSchema);
    }
    if report.fingerprint_schema != CURRENT_FINGERPRINT_SCHEMA {
        return Err(ReportReadError::FingerprintSchemaMismatch);
    }
    Ok(report)
}

impl ConversionReport {
    pub fn bounded_json(&self, limits: &ParserLimits) -> String {
        let mut copy = self.clone();
        loop {
            let json = serde_json::to_string(&copy).expect("report must serialize");
            if json.len() <= limits.max_report_bytes() || copy.diagnostics.is_empty() {
                return json;
            }
            copy.summary.truncated = true;
            copy.diagnostics.truncate(copy.diagnostics.len() / 2);
        }
    }
}

pub fn report_from_adapter(
    format: FormatHint,
    output: &AdapterOutput,
    deduped: &[DedupedNode],
    duplicate_count: usize,
    limits: &ParserLimits,
) -> ConversionReport {
    let accepted = deduped.len();
    let rejected = output.rejected_count();
    let warnings = output.diagnostics.len()
        + output
            .nodes
            .iter()
            .map(|node| node.warnings.len())
            .sum::<usize>();
    let mut report = ConversionReport {
        summary: ConversionSummary {
            detected_format: format,
            accepted,
            rejected,
            duplicate: duplicate_count,
            warnings,
            source_success: accepted + duplicate_count > 0,
            truncated: false,
        },
        items: Vec::new(),
        diagnostics: Vec::new(),
        diagnostic_counts: BTreeMap::new(),
    };
    for result in &output.nodes {
        if let Some(node) = &result.node {
            let fp = fingerprint_node(node);
            report.items.push(CompactItemReport {
                index: result.item_index,
                status: CompactStatus::Accepted,
                protocol: Some(node.protocol()),
                node_id: Some(fp.display_id().to_string()),
                codes: result
                    .warnings
                    .iter()
                    .map(|warning| warning.code.clone())
                    .collect(),
            });
        } else if let Some(diagnostic) = &result.diagnostic {
            push_diagnostic(&mut report, diagnostic.clone(), limits);
            report.items.push(CompactItemReport {
                index: result.item_index,
                status: CompactStatus::Rejected,
                protocol: diagnostic.protocol,
                node_id: None,
                codes: vec![diagnostic.code.clone()],
            });
        }
        for warning in result.warnings.iter().take(limits.max_warnings_per_node()) {
            push_diagnostic(&mut report, warning.clone(), limits);
        }
    }
    for diagnostic in &output.diagnostics {
        push_diagnostic(&mut report, diagnostic.clone(), limits);
    }
    report
}

pub fn compose_outbound(node: &DedupedNode) -> Value {
    let mut object = BTreeMap::<String, Value>::new();
    object.insert("type".into(), json!(node.node.protocol().as_str()));
    object.insert("tag".into(), json!(node.node_id.as_str()));
    object.insert("server".into(), json!(node.node.endpoint().server()));
    let has_hysteria2_port_hopping = matches!(
        node.node.protocol_options(),
        ProtocolOptions::Hysteria2(options) if !options.server_ports.is_empty()
    );
    if !has_hysteria2_port_hopping {
        object.insert("server_port".into(), json!(node.node.endpoint().port()));
    }
    match node.node.credentials() {
        Credentials::Vless { uuid } => {
            object.insert("uuid".into(), json!(uuid.expose()));
        }
        Credentials::Vmess {
            uuid,
            alter_id,
            security,
        } => {
            object.insert("uuid".into(), json!(uuid.expose()));
            object.insert("alter_id".into(), json!(alter_id));
            object.insert("security".into(), json!(security.as_str()));
        }
        Credentials::Shadowsocks {
            method,
            password,
            plugin,
        } => {
            object.insert("method".into(), json!(method.as_str()));
            object.insert("password".into(), json!(password.expose()));
            if let Some(plugin) = plugin {
                object.insert("plugin".into(), json!(plugin.name.as_str()));
                object.insert("plugin_opts".into(), json!(plugin_options(plugin)));
            }
        }
        Credentials::Trojan { password } | Credentials::AnyTls { password } => {
            object.insert("password".into(), json!(password.expose()));
        }
        Credentials::Hysteria2 { password, obfs } => {
            object.insert("password".into(), json!(password.expose()));
            if let Some(obfs) = obfs {
                object.insert(
                    "obfs".into(),
                    json!({"type": obfs.kind.as_str(), "password": obfs.password.expose()}),
                );
            }
        }
        Credentials::Tuic { uuid, password } => {
            object.insert("uuid".into(), json!(uuid.expose()));
            object.insert("password".into(), json!(password.expose()));
        }
        Credentials::Http { username, password } | Credentials::Socks { username, password } => {
            if let Some(username) = username {
                object.insert("username".into(), json!(username.expose()));
            }
            if let Some(password) = password {
                object.insert("password".into(), json!(password.expose()));
            }
        }
    }
    if node.node.tls().enabled {
        object.insert("tls".into(), tls_json(node.node.tls()));
    }
    if !matches!(node.node.transport(), TransportOptions::Tcp) {
        object.insert("transport".into(), transport_json(node.node.transport()));
    }
    if let ProtocolOptions::Vless { flow: Some(flow) } = node.node.protocol_options() {
        object.insert("flow".into(), json!(flow.as_str()));
    }
    if let ProtocolOptions::Shadowsocks {
        udp_over_tcp: Some(options),
    } = node.node.protocol_options()
    {
        object.insert("udp_over_tcp".into(), udp_over_tcp_json(*options));
    }
    if let ProtocolOptions::Tuic(options) = node.node.protocol_options() {
        if let Some(value) = &options.congestion_control {
            object.insert("congestion_control".into(), json!(value.as_str()));
        }
        if let Some(value) = &options.udp_relay_mode {
            object.insert("udp_relay_mode".into(), json!(value.as_str()));
        }
        if options.udp_over_stream {
            object.insert("udp_over_stream".into(), json!(true));
        }
        if options.zero_rtt_handshake {
            object.insert("zero_rtt_handshake".into(), json!(true));
        }
        if let Some(value) = &options.heartbeat {
            object.insert("heartbeat".into(), json!(value.as_str()));
        }
    }
    if let ProtocolOptions::Hysteria2(options) = node.node.protocol_options() {
        if !options.server_ports.is_empty() {
            object.insert(
                "server_ports".into(),
                json!(
                    options
                        .server_ports
                        .iter()
                        .map(|value| value.as_str())
                        .collect::<Vec<_>>()
                ),
            );
        }
        if let Some(value) = &options.hop_interval {
            object.insert("hop_interval".into(), json!(value.as_str()));
        }
        if let Some(value) = options.up_mbps {
            object.insert("up_mbps".into(), json!(value));
        }
        if let Some(value) = options.down_mbps {
            object.insert("down_mbps".into(), json!(value));
        }
    }
    if let ProtocolOptions::AnyTls(options) = node.node.protocol_options() {
        if let Some(value) = &options.idle_session_check_interval {
            object.insert("idle_session_check_interval".into(), json!(value.as_str()));
        }
        if let Some(value) = &options.idle_session_timeout {
            object.insert("idle_session_timeout".into(), json!(value.as_str()));
        }
        if let Some(value) = options.min_idle_session {
            object.insert("min_idle_session".into(), json!(value));
        }
    }
    if let ProtocolOptions::Http(options) = node.node.protocol_options() {
        if let Some(path) = &options.path {
            object.insert("path".into(), json!(path.as_str()));
        }
        if !options.headers.is_empty() {
            object.insert(
                "headers".into(),
                json!(
                    options
                        .headers
                        .iter()
                        .map(|(key, value)| (key, value.expose()))
                        .collect::<BTreeMap<_, _>>()
                ),
            );
        }
    }
    if let ProtocolOptions::Socks(options) = node.node.protocol_options() {
        object.insert("version".into(), json!(options.version.as_str()));
        object.insert(
            "network".into(),
            if node.node.capabilities().udp {
                json!(["tcp", "udp"])
            } else {
                json!(["tcp"])
            },
        );
        if let Some(options) = options.udp_over_tcp {
            object.insert("udp_over_tcp".into(), udp_over_tcp_json(options));
        }
    }
    serde_json::to_value(object).expect("outbound object must serialize")
}

pub fn compose_outbounds_json(nodes: &[DedupedNode]) -> String {
    serde_json::to_string(&nodes.iter().map(compose_outbound).collect::<Vec<_>>())
        .expect("outbounds must serialize")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceInput {
    pub source_id: SourceId,
    pub format_hint: FormatHint,
    pub bytes: Vec<u8>,
}

const MAX_FILTER_RULES: usize = 32;
const MAX_FILTER_PATTERN_BYTES: usize = 128;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NodeFilter {
    include_names: Vec<String>,
    exclude_names: Vec<String>,
    excluded_node_ids: Vec<String>,
    protocols: Vec<ProxyProtocol>,
}

impl NodeFilter {
    pub fn new(
        mut include_names: Vec<String>,
        mut exclude_names: Vec<String>,
        mut protocols: Vec<ProxyProtocol>,
    ) -> Result<Self, NodeFilterError> {
        if include_names.len() + exclude_names.len() > MAX_FILTER_RULES
            || include_names.iter().chain(&exclude_names).any(|pattern| {
                pattern.is_empty()
                    || pattern.len() > MAX_FILTER_PATTERN_BYTES
                    || pattern.chars().any(char::is_control)
            })
            || protocols.len() > ProxyProtocol::ALL.len()
        {
            return Err(NodeFilterError::InvalidRule);
        }
        include_names.sort();
        exclude_names.sort();
        protocols.sort();
        if include_names.windows(2).any(|pair| pair[0] == pair[1])
            || exclude_names.windows(2).any(|pair| pair[0] == pair[1])
            || protocols.windows(2).any(|pair| pair[0] == pair[1])
        {
            return Err(NodeFilterError::DuplicateRule);
        }
        Ok(Self {
            include_names,
            exclude_names,
            excluded_node_ids: Vec::new(),
            protocols,
        })
    }

    pub fn new_with_node_ids(
        include_names: Vec<String>,
        exclude_names: Vec<String>,
        mut excluded_node_ids: Vec<String>,
        protocols: Vec<ProxyProtocol>,
    ) -> Result<Self, NodeFilterError> {
        let mut filter = Self::new(include_names, exclude_names, protocols)?;
        if excluded_node_ids.len() > MAX_FILTER_RULES
            || excluded_node_ids.iter().any(|id| {
                id.len() != 21
                    || !id.starts_with("nh1s-")
                    || !id[5..]
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
        {
            return Err(NodeFilterError::InvalidRule);
        }
        excluded_node_ids.sort();
        if excluded_node_ids.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(NodeFilterError::DuplicateRule);
        }
        filter.excluded_node_ids = excluded_node_ids;
        Ok(filter)
    }

    pub fn include_names(&self) -> &[String] {
        &self.include_names
    }

    pub fn exclude_names(&self) -> &[String] {
        &self.exclude_names
    }

    pub fn protocols(&self) -> &[ProxyProtocol] {
        &self.protocols
    }

    pub fn excluded_node_ids(&self) -> &[String] {
        &self.excluded_node_ids
    }

    fn accepts(&self, node: &ProxyNode) -> bool {
        let name = node.display_name().as_str();
        let node_id = fingerprint_node(node).display_id().to_string();
        (self.protocols.is_empty() || self.protocols.binary_search(&node.protocol()).is_ok())
            && (self.include_names.is_empty()
                || self
                    .include_names
                    .iter()
                    .any(|pattern| contains_ascii_case_insensitive(name, pattern)))
            && !self
                .exclude_names
                .iter()
                .any(|pattern| contains_ascii_case_insensitive(name, pattern))
            && !self.excluded_node_ids.binary_search(&node_id).is_ok()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum NodeFilterError {
    #[error("node filter rule is invalid or exceeds its bound")]
    InvalidRule,
    #[error("node filter contains a duplicate rule")]
    DuplicateRule,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilteredSourceInput {
    pub source: SourceInput,
    pub filter: NodeFilter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StableConversion {
    pub nodes: Vec<DedupedNode>,
    pub outbounds_json: String,
    pub report: ConversionReport,
    pub source_outcomes: BTreeMap<SourceId, SourceOutcome>,
    pub elapsed_micros: u128,
}

pub fn convert_stable_sources(
    inputs: Vec<SourceInput>,
    limits: &ParserLimits,
    matrix: &CapabilityMatrix,
) -> StableConversion {
    convert_sources(
        inputs
            .into_iter()
            .map(|source| FilteredSourceInput {
                source,
                filter: NodeFilter::default(),
            })
            .collect(),
        limits,
        matrix,
    )
}

pub fn convert_filtered_sources(
    inputs: Vec<FilteredSourceInput>,
    limits: &ParserLimits,
    matrix: &CapabilityMatrix,
) -> StableConversion {
    convert_sources(inputs, limits, matrix)
}

fn convert_sources(
    inputs: Vec<FilteredSourceInput>,
    limits: &ParserLimits,
    matrix: &CapabilityMatrix,
) -> StableConversion {
    let started = Instant::now();
    let mut batches = Vec::new();
    let mut detected = FormatHint::Auto;
    let mut report = ConversionReport {
        summary: ConversionSummary::default(),
        items: Vec::new(),
        diagnostics: Vec::new(),
        diagnostic_counts: BTreeMap::new(),
    };
    for input in inputs {
        let mut output = parse_source(&input.source, limits, matrix);
        detected = input.source.format_hint;
        let parsed_accepted = output.accepted_count();
        let mut rejected = output.rejected_count();
        let source_warnings = output.diagnostics.len();
        let mut nodes = Vec::with_capacity(output.accepted_count());
        for mut item in output.nodes.drain(..) {
            if let Some(node) = item.node.take() {
                if !input.filter.accepts(&node) {
                    rejected += 1;
                    report.items.push(CompactItemReport {
                        index: item.item_index,
                        status: CompactStatus::Rejected,
                        protocol: Some(node.protocol()),
                        node_id: None,
                        codes: vec![DiagnosticCode::NodeFilteredOut],
                    });
                    continue;
                }
                let fingerprint = fingerprint_node(&node);
                let codes = item
                    .warnings
                    .iter()
                    .take(limits.max_warnings_per_node())
                    .map(|warning| warning.code.clone())
                    .collect();
                report.items.push(CompactItemReport {
                    index: item.item_index,
                    status: CompactStatus::Accepted,
                    protocol: Some(node.protocol()),
                    node_id: Some(fingerprint.display_id().to_string()),
                    codes,
                });
                for warning in item
                    .warnings
                    .into_iter()
                    .take(limits.max_warnings_per_node())
                {
                    report.summary.warnings += 1;
                    push_diagnostic(&mut report, warning, limits);
                }
                nodes.push(node);
            } else if let Some(diagnostic) = item.diagnostic.take() {
                report.items.push(CompactItemReport {
                    index: item.item_index,
                    status: CompactStatus::Rejected,
                    protocol: diagnostic.protocol,
                    node_id: None,
                    codes: vec![diagnostic.code.clone()],
                });
                push_diagnostic(&mut report, diagnostic, limits);
            }
        }
        for diagnostic in output.diagnostics.drain(..) {
            report.summary.warnings += 1;
            push_diagnostic(&mut report, diagnostic, limits);
        }
        if parsed_accepted > 0 && nodes.is_empty() {
            push_diagnostic(
                &mut report,
                NodeDiagnostic::new(DiagnosticCode::SourceFilteredEmpty, Severity::Error),
                limits,
            );
        }
        batches.push(SourceBatch {
            source_id: input.source.source_id,
            nodes,
            rejected,
            warnings: source_warnings,
        });
    }
    let (nodes, outcomes) = dedupe_sources(batches, limits);
    let duplicate_count = outcomes.values().map(|outcome| outcome.duplicate).sum();
    report.summary.detected_format = detected;
    report.summary.accepted = nodes.len();
    report.summary.rejected = outcomes.values().map(|outcome| outcome.rejected).sum();
    report.summary.duplicate = duplicate_count;
    report.summary.source_success = nodes.len() + duplicate_count > 0;
    let outbounds_json = compose_outbounds_json(&nodes);
    StableConversion {
        nodes,
        outbounds_json,
        report,
        source_outcomes: outcomes,
        elapsed_micros: started.elapsed().as_micros(),
    }
}

fn contains_ascii_case_insensitive(haystack: &str, needle: &str) -> bool {
    let haystack = haystack.as_bytes();
    let needle = needle.as_bytes();
    !needle.is_empty()
        && needle.len() <= haystack.len()
        && haystack.windows(needle.len()).any(|window| {
            window
                .iter()
                .zip(needle)
                .all(|(left, right)| left.eq_ignore_ascii_case(right))
        })
}

fn parse_source(
    input: &SourceInput,
    limits: &ParserLimits,
    matrix: &CapabilityMatrix,
) -> AdapterOutput {
    match input.format_hint {
        FormatHint::Auto => match crate::detect_bytes(&input.bytes, FormatHint::Auto, limits) {
            Ok(detected) => parse_source(
                &SourceInput {
                    source_id: input.source_id.clone(),
                    format_hint: detected.format(),
                    bytes: input.bytes.clone(),
                },
                limits,
                matrix,
            ),
            Err(error) => single_error(error.code()),
        },
        FormatHint::UriList => uri_output(&input.bytes, Some(&input.source_id), limits, matrix),
        FormatHint::Base64List => match crate::decode_base64_and_detect(&input.bytes, limits) {
            Ok(decoded) => parse_source(
                &SourceInput {
                    source_id: input.source_id.clone(),
                    format_hint: decoded.detected_format(),
                    bytes: decoded.bytes().to_vec(),
                },
                limits,
                matrix,
            ),
            Err(error) => single_error(error.code()),
        },
        #[cfg(feature = "format-clash-yaml")]
        FormatHint::ClashYaml => {
            crate::parse_clash_yaml(&input.bytes, Some(&input.source_id), limits, matrix)
                .unwrap_or_else(|error| source_error(error.code))
        }
        #[cfg(feature = "format-singbox-json")]
        FormatHint::SingboxJson => {
            crate::parse_singbox_json(&input.bytes, Some(&input.source_id), limits, matrix)
                .unwrap_or_else(|error| source_error(error.code))
        }
        #[cfg(feature = "format-surfboard")]
        FormatHint::IniProfile | FormatHint::SurfboardIni => {
            crate::parse_surfboard_ini(&input.bytes, Some(&input.source_id), limits, matrix)
                .unwrap_or_else(|error| source_error(error.code))
        }
        #[cfg(not(feature = "format-clash-yaml"))]
        FormatHint::ClashYaml => single_error(DiagnosticCode::UnsupportedSemantics),
        #[cfg(not(feature = "format-singbox-json"))]
        FormatHint::SingboxJson => single_error(DiagnosticCode::UnsupportedSemantics),
        #[cfg(not(feature = "format-surfboard"))]
        FormatHint::IniProfile | FormatHint::SurfboardIni => {
            single_error(DiagnosticCode::UnsupportedSemantics)
        }
    }
}

fn uri_output(
    bytes: &[u8],
    source_id: Option<&SourceId>,
    limits: &ParserLimits,
    matrix: &CapabilityMatrix,
) -> AdapterOutput {
    let mut output = AdapterOutput::default();
    for item in crate::parse_uri_list(bytes, source_id, limits) {
        if let Some(candidate) = item.candidate() {
            let mut spec = match node_spec_from_uri(candidate) {
                Ok(spec) => spec,
                Err(error) => {
                    output.nodes.push(AdapterNodeResult::rejected(
                        item.item_index(),
                        semantic_diagnostic(
                            error,
                            SourceLocation::new(
                                item.item_index(),
                                Some(item.line()),
                                Some(1),
                                None,
                            )
                            .ok(),
                        ),
                    ));
                    continue;
                }
            };
            spec.source_ref = source_id.cloned().map(|source_id| SourceRef {
                source_id,
                item_index: item.item_index(),
                format: FormatHint::UriList,
                line: Some(item.line()),
            });
            match validate_node_spec(spec, matrix) {
                Ok(outcome) => output.nodes.push(AdapterNodeResult::accepted(
                    item.item_index(),
                    outcome.node,
                    outcome.warnings,
                )),
                Err(error) => output.nodes.push(AdapterNodeResult::rejected(
                    item.item_index(),
                    semantic_diagnostic(
                        error,
                        SourceLocation::new(item.item_index(), Some(item.line()), Some(1), None)
                            .ok(),
                    ),
                )),
            }
        } else if let Some(diagnostic) = item.diagnostic() {
            output.nodes.push(AdapterNodeResult::rejected(
                item.item_index(),
                diagnostic.clone(),
            ));
        }
    }
    output
}

fn encode_credentials(out: &mut Vec<u8>, credentials: &Credentials) {
    match credentials {
        Credentials::Vless { uuid } => field(out, "uuid", uuid.expose()),
        Credentials::Vmess {
            uuid,
            alter_id,
            security,
        } => {
            field(out, "uuid", uuid.expose());
            field(out, "alter_id", &alter_id.to_string());
            field(out, "security", security.as_str());
        }
        Credentials::Shadowsocks {
            method,
            password,
            plugin,
        } => {
            field(out, "method", method.as_str());
            field(out, "password", password.expose());
            field(
                out,
                "plugin",
                plugin.as_ref().map_or("", |plugin| plugin.name.as_str()),
            );
            if let Some(plugin) = plugin {
                field(out, "plugin_opts", &plugin_options(plugin));
            }
        }
        Credentials::Trojan { password } | Credentials::AnyTls { password } => {
            field(out, "password", password.expose())
        }
        Credentials::Hysteria2 { password, obfs } => {
            field(out, "password", password.expose());
            if let Some(obfs) = obfs {
                field(out, "obfs", obfs.kind.as_str());
                field(out, "obfs_password", obfs.password.expose());
            }
        }
        Credentials::Tuic { uuid, password } => {
            field(out, "uuid", uuid.expose());
            field(out, "password", password.expose());
        }
        Credentials::Http { username, password } | Credentials::Socks { username, password } => {
            field(
                out,
                "username",
                username.as_ref().map_or("", |value| value.expose()),
            );
            field(
                out,
                "password",
                password.as_ref().map_or("", |value| value.expose()),
            );
        }
    }
}

fn encode_tls(out: &mut Vec<u8>, tls: &crate::protocol::TlsOptions) {
    field(out, "tls", if tls.enabled { "1" } else { "0" });
    field(out, "insecure", if tls.insecure { "1" } else { "0" });
    field(
        out,
        "sni",
        tls.server_name.as_ref().map_or("", |value| value.as_str()),
    );
    for value in &tls.alpn {
        field(out, "alpn", value.as_str());
    }
    field(
        out,
        "utls",
        tls.client_fingerprint
            .as_ref()
            .map_or("", |value| value.as_str()),
    );
    if let Some(reality) = &tls.reality {
        field(out, "reality_public", reality.public_key.expose());
        field(
            out,
            "reality_short",
            reality.short_id.as_ref().map_or("", |value| value.expose()),
        );
        field(
            out,
            "reality_fp",
            reality
                .fingerprint
                .as_ref()
                .map_or("", |value| value.as_str()),
        );
    }
}

fn encode_transport(out: &mut Vec<u8>, transport: &TransportOptions) {
    field(out, "transport", &format!("{:?}", transport.kind()));
    match transport {
        TransportOptions::Tcp | TransportOptions::Quic => {}
        TransportOptions::WebSocket { path, headers }
        | TransportOptions::HttpUpgrade { path, headers } => {
            field(out, "path", path.as_str());
            for (key, value) in headers {
                field(out, key, value.as_str());
            }
        }
        TransportOptions::Http { path, hosts } => {
            field(out, "path", path.as_str());
            for host in hosts {
                field(out, "host", host.as_str());
            }
        }
        TransportOptions::Grpc { service_name } => field(out, "service", service_name.as_str()),
    }
}

fn encode_protocol_options(out: &mut Vec<u8>, options: &ProtocolOptions) {
    match options {
        ProtocolOptions::Vless { flow } => field(
            out,
            "flow",
            flow.as_ref().map_or("", |value| value.as_str()),
        ),
        ProtocolOptions::Tuic(options) => {
            field(
                out,
                "congestion_control",
                options
                    .congestion_control
                    .as_ref()
                    .map_or("", |value| value.as_str()),
            );
            field(
                out,
                "udp_relay_mode",
                options
                    .udp_relay_mode
                    .as_ref()
                    .map_or("", |value| value.as_str()),
            );
            field(out, "udp_over_stream", &options.udp_over_stream.to_string());
            field(
                out,
                "zero_rtt_handshake",
                &options.zero_rtt_handshake.to_string(),
            );
            field(
                out,
                "heartbeat",
                options
                    .heartbeat
                    .as_ref()
                    .map_or("", |value| value.as_str()),
            );
        }
        ProtocolOptions::Hysteria2(options) => {
            for value in &options.server_ports {
                field(out, "server_port", value.as_str());
            }
            field(
                out,
                "hop_interval",
                options
                    .hop_interval
                    .as_ref()
                    .map_or("", |value| value.as_str()),
            );
            field(
                out,
                "up_mbps",
                &options.up_mbps.map_or(0, u32::from).to_string(),
            );
            field(
                out,
                "down_mbps",
                &options.down_mbps.map_or(0, u32::from).to_string(),
            );
        }
        ProtocolOptions::AnyTls(options) => {
            field(
                out,
                "idle_session_check_interval",
                options
                    .idle_session_check_interval
                    .as_ref()
                    .map_or("", |value| value.as_str()),
            );
            field(
                out,
                "idle_session_timeout",
                options
                    .idle_session_timeout
                    .as_ref()
                    .map_or("", |value| value.as_str()),
            );
            field(
                out,
                "min_idle_session",
                &options.min_idle_session.map_or(0, u32::from).to_string(),
            );
        }
        ProtocolOptions::Shadowsocks {
            udp_over_tcp: Some(options),
        } => {
            field(out, "udp_over_tcp_enabled", &options.enabled.to_string());
            field(out, "udp_over_tcp_version", &options.version.to_string());
        }
        ProtocolOptions::Http(options) => {
            field(
                out,
                "http_path",
                options.path.as_ref().map_or("", |value| value.as_str()),
            );
            for (key, value) in &options.headers {
                field(out, key, value.expose());
            }
        }
        ProtocolOptions::Socks(options) => {
            field(out, "socks_version", options.version.as_str());
            if let Some(options) = options.udp_over_tcp {
                field(out, "udp_over_tcp_enabled", &options.enabled.to_string());
                field(out, "udp_over_tcp_version", &options.version.to_string());
            }
        }
        _ => {}
    }
}

fn udp_over_tcp_json(options: crate::UdpOverTcpOptions) -> Value {
    match options.version {
        0 | 2 => json!(options.enabled),
        version => json!({"enabled": options.enabled, "version": version}),
    }
}

fn tls_json(tls: &crate::protocol::TlsOptions) -> Value {
    let mut object = BTreeMap::<String, Value>::new();
    object.insert("enabled".into(), json!(tls.enabled));
    if let Some(server_name) = &tls.server_name {
        object.insert("server_name".into(), json!(server_name.as_str()));
    }
    if tls.insecure {
        object.insert("insecure".into(), json!(true));
    }
    if !tls.alpn.is_empty() {
        object.insert(
            "alpn".into(),
            json!(
                tls.alpn
                    .iter()
                    .map(|value| value.as_str())
                    .collect::<Vec<_>>()
            ),
        );
    }
    if let Some(fingerprint) = &tls.client_fingerprint {
        object.insert(
            "utls".into(),
            json!({"enabled": true, "fingerprint": fingerprint.as_str()}),
        );
    }
    if let Some(reality) = &tls.reality {
        object.insert("reality".into(), json!({"enabled": true, "public_key": reality.public_key.expose(), "short_id": reality.short_id.as_ref().map(|value| value.expose())}));
    }
    serde_json::to_value(object).expect("tls object must serialize")
}

fn transport_json(transport: &TransportOptions) -> Value {
    match transport {
        TransportOptions::Tcp => json!({"type":"tcp"}),
        TransportOptions::Quic => json!({"type":"quic"}),
        TransportOptions::WebSocket { path, headers } => {
            json!({"type":"ws", "path": path.as_str(), "headers": headers.iter().map(|(k, v)| (k, v.as_str())).collect::<BTreeMap<_, _>>() })
        }
        TransportOptions::Http { path, hosts } => {
            json!({"type":"http", "path": path.as_str(), "host": hosts.iter().map(|value| value.as_str()).collect::<Vec<_>>() })
        }
        TransportOptions::HttpUpgrade { path, headers } => {
            json!({"type":"httpupgrade", "path": path.as_str(), "headers": headers.iter().map(|(k, v)| (k, v.as_str())).collect::<BTreeMap<_, _>>() })
        }
        TransportOptions::Grpc { service_name } => {
            json!({"type":"grpc", "service_name": service_name.as_str()})
        }
    }
}

fn field(out: &mut Vec<u8>, name: &str, value: &str) {
    out.extend_from_slice(&(name.len() as u32).to_be_bytes());
    out.extend_from_slice(name.as_bytes());
    out.extend_from_slice(&(value.len() as u32).to_be_bytes());
    out.extend_from_slice(value.as_bytes());
}

fn plugin_options(plugin: &crate::protocol::PluginSpec) -> String {
    plugin
        .options
        .iter()
        .map(|(key, value)| format!("{}={}", key.as_str(), value.as_str()))
        .collect::<Vec<_>>()
        .join(";")
}

fn merge_refs(target: &mut Vec<SourceRef>, refs: &[SourceRef], cap: usize) {
    for reference in refs {
        if target.len() >= cap {
            break;
        }
        if !target.contains(reference) {
            target.push(reference.clone());
        }
    }
}

fn push_diagnostic(
    report: &mut ConversionReport,
    diagnostic: NodeDiagnostic,
    limits: &ParserLimits,
) {
    *report
        .diagnostic_counts
        .entry(diagnostic.code.clone())
        .or_insert(0) += 1;
    if report.diagnostics.len() < limits.max_detailed_diagnostics() {
        report.diagnostics.push(diagnostic);
    } else {
        report.summary.truncated = true;
    }
}

fn source_error(code: DiagnosticCode) -> AdapterOutput {
    let mut output = AdapterOutput::default();
    output
        .diagnostics
        .push(NodeDiagnostic::new(code, Severity::Error));
    output
}
fn single_error(code: DiagnosticCode) -> AdapterOutput {
    source_error(code)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
