//! The one door onto a workflow: read from bytes and held to the rules
//! that are true of the file alone.
//!
//! A workflow that parsed used to be a workflow nobody had checked —
//! the graph rules lived in the engine's `check`, and eleven places
//! reached `Workflow` through the parser without them. So a run could
//! be created from a file with two nodes of one id, or a `depends_on`
//! naming nothing, and find out at replay.
//!
//! [`read`] is that door. It parses, expands what the file implies —
//! a fan-out into its siblings, a context reference into the edge it
//! is — and then asks the four questions the file alone can answer:
//! is every id declared once, does every reference reach something,
//! can two parallel children reach the same files, and does every mode
//! leave a graph that still runs. Everything else `yunta check` asks
//! needs the merged config or the adapters this binary built, and stays
//! there.

use std::collections::HashSet;
use std::path::Path;

use crate::diagnostic::{
    Diagnostic, DocumentKind, DocumentRef, Named, Problem, Report, RuleCode, Subject,
};
use crate::glob::might_overlap;
use crate::{ContextSpec, ModeInclude, Node, NodeId, NodeKind, Workflow};

/// Every rule a workflow is held to by the file alone, stated for
/// whoever writes one — the same list [`read`] enforces, read the
/// other way round.
pub const RULES: &[crate::diagnostic::Rule] = &[
    crate::diagnostic::Rule {
        code: RuleCode::DuplicateId,
        demand: "each node `id` is declared once, `parallel` children included",
    },
    crate::diagnostic::Rule {
        code: RuleCode::UnknownDependency,
        demand: "every reference to a node — `depends_on`, `on_failure.goto`, a gate option's \
                 `on`, a `context` artifact's producer, a `mounts` entry, a mode's `include` — \
                 names a node the workflow declares",
    },
    crate::diagnostic::Rule {
        code: RuleCode::OverlappingScope,
        demand: "two children of one `parallel` group declare scopes that cannot reach the same \
                 files",
    },
    crate::diagnostic::Rule {
        code: RuleCode::IncoherentMode,
        demand: "a declared mode keeps every `invariant` node, and keeps whatever the nodes it \
                 keeps reroute to",
    },
];

/// The workflow `bytes` declare, or every problem the file has.
///
/// `path` is where a reader opens the file, so a report names the file
/// somebody has to fix. The workflow that comes back is the expanded
/// one — the graph a run would build — because that is the graph the
/// rules are about and the graph every reader needs.
pub fn read(bytes: &str, path: &Path) -> Result<Workflow, Report> {
    let document = DocumentRef::new(DocumentKind::Workflow, path.display().to_string());
    let mut workflow: Workflow = crate::yaml::parse(bytes).map_err(|error| {
        let (path, message) = match error {
            crate::yaml::YamlError::Parse { path, message } => (path, message),
            other => (String::new(), other.to_string()),
        };
        Report::new(
            document.clone(),
            vec![Diagnostic::new(
                Subject::Document,
                Problem::parse(path, message),
            )],
        )
    })?;
    // Fan-out declarations are about the shape as written, so they are
    // read before the expansion multiplies them; every rule after sees
    // the graph that will actually run.
    expand_runner_fanout(&mut workflow);
    expand_implicit_dependencies(&mut workflow);

    let broken = check(&workflow);
    if broken.is_empty() {
        Ok(workflow)
    } else {
        Err(Report::new(document, broken))
    }
}

/// Every rule the file alone decides, collected rather than stopped at
/// the first — whoever writes a workflow by hand corrects once.
fn check(workflow: &Workflow) -> Vec<Diagnostic> {
    let declared = declared_once(workflow);
    let mut broken = declared.broken;
    broken.extend(references_reach(workflow, &declared.ids));
    broken.extend(parallel_scopes(&workflow.nodes));
    broken.extend(modes_still_run(workflow));
    broken
}

struct Declared<'a> {
    ids: HashSet<&'a NodeId>,
    broken: Vec<Diagnostic>,
}

/// Every id, and the ones declared twice.
///
/// Global rather than per group: replay derives node state from one flat
/// map of id to state, so a `parallel` child colliding with anything —
/// a sibling, a top-level node, another group's child — would corrupt
/// the derivation rather than merely read oddly.
fn declared_once(workflow: &Workflow) -> Declared<'_> {
    let mut ids = HashSet::new();
    let mut broken = Vec::new();
    for (index, node) in workflow.iter_nodes().enumerate() {
        if !ids.insert(&node.id) {
            broken.push(about(
                index,
                &node.id,
                RuleCode::DuplicateId,
                "another node already carries this id; every id is declared once, \
                 `parallel` children included"
                    .to_string(),
            ));
        }
    }
    Declared { ids, broken }
}

/// Every reference a node makes reaches a node the file declares.
fn references_reach(workflow: &Workflow, ids: &HashSet<&NodeId>) -> Vec<Diagnostic> {
    let mut broken = Vec::new();
    for (index, node) in workflow.nodes.iter().enumerate() {
        let mut reaches = |field: &str, target: &NodeId| {
            if !ids.contains(target) {
                broken.push(about(
                    index,
                    &node.id,
                    RuleCode::UnknownDependency,
                    format!("`{field}` names `{target}`, and no node carries that id"),
                ));
            }
        };
        for dep in &node.depends_on {
            reaches("depends_on", dep);
        }
        if let Some(on_failure) = &node.on_failure {
            reaches("on_failure.goto", &on_failure.goto);
        }
        if let NodeKind::Gate { on, .. } = &node.kind {
            for target in on.values() {
                reaches("on", target);
            }
        }
        for source in &node.context {
            if let ContextSpec::Artifact { artifact } = source {
                if let Some(producer) = &artifact.node {
                    reaches("context.artifact.node", producer);
                }
            }
        }
        if let NodeKind::Workflow { mounts, .. } = &node.kind {
            for mount in mounts {
                reaches("mounts", &mount.artifact.node);
            }
        }
    }
    broken
}

