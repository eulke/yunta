//! See [`super`]. Where what a node reads comes from: the tasks document
//! a loop works through, an artifact a context source, a mount or an
//! external gate names, the output a `node-output:` source reads.
//!
//! A reference nothing in the run can answer stops the node that makes
//! it every time it runs, so each is refused here. A source that exists
//! but might run after its reader is the run's to find out: that is a
//! question of order, not of whether the answer can exist at all.

use super::*;
use yunta_core::events::ArtifactId;
use yunta_core::{ArtifactKind, ArtifactRefId, ContextSpec, ModeName, MountSpec};

/// A read nothing in the run can answer: what it asks for, and who asks.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Unanswerable {
    /// A loop works through the run's tasks document, and nothing in
    /// the graph a run of this workflow can be gives it one.
    #[error(
        "loop `{node}` works through the run's tasks document, and nothing{} gives it one — \
         declare `produces: [tasks]` on a node that runs before it, declare a `type: document` \
         input of `kind: tasks`, or mount one from the workflow that composes this one",
        in_mode(.mode)
    )]
    LoopWithoutTasks {
        node: NodeId,
        mode: Option<yunta_core::ModeName>,
    },

    /// A reference that names the node it reads from reaches that
    /// node's artifacts and nothing else, and the node does not declare
    /// this one.
    #[error(
        "{site} reads the {} of node `{node}`, which does not declare it in \
         `artifacts.produces` — declare it on `{node}`, or read it from the node that produces it",
        .artifact.label()
    )]
    ArtifactNotDeclared {
        site: String,
        node: NodeId,
        artifact: yunta_core::events::ArtifactId,
    },

    /// A reference that asks the run for an artifact, whichever node
    /// produced it, and nothing in the run can hold one.
    #[error(
        "{site} reads the run's {}, and nothing{} can hold one: no node declares it, no \
         input brings it and no workflow composing this one mounts it — declare it on the node \
         that writes it",
        .artifact.label(),
        in_mode(.mode)
    )]
    ArtifactFromNowhere {
        site: String,
        artifact: yunta_core::events::ArtifactId,
        mode: Option<yunta_core::ModeName>,
    },

    /// A run started in a mode reads from a node the mode leaves out:
    /// only a run promoted into the mode would hold what that node made.
    #[error(
        "{site} reads from `{from}`, which mode `{mode}` leaves out — a run started in \
         `{mode}` holds nothing of it; start an earlier mode and promote, or include `{from}`"
    )]
    SourceLeftOut {
        site: String,
        from: NodeId,
        mode: ModeName,
    },

    /// Only a `kind: bash` node captures the output a `node-output:`
    /// source reads.
    #[error(
        "node `{node}` reads the output of `{referenced}`, {} — only a `kind: bash` node \
         captures output",
        match .kind {
            Some(kind) => format!("a `kind: {kind}` node"),
            None => "which the workflow does not declare".to_string(),
        }
    )]
    NodeOutputOfNonBash {
        node: NodeId,
        referenced: NodeId,
        kind: Option<&'static str>,
    },
}

/// How a refusal that holds in one mode names it: nothing when the
/// workflow declares no modes, since then there is one graph to be.
fn in_mode(mode: &Option<ModeName>) -> String {
    mode.as_ref()
        .map(|mode| format!(" in mode `{mode}`"))
        .unwrap_or_default()
}

/// What a run holds before any node of its own runs: the documents its
/// inputs bring, and what the parent that composed it mounts.
pub(crate) struct Birth {
    held: Vec<ArtifactId>,
}

impl Birth {
    /// The birth of a run of `workflow`, given `mounts` by the node that
    /// composed it — none for a run a person starts.
    pub(crate) fn of(workflow: &Workflow, mounts: &[MountSpec]) -> Self {
        let documents = workflow.inputs.values().filter_map(|spec| match spec {
            InputSpec::Document { kind, .. } => Some(ArtifactId::Interpreted { kind: *kind }),
            _ => None,
        });
        // A mount renamed with `as:` is held under its new name, which is
        // the only name that identifies an opaque artifact.
        let mounted = mounts.iter().map(|mount| match &mount.artifact.rename {
            Some(name) => ArtifactId::Opaque { name: name.clone() },
            None => ArtifactId::from(&mount.artifact.id),
        });
        Birth {
            held: documents.chain(mounted).collect(),
        }
    }

