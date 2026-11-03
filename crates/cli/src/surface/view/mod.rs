//! How a [`RunFrame`] reads as the rows a person watches: the line that
//! says whether the run needs them, the line each working node gets, and
//! the counters under it all.
//!
//! Every surface here draws the same frame, so this is where the words
//! for it are chosen once. What the words are *drawn with* — the glyphs,
//! the column widths, the duration format — belongs to
//! [`crate::render`], and nothing here reimplements a piece of it.

use std::time::Duration;

use surco::words::{Standing, NOTHING_NEEDS_YOU};
use yunta_core::RunId;
use yunta_engine::{NodeFrame, NodeStanding, RunFrame};
use yunta_render::spend::Breakdown;

mod children;

use children::children_of;
pub(crate) use children::{child_row, children_by_node};
pub(super) use children::{child_standing, closed_as};

use crate::commands::advice;
use surco::figures::duration;
use surco::glyphs::{Glyphs, Mark};
use surco::line::{Line, Tone};
use surco::look::Look;
use surco::surface::Terminal;
use surco::width::{cell_width, indent, CHILD_DEPTH};

/// How deep a node's detail sits under the node's own row, in steps of
/// [`indent`] — the step every surface here shares, so the detail lines
/// up with the blocks a run's other surfaces nest.
const DETAIL_DEPTH: usize = 2;

/// The row that opens the region while the frame stands as
/// `Standing::NeedsYou`: the command that answers it, and what it is about.
/// Any other standing draws none, since the footer says that nothing needs
/// the person, or that the run waits for a reset: a pause a person chose
/// stands as `Standing::Stopped`, and a reset that resumes on its own as
/// `Standing::Waiting`. A row saying nothing would cost the work a row.
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
    if !matches!(frame.standing, Standing::NeedsYou) {
        return None;
    }
    let on = advice::parked(&frame.phase)?;
    Some(
        Line::new()
            .push(
                Tone::NeedsYou,
                format!(
                    "{} {}",
                    glyphs.mark(Mark::NeedsYou),
                    Standing::NeedsYou.word()
                ),
            )
            .plain(": ")
            .push(Tone::Strong, answer_command(run_id, answerable))
            .push(Tone::Muted, format!(" ({})", advice::parked_on(on))),
    )
}

/// The region's last row, which answers on every redraw: the run's furrow
/// when the row has room for it, then how far the run has come, what it has
/// spent, how long it has taken — beside how long this workflow usually
/// takes, when its history says — and whether anything needs the person.
///
/// It is drawn for every phase, including the ones that need nobody,
/// because a surface that says so only when there is something to say
/// makes its absence mean two things at once — nothing is needed, or the
/// surface has not drawn yet. The words are measured first, and the furrow
/// takes what the row leaves: whole when it fits, squeezed when the row
/// leaves it [`FURROW_LEAST`] cells, and left out on a row that leaves fewer.
pub(super) fn footer(frame: &RunFrame, look: &Look) -> Line {
    let width = look.width.cells();
    let words = footer_words(frame, look.glyphs, width);
    let room = width.saturating_sub(cell_width(&words.text()) + 1);
    let nodes: Vec<Standing> = frame.nodes.iter().map(|node| node.standing).collect();
    let terminal = Terminal::on(*look);
    let whole = terminal.furrow(&nodes, usize::MAX);
    let furrow = match cell_width(&whole.text()) {
        0 => return words,
        drawn if drawn <= room => whole,
        _ if room >= FURROW_LEAST => terminal.furrow(&nodes, room),
        _ => return words,
    };
    furrow.plain(" ").then(words)
}

/// The fewest cells a furrow squeezed into a footer is drawn in: fewer say
/// nothing of where the nodes stand, and the words keep the row.
const FURROW_LEAST: usize = 8;

/// What a footer says before its answer, in the order it is dropped from a
/// row too narrow for it: the tokens first, then the usual time, then the
/// elapsed time, then the counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Said {
    Tokens,
    Usually,
    Elapsed,
    Counter,
}

