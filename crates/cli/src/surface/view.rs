//! How a [`RunFrame`] reads as the rows a person watches: the line that
//! says whether the run needs them, the line each working node gets, and
//! the counters under it all.
//!
//! Every surface here draws the same frame, so this is where the words
//! for it are chosen once. What the words are *drawn with* — the glyphs,
//! the column widths, the duration format — belongs to
//! [`crate::render`], and nothing here reimplements a piece of it.

use yunta_engine::{Counter, NodeFrame, NodeStanding, RunFrame};

use yunta_core::{NodeId, RunId};

use crate::commands::advice;
use crate::render::{format_duration, Glyphs, NodeDisplay, StateWord};

/// How deep a node's detail sits under the node's own row.
pub(super) const DETAIL_INDENT: &str = "    ";

/// The line that answers "does this run need me?", in the words a reader
/// acts on: either nothing does, or something does and this is the
/// command that answers it.
///
/// It is produced for every phase, including the ones that need nobody,
/// because a surface that shows it only when there is something to say
/// makes its absence mean two things at once — nothing is needed, or the
/// surface has not drawn yet.
///
/// The command comes before what it is about, so that a row cut to fit
/// loses the subject and never the thing to run. A reader who lost the
/// subject still has `yunta status`; a reader who lost the command has
/// nothing.
pub(super) fn demand_line(frame: &RunFrame, run_id: &RunId, answerable: bool) -> String {
    match advice::parked(&frame.phase) {
        None => "nothing needs you".to_string(),
        Some(on) => format!(
            "needs you: {} ({})",
            answer_command(run_id, answerable),
            advice::parked_on(on)
        ),
    }
}

/// The command that moves a parked run: an option off its own menu when
/// the run stopped on one, and handing the run back when what stopped it
/// is settled somewhere else — a budget, a scope, an answers file, a
/// review on a forge.
pub(super) fn answer_command(run_id: &RunId, answerable: bool) -> String {
    match answerable {
        true => advice::resolve_gate(run_id),
        false => advice::resume(run_id),
    }
}

/// The rows one working node takes: what it is, what its tasks are doing,
/// and what it last reached for.
///
/// Its liveness is the age of its last event and never a spinner: the age
/// is measured, it grows while the node says nothing, and it is the one
/// signal that can tell a busy node from a stuck one.
pub(super) fn node_rows(node: &NodeFrame, glyphs: Glyphs) -> Vec<String> {
    let mut rows = vec![headline(node, glyphs)];
    if !node.running_tasks.is_empty() {
        rows.push(format!(
            "{DETAIL_INDENT}tasks running: {}",
            join(node.running_tasks.iter().map(|task| task.to_string()))
        ));
    }
    let calls = recent_calls(node);
    if !calls.is_empty() {
        // "this node's": the event envelope names a node and never a
        // session, so at loop concurrency above one no log can say which
        // of a node's sessions made a call. Labelling them the node's is
        // the whole truth the log carries.
        rows.push(format!("{DETAIL_INDENT}this node's recent calls: {calls}"));
    }
    rows
}

/// The node's own row: its state, its id, the runner it resolved through
/// with the adapter and model behind it, how long it has been working,
/// and how long ago it last said anything.
fn headline(node: &NodeFrame, glyphs: Glyphs) -> String {
    let state = standing(node);
    let mut row = format!(
        "{} {} {}",
        glyphs.state(state.word),
        state.word.short(),
        node.id
    );
    if let Some(runner) = &node.runner {
        row.push_str(&format!(
            " · {} {}/{}",
            runner.runner, runner.chosen.adapter, runner.chosen.model
        ));
        if let Some(agent) = &runner.chosen.agent {
            row.push_str(&format!("/{agent}"));
        }
    }
    if let Some(elapsed) = node.elapsed {
        row.push_str(&format!(" · {}", format_duration(elapsed)));
    }
    if let Some(age) = node.last_event_age {
        row.push_str(&format!(" · last event {} ago", format_duration(age)));
    }
    row
}

