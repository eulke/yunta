//! Every reference a node writes reaches a node the run will have.

use std::collections::HashSet;

use super::read::about;
use super::WorkflowFault;
use crate::diagnostic::Diagnostic;
use crate::{NodeId, NodeKind, Workflow};

/// Every node a run of `workflow` has, by id: each the file declares, and
/// each sibling a `runners:` fan-out generates.
fn nodes_run(workflow: &Workflow) -> HashSet<NodeId> {
    workflow
        .iter_nodes()
        .flat_map(|node| {
            std::iter::once(node.id.clone()).chain(
                node.runners
                    .iter()
                    .map(|runner| NodeId::fan_out(&node.id, runner)),
            )
        })
        .collect()
}

/// The id among the file's other nodes nearest a reference that names no
/// node. Never the node's own id: a node that named itself would be a cycle,
/// not a slip.
fn nearest_other(ids: &HashSet<NodeId>, own: &NodeId, target: &NodeId) -> Option<NodeId> {
    let others: Vec<&NodeId> = ids.iter().filter(|id| *id != own).collect();
    surco::text::nearest(target.as_str(), others.iter().map(|id| id.as_str()))
        .and_then(|near| others.iter().find(|id| id.as_str() == near))
        .map(|id| (**id).clone())
}

/// Every reference a node writes reaches a node the run will have: one
/// the file declares, or a sibling a `runners:` fan-out generates.
pub(super) fn references_reach(workflow: &Workflow) -> Vec<Diagnostic> {
    let ids = &nodes_run(workflow);
    let mut broken = Vec::new();
    for (index, node) in workflow.nodes.iter().enumerate() {
        let mut reaches = |field: &str, target: &NodeId, at: crate::yaml::Pointer| {
            if !ids.contains(target) {
                let nearest = nearest_other(ids, &node.id, target);
                broken.push(
                    about(
                        index,
                        &node.id,
                        WorkflowFault::UnknownReference {
                            field: field.to_string(),
                            target: target.clone(),
                            nearest,
                        },
                    )
                    .within(at),
                );
            }
        };
        let at = crate::yaml::Pointer::root;
        for (position, dep) in node.depends_on.iter().enumerate() {
            reaches("depends_on", dep, at().key("depends_on").index(position));
        }
        if let Some(on_failure) = &node.on_failure {
            reaches(
                "on_failure.goto",
                &on_failure.goto,
                at().key("on_failure").key("goto"),
            );
        }
        if let NodeKind::Gate { on, .. } = &node.kind {
            for (option, target) in on {
                reaches("on", target, at().key("on").key(option.as_str()));
            }
        }
        for read in super::reads::artifact_reads(node) {
            if let Some(producer) = read.node {
                reaches(
                    read.site.field(),
                    producer,
                    read.site.names_its_node(read.entry),
                );
            }
        }
    }
    broken
}
