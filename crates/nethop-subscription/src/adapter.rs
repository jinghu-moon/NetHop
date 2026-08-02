use crate::{diagnostics::NodeDiagnostic, protocol::ProxyNode};

/// Bounded, ordered adapter output. It contains only terminal nodes and compact diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AdapterOutput {
    pub nodes: Vec<AdapterNodeResult>,
    pub diagnostics: Vec<NodeDiagnostic>,
}

impl AdapterOutput {
    pub fn accepted_count(&self) -> usize {
        self.nodes.iter().filter(|item| item.node.is_some()).count()
    }

    pub fn rejected_count(&self) -> usize {
        self.nodes
            .iter()
            .filter(|item| item.diagnostic.is_some())
            .count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterNodeResult {
    pub item_index: u32,
    pub node: Option<ProxyNode>,
    pub diagnostic: Option<NodeDiagnostic>,
    pub warnings: Vec<NodeDiagnostic>,
}

impl AdapterNodeResult {
    pub fn accepted(item_index: u32, node: ProxyNode, warnings: Vec<NodeDiagnostic>) -> Self {
        Self {
            item_index,
            node: Some(node),
            diagnostic: None,
            warnings,
        }
    }

    pub fn rejected(item_index: u32, diagnostic: NodeDiagnostic) -> Self {
        Self {
            item_index,
            node: None,
            diagnostic: Some(diagnostic),
            warnings: Vec::new(),
        }
    }
}
