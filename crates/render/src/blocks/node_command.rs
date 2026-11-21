//! What a node runs itself, under its heading: a command node's own command, as the project
//! resolves it, or the command a check runs in the run's tree. The baseline's suite is the
//! one the run's lineage measured, as its log holds it; the run's frozen configuration says
//! it before a measurement does. A check that reads the log, a session and any other node
//! run no command of their own.

use surco::doc::Block;
use surco::line::{Line, Tone};
use yunta_core::{CheckBuiltin, ConfigLayer, Node, NodeKind};
use yunta_replay::RunState;

/// The words over what a node runs.
const HEADING: &str = "what it runs";

/// The command a node runs itself, as its pane shows it. Not [`yunta_core::RunCommand`],
/// what a workflow declares a command node runs, which this is read from.
pub struct NodeCommand {
    command: String,
}

impl NodeCommand {
    /// What `node` runs under the run's frozen `config`, the run in `state` holding what its
    /// lineage measured. `None` for a node that runs no command of its own.
    pub fn of(node: &Node, config: &ConfigLayer, state: &RunState) -> Option<NodeCommand> {
        let command = match &node.kind {
            NodeKind::Bash { run } => Some(
                run.text(config)
                    .map_or_else(|| run.to_string(), str::to_string),
            ),
            NodeKind::Check(CheckBuiltin::BaselineCompare) => {
                let measured = state.run.baseline().map(|baseline| &baseline.command);
                let declared = config.baseline.as_ref().map(|baseline| &baseline.suite);
                measured.or(declared).cloned()
            }
            NodeKind::Check(CheckBuiltin::CoverageGate) => config
                .coverage
                .as_ref()
                .map(|coverage| coverage.cmd.clone()),
            NodeKind::Check(CheckBuiltin::FindingsGate { .. })
            | NodeKind::Prompt { .. }
            | NodeKind::Loop { .. }
            | NodeKind::Parallel { .. }
            | NodeKind::Executor { .. }
            | NodeKind::Gate { .. }
            | NodeKind::Workflow { .. }
            | NodeKind::PullRequest { .. } => None,
        };
        command.map(|command| NodeCommand { command })
    }

    /// Its heading, then the command on a row of its own.
    pub fn blocks(&self) -> Vec<Block> {
        let row = Line::new().push(Tone::Command, self.command.as_str());
        vec![Block::Heading(HEADING.to_string()), Block::Rows(vec![row])]
    }
}
