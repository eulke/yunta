//! What lies past a decision's options, beyond the names of the nodes they lead to, as a
//! run froze it: the pull request a node opens and on which repository of which forge, or
//! why the run leaves a node out. Read from the run's frozen workflow and config, so the
//! engine that asks a decision and every page that draws it say the same of where an
//! option leads; the log holds none of it, and no event carries it.

use std::collections::BTreeMap;

use crate::{Because, ConfigLayer, GitHubRepo, LeftOut, NodeId, NodeKind, Workflow};

/// What a node an option may lead to does that its name does not say. Not a route, what
/// runs after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ahead {
    /// It opens the run's pull request on GitHub, the one forge a config declares, in
    /// `repo`: the forge the node itself opens it through.
    OpensPullRequest { repo: GitHubRepo },
    /// The run leaves it out, and why: it never runs.
    LeftOut(Because),
}

/// What lies ahead at the nodes of one run, by node. Empty where nothing was read, which
/// says nothing of any node.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NodesAhead(BTreeMap<NodeId, Ahead>);

impl NodesAhead {
    /// What lies ahead at each node of `workflow`, run under `config`, which leaves out
    /// `left_out`: a node left out is said as left out, whatever its kind; a
    /// `pull_request` node opens its pull request on the forge `config` declares. A
    /// `pull_request` node with no forge declared is left out or refuses the run before
    /// it starts, so it is never ahead of a decision as one that opens anything.
    pub fn of(workflow: &Workflow, config: &ConfigLayer, left_out: &[LeftOut]) -> Self {
        let repo = config
            .forge
            .as_ref()
            .and_then(|forge| forge.github.as_ref())
            .map(|github| github.repo.clone());
        let mut ahead = BTreeMap::new();
        for node in workflow.iter_nodes() {
            if let Some(left) = left_out.iter().find(|left| left.node == node.id) {
                ahead.insert(node.id.clone(), Ahead::LeftOut(left.because.clone()));
            } else if let (NodeKind::PullRequest { .. }, Some(repo)) = (&node.kind, &repo) {
                let repo = repo.clone();
                ahead.insert(node.id.clone(), Ahead::OpensPullRequest { repo });
            }
        }
        NodesAhead(ahead)
    }

    /// What lies ahead at `node`, where anything does beyond its name.
    pub fn at(&self, node: &NodeId) -> Option<&Ahead> {
        self.0.get(node)
    }
}