/// The footer's words within `width` cells, in the order a reader reads
/// them: the counter, what the run has spent, how long it has taken and
/// usually takes, and last the word that answers whether anything needs the
/// person. A row too narrow for them drops what it can spare in [`Said`]'s
/// order; the answer is never dropped.
fn footer_words(frame: &RunFrame, glyphs: Glyphs, width: usize) -> Line {
    let sep = format!(" {} ", glyphs.sep());
    let mut said = vec![(Said::Counter, crate::render::counter::line(frame, glyphs))];
    if frame.tokens.total() > 0 {
        said.push((Said::Tokens, Breakdown(&frame.tokens).to_string()));
    }
    if let Some(elapsed) = frame.elapsed {
        said.push((Said::Elapsed, duration(elapsed)));
    }
    if let Some(usually) = frame
        .prior
        .as_ref()
        .and_then(|prior| prior.wall_clock_secs.as_ref())
    {
        said.push((
            Said::Usually,
            format!(
                "usually ~{}",
                duration(Duration::from_secs_f64(usually.median))
            ),
        ));
    }
    let (tone, answer) = answer(frame);
    let before = |said: &[(Said, String)]| -> String {
        said.iter()
            .map(|(_, words)| format!("{words}{sep}"))
            .collect()
    };
    while cell_width(&format!("{}{answer}", before(&said))) > width {
        let Some(least) = said.iter().map(|(rank, _)| *rank).min() else {
            break;
        };
        said.retain(|(rank, _)| *rank != least);
    }
    Line::new()
        .push(Tone::Muted, before(&said))
        .push(tone, answer)
}

/// The word the footer answers with, in its tone: that the run waits on a
/// clock, that it needs the person, or that nothing does.
fn answer(frame: &RunFrame) -> (Tone, String) {
    match frame.standing {
        Standing::Waiting => (
            Tone::Muted,
            format!("{} for a reset", Standing::Waiting.word()),
        ),
        Standing::NeedsYou => (Tone::NeedsYou, Standing::NeedsYou.word().to_string()),
        _ => (Tone::Muted, NOTHING_NEEDS_YOU.to_string()),
    }
}

/// The command that moves a parked run: an option off its own menu when
/// the run stopped on one, and handing the run back when what stopped it
/// is settled somewhere else — a budget, a scope, an answers file, a
/// review on a forge.
pub(super) fn answer_command(run_id: &RunId, answerable: bool) -> String {
    match answerable {
        true => advice::decide(run_id.handle()),
        false => advice::resume(run_id.handle()),
    }
}

/// The rows one working node takes: what it is, what its tasks are doing,
/// and what its last calls reached for.
///
/// Its liveness is the age of its last event and never a spinner: the age
/// is measured, it grows while the node says nothing, and it is the one
/// signal that can tell a busy node from a stuck one.
pub(super) fn node_rows(frame: &RunFrame, node: &NodeFrame, look: &Look) -> Vec<Line> {
    let glyphs = look.glyphs;
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
    let calls = calls_row(node, look, cell_width(&detail));
    if !calls.is_empty() {
        // The envelope names a node and never a session, so at loop
        // concurrency above one no log can say which of a node's
        // sessions made a call: they are the node's, which is the whole
        // truth the log carries.
        rows.push(Line::new().plain(detail.as_str()).push(Tone::Muted, calls));
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
    let state = crate::render::node_display(node);
    let mark = state.standing.mark();
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
        .push(
            tone,
            format!("{} {}", glyphs.mark(mark), state.standing.word()),
        )
        .plain(" ")
        .push(Tone::Strong, node.id.as_str());
    match rest.is_empty() {
        true => row,
        false => row.push(Tone::Muted, format!("{sep}{}", rest.join(&sep))),
    }
}

/// What this node's calls on the attempt it is running came to, as much as a
/// row has room for past `taken` cells: the oldest let go first, so what it
/// does now is what stays.
fn calls_row(node: &NodeFrame, look: &Look, taken: usize) -> String {
    let room = look.width.cells().saturating_sub(taken);
    let apart = format!(" {} ", look.glyphs.sep());
    let gathered = yunta_render::target::gathered(&node.activity, look.glyphs.times());
    let mut kept: Vec<&str> = Vec::new();
    let mut used = 0;
    for group in gathered.iter().rev() {
        let more = cell_width(group)
            + if kept.is_empty() {
                0
            } else {
                cell_width(&apart)
            };
        if !kept.is_empty() && used + more > room {
            break;
        }
        used += more;
        kept.push(group);
    }
    kept.reverse();
    kept.join(apart.as_str())
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
mod tests;
