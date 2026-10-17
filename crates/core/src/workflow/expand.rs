//! What a workflow file implies, made explicit: a fan-out into its
//! siblings, a context reference into the edge it is.

use crate::{Node, NodeId, NodeKind, Workflow};

/// A node with `runners: [a, b]` becomes one `<id>@<runner>`
/// node per runner — **statically, in the manifest**, before anything
/// runs: the fan-out is visible in `status`, each expanded node
/// resolves its own runner and renders its own `{{runner.name}}`, and
/// the scheduler needs zero fan-out awareness. Every reference to the
/// original id follows the expansion: downstream `depends_on` rewires
/// onto all siblings, and mode include lists name them all (so a mode
/// that covered `review` still covers the whole review). Re-route and
/// gate targets onto a fan-out node are check errors — there is no
/// unambiguous "return control to review" once review is many nodes —
/// so this function never sees one.
pub fn expand_runner_fanout(workflow: &mut Workflow) {
    let mut expansion: std::collections::HashMap<NodeId, Vec<NodeId>> =
        std::collections::HashMap::new();
    let mut nodes = Vec::with_capacity(workflow.nodes.len());
    for node in workflow.nodes.drain(..) {
        if node.runners.is_empty() {
            nodes.push(node);
            continue;
        }
        let mut expanded_ids = Vec::new();
        for runner in &node.runners {
            let mut sibling = node.clone();
            sibling.id = NodeId::fan_out(&node.id, runner);
            sibling.runner = Some(runner.clone());
            sibling.runners = Vec::new();
            expanded_ids.push(sibling.id.clone());
            nodes.push(sibling);
        }
        expansion.insert(node.id.clone(), expanded_ids);
    }
    for node in &mut nodes {
        let mut rewired = Vec::with_capacity(node.depends_on.len());
        for dep in node.depends_on.drain(..) {
            match expansion.get(&dep) {
                Some(siblings) => rewired.extend(siblings.iter().cloned()),
                None => rewired.push(dep),
            }
        }
        node.depends_on = rewired;
    }
    if let Some(modes) = &mut workflow.modes {
        for spec in modes.values_mut() {
            if let crate::ModeInclude::Nodes(included) = &mut spec.include {
                let mut rewritten = Vec::with_capacity(included.len());
                for id in included.drain(..) {
                    match expansion.get(&id) {
                        Some(siblings) => rewritten.extend(siblings.iter().cloned()),
                        None => rewritten.push(id),
                    }
                }
                *included = rewritten;
            }
        }
    }
    workflow.nodes = nodes;
}

/// `context: [{ artifact: { node, name } }]` creates an *implicit*
/// `depends_on` edge onto `node` — folded into the ordinary field here,
/// once, so `check`'s cycle detection and the scheduler's own readiness
/// calculation (both already only ever read `Node.depends_on`) need zero
/// awareness of `context:` existing at all. [`super::read::read`] calls this on its own copy, so a cycle created purely
/// by two nodes' context-artifact references is still caught where the
/// file is read rather than deadlocking a real run. Idempotent: a
/// node that already lists the referenced node explicitly gets no
/// duplicate.
pub fn expand_implicit_dependencies(workflow: &mut Workflow) {
    for node in &mut workflow.nodes {
        expand_implicit_dependencies_in(node);
    }
}

fn expand_implicit_dependencies_in(node: &mut Node) {
    if let NodeKind::Parallel { nodes, .. } = &mut node.kind {
        for child in nodes {
            expand_implicit_dependencies_in(child);
        }
    }
    // Every artifact a node names of another node orders it behind that
    // node: a context source reads it, a mount copies it into a child
    // born after it, a gate shows it to the person deciding. A node-less
    // reference reads this run's own artifacts — no producer to order
    // behind.
    let implied: Vec<NodeId> = super::reads::artifact_reads(node)
        .into_iter()
        .filter_map(|read| read.node.cloned())
        .collect();
    for referenced in implied {
        if !node.depends_on.contains(&referenced) {
            node.depends_on.push(referenced);
        }
    }
}