    fn holds(&self, wanted: &ArtifactId) -> bool {
        self.held.contains(wanted)
    }
}

/// One graph a run of this workflow can be: the mode it runs, every node
/// that mode keeps, `parallel` children included, and every node the
/// modes declared before it keep — what a run promoted into this mode is
/// born holding the work of. A workflow that declares modes always runs
/// one of them; one that declares none has one variant, the whole of it.
struct Variant<'a> {
    mode: Option<&'a ModeName>,
    nodes: Vec<&'a Node>,
    earlier: Vec<&'a Node>,
}

fn variants(workflow: &Workflow) -> Vec<Variant<'_>> {
    let keeps = |included: Option<&HashSet<NodeId>>| {
        workflow
            .iter_nodes_with_group()
            .filter(|(node, group)| {
                let top = group.map_or(&node.id, |group| &group.id);
                included.is_none_or(|set| set.contains(top))
            })
            .map(|(node, _)| node)
            .collect::<Vec<_>>()
    };
    let Some(modes) = &workflow.modes else {
        return vec![Variant {
            mode: None,
            nodes: keeps(None),
            earlier: Vec::new(),
        }];
    };
    let mut earlier: Vec<&Node> = Vec::new();
    let mut found = Vec::new();
    for mode in modes.keys() {
        let nodes = keeps(crate::modes::mode_included_nodes(workflow, mode).as_ref());
        found.push(Variant {
            mode: Some(mode),
            nodes: nodes.clone(),
            earlier: earlier.clone(),
        });
        for node in nodes {
            if !earlier.iter().any(|kept| kept.id == node.id) {
                earlier.push(node);
            }
        }
    }
    found
}

/// Where a read is answered from, in one variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reach {
    /// The run's birth, or a node the mode keeps.
    Here,
    /// Only a node an earlier mode keeps: a run promoted into this mode
    /// holds it, one started in it does not.
    Promoted,
    Nowhere,
}

/// What the read rules found: the reads nothing can answer, and the
/// ones only a promotion into the mode answers.
#[derive(Default)]
pub(crate) struct Reads {
    pub(crate) errors: Vec<CheckError>,
    pub(crate) warnings: Vec<CheckWarning>,
}

/// Every read of `workflow` whose answer does not depend on order: a
/// loop's tasks document, an artifact asked of the run, and, in each
/// declared mode, a node read from by name. `start` is the mode a run is
/// about to be started in, fresh: it holds nothing an earlier mode made,
/// so what only a promotion would answer is refused for it.
pub(crate) fn check_reads(workflow: &Workflow, birth: &Birth, start: Option<&ModeName>) -> Reads {
    let mut found = Reads::default();
    let tasks = ArtifactId::Interpreted {
        kind: ArtifactKind::Tasks,
    };
    for mut variant in variants(workflow) {
        if let Some(start) = start {
            if variant.mode != Some(start) {
                continue;
            }
            variant.earlier.clear();
        }
        let mode = variant.mode.cloned();
        for node in &variant.nodes {
            // A loop reads the run's tasks document before its first
            // session, and cannot be its own source: what it declares
            // lands at its close.
            let loop_tasks = matches!(node.kind, NodeKind::Loop { .. })
                .then(|| (format!("loop `{}`", node.id), tasks.clone()));
            for (site, wanted) in loop_tasks.into_iter().chain(run_references(node)) {
                match reach(&wanted, node, &variant, birth) {
                    Reach::Here => {}
                    Reach::Promoted => {
                        found.warnings.push(CheckWarning::ReadOnlyThroughPromotion {
                            site,
                            what: format!("the run's {}", wanted.label()),
                            mode: mode.clone().unwrap_or_default(),
                        })
                    }
                    Reach::Nowhere
                        if matches!(node.kind, NodeKind::Loop { .. }) && wanted == tasks =>
                    {
                        found
                            .errors
                            .push(CheckError::from(Unanswerable::LoopWithoutTasks {
                                node: node.id.clone(),
                                mode: mode.clone(),
                            }))
                    }
                    Reach::Nowhere => {
                        found
                            .errors
                            .push(CheckError::from(Unanswerable::ArtifactFromNowhere {
                                site,
                                artifact: wanted,
                                mode: mode.clone(),
                            }))
                    }
                }
            }
            // A source read by name that the mode leaves out and no
            // earlier mode keeps is refused where the workflow is read.
            let Some(mode) = &mode else { continue };
            for (field, source) in yunta_core::workflow::read::read_sources(node) {
                let kept = |nodes: &[&Node]| nodes.iter().any(|kept| kept.id == *source);
                if kept(&variant.nodes) {
                    continue;
                }
                let site = format!("`{field}` of node `{}`", node.id);
                if kept(&variant.earlier) {
                    found.warnings.push(CheckWarning::ReadOnlyThroughPromotion {
                        site,
                        what: format!("from `{source}`"),
                        mode: mode.clone(),
                    });
                } else if start.is_some() {
                    found
                        .errors
                        .push(CheckError::from(Unanswerable::SourceLeftOut {
                            site,
                            from: source.clone(),
                            mode: mode.clone(),
                        }));
                }
            }
        }
    }
    found
}

