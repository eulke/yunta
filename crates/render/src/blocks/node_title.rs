//! A node's pane title: the node and its kind, and at the far end who works it, as the
//! region's title and edge. A session's runner on its candidate (`runner`), a check's builtin,
//! a gate's assignee; nothing for a command the engine runs itself or a pull request, which
//! no one works. A pane about one task of a loop names the task there instead.
//!
//! A session's node that resolved no runner yet is worked by the runner its run resolved
//! for the role it names, on another node: the run's config is frozen and its pin travels
//! with it, so the engine resolves a role the same way for every node that names it. The
//! title reads that decision rather than resolving the role again; a role the run never
//! resolved names no one yet. The node's printed page names the same runner, read by the
//! same function ([`runner_of`]).

use surco::components::Region;
use yunta_core::events::ResolvedRunner;
use yunta_core::{ConfigLayer, Node, NodeId, NodeKind, RunnerName, TaskId};
use yunta_replay::frame::NodeFrame;
use yunta_replay::RunState;

/// The words before a gate's assignee at the far end of its pane's title.
const ASSIGNEE: &str = "assignee";

/// What a node's pane is titled by: the node, its kind, and who works it. Not the node's
/// row in the nodes list, which says where it stands.
pub struct NodeTitle {
    node: String,
    kind: &'static str,
    works_it: Option<String>,
}

impl NodeTitle {
    /// The title of the node `node` frames, as the run's workflow `declared` it under the
    /// run's frozen `config`: who works it read from its kind and the runner it resolved, or,
    /// where it resolved none yet, the one the run in `state` resolved for its role.
    pub fn of(
        node: &NodeFrame,
        declared: Option<&Node>,
        state: &RunState,
        config: &ConfigLayer,
    ) -> NodeTitle {
        let runner = || {
            runner_of(node, declared, state, config)
                .map(|runner| runner_on(runner, &node.id, state))
        };
        let works_it = match declared.map(|declared| &declared.kind) {
            Some(NodeKind::Check(builtin)) => Some(builtin.builtin_name().to_string()),
            Some(NodeKind::Gate { assignee, .. }) => Some(format!("{ASSIGNEE} {assignee}")),
            Some(NodeKind::Bash { .. } | NodeKind::PullRequest { .. }) => None,
            Some(
                NodeKind::Prompt { .. }
                | NodeKind::Loop { .. }
                | NodeKind::Parallel { .. }
                | NodeKind::Executor { .. }
                | NodeKind::Workflow { .. },
            )
            | None => runner(),
        };
        NodeTitle {
            node: node.id.to_string(),
            kind: node.kind,
            works_it,
        }
    }

    /// This title with `task` at the far end in place of who works it: a pane about one task
    /// of a loop, a card asking about its request or its departure, names it there.
    pub fn about_the_task(self, task: &TaskId) -> NodeTitle {
        NodeTitle {
            works_it: Some(task.to_string()),
            ..self
        }
    }

    /// `region` under this title: the node first, then its kind, and who works it at the far
    /// end of the title's row where anyone does.
    pub fn titled(self, region: Region) -> Region {
        let region = region.titled([self.node, self.kind.to_string()]);
        match self.works_it {
            Some(works_it) => region.edge(works_it),
            None => region,
        }
    }
}

/// The runner the node `node` frames works through, as the run's workflow `declared` it
/// under the run's frozen `config`: the one it resolved, or, where it resolved none yet,
/// the one the run in `state` resolved for the role it names. Not [`NodeFrame::runner`],
/// the one the node resolved itself.
pub fn runner_of<'a>(
    node: &'a NodeFrame,
    declared: Option<&Node>,
    state: &'a RunState,
    config: &ConfigLayer,
) -> Option<&'a ResolvedRunner> {
    let role = || declared.and_then(|declared| config.runner_role(declared));
    let resolved = || role().and_then(|role| resolved_for(role, state));
    node.runner.as_ref().or_else(resolved)
}

/// The runner the run in `state` resolved for the role `named` on a node of its own: where
/// several nodes resolved it, the one of the node that came to it latest, by its first
/// resolution. None where no node resolved it.
fn resolved_for<'s>(named: &RunnerName, state: &'s RunState) -> Option<&'s ResolvedRunner> {
    state
        .nodes
        .values()
        .filter_map(|record| record.first_runner.as_ref().zip(record.runner.as_ref()))
        .filter(|(_, runner)| runner.runner == *named)
        .max_by_key(|((seq, _), _)| *seq)
        .map(|(_, runner)| runner)
}

/// Who works the node `node` through `runner`: the runner's name on the adapter it chose,
/// with the model its candidate names, else the first the node's sessions reported in
/// `state`; the adapter alone while neither is known.
pub(crate) fn runner_on(runner: &ResolvedRunner, node: &NodeId, state: &RunState) -> String {
    let reported = state
        .nodes
        .get(node)
        .and_then(|record| record.models.first());
    let model = runner.chosen.model.as_ref().or(reported);
    crate::runner::on(&runner.runner, &runner.chosen.adapter, model)
}