/// Two children of one `parallel` group never reach for the same files:
/// they run at once, in one tree, so an overlap is a race the run
/// cannot resolve.
fn parallel_scopes(nodes: &[Node]) -> Vec<Diagnostic> {
    let mut broken = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        let NodeKind::Parallel {
            nodes: children, ..
        } = &node.kind
        else {
            continue;
        };
        for (i, a) in children.iter().enumerate() {
            for b in children.iter().skip(i + 1) {
                for glob_a in &a.scope {
                    for glob_b in &b.scope {
                        if might_overlap(glob_a, glob_b) {
                            broken.push(about(
                                index,
                                &node.id,
                                RuleCode::OverlappingScope,
                                format!(
                                    "`{}` scopes `{glob_a}` and `{}` scopes `{glob_b}`, and the \
                                     two can reach the same files; they run at once, in one tree",
                                    a.id, b.id
                                ),
                            ));
                        }
                    }
                }
            }
        }
        broken.extend(parallel_scopes(children));
    }
    broken
}

/// Every declared mode leaves a graph that still runs: it names nodes
/// the file declares, keeps every node the workflow cannot run without,
/// and keeps whatever the nodes it kept reroute to.
///
/// A mode's `include:` names top-level nodes only — a `parallel` group
/// is in or out as a whole — so the ids it is read against are the
/// top-level ones, deliberately narrower than the set uniqueness is
/// checked over.
fn modes_still_run(workflow: &Workflow) -> Vec<Diagnostic> {
    let Some(modes) = &workflow.modes else {
        return Vec::new();
    };
    let top_level: HashSet<&NodeId> = workflow.nodes.iter().map(|node| &node.id).collect();
    let invariants: Vec<&NodeId> = workflow
        .nodes
        .iter()
        .filter(|node| node.invariant)
        .map(|node| &node.id)
        .collect();
    let mut broken = Vec::new();
    for (mode, spec) in modes {
        let ModeInclude::Nodes(named) = &spec.include else {
            // `all` holds every invariant vacuously.
            continue;
        };
        let included: HashSet<&NodeId> = named.iter().collect();
        let mut fails = |index: usize, id: &NodeId, code: RuleCode, detail: String| {
            broken.push(about(index, id, code, detail));
        };
        for (index, id) in named.iter().enumerate() {
            if !top_level.contains(id) {
                fails(
                    index,
                    id,
                    RuleCode::UnknownDependency,
                    format!("mode `{mode}` includes `{id}`, and no top-level node carries that id"),
                );
            }
        }
        for (index, id) in invariants.iter().enumerate() {
            if !included.contains(id) {
                fails(
                    index,
                    id,
                    RuleCode::IncoherentMode,
                    format!(
                        "this node is `invariant` and mode `{mode}` leaves it out; a mode chooses \
                         what to skip, never what the workflow cannot run without"
                    ),
                );
            }
        }
        for (index, node) in workflow
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| included.contains(&node.id))
        {
            for (field, target) in reroute_targets(node) {
                if !included.contains(target) {
                    fails(
                        index,
                        &node.id,
                        RuleCode::IncoherentMode,
                        format!(
                            "`{field}` sends control to `{target}`, and mode `{mode}` leaves \
                             `{target}` out; a mode that keeps a node keeps what it reroutes to"
                        ),
                    );
                }
            }
        }
    }
    broken
}

/// Where a node can send control: its re-route, and every option of a
/// gate. Both are the same class of reference, so both are read here.
fn reroute_targets(node: &Node) -> Vec<(&'static str, &NodeId)> {
    let mut targets = Vec::new();
    if let Some(on_failure) = &node.on_failure {
        targets.push(("on_failure.goto", &on_failure.goto));
    }
    if let NodeKind::Gate { on, .. } = &node.kind {
        targets.extend(on.values().map(|target| ("on", target)));
    }
    targets
}

fn about(index: usize, id: &NodeId, code: RuleCode, detail: String) -> Diagnostic {
    Diagnostic::new(
        Subject::Node(Named::new(id.clone(), index)),
        Problem::rule(code, detail),
    )
}

/// `context: [{ artifact: { node, name } }]` creates an *implicit*
/// `depends_on` edge onto `node` — folded into the ordinary field here,
/// once, so `check`'s cycle detection and the scheduler's own readiness
/// calculation (both already only ever read `Node.depends_on`) need zero
/// awareness of `context:` existing at all. [`read`] calls this on its own copy, so a cycle created purely
/// by two nodes' context-artifact references is still caught where the
/// file is read rather than deadlocking a real run. Idempotent: a
/// node that already lists the referenced node explicitly gets no
/// duplicate.
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
    // A mount is a read of the referenced node's outcome, so
    // it orders behind it exactly like a context artifact does — and
    // it's this edge that guarantees the source node has already
    // finished at the time the child is born and the copy happens.
    let mut implied: Vec<NodeId> = Vec::new();
    if let NodeKind::Workflow { mounts, .. } = &node.kind {
        implied.extend(mounts.iter().map(|mount| mount.artifact.node.clone()));
    }
    for spec in &node.context {
        if let crate::ContextSpec::Artifact { artifact } = spec {
            // A node-less reference reads this run's own
            // artifacts dir — no producer to order behind.
            if let Some(referenced) = &artifact.node {
                implied.push(referenced.clone());
            }
        }
    }
    for referenced in implied {
        if !node.depends_on.contains(&referenced) {
            node.depends_on.push(referenced);
        }
    }
}
