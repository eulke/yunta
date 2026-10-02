//! How a [`RunFrame`] reads as the rows a person watches: the line that
//! says whether the run needs them, the line each working node gets, and
//! the counters under it all.
//!
//! Every surface here draws the same frame, so this is where the words
//! for it are chosen once. What the words are *drawn with* — the glyphs,
//! the column widths, the duration format — belongs to
//! [`crate::render`], and nothing here reimplements a piece of it.

use std::time::Duration;

use yunta_engine::{NodeFrame, NodeStanding, RunFrame};

use yunta_core::RunId;

mod children;

use children::children_of;
pub(super) use children::{child_row, child_standing, children_by_node, closed_as};

use crate::commands::advice;
use crate::render::ink::{Line, Tone};
use crate::render::{duration, indent, Glyphs, Mark, NodeDisplay, Tokens, CHILD_DEPTH};

/// How deep a node's detail sits under the node's own row, in steps of
/// [`indent`] — the step every surface here shares, so the detail lines
/// up with the blocks a run's other surfaces nest.
const DETAIL_DEPTH: usize = 2;

/// The row that opens the region while something needs the person:
/// the command that answers it, and what it is about. `None` while the
/// run needs nobody — the footer says so, and a row saying nothing would
/// cost the work a row.
///
/// The command comes before what it is about, so that a row cut to fit
/// loses the subject and never the thing to run. A reader who lost the
/// subject still has `yunta status`; a reader who lost the command has
/// nothing.
pub(super) fn attention(
    frame: &RunFrame,
    run_id: &RunId,
    answerable: bool,
    glyphs: Glyphs,
) -> Option<Line> {
    let on = advice::parked(&frame.phase)?;
    Some(
        Line::new()
            .push(
                Tone::NeedsYou,
                format!("{} needs you", glyphs.mark(Mark::NeedsYou)),
            )
            .plain(": ")
            .push(Tone::Strong, answer_command(run_id, answerable))
            .push(Tone::Muted, format!(" ({})", advice::parked_on(on))),
    )
}

/// The region's last row, which answers on every redraw: how far the run
/// has come, what it has spent, how long it has taken — beside how long
/// this workflow usually takes, when its history says — and whether
/// anything needs the person.
///
/// It is drawn for every phase, including the ones that need nobody,
/// because a surface that says so only when there is something to say
/// makes its absence mean two things at once — nothing is needed, or the
/// surface has not drawn yet.
pub(super) fn footer(frame: &RunFrame, glyphs: Glyphs) -> Line {
    let sep = format!(" {} ", glyphs.sep());
    let mut said = vec![crate::render::counter::line(frame)];
    let spent = frame.tokens.input + frame.tokens.output;
    if spent > 0 {
        said.push(Tokens(spent).to_string());
    }
    if let Some(elapsed) = frame.elapsed {
        said.push(duration(elapsed));
    }
    if let Some(usually) = frame
        .prior
        .as_ref()
        .and_then(|prior| prior.wall_clock_secs.as_ref())
    {
        said.push(format!(
            "usually ~{}",
            duration(Duration::from_secs_f64(usually.median))
        ));
    }
    let line = Line::new().push(Tone::Muted, format!("{}{sep}", said.join(&sep)));
    match advice::parked(&frame.phase) {
        Some(_) => line.push(Tone::NeedsYou, "needs you"),
        None => line.push(Tone::Muted, "nothing needs you"),
    }
}

/// The command that moves a parked run: an option off its own menu when
/// the run stopped on one, and handing the run back when what stopped it
/// is settled somewhere else — a budget, a scope, an answers file, a
/// review on a forge.
pub(super) fn answer_command(run_id: &RunId, answerable: bool) -> String {
    match answerable {
        true => advice::resolve_gate(run_id.handle()),
        false => advice::resume(run_id.handle()),
    }
}

