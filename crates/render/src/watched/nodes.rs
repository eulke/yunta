//! See [`super`]. A run's nodes as the list beside its pane, which a person moves a pointer
//! along: a node a row in the workflow's order, a loop's tasks hung under it, and, while the
//! person leads, the key that gives the lead back to the run under them all.
//!
//! A loop works every task the run registered, whichever node's events registered them (a
//! planner's, most often), as the node's own page lists them; they hang in the order the log
//! registered them, the order their tasks document gives.
//!
//! A row is the pointer's opening ([`Line::pointed`]), the node's mark or, at work while the
//! run moves, the spinner, its name in a column, and how long it took where it is done with.
//! Every node at work turns on one frame, the run's last event's, so a person sees one run
//! moving and not several clocks. The node the run waits on a person at, as the run's frame
//! says it, is marked as needing them, though its log may say it failed: the run is parked
//! on its failure for a decision. One its log never started keeps its own mark, though the
//! pause the run stands at names it. A node a person's choice at a gate sent the run back from
//! is marked as rerouted, where its log says it failed: the run goes back through it.

use std::time::Duration;

use surco::figures::duration;
use surco::glyphs::Mark;
use surco::line::{Line, Tone};
use surco::look::Look;
use surco::motion::Motion;
use surco::words::Standing;
use yunta_core::events::{Failure, NodeState};
use yunta_core::{NodeId, NodeKind, Workflow};
use yunta_replay::frame::{NodeFrame, NodeStanding, RunFrame};
use yunta_replay::RunState;

use crate::blocks::task_mark;

/// The cells a node's name stands in: the longest of fragua's ids, `review@reviewer-alt`,
/// whole. A longer name runs past it, and its row is cut at the region's edge.
const NAME: usize = 19;

/// The cells how long a node took stands in, right-aligned after its name: a time of hours
/// and minutes, `13m53s`, with a cell before it.
const TOOK: usize = 7;

/// The cells a task's branch stands after: the pointer's opening and the mark of the loop it
/// hangs from, so the branch opens between that mark and the loop's name.
const AFTER_THE_MARK: usize = 3;

/// The key that gives the lead back to the run, and what it does, said under the nodes
/// while the person leads.
const FOLLOW: (&str, &str) = ("f", "follow what it is doing");

/// The run's nodes, one a row in the order `workflow` declares them, which a person moves
/// along: the pointer on the one `pointed` at, a node at work turning on the frame of
/// `state`'s last event while the run moves, each other with its mark, each name in its
/// column and how long a node that is done with took; under each loop the tasks it works,
/// each with its mark; and, where the run does not lead (`leads` false), a blank row, then
/// the key that gives it back. Not [`working`](super::working), the nodes the CLI's region
/// gives rows of.
pub fn nodes(
    frame: &RunFrame,
    workflow: &Workflow,
    state: &RunState,
    pointed: Option<&NodeId>,
    leads: bool,
    look: &Look,
) -> Vec<Line> {
    let turned = state.head().map_or(0, |head| head.get());
    let loops = |node: &NodeId| {
        let declared = workflow.iter_nodes().find(|declared| declared.id == *node);
        declared.is_some_and(|declared| matches!(declared.kind, NodeKind::Loop { .. }))
    };
    let moves = frame.moves();
    let waited_at = frame.waits_on_a_person_at();
    let mut rows = Vec::new();
    for node in &frame.nodes {
        let on = pointed == Some(&node.id);
        let mark = mark(node, waited_at == Some(&node.id));
        rows.push(row(node, mark, moves, turned, on, look));
        if loops(&node.id) {
            rows.extend(tasks(state, look));
        }
    }
    if !leads {
        let (key, does) = FOLLOW;
        rows.push(Line::new());
        rows.push(
            Line::new()
                .plain(" ")
                .push(Tone::Strong, key)
                .push(Tone::Muted, format!(" {does}")),
        );
    }
    rows
}

