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
use yunta_core::workflow::reads::{artifact_reads, ReadSite};
use yunta_core::{ArtifactKind, ArtifactRefId, ContextSpec, ModeName, MountSpec};

/// A read nothing in the run can answer: what it asks for, and who asks.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Unanswerable {
    /// A loop works through the run's tasks document, and nothing in
    /// the graph a run of this workflow can be gives it one.
    #[error("unanswerable_loop_without_tasks")]
    LoopWithoutTasks {
        node: NodeId,
        mode: Option<yunta_core::ModeName>,
    },

    /// A reference that names the node it reads from reaches that
    /// node's artifacts and nothing else, and the node does not declare
    /// this one.
    #[error("unanswerable_artifact_not_declared")]
    ArtifactNotDeclared {
        place: Place,
        node: NodeId,
        artifact: yunta_core::events::ArtifactId,
    },

    /// A reference that asks the run for an artifact, whichever node
    /// produced it, and nothing in the run can hold one.
    #[error("unanswerable_artifact_from_nowhere")]
    ArtifactFromNowhere {
        place: Place,
        artifact: yunta_core::events::ArtifactId,
        mode: Option<yunta_core::ModeName>,
    },

    /// A run started in a mode reads from a node the mode leaves out:
    /// only a run promoted into the mode would hold what that node made.
    #[error("unanswerable_source_left_out")]
    SourceLeftOut {
        place: Place,
        from: NodeId,
        mode: ModeName,
    },

    /// Only a `kind: bash` node captures the output a `node-output:`
    /// source reads.
    #[error("unanswerable_node_output_of_non_bash")]
    NodeOutputOfNonBash {
        node: NodeId,
        referenced: NodeId,
        kind: Option<&'static str>,
    },
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
        workflow.iter_nodes_under(included).collect::<Vec<_>>()
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
    pub(crate) warnings: Vec<Caution>,
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
            let loop_tasks = matches!(node.kind, NodeKind::Loop { .. }).then(|| {
                (
                    Place::LoopTasks {
                        node: node.id.clone(),
                    },
                    tasks.clone(),
                )
            });
            for (place, wanted) in loop_tasks.into_iter().chain(run_references(node)) {
                match reach(&wanted, node, &variant, birth) {
                    Reach::Here => {}
                    Reach::Promoted => found.warnings.push(Caution::ReadOnlyThroughPromotion {
                        place,
                        what: PromotedRead::Artifact(wanted.clone()),
                        mode: mode.clone().unwrap_or_default(),
                    }),
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
                                place,
                                artifact: wanted,
                                mode: mode.clone(),
                            }))
                    }
                }
            }
            // A source read by name that the mode leaves out and no
            // earlier mode keeps is refused where the workflow is read.
            let Some(mode) = &mode else { continue };
            for (field, source) in yunta_core::workflow::reads::read_sources(node) {
                let kept = |nodes: &[&Node]| nodes.iter().any(|kept| kept.id == *source);
                if kept(&variant.nodes) {
                    continue;
                }
                let place = Place::Field {
                    node: node.id.clone(),
                    field: field.to_string(),
                };
                if kept(&variant.earlier) {
                    found.warnings.push(Caution::ReadOnlyThroughPromotion {
                        place,
                        what: PromotedRead::Source(source.clone()),
                        mode: mode.clone(),
                    });
                } else if start.is_some() {
                    found
                        .errors
                        .push(CheckError::from(Unanswerable::SourceLeftOut {
                            place,
                            from: source.clone(),
                            mode: mode.clone(),
                        }));
                }
            }
        }
    }
    found
}

/// `(place, identity)` for every reference `node` makes to the run's
/// artifact of an identity, whichever node produced it.
fn run_references(node: &Node) -> Vec<(Place, ArtifactId)> {
    let mut references: Vec<(Place, ArtifactId)> = artifact_reads(node)
        .into_iter()
        .filter(|read| read.node.is_none() && literal_ref(read.id))
        // A gate showing the run's findings shows a view the log
        // derives, whatever node did or did not report one.
        .filter(|read| !(read.site == ReadSite::Shows && is_findings(read.id)))
        .map(|read| {
            let place = Place::Read {
                node: node.id.clone(),
                read: read.site,
            };
            (place, ArtifactId::from(read.id))
        })
        .collect();
    if let NodeKind::Gate {
        external: Some(external),
        ..
    } = &node.kind
    {
        for spec in external.artifacts.iter().filter(|spec| literal_spec(spec)) {
            references.push((
                Place::ExternalGate {
                    node: node.id.clone(),
                },
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
        for (place, named, id) in named_references(node) {
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
                    place,
                    node: named.clone(),
                    artifact: wanted,
                }));
            }
        }
    }
}

/// `(place, node, identity)` for every reference `node` makes that names
/// the node it reads from.
fn named_references(node: &Node) -> Vec<(Place, &NodeId, &ArtifactRefId)> {
    artifact_reads(node)
        .into_iter()
        .filter_map(|read| {
            read.node.map(|named| {
                let place = Place::Read {
                    node: node.id.clone(),
                    read: read.site,
                };
                (place, named, read.id)
            })
        })
        .collect()
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
/// Whether `id` names the `findings` kind.
fn is_findings(id: &ArtifactRefId) -> bool {
    ArtifactId::from(id)
        == ArtifactId::Interpreted {
            kind: ArtifactKind::Findings,
        }
}

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
    let mut answers_of = |place: Place, named: Option<&yunta_core::NodeId>, id: &ArtifactRefId| {
        let ArtifactRefId::Kind { kind } = id else {
            return;
        };
        if *kind != yunta_core::ArtifactKind::Answers {
            return;
        }
        if let Some(named) = named.filter(|named| !asks(named)) {
            errors.push(CheckError::AnswersFromNodeThatNeverAsks {
                node: named.clone(),
                place,
            });
        }
    };
    for node in workflow.iter_nodes() {
        for read in artifact_reads(node) {
            let place = Place::Read {
                node: node.id.clone(),
                read: read.site,
            };
            answers_of(place, read.node, read.id);
        }
    }
}
