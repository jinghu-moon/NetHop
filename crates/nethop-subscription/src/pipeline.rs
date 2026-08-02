use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    time::Instant,
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as ShaDigest, Sha256};

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
                    first_source_order: source_order,
                    first_item_index,
                });
                outcome.accepted += 1;
            }
        }
        outcomes.insert(batch.source_id, outcome);
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
    object.insert("server_port".into(), json!(node.node.endpoint().port()));
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
            method, password, ..
        } => {
            object.insert("method".into(), json!(method.as_str()));
            object.insert("password".into(), json!(password.expose()));
        }
        Credentials::Trojan { password } | Credentials::AnyTls { password } => {
            object.insert("password".into(), json!(password.expose()));
        }
        Credentials::Hysteria2 { password, obfs } => {
            object.insert("password".into(), json!(password.expose()));
            if let Some(obfs) = obfs {
                object.insert("obfs".into(), json!({"type": obfs.as_str()}));
            }
        }
        Credentials::Tuic { uuid, password } => {
            object.insert("uuid".into(), json!(uuid.expose()));
            object.insert("password".into(), json!(password.expose()));
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StableConversion {
    pub nodes: Vec<DedupedNode>,
    pub outbounds_json: String,
    pub report: ConversionReport,
    pub elapsed_micros: u128,
}

pub fn convert_stable_sources(
    inputs: Vec<SourceInput>,
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
        let mut output = parse_source(&input, limits, matrix);
        detected = input.format_hint;
        let rejected = output.rejected_count();
        let source_warnings = output.diagnostics.len();
        let mut nodes = Vec::with_capacity(output.accepted_count());
        for mut item in output.nodes.drain(..) {
            if let Some(node) = item.node.take() {
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
        batches.push(SourceBatch {
            source_id: input.source_id,
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
        elapsed_micros: started.elapsed().as_micros(),
    }
}

fn parse_source(
    input: &SourceInput,
    limits: &ParserLimits,
    matrix: &CapabilityMatrix,
) -> AdapterOutput {
    match input.format_hint {
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
        _ => single_error(DiagnosticCode::UnsupportedSemantics),
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
        }
        Credentials::Trojan { password } | Credentials::AnyTls { password } => {
            field(out, "password", password.expose())
        }
        Credentials::Hysteria2 { password, obfs } => {
            field(out, "password", password.expose());
            field(
                out,
                "obfs",
                obfs.as_ref().map_or("", |value| value.as_str()),
            );
        }
        Credentials::Tuic { uuid, password } => {
            field(out, "uuid", uuid.expose());
            field(out, "password", password.expose());
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
    if let ProtocolOptions::Vless { flow } = options {
        field(
            out,
            "flow",
            flow.as_ref().map_or("", |value| value.as_str()),
        );
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
        object.insert("utls".into(), json!({"fingerprint": fingerprint.as_str()}));
    }
    if let Some(reality) = &tls.reality {
        object.insert("reality".into(), json!({"public_key": reality.public_key.expose(), "short_id": reality.short_id.as_ref().map(|value| value.expose())}));
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