/// The rows one working node takes: what it is, what its tasks are doing,
/// and what its last calls reached for.
///
/// Its liveness is the age of its last event and never a spinner: the age
/// is measured, it grows while the node says nothing, and it is the one
/// signal that can tell a busy node from a stuck one.
pub(super) fn node_rows(frame: &RunFrame, node: &NodeFrame, glyphs: Glyphs) -> Vec<Line> {
    // A `parallel` group's children sit one step under the group, the
    // same step a child run sits under the node that bore it.
    let under = match node.group {
        Some(_) => indent(CHILD_DEPTH),
        None => String::new(),
    };
    let detail = format!("{under}{}", indent(DETAIL_DEPTH));
    let mut rows = vec![headline(node, glyphs).under(&under)];
    if !node.running_tasks.is_empty() {
        rows.push(Line::new().plain(detail.as_str()).push(
            Tone::Muted,
            format!(
                "tasks running: {}",
                join(node.running_tasks.iter().map(|task| match task.in_session {
                    true => task.id.to_string(),
                    false => format!("{} (checking criteria)", task.id),
                }))
            ),
        ));
    }
    let calls = recent_calls(node);
    if !calls.is_empty() {
        // The envelope names a node and never a session, so at loop
        // concurrency above one no log can say which of a node's
        // sessions made a call: they are the node's, which is the whole
        // truth the log carries.
        rows.push(
            Line::new()
                .plain(detail.as_str())
                .push(Tone::Muted, format!("calls: {calls}")),
        );
    }
    rows.extend(children_of(frame, &node.id).into_iter().map(|child| {
        Line::new()
            .plain(detail.as_str())
            .plain(child_row(child, glyphs))
    }));
    rows
}

/// The node's own row: its state, its id, the runner it resolved through
/// with the adapter and model behind it, how long it has been working,
/// and how long ago it last said anything.
fn headline(node: &NodeFrame, glyphs: Glyphs) -> Line {
    let state = NodeDisplay::standing(&node.state);
    let mark = state.word.mark();
    let tone = Tone::of(mark);
    let mut rest = Vec::new();
    if let Some(runner) = &node.runner {
        let mut chose = format!(
            "{} {}/{}",
            runner.runner, runner.chosen.adapter, runner.chosen.model
        );
        if let Some(agent) = &runner.chosen.agent {
            chose.push_str(&format!("/{agent}"));
        }
        rest.push(chose);
    }
    if let Some(elapsed) = node.elapsed {
        rest.push(duration(elapsed));
    }
    if let Some(age) = node.last_event_age {
        rest.push(format!("last event {} ago", duration(age)));
    }
    let sep = format!(" {} ", glyphs.sep());
    let row = Line::new()
        .push(tone, format!("{} {}", glyphs.mark(mark), state.word.word()))
        .plain(" ")
        .push(Tone::Strong, node.id.as_str());
    match rest.is_empty() {
        true => row,
        false => row.push(Tone::Muted, format!("{sep}{}", rest.join(&sep))),
    }
}

