//! `optional: true` — a node a project may lack what it needs to run,
//! and what a run does without it.
//!
//! An optional node that needs a key the config leaves unset — a
//! command, a forge, a runner — is left out of the run the way a mode
//! leaves a node out: never scheduled, its dependents waiting on what it
//! waited on. A node only it leads to — the re-route that fixes what it
//! finds — goes with it. A required node that needs the same key is
//! refused before the run instead.

use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::read::about;
use super::{Node, NodeKind, Workflow};
use crate::config::{ConfigKey, ConfigLayer};
use crate::diagnostic::{Diagnostic, RuleCode};
use crate::ids::NodeId;

/// A top-level node a run leaves out, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LeftOut {
    pub node: NodeId,
    #[serde(flatten)]
    pub because: Because,
}

/// Why a node is left out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum Because {
    /// It is optional and needs these keys, which the config leaves
    /// unset — never empty.
    Lacks { lacks: Vec<ConfigKey> },
    /// Nothing but `through`, itself left out, leads to it.
    Through { through: NodeId },
}

impl fmt::Display for Because {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Because::Lacks { lacks } => {
                let lacking: Vec<String> = lacks.iter().map(ConfigKey::undeclared).collect();
                write!(f, "the project declares {}", lacking.join(", "))
            }
            Because::Through { through } => {
                write!(f, "only `{through}` leads to it, and it is not in this run")
            }
        }
    }
}

/// Every top-level node `workflow` leaves out under `config`: each
/// optional one that needs a key `config` leaves unset — itself or, for
/// a `parallel` group, any of its children — and then every node that
/// only a left-out one leads to.
pub fn left_out(workflow: &Workflow, config: &ConfigLayer) -> Vec<LeftOut> {
    let mut out: Vec<LeftOut> = workflow
        .nodes
        .iter()
        .filter(|node| node.optional)
        .filter_map(|node| {
            let lacks = lacking(node, config);
            (!lacks.is_empty()).then(|| LeftOut {
                node: node.id.clone(),
                because: Because::Lacks { lacks },
            })
        })
        .collect();
    let gone: HashSet<NodeId> = out.iter().map(|left| left.node.clone()).collect();
    out.extend(
        followers(workflow, gone)
            .into_iter()
            .map(|(node, through)| LeftOut {
                node,
                because: Because::Through { through },
            }),
    );
    out
}

/// The nodes that go with `gone`: each `(node, through)` where nothing
/// but `through`, already gone, leads to `node` — found until none is
/// left, since what goes can take another node with it.
pub fn followers(workflow: &Workflow, mut gone: HashSet<NodeId>) -> Vec<(NodeId, NodeId)> {
    let mut followers = Vec::new();
    while let Some((node, through)) = workflow
        .nodes
        .iter()
        .filter(|node| !gone.contains(&node.id))
        .find_map(|node| only_through(workflow, node, &gone).map(|t| (node.id.clone(), t)))
    {
        gone.insert(node.clone());
        followers.push((node, through));
    }
    followers
}

/// Every key `node` needs that `config` leaves unset, its `parallel`
/// children's included, each once.
fn lacking(node: &Node, config: &ConfigLayer) -> Vec<ConfigKey> {
    let mut keys = ConfigKey::unset(node, config);
    if let NodeKind::Parallel { nodes, .. } = &node.kind {
        for key in nodes
            .iter()
            .flat_map(|child| ConfigKey::unset(child, config))
        {
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
    }
    keys
}

/// The gone node that alone leads to `node`: it waits on nothing,
/// nothing waits on it, and every node that re-routes to it is gone.
/// `None` when anything else reaches it — a node nothing re-routes to
/// starts the run on its own.
fn only_through(workflow: &Workflow, node: &Node, gone: &HashSet<NodeId>) -> Option<NodeId> {
    let waited_on = workflow
        .nodes
        .iter()
        .any(|other| other.depends_on.contains(&node.id));
    if !node.depends_on.is_empty() || waited_on {
        return None;
    }
    let sources = reroute_sources(workflow, &node.id);
    let first = sources.first()?;
    sources
        .iter()
        .all(|source| gone.contains(*source))
        .then(|| (*first).clone())
}

/// The top-level nodes that re-route to `target` — on failure, or from
/// a gate's option.
pub fn reroute_sources<'a>(workflow: &'a Workflow, target: &NodeId) -> Vec<&'a NodeId> {
    workflow
        .nodes
        .iter()
        .filter(|node| {
            node.on_failure
                .as_ref()
                .is_some_and(|on_failure| on_failure.goto == *target)
                || matches!(&node.kind, NodeKind::Gate { on, .. } if on.values().any(|to| to == target))
        })
        .map(|node| &node.id)
        .collect()
}

/// An optional node is one a run may leave out, so it is declared where
/// leaving it out leaves the run whole: at the top level, where modes
/// and the scheduler read, and never as something a node the run keeps
/// re-routes to or reads from. The node only it leads to goes with it.
pub(super) fn may_go(workflow: &Workflow) -> Vec<Diagnostic> {
    let mut broken = optional_in_groups(workflow);
    let optional: HashSet<NodeId> = workflow
        .nodes
        .iter()
        .filter(|node| node.optional)
        .map(|node| node.id.clone())
        .collect();
    let mut gone = optional.clone();
    gone.extend(
        followers(workflow, optional.clone())
            .into_iter()
            .map(|(node, _)| node),
    );
    for (index, node) in workflow.nodes.iter().enumerate() {
        if !gone.contains(&node.id) {
            broken.extend(needs_what_may_go(index, node, &optional, &gone));
        }
    }
    broken
}

/// `optional` on a child of a `parallel` group, which only its group can
/// honor.
fn optional_in_groups(workflow: &Workflow) -> Vec<Diagnostic> {
    workflow
        .iter_nodes_with_group()
        .enumerate()
        .filter_map(|(index, (node, group))| {
            let group = group.filter(|_| node.optional)?;
            Some(about(
                index,
                &node.id,
                RuleCode::IncoherentOptional,
                format!(
                    "`{}` is a child of the `parallel` group `{}` and declares `optional: \
                     true`, which only a top-level node can honor; declare it on the group",
                    node.id, group.id
                ),
            ))
        })
        .collect()
}

/// What `node`, which a run keeps, would lose if the optional nodes
/// went: a re-route with nowhere to go, or a read of what nobody made.
fn needs_what_may_go(
    index: usize,
    node: &Node,
    optional: &HashSet<NodeId>,
    gone: &HashSet<NodeId>,
) -> Vec<Diagnostic> {
    let reroutes = node
        .on_failure
        .iter()
        .map(|on_failure| &on_failure.goto)
        .chain(match &node.kind {
            NodeKind::Gate { on, .. } => on.values().collect(),
            _ => Vec::new(),
        });
    let lost_reroutes = reroutes
        .filter(|target| optional.contains(*target))
        .map(|target| {
            format!("re-routes to `{target}`, which is optional: a run that leaves it out would have nowhere to go")
        });
    let lost_reads = super::reads::read_sources(node)
        .into_iter()
        .filter(|(_, source)| gone.contains(*source))
        .map(|(field, source)| {
            format!(
                "`{field}` reads from `{source}`, which a run may leave out; make `{}` \
                 optional too, or `{source}` required",
                node.id
            )
        });
    lost_reroutes
        .chain(lost_reads)
        .map(|detail| about(index, &node.id, RuleCode::IncoherentOptional, detail))
        .collect()
}
