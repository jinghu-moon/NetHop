use std::collections::{BTreeMap, HashSet};

use nethop_core::{ComposerError, TerminalOutbound};
use serde_json::{Map, Value};
use thiserror::Error;

use crate::{DedupedNode, compose_outbound};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TerminalOutboundAdapterError {
    #[error("composed outbound is not an object")]
    NotObject,
    #[error("composed outbound is missing a string tag")]
    MissingTag,
    #[error("composed outbound is missing a string protocol type")]
    MissingProtocol,
    #[error("composed outbound tags must be unique")]
    DuplicateTag,
    #[error("terminal outbound was rejected: {0}")]
    Rejected(#[from] ComposerError),
}

/// Adapts one validated and deduplicated parser node into the core domain.
///
/// Protocol mapping remains owned by `compose_outbound`; this boundary only
/// separates the core-owned tag/type fields from the audited terminal fields.
pub fn adapt_terminal_outbound(
    node: &DedupedNode,
) -> Result<TerminalOutbound, TerminalOutboundAdapterError> {
    terminal_from_composed(compose_outbound(node))
}

pub fn adapt_terminal_outbounds(
    nodes: &[DedupedNode],
) -> Result<Vec<TerminalOutbound>, TerminalOutboundAdapterError> {
    let mut tags = HashSet::with_capacity(nodes.len());
    let mut outbounds = Vec::with_capacity(nodes.len());
    for node in nodes {
        let outbound = adapt_terminal_outbound(node)?;
        if !tags.insert(outbound.tag().to_owned()) {
            return Err(TerminalOutboundAdapterError::DuplicateTag);
        }
        outbounds.push(outbound);
    }
    Ok(outbounds)
}

fn terminal_from_composed(value: Value) -> Result<TerminalOutbound, TerminalOutboundAdapterError> {
    let Value::Object(mut object) = value else {
        return Err(TerminalOutboundAdapterError::NotObject);
    };
    let tag = take_string(&mut object, "tag").ok_or(TerminalOutboundAdapterError::MissingTag)?;
    let protocol =
        take_string(&mut object, "type").ok_or(TerminalOutboundAdapterError::MissingProtocol)?;
    let fields = object.into_iter().collect::<BTreeMap<_, _>>();
    TerminalOutbound::new(tag, protocol, fields).map_err(Into::into)
}

fn take_string(object: &mut Map<String, Value>, key: &str) -> Option<String> {
    object.remove(key)?.as_str().map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{TerminalOutboundAdapterError, terminal_from_composed};

    #[test]
    fn malformed_composer_shapes_fail_closed() {
        assert_eq!(
            terminal_from_composed(json!([])).unwrap_err(),
            TerminalOutboundAdapterError::NotObject
        );
        assert_eq!(
            terminal_from_composed(json!({"type": "vless"})).unwrap_err(),
            TerminalOutboundAdapterError::MissingTag
        );
        assert_eq!(
            terminal_from_composed(json!({"tag": "node"})).unwrap_err(),
            TerminalOutboundAdapterError::MissingProtocol
        );
        assert_eq!(
            terminal_from_composed(json!({"tag": 1, "type": "vless"})).unwrap_err(),
            TerminalOutboundAdapterError::MissingTag
        );
    }
}
