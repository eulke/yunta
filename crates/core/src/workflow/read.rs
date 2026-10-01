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

pub use super::expand::{expand_implicit_dependencies, expand_runner_fanout};
use crate::{ModeInclude, Node, NodeId, NodeKind, Workflow};

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
                 keeps reroute to or read from",
    },
    crate::diagnostic::Rule {
        code: RuleCode::InvariantInParallel,
        demand: "`invariant: true` is declared on a top-level node, never on a child of a \
                 `parallel` group",
    },
    crate::diagnostic::Rule {
        code: RuleCode::IncoherentOptional,
        demand: "`optional: true` is declared on a top-level node, and no node a run keeps \
                 re-routes to an optional node or reads from one — but the node only an \
                 optional one leads to, which goes with it",
    },
];

/// The workflow `bytes` declare, or every problem the file has.
///
/// `path` is where a reader opens the file, so a report names the file
/// somebody has to fix. The workflow that comes back is the expanded
/// one — the graph a run would build — because that is the graph the
/// rules are about and the graph every reader needs.
pub fn read(bytes: &str, path: &Path) -> Result<Workflow, Report> {
    match read_all(bytes, path) {
        (Some(workflow), report) if report.is_empty() => Ok(workflow),
        (_, report) => Err(report),
    }
}

/// Every problem the file has, in the order the file has them, and the
/// workflow it declares once the keys nothing reads are taken out —
/// when what is left reads at all.
///
/// For a reader that reports every problem a person has to fix in one
/// round, `yunta check` among them: it judges the rest of what the file
/// says as the person meant it. A run reads with [`read`], which refuses
/// a file with any problem.
pub fn read_all(bytes: &str, path: &Path) -> (Option<Workflow>, Report) {
    let document = DocumentRef::new(DocumentKind::Workflow, path.display().to_string());
    let finish = |broken: Vec<Diagnostic>| {
        let mut report = Report::new(document.clone(), broken).located(bytes);
        report.diagnostics.sort_by_key(|diagnostic| {
            diagnostic
                .at
                .map_or((usize::MAX, 0), |at| (at.line, at.col))
        });
        report
    };
    let document_value = match crate::yaml::parse::<crate::yaml::Value>(bytes) {
        Ok(value) => value,
        Err(error) => return (None, finish(vec![unread(error)])),
    };
    let audit = super::audit::audit(document_value, bytes);
    let mut broken = audit.broken;
    // Read from the text itself when nothing was taken out of it, so a
    // file with nothing wrong reads exactly as the parser reads it.
    let read = match broken.is_empty() {
        true => crate::yaml::parse::<Workflow>(bytes),
        false => crate::yaml::from_value::<Workflow>(audit.repaired),
    };
    let workflow = match read {
        Ok(workflow) => Some(workflow),
        Err(error) => {
            broken.push(unread(error));
            None
        }
    };
    let workflow = workflow.and_then(|mut workflow| {
        // This is the authored frontier. A persisted manifest reads the
        // same Node type after fan-out and legitimately contains `@`.
        let authored = authored_fan_out(&workflow);
        if !authored.is_empty() {
            broken.extend(authored);
            return None;
        }
        // Fan-out declarations are about the shape as written, so they
        // are read before the expansion multiplies them; every rule after
        // sees the graph that will actually run.
        expand_runner_fanout(&mut workflow);
        expand_implicit_dependencies(&mut workflow);
        broken.extend(check(&workflow));
        Some(workflow)
    });
    (workflow, finish(broken))
}

/// A refusal of the parser's, as a problem the document has.
fn unread(error: crate::yaml::YamlError) -> Diagnostic {
    let (path, message, at) = match error {
        crate::yaml::YamlError::Parse { path, message, at } => (path, message, at),
        other => (String::new(), other.to_string(), None),
    };
    Diagnostic::new(Subject::Document, Problem::parse(path, message)).at(at)
}

/// Every authored node id that spells a fan-out sibling's: `@` is
/// reserved for the ids `runners:` generates.
fn authored_fan_out(workflow: &Workflow) -> Vec<Diagnostic> {
    workflow
        .iter_nodes()
        .enumerate()
        .filter(|(_, node)| node.id.is_fan_out())
        .map(|(index, node)| {
            Diagnostic::new(
                Subject::Node(Named::new(node.id.clone(), index)),
                Problem::parse(
                    "id".to_string(),
                    "`@` is reserved for fan-out siblings generated from `runners:`".to_string(),
                ),
            )
        })
        .collect()
}

