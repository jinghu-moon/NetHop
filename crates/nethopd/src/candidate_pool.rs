use std::collections::HashSet;

use nethop_subscription::SourceId;
use thiserror::Error;

use crate::{
    NodeAttribution, StableNodeId, SubscriptionMode,
    worker_config::{MAX_AUTO_CANDIDATES, MAX_SOURCES},
};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CandidatePoolError {
    #[error("active source set is invalid")]
    InvalidActiveSources,
    #[error("candidate limit is invalid")]
    InvalidLimit,
    #[error("candidate node IDs must be unique")]
    DuplicateNode,
    #[error("no candidate belongs to the active source set")]
    EmptyCandidates,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidatePoolNode {
    id: StableNodeId,
    attribution: NodeAttribution,
}

impl CandidatePoolNode {
    pub const fn new(id: StableNodeId, attribution: NodeAttribution) -> Self {
        Self { id, attribution }
    }

    pub const fn id(&self) -> &StableNodeId {
        &self.id
    }

    pub const fn attribution(&self) -> &NodeAttribution {
        &self.attribution
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceContribution {
    source_id: SourceId,
    candidates: usize,
}

impl SourceContribution {
    pub const fn source_id(&self) -> &SourceId {
        &self.source_id
    }

    pub const fn candidates(&self) -> usize {
        self.candidates
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidatePools {
    auto: Vec<StableNodeId>,
    manual: Vec<StableNodeId>,
    contributions: Vec<SourceContribution>,
}

impl CandidatePools {
    pub fn auto(&self) -> &[StableNodeId] {
        &self.auto
    }

    pub fn manual(&self) -> &[StableNodeId] {
        &self.manual
    }

    pub fn contributions(&self) -> &[SourceContribution] {
        &self.contributions
    }
}

pub fn build_candidate_pools(
    mode: SubscriptionMode,
    active_source_ids: &[SourceId],
    nodes: &[CandidatePoolNode],
    max_candidates: usize,
) -> Result<CandidatePools, CandidatePoolError> {
    validate_inputs(mode, active_source_ids, nodes, max_candidates)?;

    let mut eligible = nodes
        .iter()
        .filter(|node| {
            active_source_ids
                .iter()
                .any(|source_id| node.attribution.contains(source_id))
        })
        .collect::<Vec<_>>();
    eligible.sort_unstable_by(|left, right| left.id.cmp(&right.id));
    if eligible.is_empty() {
        return Err(CandidatePoolError::EmptyCandidates);
    }
    let manual = eligible.iter().map(|node| node.id.clone()).collect();
    let mut contributions = active_source_ids
        .iter()
        .cloned()
        .map(|source_id| SourceContribution {
            source_id,
            candidates: 0,
        })
        .collect::<Vec<_>>();

    let auto = match mode {
        SubscriptionMode::Single => {
            let selected = eligible
                .iter()
                .take(max_candidates)
                .map(|node| node.id.clone())
                .collect::<Vec<_>>();
            contributions[0].candidates = selected.len();
            selected
        }
        SubscriptionMode::Merge => round_robin(
            active_source_ids,
            &eligible,
            max_candidates,
            &mut contributions,
        ),
    };

    Ok(CandidatePools {
        auto,
        manual,
        contributions,
    })
}

fn validate_inputs(
    mode: SubscriptionMode,
    active_source_ids: &[SourceId],
    nodes: &[CandidatePoolNode],
    max_candidates: usize,
) -> Result<(), CandidatePoolError> {
    let unique_sources = active_source_ids.iter().collect::<HashSet<_>>();
    if active_source_ids.is_empty()
        || active_source_ids.len() > MAX_SOURCES
        || unique_sources.len() != active_source_ids.len()
        || (mode == SubscriptionMode::Single && active_source_ids.len() != 1)
    {
        return Err(CandidatePoolError::InvalidActiveSources);
    }
    if !(1..=usize::from(MAX_AUTO_CANDIDATES)).contains(&max_candidates) {
        return Err(CandidatePoolError::InvalidLimit);
    }
    let unique_nodes = nodes
        .iter()
        .map(CandidatePoolNode::id)
        .collect::<HashSet<_>>();
    if unique_nodes.len() != nodes.len() {
        return Err(CandidatePoolError::DuplicateNode);
    }
    Ok(())
}

fn round_robin(
    active_source_ids: &[SourceId],
    nodes: &[&CandidatePoolNode],
    max_candidates: usize,
    contributions: &mut [SourceContribution],
) -> Vec<StableNodeId> {
    let mut queues = vec![Vec::<usize>::new(); active_source_ids.len()];
    for (node_index, node) in nodes.iter().enumerate() {
        for (source_index, source_id) in active_source_ids.iter().enumerate() {
            if node.attribution.contains(source_id) {
                queues[source_index].push(node_index);
            }
        }
    }

    let mut cursors = vec![0_usize; queues.len()];
    let mut visited = vec![false; nodes.len()];
    let mut selected = Vec::with_capacity(max_candidates.min(nodes.len()));
    while selected.len() < max_candidates {
        let mut progressed = false;
        for source_index in 0..queues.len() {
            while let Some(&node_index) = queues[source_index].get(cursors[source_index]) {
                cursors[source_index] += 1;
                if visited[node_index] {
                    continue;
                }
                visited[node_index] = true;
                selected.push(nodes[node_index].id.clone());
                contributions[source_index].candidates += 1;
                progressed = true;
                break;
            }
            if selected.len() == max_candidates {
                break;
            }
        }
        if !progressed {
            break;
        }
    }
    selected
}
