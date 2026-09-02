//! What a run's mode does to the workflow graph: which top-level nodes it
//! schedules, and what each of them waits on once the excluded nodes drop
//! out. Both derivations are pure and shared by the scheduler, the
//! escalation paths and `status`, so every reader of a moded run sees the
//! same graph.

use std::collections::{HashMap, HashSet};

use yunta_core::{ModeInclude, Node, NodeId, Workflow};

/// The top-level node ids `mode_name` makes schedulable, or `None` when
/// nothing narrows the graph — no `modes:` declared at all, or the
/// resolved mode's own `include: all`. A name with no matching entry in
/// `modes:` never reaches this function — `create_run` already refused
/// it before the run existed.
pub fn mode_included_nodes(workflow: &Workflow, mode_name: &str) -> Option<HashSet<NodeId>> {
    let spec = workflow.modes.as_ref()?.get(mode_name)?;
    match &spec.include {
        ModeInclude::All => None,
        ModeInclude::Nodes(ids) => Some(ids.iter().cloned().collect()),
    }
}

/// The dependencies each included top-level node waits on under a mode.
///
/// An included dependency stays as declared. An excluded one is replaced
/// by that node's own dependencies, transitively, so a mode that cuts a
/// node out of a chain keeps the chain's order instead of snapping the
/// edge: with `approve-plan` excluded, `implement` waits for `plan`, the
/// node `approve-plan` itself waited for. Each list keeps declaration
/// order without duplicates; a node the mode excludes has no entry.
/// `included: None` (nothing narrows the graph) returns every node's
/// declared list verbatim.
///
/// A `depends_on` cycle ends the walk where it closes rather than
/// recursing forever; `check` rejects such a workflow before a run
/// exists, so this only matters for a graph `check` never saw.
pub fn dependencies_in_mode(
    workflow: &Workflow,
    included: Option<&HashSet<NodeId>>,
) -> HashMap<NodeId, Vec<NodeId>> {
    let by_id: HashMap<&NodeId, &Node> = workflow.nodes.iter().map(|n| (&n.id, n)).collect();
    let is_included = |id: &NodeId| included.is_none_or(|set| set.contains(id));
    let mut memo: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    for node in workflow.nodes.iter().filter(|n| is_included(&n.id)) {
        effective_dependencies(&node.id, &by_id, &is_included, &mut memo, &mut Vec::new());
    }
    memo.retain(|id, _| is_included(id));
    memo
}

fn effective_dependencies(
    id: &NodeId,
    by_id: &HashMap<&NodeId, &Node>,
    is_included: &dyn Fn(&NodeId) -> bool,
    memo: &mut HashMap<NodeId, Vec<NodeId>>,
    visiting: &mut Vec<NodeId>,
) -> Vec<NodeId> {
    if let Some(deps) = memo.get(id) {
        return deps.clone();
    }
    if visiting.contains(id) {
        return Vec::new();
    }
    visiting.push(id.clone());
    let mut out: Vec<NodeId> = Vec::new();
    if let Some(node) = by_id.get(id) {
        for dep in &node.depends_on {
            if is_included(dep) {
                push_unique(&mut out, dep.clone());
            } else {
                for inherited in effective_dependencies(dep, by_id, is_included, memo, visiting) {
                    push_unique(&mut out, inherited);
                }
            }
        }
    }
    visiting.pop();
    memo.insert(id.clone(), out.clone());
    out
}

fn push_unique(list: &mut Vec<NodeId>, id: NodeId) {
    if !list.contains(&id) {
        list.push(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workflow() -> Workflow {
        serde_yaml::from_str(
            r#"
name: chain
modes:
  quick: { include: [start, ship, audit] }
  full:  { include: all }
nodes:
  - { id: start, kind: bash, run: "true" }
  - { id: review, kind: bash, run: "true", depends_on: [start] }
  - { id: extra, kind: bash, run: "true", depends_on: [review, start] }
  - { id: ship, kind: bash, run: "true", depends_on: [extra] }
  - { id: audit, kind: bash, run: "true", depends_on: [review] }
"#,
        )
        .unwrap()
    }

    fn ids(list: &[&str]) -> Vec<NodeId> {
        list.iter().map(|id| NodeId::from(*id)).collect()
    }

    #[test]
    fn an_excluded_dependency_is_replaced_by_its_own_included_dependencies_transitively() {
        let workflow = workflow();
        let included = mode_included_nodes(&workflow, "quick");
        let deps = dependencies_in_mode(&workflow, included.as_ref());
        // `ship` -> `extra` (excluded) -> `review` (excluded) -> `start`, plus
        // `extra`'s own direct `start`, which collapses into one entry.
        assert_eq!(deps[&NodeId::from("ship")], ids(&["start"]));
        assert_eq!(deps[&NodeId::from("audit")], ids(&["start"]));
        assert_eq!(deps[&NodeId::from("start")], ids(&[]));
        assert!(
            !deps.contains_key(&NodeId::from("extra")),
            "an excluded node has no entry of its own"
        );
    }

    #[test]
    fn without_a_narrowing_mode_every_declared_list_is_returned_verbatim() {
        let workflow = workflow();
        let deps = dependencies_in_mode(&workflow, None);
        assert_eq!(deps[&NodeId::from("extra")], ids(&["review", "start"]));
        assert_eq!(deps[&NodeId::from("ship")], ids(&["extra"]));
        assert_eq!(deps.len(), workflow.nodes.len());
    }
}