/// The tool calls this node made on the attempt it is running, newest
/// first, each with what it acted on, as many as a row has room for.
fn recent_calls(node: &NodeFrame) -> String {
    /// How many calls a row shows. Enough to see what a node is working
    /// through, few enough that the names still fit beside the label.
    const SHOWN: usize = 4;
    join(node.activity.iter().take(SHOWN).map(|call| {
        let tool = call
            .tool_name
            .clone()
            .unwrap_or_else(|| "an unnamed tool".to_string());
        match &call.target {
            Some(target) => format!("{tool} {}", target.sentence()),
            None => tool,
        }
    }))
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

/// Names in a row, separated so a reader's eye stops between them.
fn join(names: impl Iterator<Item = String>) -> String {
    names.collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use yunta_core::NodeId;
    use yunta_engine::RunPhase;
    use yunta_testkit::run_frame;

    use super::*;
    use crate::render::ink::Ink;

    fn drawn(rows: Vec<Line>) -> Vec<String> {
        rows.iter().map(|row| Ink::Plain.paint(row)).collect()
    }

    const RUN: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P5");

    /// A `parallel` group's children sit one step under the group,
    /// exactly where a child run sits under the node that bore it.
    #[test]
    fn the_live_view_indents_a_groups_children_under_it() {
        let frame = run_frame(&RUN);
        let group = yunta_testkit::node_frame(&NodeId::from("review"), NodeStanding::ToGo);
        let mut child = yunta_testkit::node_frame(&NodeId::from("review-a"), NodeStanding::ToGo);
        child.group = Some(group.id.clone());

        let top = drawn(node_rows(&frame, &group, Glyphs::Ascii));
        let under = drawn(node_rows(&frame, &child, Glyphs::Ascii));
        assert!(
            !top[0].starts_with(' '),
            "a top-level node starts at the margin: {top:?}"
        );
        assert!(
            under[0].starts_with(&indent(CHILD_DEPTH)) && !under[0].starts_with(&indent(2)),
            "its children sit exactly one step under it: {under:?}"
        );
    }

    #[test]
    fn a_running_task_no_session_works_is_said_to_be_checking_its_criteria() {
        let frame = run_frame(&RUN);
        let mut node = yunta_testkit::node_frame(&NodeId::from("implement"), NodeStanding::ToGo);
        node.running_tasks = vec![
            yunta_engine::RunningTask {
                id: "T001".into(),
                in_session: false,
            },
            yunta_engine::RunningTask {
                id: "T002".into(),
                in_session: true,
            },
        ];

        let rows = drawn(node_rows(&frame, &node, Glyphs::Ascii));
        assert!(
            rows.iter()
                .any(|row| row.trim() == "tasks running: T001 (checking criteria), T002"),
            "{rows:?}"
        );
    }

    #[test]
    fn a_node_row_names_what_its_calls_touched() {
        let frame = run_frame(&RUN);
        let mut node = yunta_testkit::node_frame(&NodeId::from("implement"), NodeStanding::ToGo);
        let call =
            |tool: &str, target: Option<yunta_core::events::ToolTarget>| yunta_engine::ToolCall {
                tool_name: Some(tool.to_string()),
                target,
                at: chrono::DateTime::UNIX_EPOCH,
            };
        node.activity = vec![
            call(
                "Edit",
                Some(yunta_core::events::ToolTarget::of_path(
                    std::path::Path::new("src/lib.rs"),
                )),
            ),
            call("Bash", None),
        ];

        let rows = drawn(node_rows(&frame, &node, Glyphs::Ascii));
        assert!(
            rows.iter()
                .any(|row| row.trim() == "calls: Edit src/lib.rs, Bash"),
            "{rows:?}"
        );
    }

    /// A run in `phase`, with `prior` as what its workflow's history
    /// says.
    fn standing(phase: RunPhase, prior: Option<yunta_engine::PriorEstimation>) -> RunFrame {
        RunFrame {
            phase,
            prior,
            elapsed: Some(Duration::from_secs(3)),
            ..run_frame(&RUN)
        }
    }

    fn parked() -> RunPhase {
        RunPhase::Waiting {
            on: yunta_engine::WaitingOn::Node {
                node: NodeId::from_static("plan"),
                on: yunta_engine::NodeWait::Gate { external_ref: None },
                reason: None,
            },
        }
    }

    #[test]
    fn the_footer_says_nothing_needs_you_while_the_run_moves() {
        let paint = |frame: &RunFrame| Ink::Plain.paint(&footer(frame, Glyphs::Ascii));
        let moving = paint(&standing(RunPhase::Running, None));
        assert!(moving.ends_with(" nothing needs you"), "{moving}");
        assert!(moving.contains("3s"), "how long it has taken: {moving}");
        let waiting = paint(&standing(parked(), None));
        assert!(
            waiting.ends_with(" needs you") && !waiting.contains("nothing"),
            "{waiting}"
        );
    }

    #[test]
    fn the_attention_row_is_drawn_only_while_something_needs_you() {
        assert!(attention(
            &standing(RunPhase::Running, None),
            &RUN,
            false,
            Glyphs::Ascii
        )
        .is_none());
        let row = attention(&standing(parked(), None), &RUN, true, Glyphs::Ascii)
            .map(|row| Ink::Plain.paint(&row))
            .unwrap_or_default();
        assert!(
            row.contains(&format!(
                "needs you: yunta resolve-gate {} <option>",
                RUN.handle()
            )),
            "it carries the command that answers it, whole: {row}"
        );
        assert!(row.contains("node `plan`"), "and what it is about: {row}");
    }

    #[test]
    fn the_estimate_appears_beside_the_time_once_the_workflow_has_a_history() {
        let paint = |frame: &RunFrame| Ink::Plain.paint(&footer(frame, Glyphs::Ascii));
        let known = yunta_engine::PriorEstimation {
            sample_count: 3,
            tokens: yunta_engine::Percentiles {
                median: 1_000.0,
                p90: 2_000.0,
            },
            wall_clock_secs: Some(yunta_engine::Percentiles {
                median: 6_840.0,
                p90: 9_000.0,
            }),
            tasks: yunta_engine::Percentiles {
                median: 0.0,
                p90: 0.0,
            },
        };
        let first = paint(&standing(RunPhase::Running, None));
        assert!(!first.contains("usually"), "{first}");
        let later = paint(&standing(RunPhase::Running, Some(known)));
        let sep = Glyphs::Ascii.sep();
        assert!(
            later.contains(&format!("3s {sep} usually ~1h54m")),
            "{later}"
        );
    }
}
