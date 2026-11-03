//! The whole story a reader asks for with `--full`: the run's trace, every
//! node with where it stands, where its failure returns, what it said or the
//! plan it accepted, how long it worked or a person took on it and what it
//! spent; the calls that failed on the way; and every task.

use crate::commands::advice;
use surco::components::{Branch, Cell, Column, Fields, Holds, NodeRow, NodeTable, Row, Table};
use surco::doc::{Block, Depth};
use surco::figures::{duration, Tokens};
use surco::glyphs::{Glyphs, Mark};
use surco::line::Tone;
use surco::prose;
use surco::text::{counted, ordinal};
use surco::words::{Standing, WAITED_ON_YOU};
use yunta_core::events::TaskStatus;
use yunta_core::NodeId;
use yunta_engine::{NodeFrame, NodeStanding, NodeState, RunState};
use yunta_render::state::NodeDisplay;

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
    /// group's children hanging from it by their branches. Each row holds
    /// where the node stands, where its failure returns, what it said or
    /// the plan it had accepted, how long it worked or a person took on it,
    /// and what it spent.
    fn node_table(&self, glyphs: Glyphs) -> NodeTable {
        let nodes = &self.frame.nodes;
        let rows = nodes
            .iter()
            .enumerate()
            .map(|(at, node)| {
                let display = crate::render::node_display(node);
                let person = node.person_took();
                NodeRow {
                    mark: display.standing.mark(),
                    word: display.standing.word(),
                    id: node.id.to_string(),
                    branch: branch(nodes, at),
                    returns: node
                        .declares
                        .on_failure
                        .as_ref()
                        .map(|rule| rule.goto.to_string()),
                    note: self.note(node, &display, person, glyphs),
                    took: match person {
                        true => duration(node.people),
                        false => node.elapsed.map(duration).unwrap_or_default(),
                    },
                    took_by_person: person,
                    spent: match node.tokens.total() {
                        0 => String::new(),
                        spent => Tokens(spent).to_string(),
                    },
                }
            })
            .collect();
        NodeTable { rows }
    }

    /// What a node's row says beside it, its parts apart by the separator:
    /// what it said or the plan it had accepted, or that a live engine is
    /// asking about it at its terminal; how its documents were accepted, how
    /// many attempts it took, how many times its own failure rerouted it,
    /// its rounds with a person, a wait on a person its time column does not
    /// show, and, for a node not yet run, what it will do.
    fn note(
        &self,
        node: &NodeFrame,
        display: &NodeDisplay,
        person: bool,
        glyphs: Glyphs,
    ) -> String {
        let accepted = self.plans.iter().find(|(id, _)| *id == node.id);
        let said = match (self.engine.asks_about(&node.id), accepted) {
            (true, _) => Some("asking at its terminal".to_string()),
            (false, Some((_, summary))) => Some(summary.clone()),
            (false, None) => display
                .modifier
                .as_ref()
                .map(|said| prose::first_sentence(said)),
        };
        let waited = (!person && !node.people.is_zero()).then(|| match node.person_rounds() {
            0 | 1 => format!("{WAITED_ON_YOU} {}", duration(node.people)),
            rounds => format!(
                "{WAITED_ON_YOU} {} over {rounds} rounds of questions",
                duration(node.people)
            ),
        });
        let rerouted = (node.reroutes > 0).then(|| match node.standing {
            // The row's word already says the node was rerouted, so the note
            // says how often.
            Standing::Rerouted => times(node.reroutes),
            _ => format!("{} {}", Standing::Rerouted.word(), times(node.reroutes)),
        });
        [
            said,
            handed(self.state, &node.id),
            attempts(node),
            rerouted,
            rounds(node),
            waited,
        ]
        .into_iter()
        .flatten()
        .chain(ahead(node))
        .collect::<Vec<_>>()
        .join(&format!(" {} ", glyphs.sep()))
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
                if let Some((_, commit)) = &record.left_work {
                    let shown = advice::show_commit(commit.abbreviated());
                    said.push_str(&format!("; its work is at {shown}"));
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

/// The branch the node at `at` of `nodes` hangs from, when a group holds it:
/// the group's last child, the one no sibling follows, hangs from the last.
fn branch(nodes: &[NodeFrame], at: usize) -> Option<Branch> {
    let group = nodes.get(at)?.group.as_ref()?;
    let followed = nodes
        .get(at + 1)
        .is_some_and(|next| next.group.as_ref() == Some(group));
    Some(match followed {
        true => Branch::Child,
        false => Branch::Last,
    })
}

/// How many attempts a node took, when it took more than one and its state
/// does not already name the attempt it is on: a node running or paused on
/// one says that attempt in its state, and a count would say it again.
fn attempts(node: &NodeFrame) -> Option<String> {
    if matches!(
        node.state,
        NodeStanding::Paused { .. } | NodeStanding::Reached(NodeState::Running { .. })
    ) {
        return None;
    }
    node.attempt
        .filter(|attempt| *attempt > 1)
        .map(|attempt| counted(attempt as usize, "attempt"))
}

/// A gate's rounds with a person, past its first: the round it waits in, or
/// the round a person decided it on.
fn rounds(node: &NodeFrame) -> Option<String> {
    node.declares.gate.as_ref()?;
    match (node.standing, node.person_rounds()) {
        (_, 0 | 1) => None,
        (Standing::NeedsYou, round) => Some(format!("its {} round", ordinal(round))),
        (_, round) => Some(format!("on its {} round", ordinal(round))),
    }
}

/// What a node not yet run will do, as its workflow declares: a gate what
/// it will ask a person, and a node whose failure returns how many times at
/// most. Where the failure goes, its row's return arrow says.
fn ahead(node: &NodeFrame) -> Vec<String> {
    if !matches!(node.state, NodeStanding::ToGo) {
        return Vec::new();
    }
    let gate = node
        .declares
        .gate
        .as_ref()
        .map(|gate| match (&gate.question, gate.on_forge) {
            (_, true) => "a gate: you approve the pull request".to_string(),
            (Some(question), false) => format!("a gate: {question}"),
            (None, false) => "a gate: you decide".to_string(),
        });
    let returns = node
        .declares
        .on_failure
        .as_ref()
        .filter(|rule| rule.max_reroutes > 0)
        .map(|rule| format!("sends a failure back at most {}", times(rule.max_reroutes)));
    gate.into_iter().chain(returns).collect()
}

/// How many times, as a person says it: once, twice, or a number of times.
fn times(count: u32) -> String {
    match count {
        1 => "once".to_string(),
        2 => "twice".to_string(),
        n => format!("{n} times"),
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
