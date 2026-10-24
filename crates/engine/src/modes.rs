//! What a run's mode does to the workflow graph: which top-level nodes it
//! schedules, which nodes it reaches, and what each scheduled node waits on
//! once the excluded nodes drop out. The derivations are pure and shared by
//! the scheduler, the escalation paths, `status` and the refusals a run
//! meets before it starts, so every reader of a moded run sees the same
//! graph.
//!
//! A mode is the workflow's: `modes:` declares as many as its author
//! wants, under any names, in the order a run may be promoted along, and
//! the engine names none. A mode chooses how much deliberation a run gets,
//! never how much verification — it keeps every `invariant` node, and what
//! the nodes it keeps re-route to or read from, rules the workflow is held
//! to where it is read. A node a mode leaves out is never scheduled, and a
//! node that waited on it waits on what it waited on, so cutting a node
//! out keeps the order of the work that remains. A node the project
//! cannot run, declared `optional`, drops out the same way. The mode is
//! chosen when the run is created and recorded on `run_created`; a run
//! never changes it.

use std::collections::{HashMap, HashSet};

use yunta_core::{LeftOut, ModeInclude, ModeName, Node, NodeId, Workflow};

/// The top-level node ids a run schedules: what its mode includes, less
/// what it left out because its config lacks what they need. `None` when
/// nothing narrows the graph at all.
pub fn included_nodes(
    workflow: &Workflow,
    mode_name: &ModeName,
    left_out: &[LeftOut],
) -> Option<HashSet<NodeId>> {
    let by_mode = mode_included_nodes(workflow, mode_name);
    if left_out.is_empty() {
        return by_mode;
    }
    let every = || workflow.nodes.iter().map(|node| node.id.clone()).collect();
    let mut included: HashSet<NodeId> = by_mode.unwrap_or_else(every);
    for gone in left_out {
        included.remove(&gone.node);
    }
    Some(included)
}

/// Every node a run in `mode_name` reaches: the top-level nodes its mode
/// includes, less what it left out, each with every node nested in it.
pub fn reached_nodes<'a>(
    workflow: &'a Workflow,
    mode_name: &ModeName,
    left_out: &[LeftOut],
) -> Vec<&'a Node> {
    let included = included_nodes(workflow, mode_name, left_out);
    workflow.iter_nodes_under(included.as_ref()).collect()
}

/// The top-level node ids `mode_name` makes schedulable, or `None` when
/// nothing narrows the graph — no `modes:` declared at all, or the
/// resolved mode's own `include: all`. A name with no matching entry in
/// `modes:` never reaches this function — `create_run` already refused
/// it before the run existed.
pub fn mode_included_nodes(workflow: &Workflow, mode_name: &ModeName) -> Option<HashSet<NodeId>> {
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
        yunta_core::yaml::parse(
            r#"
name: chain
modes:
  alpha: { include: [start, ship, audit] }
  beta:  { include: all }
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
        let included = mode_included_nodes(&workflow, &"alpha".into());
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
    fn a_mode_reaches_the_children_of_a_group_it_includes_and_nothing_it_leaves_out() {
        let workflow: Workflow = yunta_core::yaml::parse(
            r#"
name: grouped
modes:
  alpha: { include: [group] }
nodes:
  - id: group
    kind: parallel
    nodes:
      - { id: left, kind: bash, run: "true" }
      - { id: right, kind: bash, run: "true" }
  - { id: after, kind: bash, run: "true", depends_on: [group] }
"#,
        )
        .unwrap();
        let reached: Vec<&str> = reached_nodes(&workflow, &"alpha".into(), &[])
            .into_iter()
            .map(|node| node.id.as_str())
            .collect();
        assert_eq!(reached, ["group", "left", "right"]);
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
