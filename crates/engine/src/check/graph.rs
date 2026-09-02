//! See [`super`]. One family of workflow-check rules.

use super::*;

pub(crate) fn find_depends_on_cycle(nodes: &[Node]) -> Option<Vec<NodeId>> {
    let adjacency: std::collections::BTreeMap<NodeId, Vec<NodeId>> = nodes
        .iter()
        .map(|n| (n.id.clone(), n.depends_on.clone()))
        .collect();
    crate::graph::find_cycle(&adjacency)
}

/// Per-file workflow-node rules: no runner bindings (a workflow
/// node opens no session — the child's nodes bind their own), and an
/// `inherit` child of a `parallel` group must declare scope so the
/// disjointness demand is verifiable at all.
pub(crate) fn check_workflow_nodes(
    nodes: &[Node],
    group: Option<&Node>,
    errors: &mut Vec<CheckError>,
) {
    for node in nodes {
        if let NodeKind::Workflow { isolation, .. } = &node.kind {
            for (present, field) in [
                (node.runner.is_some(), "runner"),
                (!node.runners.is_empty(), "runners"),
                (node.agent.is_some(), "agent"),
            ] {
                if present {
                    errors.push(CheckError::WorkflowNodeRunnerBinding {
                        node: node.id.clone(),
                        field,
                    });
                }
            }
            if *isolation == yunta_core::WorkflowIsolation::Inherit && node.scope.is_empty() {
                if let Some(group) = group {
                    errors.push(CheckError::InheritChildWithoutScope {
                        group: group.id.clone(),
                        node: node.id.clone(),
                    });
                }
            }
        }
        if let NodeKind::Parallel {
            nodes: children, ..
        } = &node.kind
        {
            check_workflow_nodes(children, Some(node), errors);
        }
    }
}

/// Mount declaration rules, on the original (pre-expansion) shape:
/// mounts live on top-level `kind: workflow` nodes, reference an
/// existing node other than themselves, and never appear inside a
/// `parallel` group (no order there — "the source node already
/// finished" cannot hold). `MountOnFanOut` lives in
/// [`check_runner_fanout`], next to the other fan-out target rules.
pub(crate) fn check_mounts(workflow: &Workflow, errors: &mut Vec<CheckError>) {
    let known: HashSet<&NodeId> = workflow.iter_nodes().map(|node| &node.id).collect();
    for node in &workflow.nodes {
        if let NodeKind::Workflow { mounts, .. } = &node.kind {
            for mount in mounts {
                let target = &mount.artifact.node;
                if target == &node.id {
                    errors.push(CheckError::MountOnSelf {
                        node: node.id.clone(),
                    });
                } else if !known.contains(target) {
                    errors.push(CheckError::BrokenReference {
                        node: node.id.clone(),
                        field: "mounts".to_string(),
                        target: target.clone(),
                    });
                }
            }
        }
        if let NodeKind::Parallel {
            nodes: children, ..
        } = &node.kind
        {
            for child in children {
                if let NodeKind::Workflow { mounts, .. } = &child.kind {
                    if !mounts.is_empty() {
                        errors.push(CheckError::MountInsideParallel {
                            group: node.id.clone(),
                            node: child.id.clone(),
                        });
                    }
                }
            }
        }
    }
}
