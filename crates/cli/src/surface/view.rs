//! How a [`RunFrame`] reads as the rows a person watches: the line that
//! says whether the run needs them, the line each working node gets, and
//! the counters under it all.
//!
//! Every surface here draws the same frame, so this is where the words
//! for it are chosen once. What the words are *drawn with* — the glyphs,
//! the column widths, the duration format — belongs to
//! [`crate::render`], and nothing here reimplements a piece of it.

use yunta_core::events::TerminalState;
use yunta_engine::{ChildLink, Counter, NodeFrame, NodeStanding, RunFrame};

use yunta_core::{NodeId, RunId};

use crate::commands::advice;
use crate::render::{format_duration, indent, Glyphs, NodeDisplay, StateWord};

/// How deep a node's detail sits under the node's own row, in steps of
/// [`indent`] — the step every surface here shares, so the detail lines
/// up with the blocks a run's other surfaces nest.
const DETAIL_DEPTH: usize = 2;

/// What a child run whose link the log recorded under no node is filed
/// under: what the log does not say, rather than a node it might not
/// belong to.
const NO_NODE: &str = "under no node on this run's log";

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
pub(super) fn node_rows(frame: &RunFrame, node: &NodeFrame, glyphs: Glyphs) -> Vec<String> {
    let detail = indent(DETAIL_DEPTH);
    let mut rows = vec![headline(node, glyphs)];
    if !node.running_tasks.is_empty() {
        rows.push(format!(
            "{detail}tasks running: {}",
            join(node.running_tasks.iter().map(|task| task.to_string()))
        ));
    }
    let calls = recent_calls(node);
    if !calls.is_empty() {
        // "this node's": the event envelope names a node and never a
        // session, so at loop concurrency above one no log can say which
        // of a node's sessions made a call. Labelling them the node's is
        // the whole truth the log carries.
        rows.push(format!("{detail}this node's recent calls: {calls}"));
    }
    rows.extend(
        children_of(frame, &node.id)
            .into_iter()
            .map(|child| format!("{detail}{}", child_row(child, glyphs))),
    );
    rows
}

/// The row one child run takes under the node that bore it: where the
/// parent's log says it stands, and which run to go and look at.
///
/// What it does not say is which workflow the child ran. No event
/// carries a child's name — `child_run_created` records the link and
/// the child's workflow hash — and naming one means reading the child's
/// own frozen manifest, which is a read no frame does. The row says
/// what this run's log says, and nothing it would have to guess.
///
/// Where the child stands comes first, so a row cut to a narrow
/// terminal still says a child is open and loses only which one. What
/// it never becomes is a number: heterogeneous children averaged into
/// one figure is the lying percentage under another name
/// (`contrato-del-run.md` §8.5).
pub(super) fn child_row(child: &ChildLink, glyphs: Glyphs) -> String {
    let standing = child_standing(child.terminal);
    format!(
        "{} {} · child run {}",
        glyphs.state(standing.0),
        standing.1,
        child.run_id
    )
}

/// Every child this run bore, grouped under the node that bore it, each
/// group in the order the parent's log recorded it.
pub(super) fn children_by_node(frame: &RunFrame) -> Vec<(String, Vec<&ChildLink>)> {
    let mut groups: Vec<(String, Vec<&ChildLink>)> = Vec::new();
    for child in &frame.children {
        let parent = match &child.node {
            Some(node) => format!("node `{node}`"),
            None => NO_NODE.to_string(),
        };
        match groups.iter_mut().find(|(under, _)| under == &parent) {
            Some((_, born)) => born.push(child),
            None => groups.push((parent, vec![child])),
        }
    }
    groups
}

/// The children `node` bore, in the order the parent's log recorded
/// them.
fn children_of<'a>(frame: &'a RunFrame, node: &NodeId) -> Vec<&'a ChildLink> {
    frame
        .children
        .iter()
        .filter(|child| child.node.as_ref() == Some(node))
        .collect()
}

/// Where a child stands, as the mark that carries it for the eye and
/// the word that carries it for everybody.
///
/// A child the parent's log has no close for is open and nothing more:
/// how far it has got is on the child's own log, which this run never
/// opens.
fn child_standing(terminal: Option<TerminalState>) -> (StateWord, &'static str) {
    match terminal {
        None => (StateWord::Run, "still open"),
        Some(TerminalState::Done) => (StateWord::Done, closed_as(TerminalState::Done)),
        Some(TerminalState::Failed) => (StateWord::Fail, closed_as(TerminalState::Failed)),
        Some(TerminalState::Cancelled) => (StateWord::Fail, closed_as(TerminalState::Cancelled)),
        Some(TerminalState::Promoted) => (StateWord::Wait, closed_as(TerminalState::Promoted)),
    }
}