/// `(site, identity)` for every reference `node` makes to the run's
/// artifact of an identity, whichever node produced it.
fn run_references(node: &Node) -> Vec<(String, ArtifactId)> {
    let mut references = Vec::new();
    for source in &node.context {
        if let ContextSpec::Artifact { artifact } = source {
            if artifact.node.is_none() && literal_ref(&artifact.id) {
                references.push((
                    format!("the `artifact:` context source of node `{}`", node.id),
                    ArtifactId::from(&artifact.id),
                ));
            }
        }
    }
    if let NodeKind::Gate {
        external: Some(external),
        ..
    } = &node.kind
    {
        for spec in external.artifacts.iter().filter(|spec| literal_spec(spec)) {
            references.push((
                format!("the external gate of node `{}`", node.id),
                ArtifactId::from(spec),
            ));
        }
    }
    references
}

/// Where `wanted` can come from for `reader` in `variant`: the run's
/// birth, a node other than the reader that declares it, a `kind:
/// workflow` node, which may hand anything over from its child, or — for
/// answers — a node that asks.
fn reach(wanted: &ArtifactId, reader: &Node, variant: &Variant<'_>, birth: &Birth) -> Reach {
    let answers = |nodes: &[&Node]| {
        nodes
            .iter()
            .filter(|node| node.id != reader.id)
            .any(|node| {
                matches!(node.kind, NodeKind::Workflow { .. })
                    || declares(node, wanted)
                    || (*wanted
                        == ArtifactId::Interpreted {
                            kind: ArtifactKind::Answers,
                        }
                        && node.asks())
            })
    };
    if birth.holds(wanted) || answers(&variant.nodes) {
        Reach::Here
    } else if answers(&variant.earlier) {
        Reach::Promoted
    } else {
        Reach::Nowhere
    }
}

/// Every reference that names the node it reads from names one that
/// declares the artifact.
///
/// A `kind: workflow` node's artifacts are its child's, which this does
/// not read; answers are the engine's to write, and are held to the node
/// asking instead; a name built from a template is only known once the
/// run renders it.
pub(crate) fn check_named_artifact_sources(workflow: &Workflow, errors: &mut Vec<CheckError>) {
    let by_id: HashMap<&NodeId, &Node> = workflow.iter_nodes().map(|n| (&n.id, n)).collect();
    for node in workflow.iter_nodes() {
        for (site, named, id) in named_references(node) {
            if !literal_ref(id)
                || matches!(id, ArtifactRefId::Kind { kind } if *kind == ArtifactKind::Answers)
            {
                continue;
            }
            let Some(producer) = by_id.get(named) else {
                continue;
            };
            if matches!(producer.kind, NodeKind::Workflow { .. }) {
                continue;
            }
            let wanted = ArtifactId::from(id);
            let templated = producer
                .artifacts
                .iter()
                .flat_map(|artifacts| &artifacts.produces)
                .any(|spec| !literal_spec(spec));
            if !templated && !declares(producer, &wanted) {
                errors.push(CheckError::from(Unanswerable::ArtifactNotDeclared {
                    site,
                    node: named.clone(),
                    artifact: wanted,
                }));
            }
        }
    }
}