/// The tool calls this node made on the attempt it is running, newest
/// first, as many as a row has room for.
fn recent_calls(node: &NodeFrame) -> String {
    /// How many calls a row shows. Enough to see what a node is working
    /// through, few enough that the names still fit beside the label.
    const SHOWN: usize = 4;
    join(node.activity.iter().take(SHOWN).map(|call| {
        call.tool_name
            .clone()
            .unwrap_or_else(|| "(unnamed tool)".to_string())
    }))
}

/// Where a node stands, in the vocabulary every surface says it in.
fn standing(node: &NodeFrame) -> NodeDisplay {
    match &node.state {
        NodeStanding::Skipped => NodeDisplay::skipped(),
        NodeStanding::ToGo => NodeDisplay::of(None),
        NodeStanding::Reached(state) => NodeDisplay::of(Some(state)),
    }
}

/// The counters, at both levels the contract names: the DAG's nodes and
/// the ledger's tasks, each as done over what this run will do, with
/// every other bucket beside it.
///
/// The buckets are not decoration. A task that fails at integration goes
/// back to ready and a re-routed node runs again, so `done` alone walks
/// backwards; printed beside `failed`, `running` and `waiting`, that same
/// event reads as a move between buckets, which is what makes a
/// denominator that grew attributable to the event that grew it.
pub(super) fn counter_line(frame: &RunFrame) -> String {
    let mut line = format!("nodes {}", counter(&frame.flow));
    if let Some(tasks) = &frame.tasks {
        line.push_str(&format!(" · tasks {}", counter(tasks)));
    }
    if frame.reroutes > 0 {
        line.push_str(&format!(
            " · {}",
            crate::commands::counted(frame.reroutes, "reroute")
        ));
    }
    line
}

/// One level's counters: done over the total this run will do, then each
/// bucket that holds anything, then what the run's mode left out.
fn counter(counter: &Counter) -> String {
    let mut text = format!("{}/{}", counter.done, counter.total);
    for (count, word) in [
        (counter.failed, StateWord::Fail),
        (counter.running, StateWord::Run),
        (counter.waiting, StateWord::Wait),
    ] {
        if count > 0 {
            text.push_str(&format!(" · {count} {}", word.short()));
        }
    }
    if let Some(mode) = &counter.skipped_by {
        text.push_str(&format!(" · {} skipped by `{mode}`", counter.skipped));
    }
    text
}

/// Every node the run is working on right now, in the workflow's own
/// declaration order.
pub(super) fn working(frame: &RunFrame) -> Vec<&NodeFrame> {
    frame
        .nodes
        .iter()
        .filter(|node| {
            matches!(
                node.state,
                NodeStanding::Reached(
                    yunta_engine::NodeState::Running { .. }
                        | yunta_engine::NodeState::Waiting { .. }
                )
            )
        })
        .collect()
}

/// The line a node leaves behind when it stops working: its final state,
/// and what that state carries.
pub(super) fn graduation(node: &NodeFrame, glyphs: Glyphs) -> String {
    let state = standing(node);
    let elapsed = node
        .elapsed
        .map(|elapsed| format!(" · {}", format_duration(elapsed)))
        .unwrap_or_default();
    format!(
        "{} {} — {}{elapsed}",
        glyphs.state(state.word),
        node.id,
        state.label()
    )
}

/// Every node that has stopped working, by id — what a surface compares
/// against to find the ones it has not seen stop yet.
pub(super) fn settled_nodes(frame: &RunFrame) -> Vec<&NodeId> {
    frame
        .nodes
        .iter()
        .filter(|node| {
            matches!(
                node.state,
                NodeStanding::Reached(
                    yunta_engine::NodeState::Finished { .. }
                        | yunta_engine::NodeState::Failed { .. }
                )
            )
        })
        .map(|node| &node.id)
        .collect()
}

/// Names in a row, separated so a reader's eye stops between them.
fn join(names: impl Iterator<Item = String>) -> String {
    names.collect::<Vec<_>>().join(", ")
}
