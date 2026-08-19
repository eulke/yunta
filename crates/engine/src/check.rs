//! `yunta check` (T1.3) — **M-0/M4 cut only**.
//!
//! Full T1.3 also validates mode coherence, template variables,
//! workflow-composition depth, permission ceilings and warns on pushes to
//! the base branch — all of it for schema surface (`modes:`, `context:`,
//! `permissions:`, composition) that this recorte doesn't have yet
//! (T1.1/T1.2). What's checked here is exactly what the recortado schema
//! can be wrong about:
//!
//! - node ids are unique, globally — including every `parallel` child,
//!   nested arbitrarily deep (T4.6);
//! - `depends_on` references exist and its graph is acyclic (I14: this
//!   check never looks at `on_failure.goto` — re-route edges are a
//!   separate set that never relaxes `depends_on` acyclicity);
//! - `on_failure.goto` targets exist;
//! - a node's `runner:` resolves to a role with at least one candidate in
//!   the merged config's `runners:`;
//! - a `parallel` group's children don't declare overlapping scope
//!   (D100/§5.8) — error, since it's verifiable in advance from the
//!   workflow alone (see `check_warnings` for the "can't verify" case).
//!
//! Capability-aware checks ("existencia de agentes pedidos", required
//! capabilities) wait for the `Adapter` trait (T3.1) to exist — there is
//! nothing to probe yet.

use std::collections::{HashMap, HashSet};

use thiserror::Error;
use yunta_core::{ConfigLayer, Node, NodeId, NodeKind, Workflow};

use crate::ledger::globs_might_overlap;

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

    /// D100/§5.8: `parallel`'s children share one worktree — a scope
    /// overlap between two of them is a verifiable-in-advance write
    /// collision, error rather than warning.
    #[error(
        "parallel group `{group}`: children `{a}` and `{b}` declare overlapping scope \
         (`{glob_a}` / `{glob_b}`) — they run at once and share one worktree"
    )]
    OverlappingParallelScope {
        group: NodeId,
        a: NodeId,
        b: NodeId,
        glob_a: String,
        glob_b: String,
    },
}

/// A non-blocking finding — the run can still start (D100/§5.8: `check`
/// warns, it doesn't refuse, when a collision can't be verified for lack
/// of declared scope). Kept separate from `CheckError` rather than adding
/// a severity field to it: every existing caller of `check()` keeps
/// treating its `Vec<CheckError>` as "must be empty to proceed" without
/// learning to filter by severity.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CheckWarning {
    #[error(
        "parallel group `{group}`: two or more children can write and don't declare scope as \
         disjoint — the engine can't verify they won't collide (D100); declare `scope` on each \
         to make the check real"
    )]
    UndeclaredParallelScope { group: NodeId },
}

/// Validates a workflow against the M-0 rule set. Every applicable rule is
/// checked and every violation reported — not just the first one (same
/// spirit as the ledger's T1.0 §4: whoever writes this by hand corrects
/// once, not once per `yunta check` run).
pub fn check(workflow: &Workflow, config: &ConfigLayer) -> Vec<CheckError> {
    let mut errors = Vec::new();

    // Global, not per-group: replay derives node state from one flat
    // NodeId -> NodeState map (I2), so a `parallel` child's id colliding
    // with anything else — a sibling, a top-level node, another group's
    // child — would corrupt derivation, not just read oddly.
    let mut known_ids: HashSet<NodeId> = HashSet::new();
    collect_ids(&workflow.nodes, &mut known_ids, &mut errors);

    check_parallel_scopes(&workflow.nodes, &mut errors);

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

/// Non-blocking findings — D100/§5.8's "can't verify, so warn" case.
/// Separate entry point from [`check`] rather than a severity field on
/// `CheckError`, so nothing that already treats `check()`'s output as
/// "must be empty to proceed" has to learn to filter by severity.
///
/// D100's real condition is "two or more children **with write
/// permissions**" — `permissions:` doesn't exist in this recorte's schema
/// (T5.7), so every node is implicitly write-capable today (`bash` always
/// runs unrestricted; `prompt`/`loop` always dispatch with
/// `PermissionProfile::Edit`, `node_exec.rs`). This warns on any group of
/// two or more children rather than filtering by a permission that can't
/// be declared yet — honest given the schema, not a narrower rule than
/// D100 intends.
pub fn check_warnings(workflow: &Workflow) -> Vec<CheckWarning> {
    let mut warnings = Vec::new();
    collect_parallel_warnings(&workflow.nodes, &mut warnings);
    warnings
}

fn collect_ids(nodes: &[Node], known_ids: &mut HashSet<NodeId>, errors: &mut Vec<CheckError>) {
    for node in nodes {
        if !known_ids.insert(node.id.clone()) {
            errors.push(CheckError::DuplicateNodeId {
                id: node.id.clone(),
            });
        }
        if let NodeKind::Parallel {
            nodes: children, ..
        } = &node.kind
        {
            collect_ids(children, known_ids, errors);
        }
    }
}

/// One `parallel` group's scope-collision status (D100): every pair of
/// children whose declared scopes might overlap, and whether every child
/// declared a scope at all — the two facts `check`'s error and
/// `check_warnings`' warning each need, computed once so they can never
/// disagree with each other.
struct GroupScope<'a> {
    overlaps: Vec<(&'a Node, &'a Node, &'a str, &'a str)>,
    all_declared: bool,
}

fn evaluate_group_scope(children: &[Node]) -> GroupScope<'_> {
    let mut overlaps = Vec::new();
    for i in 0..children.len() {
        for j in (i + 1)..children.len() {
            let (a, b) = (&children[i], &children[j]);
            for glob_a in &a.scope {
                for glob_b in &b.scope {
                    if globs_might_overlap(glob_a, glob_b) {
                        overlaps.push((a, b, glob_a.as_str(), glob_b.as_str()));
                    }
                }
            }
        }
    }
    GroupScope {
        overlaps,
        all_declared: children.iter().all(|child| !child.scope.is_empty()),
    }
}

fn check_parallel_scopes(nodes: &[Node], errors: &mut Vec<CheckError>) {
    for node in nodes {
        if let NodeKind::Parallel {
            nodes: children, ..
        } = &node.kind
        {
            let group = evaluate_group_scope(children);
            for (a, b, glob_a, glob_b) in group.overlaps {
                errors.push(CheckError::OverlappingParallelScope {
                    group: node.id.clone(),
                    a: a.id.clone(),
                    b: b.id.clone(),
                    glob_a: glob_a.to_string(),
                    glob_b: glob_b.to_string(),
                });
            }
            check_parallel_scopes(children, errors);
        }
    }
}

fn collect_parallel_warnings(nodes: &[Node], warnings: &mut Vec<CheckWarning>) {
    for node in nodes {
        if let NodeKind::Parallel {
            nodes: children, ..
        } = &node.kind
        {
            if children.len() >= 2 {
                let group = evaluate_group_scope(children);
                if group.overlaps.is_empty() && !group.all_declared {
                    warnings.push(CheckWarning::UndeclaredParallelScope {
                        group: node.id.clone(),
                    });
                }
            }
            collect_parallel_warnings(children, warnings);
        }
    }
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