/// `(site, node, identity)` for every reference `node` makes that names
/// the node it reads from.
fn named_references(node: &Node) -> Vec<(String, &NodeId, &ArtifactRefId)> {
    let mut references = Vec::new();
    for source in &node.context {
        if let ContextSpec::Artifact { artifact } = source {
            if let Some(named) = &artifact.node {
                references.push((
                    format!("the `artifact:` context source of node `{}`", node.id),
                    named,
                    &artifact.id,
                ));
            }
        }
    }
    if let NodeKind::Workflow { mounts, .. } = &node.kind {
        for mount in mounts {
            references.push((
                format!("a `mounts:` entry of node `{}`", node.id),
                &mount.artifact.node,
                &mount.artifact.id,
            ));
        }
    }
    references
}

/// A `node-output:` source reads what a `kind: bash` node captured, and
/// no other kind captures any.
pub(crate) fn check_node_outputs(workflow: &Workflow, errors: &mut Vec<CheckError>) {
    let by_id: HashMap<&NodeId, &Node> = workflow.iter_nodes().map(|n| (&n.id, n)).collect();
    for node in workflow.iter_nodes() {
        for source in &node.context {
            let ContextSpec::NodeOutput { node_output } = source else {
                continue;
            };
            let referenced = &node_output.node;
            let kind = by_id.get(referenced).map(|found| found.kind.kind_name());
            if kind != Some("bash") {
                errors.push(CheckError::from(Unanswerable::NodeOutputOfNonBash {
                    node: node.id.clone(),
                    referenced: referenced.clone(),
                    kind,
                }));
            }
        }
    }
}

fn declares(node: &Node, wanted: &ArtifactId) -> bool {
    node.artifacts
        .iter()
        .flat_map(|artifacts| &artifacts.produces)
        .any(|spec| ArtifactId::from(spec) == *wanted)
}

/// Whether a reference names its artifact as written, rather than with a
/// template only the run renders.
fn literal_ref(id: &ArtifactRefId) -> bool {
    match id {
        ArtifactRefId::Kind { .. } => true,
        ArtifactRefId::Name { name } => literal(name),
    }
}

fn literal_spec(spec: &ArtifactSpec) -> bool {
    match spec {
        ArtifactSpec::Interpreted(_) => true,
        ArtifactSpec::Opaque(name) => literal(name),
    }
}

fn literal(text: &str) -> bool {
    template_variables(text).is_ok_and(|vars| vars.is_empty())
}

/// A `kind: answers` reference reaches the answers of a node that asks.
///
/// The engine writes the answers when a person replies to a `questions`
/// document, so a node that never asks has none — a reference to its
/// answers resolves to nothing at run time, and says so here instead,
/// where the workflow is all it takes to know.
pub(crate) fn check_answer_sources(workflow: &Workflow, errors: &mut Vec<CheckError>) {
    let asks = |named: &yunta_core::NodeId| {
        workflow
            .iter_nodes()
            .any(|node| node.id == *named && node.asks())
    };
    let mut answers_of = |site: String, named: Option<&yunta_core::NodeId>, id: &ArtifactRefId| {
        let ArtifactRefId::Kind { kind } = id else {
            return;
        };
        if *kind != yunta_core::ArtifactKind::Answers {
            return;
        }
        if let Some(named) = named.filter(|named| !asks(named)) {
            errors.push(CheckError::AnswersFromNodeThatNeverAsks {
                node: named.clone(),
                site,
            });
        }
    };
    for node in workflow.iter_nodes() {
        for source in &node.context {
            if let yunta_core::ContextSpec::Artifact { artifact } = source {
                answers_of(
                    format!("the `artifact:` context source of node `{}`", node.id),
                    artifact.node.as_ref(),
                    &artifact.id,
                );
            }
        }
        if let yunta_core::NodeKind::Workflow { mounts, .. } = &node.kind {
            for mount in mounts {
                answers_of(
                    format!("a `mounts:` entry of node `{}`", node.id),
                    Some(&mount.artifact.node),
                    &mount.artifact.id,
                );
            }
        }
    }
}