/// `node`'s row, `on` the row pointed at or not: its sign, `mark` or, at work in a run that
/// `moves`, the spinner's frame `turned` events bring it to where the look moves; its name
/// in its column; and how long it took where it is done with.
fn row(node: &NodeFrame, mark: Mark, moves: bool, turned: u64, on: bool, look: &Look) -> Line {
    let glyphs = look.glyphs;
    let sign = match (node.standing, moves, look.motion) {
        (Standing::Running, true, Motion::Moves) => {
            surco::motion::frame(glyphs, turned).to_string()
        }
        _ => glyphs.mark(mark).to_string(),
    };
    let name = match on {
        true => Tone::Strong,
        false => Tone::Plain,
    };
    let row = Line::pointed(on, glyphs)
        .push(Tone::of(mark), sign)
        .plain(" ");
    match took(node) {
        Some(took) => row
            .push(name, format!("{:<NAME$}", node.id.as_str()))
            .push(Tone::Muted, format!("{took:>TOOK$}")),
        None => row.push(name, node.id.as_str()),
    }
}

/// The mark `node` carries: needing the person where the run waits on them at it
/// (`waited_at`) and its log started it; any other, the mark of the word it is listed as.
/// A node its log never started, which a pause names before it ran, is not where the run
/// is parked: it keeps the mark of a node that did not run.
fn mark(node: &NodeFrame, waited_at: bool) -> Mark {
    let started = match &node.state {
        NodeStanding::Paused { .. } | NodeStanding::Reached(_) => true,
        NodeStanding::ToGo | NodeStanding::Skipped | NodeStanding::LeftOut(_) => false,
    };
    match waited_at && started {
        true => Standing::NeedsYou.mark(),
        false => listed_as(node).mark(),
    }
}

/// The word `node` is listed as, where the run waits on no person at it: rerouted where a
/// person's choice at its gate sent the run back from it, as the run goes back through it,
/// though its log says it failed; any other, its standing. A node's own scene says it stands
/// by this word, under the mark it carries here. Not [`NodeFrame::standing`], what its log
/// comes to.
pub(crate) fn listed_as(node: &NodeFrame) -> Standing {
    let sent_back = matches!(
        &node.state,
        NodeStanding::Reached(NodeState::Failed {
            failure: Failure::GateChose { .. },
            ..
        })
    );
    match sent_back {
        true => Standing::Rerouted,
        false => node.standing,
    }
}

/// How long `node` took, where it is done with: finished, or failed and corrected by a
/// reroute. None for a node at work, waiting, stopped, failed or not yet run.
fn took(node: &NodeFrame) -> Option<String> {
    if !matches!(node.standing, Standing::Finished | Standing::Rerouted) {
        return None;
    }
    time_taken(node).map(duration)
}

/// How long `node` took, wherever it ran: a gate a person was asked at, how long they took,
/// as the run's page says it; any other node, its own time at work. None for a node that
/// never started.
pub(crate) fn time_taken(node: &NodeFrame) -> Option<Duration> {
    match node.person_took() {
        true => Some(node.people),
        false => node.elapsed,
    }
}

/// The rows of the tasks `state`'s run registered, in the order the log registered them:
/// each on a branch after the mark of the loop it hangs from, the last on the closing one,
/// with its mark and its id.
fn tasks(state: &RunState, look: &Look) -> Vec<Line> {
    let mut registered: Vec<_> = state.tasks.iter().collect();
    registered.sort_by_key(|(id, task)| (task.registered_at, *id));
    let last = registered.len().saturating_sub(1);
    registered
        .into_iter()
        .enumerate()
        .map(|(at, (id, task))| {
            let mark = task_mark(task.status);
            Line::new()
                .plain(" ".repeat(AFTER_THE_MARK))
                .push(Tone::Muted, look.glyphs.branch(at == last))
                .plain(" ")
                .push(Tone::of(mark), look.glyphs.mark(mark).to_string())
                .plain(" ")
                .plain(id.as_str())
        })
        .collect()
}

#[cfg(test)]
mod tests;