/// Every rule the file alone decides, collected rather than stopped at
/// the first — whoever writes a workflow by hand corrects once.
fn check(workflow: &Workflow) -> Vec<Diagnostic> {
    let declared = declared_once(workflow);
    let mut broken = declared.broken;
    broken.extend(references_reach(workflow, &declared.ids));
    broken.extend(parallel_scopes(&workflow.nodes));
    broken.extend(invariants_on_top(workflow));
    broken.extend(super::optional::may_go(workflow));
    broken.extend(modes_still_run(workflow));
    broken
}

/// An `invariant` is a verdict the run keeps for its whole graph: every
/// mode includes it, and it runs again, alone, when the tree it verified
/// moves. A child of a `parallel` group only ever runs with its group,
/// and modes name the group, so neither holds for it — a declaration the
/// run could not honor is refused rather than ignored.
fn invariants_on_top(workflow: &Workflow) -> Vec<Diagnostic> {
    workflow
        .iter_nodes_with_group()
        .enumerate()
        .filter_map(|(index, (node, group))| {
            let group = group.filter(|_| node.invariant)?;
            Some(about(
                index,
                &node.id,
                RuleCode::InvariantInParallel,
                format!(
                    "`{}` is a child of the `parallel` group `{}` and declares `invariant: \
                     true`, which only a top-level node can honor; move it out of the group",
                    node.id, group.id
                ),
            ))
        })
        .collect()
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
        let mut reaches = |field: &str, target: &NodeId, at: crate::yaml::Pointer| {
            if !ids.contains(target) {
                broken.push(
                    about(
                        index,
                        &node.id,
                        RuleCode::UnknownDependency,
                        format!(
                            "`{field}` names `{target}`, and no node carries that id{}",
                            crate::text::did_you_mean(
                                target.as_str(),
                                ids.iter().map(|id| id.as_str())
                            )
                        ),
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
                let key = read.site.field().split(':').next().unwrap_or_default();
                reaches(read.site.field(), producer, at().key(key));
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
                for glob_a in &a.scope.overlap_globs() {
                    for glob_b in &b.scope.overlap_globs() {
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
/// and keeps whatever the nodes it kept reroute to or read from by name.
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
    // A `parallel` child is in a mode exactly when its group is.
    let top_of: std::collections::HashMap<&NodeId, &NodeId> = workflow
        .iter_nodes_with_group()
        .map(|(node, group)| (&node.id, group.map_or(&node.id, |group| &group.id)))
        .collect();
    let invariants: Vec<&NodeId> = workflow
        .nodes
        .iter()
        .filter(|node| node.invariant)
        .map(|node| &node.id)
        .collect();
    let mut broken = Vec::new();
    // What the modes declared before this one keep: a run promoted into
    // a mode is born holding what its predecessor's nodes produced, so a
    // source one of them keeps is not missing from a mode that drops it.
    let mut earlier: HashSet<&NodeId> = HashSet::new();
    let mut earlier_all = false;
    for (mode, spec) in modes {
        let ModeInclude::Nodes(named) = &spec.include else {
            // `all` holds every invariant vacuously.
            earlier_all = true;
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
            // An unknown source is the unknown-reference rule's to name.
            for (field, source) in super::reads::read_sources(node) {
                if top_of.get(source).is_some_and(|top| {
                    !included.contains(*top) && !earlier_all && !earlier.contains(*top)
                }) {
                    fails(
                        index,
                        &node.id,
                        RuleCode::IncoherentMode,
                        format!(
                            "`{field}` reads from `{source}`, and mode `{mode}` leaves \
                             `{source}` out, as does every mode before it; a mode that keeps \
                             a node keeps what it reads from"
                        ),
                    );
                }
            }
        }
        earlier.extend(included);
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

pub(super) fn about(index: usize, id: &NodeId, code: RuleCode, detail: String) -> Diagnostic {
    Diagnostic::new(
        Subject::Node(Named::new(id.clone(), index)),
        Problem::rule(code, detail),
    )
}
