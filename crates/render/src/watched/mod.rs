//! How a moving run's [`RunFrame`] reads as the rows a person watches: the
//! line that says whether the run needs them, the line each working node
//! gets with what its session last said under it, and the counters under
//! it all.
//!
//! Every surface that watches a run draws the same frame, the terminal's
//! live region and the run as the user interface follows it, so this is
//! where the words for it are chosen once. What the words are *drawn with*,
//! the glyphs, the column widths and the duration format (Surco's
//! `figures`), is the look the caller hands in, and nothing here
//! reimplements a piece of it.

use std::time::Duration;

use surco::motion::Motion;
use surco::words::{Standing, NOTHING_NEEDS_YOU};
use yunta_core::step::{Choice, NextStep};
use yunta_core::RunId;
use yunta_replay::frame::{NodeFrame, NodeStanding, RunFrame};

use crate::command;
use crate::engine;
use crate::pause;
use crate::spend::Breakdown;

mod said;

use crate::children::{child_row, children_of};

use surco::components::Attention;
use surco::doc::Block;
use surco::figures::duration;
use surco::glyphs::Glyphs;
use surco::line::{Line, Tone};
use surco::look::Look;
use surco::surface::Terminal;
use surco::width::{cell_width, indent, CHILD_DEPTH};

/// How deep a node's detail sits under the node's own row, in steps of
/// [`indent`] — the step every surface here shares, so the detail lines
/// up with the blocks a run's other surfaces nest.
const DETAIL_DEPTH: usize = 2;

/// The attention row of a run whose engine asks a person at its own terminal,
/// read from its registry by a process that follows it: whom it asks, and that
/// the answer is given there, drawn at `look` as an [`Attention`] answered
/// elsewhere. The engine that asks draws no such row, since its prompt has the
/// terminal.
pub fn asked(asking: &yunta_replay::registry::Asking, look: &Look) -> Option<Line> {
    let row = Attention::elsewhere(engine::asking_on(asking));
    Terminal::on(*look)
        .block(&Block::Attention(row))
        .into_iter()
        .next()
}

/// The row that opens the region while the frame stands as
/// `Standing::NeedsYou`: the command that answers it, and what it is about,
/// drawn at `look` as an [`Attention`] keeps a command when its row is cut.
/// Any other standing draws none, since the footer says that nothing needs
/// the person, or that the run waits for a reset: a pause a person chose
/// stands as `Standing::Stopped`, and a reset that resumes on its own as
/// `Standing::Waiting`. A row saying nothing would cost the work a row.
pub fn attention(frame: &RunFrame, run_id: &RunId, answerable: bool, look: &Look) -> Option<Line> {
    if !matches!(frame.standing, Standing::NeedsYou) {
        return None;
    }
    let on = frame.phase.waiting_on()?;
    let row = Attention::command(pause::parked_on(on), answer_command(run_id, answerable));
    Terminal::on(*look)
        .block(&Block::Attention(row))
        .into_iter()
        .next()
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
/// leaves it the fewest cells a squeezed furrow is drawn in, and left out on a
/// row that leaves fewer.
pub fn footer(frame: &RunFrame, look: &Look) -> Line {
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
    let mut said = vec![(Said::Counter, crate::progress::line(frame, glyphs))];
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
/// is settled somewhere else — a scope, an answers file, a review on a
/// forge, a child.
fn answer_command(run_id: &RunId, answerable: bool) -> String {
    match answerable {
        true => command::line(&NextStep::ResolveGate {
            run: run_id.clone(),
            choice: Choice::Asked,
        }),
        false => command::line(&NextStep::Resume {
            run: run_id.clone(),
        }),
    }
}

/// The rows one working node takes: what it is, what its tasks are doing,
/// what one of its sessions said last, and what its last calls reached for.
///
/// A running node's spin turns a frame per event it says, and once it goes
/// long enough without one its row says how long. What a session said moves
/// neither: words are not evidence.
pub fn node_rows(
    frame: &RunFrame,
    node: &NodeFrame,
    said: Option<&yunta_core::port::Said>,
    look: &Look,
    motion: Motion,
) -> Vec<Line> {
    let glyphs = look.glyphs;
    // A `parallel` group's children sit one step under the group, the
    // same step a child run sits under the node that bore it.
    let under = match node.group {
        Some(_) => indent(CHILD_DEPTH),
        None => String::new(),
    };
    let detail = format!("{under}{}", indent(DETAIL_DEPTH));
    let mut rows = vec![headline(node, glyphs, motion).under(&under)];
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
    rows.extend(said.map(|said| said::row(said, &detail)));
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

/// How long a running node goes without an event before its row says so: long
/// enough that a node between two steps is not called quiet.
const QUIET_AFTER: Duration = Duration::from_secs(10);

/// The node's own row: its mark, or the frame of its spin; its id; its state and
/// how long it has worked; how long it has been quiet, past [`QUIET_AFTER`]; and
/// the runner it resolved through with the adapter and model behind it. When a
/// row is cut to fit, what it runs on goes first.
fn headline(node: &NodeFrame, glyphs: Glyphs, motion: Motion) -> Line {
    let standing = crate::state::NodeDisplay::from(node).standing;
    let running = standing == Standing::Running;
    let quiet = node
        .last_event_age
        .filter(|age| running && *age >= QUIET_AFTER);
    let mut word = standing.word().to_string();
    let mut rest = Vec::new();
    if let Some(elapsed) = node.elapsed {
        match running {
            true => word.push_str(&format!(" for {}", duration(elapsed))),
            false => rest.push(duration(elapsed)),
        }
    }
    if let Some(age) = quiet {
        rest.push(format!("quiet {}", duration(age)));
    }
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
    let sign = match (running, motion) {
        (true, Motion::Moves) => surco::motion::frame(glyphs, node.events).to_string(),
        _ => glyphs.mark(standing.mark()).to_string(),
    };
    let tone = Tone::of(standing.mark());
    let sign_tone = if quiet.is_some() { Tone::Muted } else { tone };
    let row = Line::new()
        .push(sign_tone, sign)
        .plain(" ")
        .push(Tone::Strong, node.id.as_str())
        .plain(" ")
        .push(tone, word);
    let sep = format!(" {} ", glyphs.sep());
    match rest.is_empty() {
        true => row,
        false => row.push(Tone::Muted, format!("{sep}{}", rest.join(&sep))),
    }
}

/// The window's title while the run needs its person: the mark, which run, the
/// word, and what it runs.
pub fn title(frame: &RunFrame, run_id: &RunId, glyphs: Glyphs) -> String {
    format!(
        "{} {} {} {} {} ({})",
        glyphs.mark(Standing::NeedsYou.mark()),
        run_id.handle(),
        Standing::NeedsYou.word(),
        glyphs.sep(),
        frame.workflow,
        frame.mode,
    )
}

/// What this node's calls on the attempt it is running came to, as much as a
/// row has room for past `taken` cells: the oldest let go first, so what it
/// does now is what stays.
fn calls_row(node: &NodeFrame, look: &Look, taken: usize) -> String {
    let room = look.width.cells().saturating_sub(taken);
    let apart = format!(" {} ", look.glyphs.sep());
    let gathered = crate::target::gathered(&node.activity, look.glyphs.times());
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
pub fn working(frame: &RunFrame) -> Vec<&NodeFrame> {
    frame
        .nodes
        .iter()
        .filter(|node| {
            matches!(
                node.state,
                NodeStanding::Reached(
                    yunta_core::events::NodeState::Running { .. }
                        | yunta_core::events::NodeState::Waiting { .. }
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