/// The word a run's close is named by, wherever a surface names one:
/// the child under the node that bore it, and the event line that
/// records either.
///
/// A run's terminal state is its own vocabulary, beside the one a
/// node's state is read in ([`crate::render::StateWord`]) — a run
/// promotes and a node never does — so it is said here, once, for every
/// surface that says it.
pub(super) fn closed_as(state: TerminalState) -> &'static str {
    match state {
        TerminalState::Done => "finished",
        TerminalState::Failed => "failed",
        TerminalState::Cancelled => "cancelled",
        TerminalState::Promoted => "promoted",
    }
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

/// The rows a node leaves behind when it stops working: its final state
/// and what that state carries, with the children it bore under it.
///
/// The children come with it because they leave the region with it. A
/// node in the region carries its own tree; a node that graduated
/// carries it into the scrollback, where the run's composition stays
/// readable after the node that composed it is gone.
pub(super) fn graduation(frame: &RunFrame, node: &NodeFrame, glyphs: Glyphs) -> Vec<String> {
    let state = standing(node);
    let elapsed = node
        .elapsed
        .map(|elapsed| format!(" · {}", format_duration(elapsed)))
        .unwrap_or_default();
    let mut rows = vec![format!(
        "{} {} — {}{elapsed}",
        glyphs.state(state.word),
        node.id,
        state.label()
    )];
    let detail = indent(DETAIL_DEPTH);
    rows.extend(
        children_of(frame, &node.id)
            .into_iter()
            .map(|child| format!("{detail}{}", child_row(child, glyphs))),
    );
    rows
}

/// The ids of the nodes at work — what the history has to forget,
/// because a node back at work will stop again and owes a line for it.
pub(super) fn working_nodes(frame: &RunFrame) -> Vec<&NodeId> {
    working(frame).into_iter().map(|node| &node.id).collect()
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

#[cfg(test)]
mod tests {
    use yunta_testkit::{child_link, run_frame};

    use super::*;

    const RUN: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P5");
    const FIRST: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P6");
    const SECOND: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P7");

    /// The link `run_id` left on this run's log: born under `node`, and
    /// closed `terminal` or still open.
    fn child(
        run_id: &RunId,
        node: Option<&'static str>,
        terminal: Option<TerminalState>,
    ) -> ChildLink {
        child_link(run_id, node.map(NodeId::from_static).as_ref(), terminal)
    }

    /// A run that bore `children` and declares no nodes: what these
    /// tests read is the children, and the rest of the frame is the
    /// base every surface test shares.
    fn composed(children: Vec<ChildLink>) -> RunFrame {
        RunFrame {
            children,
            ..run_frame(&RUN)
        }
    }

    #[test]
    fn every_child_is_grouped_under_the_node_that_bore_it() {
        let frame = composed(vec![
            child(&FIRST, Some("compose"), None),
            child(&SECOND, Some("compose"), Some(TerminalState::Done)),
        ]);
        let grouped = children_by_node(&frame);
        assert_eq!(grouped.len(), 1, "{grouped:?}");
        let (under, born) = grouped.first().expect("the one group");
        assert_eq!(under, "node `compose`");
        assert_eq!(
            born.iter().map(|child| &child.run_id).collect::<Vec<_>>(),
            vec![&FIRST, &SECOND],
            "in the order the parent's log recorded them"
        );
    }

    #[test]
    fn a_child_the_log_recorded_under_no_node_is_said_to_have_none() {
        let frame = composed(vec![child(&FIRST, None, None)]);
        let grouped = children_by_node(&frame);
        assert_eq!(
            grouped.first().map(|(under, _)| under.as_str()),
            Some(NO_NODE),
            "a group named for what the log does not say, never for a node it might not \
             belong to: {grouped:?}"
        );
    }

    #[test]
    fn a_child_row_says_where_the_child_stands_before_which_child_it_is() {
        let open = child(&FIRST, Some("compose"), None);
        let row = child_row(&open, Glyphs::Ascii);
        assert_eq!(row, format!("> still open · child run {FIRST}"));
        assert!(
            row.find("still open") < row.find(FIRST.as_str()),
            "a row cut to a narrow terminal loses which child, never that there is one: {row}"
        );
    }

    #[test]
    fn every_way_a_child_can_close_is_read_as_the_word_its_own_run_closed_with() {
        for (terminal, word) in [
            (TerminalState::Done, "finished"),
            (TerminalState::Failed, "failed"),
            (TerminalState::Cancelled, "cancelled"),
            (TerminalState::Promoted, "promoted"),
        ] {
            let closed = child(&FIRST, Some("compose"), Some(terminal));
            assert!(
                child_row(&closed, Glyphs::Ascii).contains(word),
                "{terminal:?} reads as something other than `{word}`"
            );
        }
    }
}
