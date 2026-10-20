//! The whole story a reader asks for with `--full`: every node with what
//! it said, how long it worked and what it spent, the calls that failed
//! on the way, and every task.

use surco::components::{Cell, Column, Fields, Holds, NodeRow, NodeTable, Row, Table};
use surco::doc::{Block, Depth};
use surco::figures::{duration, Tokens};
use surco::glyphs::{Glyphs, Mark};
use surco::line::Tone;
use surco::prose;
use surco::width::indent;
use yunta_core::events::TaskStatus;
use yunta_core::NodeId;
use yunta_engine::{NodeState, RunState};

use super::RunPage;

impl RunPage<'_> {
    /// The story's blocks, at `--full` and deeper; nothing at a glance.
    pub(super) fn story(&self, glyphs: Glyphs) -> Vec<Block> {
        if !self.depth.reaches(Depth::Full) {
            return Vec::new();
        }
        let mut blocks = vec![self.node_table(glyphs).into()];
        let calls = self.calls();
        if !calls.is_empty() {
            blocks.push(calls.into());
        }
        if let Some(tasks) = self.task_table() {
            blocks.extend([Block::Heading("tasks".to_string()), tasks.into()]);
        }
        blocks
    }

    /// Every node the frozen workflow declares, in declaration order, a
    /// group's children one step under it: where it stands, what it said
    /// or what holds it, how long it worked and what it spent.
    fn node_table(&self, glyphs: Glyphs) -> NodeTable {
        let rows = self
            .frame
            .nodes
            .iter()
            .map(|node| {
                let display = crate::render::node_display(node);
                let said = display.modifier.map(|said| prose::first_sentence(&said));
                let note = [said, handed(self.state, &node.id)]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(&format!(" {} ", glyphs.sep()));
                // The frame calls the node a live engine is asking about
                // `needs you`; where it is asked is what the row adds.
                let note = match self.engine.asks_about(&node.id) {
                    true => "asking at its terminal".to_string(),
                    false => note,
                };
                let spent = node.tokens.total();
                NodeRow {
                    mark: display.standing.mark(),
                    word: display.standing.word(),
                    id: format!("{}{}", indent(usize::from(node.group.is_some())), node.id),
                    note,
                    took: node.elapsed.map(duration).unwrap_or_default(),
                    spent: match spent {
                        0 => String::new(),
                        spent => Tokens(spent).to_string(),
                    },
                }
            })
            .collect();
        NodeTable { rows }
    }

    /// The last call of a run tool each node's attempt made that failed:
    /// what a node got wrong on its way is not why it stands where it
    /// does, so it is not its row's note. A node that finished is past
    /// it: how its documents were accepted says what the way there took.
    /// A reader who digs reads the session that made it.
    fn calls(&self) -> Fields {
        let dig = self.depth.reaches(Depth::Dig);
        let mut fields = Fields::new();
        for node in &self.frame.nodes {
            if matches!(
                self.state.nodes.state(&node.id),
                Some(NodeState::Finished { .. })
            ) {
                continue;
            }
            let Some(failed) = self
                .state
                .nodes
                .get(&node.id)
                .and_then(|record| record.last_tool_failure.as_ref())
            else {
                continue;
            };
            let mut said = format!(
                "{}: {} ({})",
                node.id,
                failed.tool.name(),
                failed.cause.as_str()
            );
            if dig {
                said.push_str(&format!(", in session {}", failed.session_id));
            }
            fields = fields.push_if("failed call", said);
        }
        fields
    }

    /// Every task the run registered, by id: where it stands and how many
    /// times it was dispatched. `None` for a run with no tasks.
    fn task_table(&self) -> Option<Table> {
        if self.state.tasks.is_empty() {
            return None;
        }
        let mut tasks: Vec<_> = self.state.tasks.iter().collect();
        tasks.sort_by(|a, b| a.0.cmp(b.0));
        let rows = tasks
            .into_iter()
            .map(|(id, record)| {
                let mut said = record.status.as_str().to_string();
                if record.attempts > 1 {
                    said.push_str(&format!(" after {} attempts", record.attempts));
                }
                Row {
                    mark: Some(task_mark(record.status)),
                    cells: vec![Cell::plain(id.as_str()), Cell::toned(Tone::Muted, said)],
                }
            })
            .collect();
        Some(Table {
            columns: vec![
                Column {
                    title: "task",
                    holds: Holds::Id,
                },
                Column {
                    title: "",
                    holds: Holds::Rest,
                },
            ],
            rows,
        })
    }
}

/// The mark a task's status carries: what is done, what failed, what a
/// cause it cannot pass on its own stopped, what is under way, and what
/// has not begun.
fn task_mark(status: TaskStatus) -> Mark {
    match status {
        TaskStatus::Done => Mark::Done,
        TaskStatus::Failed => Mark::Failed,
        TaskStatus::Blocked => Mark::Caution,
        TaskStatus::Running => Mark::Running,
        TaskStatus::Pending | TaskStatus::Ready => Mark::Absent,
    }
}

/// How node `id` got its documents accepted once it finished, when that
/// took more than one handover: what it took is part of what it did.
pub(crate) fn handed(state: &RunState, id: &NodeId) -> Option<String> {
    if !matches!(state.nodes.state(id), Some(NodeState::Finished { .. })) {
        return None;
    }
    let refused = state
        .artifacts
        .handed_over_by(id)
        .filter(|handed| handed.refusals > 0)
        .max_by_key(|handed| handed.submissions)?;
    Some(format!(
        "accepted on its {} handover ({} refused)",
        surco::text::ordinal(refused.submissions),
        refused.refusals
    ))
}
