//! `yunta check` (T1.3) — **M-0 cut only**.
//!
//! Full T1.3 also validates mode coherence, disjoint scopes in `parallel`,
//! template variables, workflow-composition depth, permission ceilings and
//! warns on pushes to the base branch — all of it for schema surface
//! (`modes:`, `parallel`, `context:`, `permissions:`, composition) that
//! M-0's recorte doesn't have yet (T1.1/T1.2). What's checked here is
//! exactly what the recortado schema can be wrong about:
//!
//! - node ids are unique;
//! - `depends_on` references exist and its graph is acyclic (I14: this
//!   check never looks at `on_failure.goto` — re-route edges are a
//!   separate set that never relaxes `depends_on` acyclicity);
//! - `on_failure.goto` targets exist;
//! - a node's `runner:` resolves to a role with at least one candidate in
//!   the merged config's `runners:`.
//!
//! Capability-aware checks ("existencia de agentes pedidos", required
//! capabilities) wait for the `Adapter` trait (T3.1) to exist — there is
//! nothing to probe yet.

use std::collections::{HashMap, HashSet};

use thiserror::Error;
use yunta_core::{ConfigLayer, Node, NodeId, Workflow};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CheckError {
    #[error("duplicate node id `{id}`")]
    DuplicateNodeId { id: NodeId },

    #[error("node `{node}` depends_on unknown node `{unknown}`")]
    UnknownDependency { node: NodeId, unknown: NodeId },

    #[error("node `{node}` on_failure.goto targets unknown node `{target}`")]
    UnknownGotoTarget { node: NodeId, target: NodeId },

    #[error("cycle in depends_on: {path}")]
    DependsOnCycle { path: String },

    #[error("node `{node}` references runner `{runner}`, which `runners:` does not define")]
    UnknownRunner { node: NodeId, runner: String },

    #[error(
        "node `{node}` references runner `{runner}`, which `runners:` defines with zero candidates"
    )]
    RunnerHasNoCandidates { node: NodeId, runner: String },
}

/// Validates a workflow against the M-0 rule set. Every applicable rule is
/// checked and every violation reported — not just the first one (same
/// spirit as the ledger's T1.0 §4: whoever writes this by hand corrects
/// once, not once per `yunta check` run).
pub fn check(workflow: &Workflow, config: &ConfigLayer) -> Vec<CheckError> {
    let mut errors = Vec::new();

    let mut known_ids: HashSet<NodeId> = HashSet::new();
    for node in &workflow.nodes {
        if !known_ids.insert(node.id.clone()) {
            errors.push(CheckError::DuplicateNodeId {
                id: node.id.clone(),
            });
        }
    }

    for node in &workflow.nodes {
        for dep in &node.depends_on {
            if !known_ids.contains(dep) {
                errors.push(CheckError::UnknownDependency {
                    node: node.id.clone(),
                    unknown: dep.clone(),
                });
            }
        }

        if let Some(on_failure) = &node.on_failure {
            if !known_ids.contains(&on_failure.goto) {
                errors.push(CheckError::UnknownGotoTarget {
                    node: node.id.clone(),
                    target: on_failure.goto.clone(),
                });
            }
        }

        if let Some(runner) = &node.runner {
            match config.runners.as_ref().and_then(|r| r.get(runner)) {
                None => errors.push(CheckError::UnknownRunner {
                    node: node.id.clone(),
                    runner: runner.clone(),
                }),
                Some(candidates) if candidates.is_empty() => {
                    errors.push(CheckError::RunnerHasNoCandidates {
                        node: node.id.clone(),
                        runner: runner.clone(),
                    })
                }
                Some(_) => {}
            }
        }
    }

    if let Some(cycle) = find_depends_on_cycle(&workflow.nodes) {
        let path = cycle
            .iter()
            .map(NodeId::as_str)
            .collect::<Vec<_>>()
            .join(" -> ");
        errors.push(CheckError::DependsOnCycle { path });
    }

    errors
}

fn find_depends_on_cycle(nodes: &[Node]) -> Option<Vec<NodeId>> {
    #[derive(Clone, Copy, PartialEq)]
    enum Color {
        White,
        /// Carries its own index in `stack`, so finding a gray node's
        /// position never needs a fallible search.
        Gray(usize),
        Black,
    }

    let adjacency: HashMap<NodeId, Vec<NodeId>> = nodes
        .iter()
        .map(|n| (n.id.clone(), n.depends_on.clone()))
        .collect();
    let mut color: HashMap<NodeId, Color> = adjacency
        .keys()
        .cloned()
        .map(|id| (id, Color::White))
        .collect();
    let mut stack: Vec<NodeId> = Vec::new();

    fn visit(
        id: &NodeId,
        adjacency: &HashMap<NodeId, Vec<NodeId>>,
        color: &mut HashMap<NodeId, Color>,
        stack: &mut Vec<NodeId>,
    ) -> Option<Vec<NodeId>> {
        color.insert(id.clone(), Color::Gray(stack.len()));
        stack.push(id.clone());

        if let Some(deps) = adjacency.get(id) {
            for dep in deps {
                if !adjacency.contains_key(dep) {
                    continue; // unknown dependency — reported separately
                }
                match color.get(dep).copied() {
                    Some(Color::Gray(pos)) => {
                        let mut cycle = stack[pos..].to_vec();
                        cycle.push(dep.clone());
                        return Some(cycle);
                    }
                    Some(Color::Black) => continue,
                    _ => {
                        if let Some(cycle) = visit(dep, adjacency, color, stack) {
                            return Some(cycle);
                        }
                    }
                }
            }
        }

        stack.pop();
        color.insert(id.clone(), Color::Black);
        None
    }

    for id in adjacency.keys() {
        if matches!(color.get(id), Some(Color::White)) {
            if let Some(cycle) = visit(id, &adjacency, &mut color, &mut stack) {
                return Some(cycle);
            }
        }
    }
    None
}
