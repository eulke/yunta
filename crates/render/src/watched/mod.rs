//! How a moving run's [`RunFrame`] reads as the rows a person watches: on the CLI's region
//! the line that says whether the run needs them, the line each working node gets with what
//! its session last said under it, and under it all the run's gist ([`gist()`]) and what it
//! has spent ([`spent()`]); across a run's screen in the application its headline
//! ([`headline`]), its ribbon with each stretch named ([`ribbon`], [`stretch_names`]) and its
//! gist, beside its pane its nodes ([`nodes()`]), above a node's work standing still what
//! holds it and what moves it again ([`still()`]), and under it what it spent.
//!
//! Every surface that watches a run draws the same frame, the terminal's
//! live region and the run as the user interface follows it, so this is
//! where the words for it are chosen once. What the words are *drawn with*,
//! the glyphs, the column widths and the duration format (Surco's
//! `figures`), is the look the caller hands in, and nothing here
//! reimplements a piece of it.
//!
//! What the run has spent is Surco's `Meter`, named here by what it says rather than by
//! the component that draws it: [`spent()`] hands the figures, and each surface draws them
//! with its own look, the CLI's region and the application's run screen alike.
//!
//! The row that asks for the person is two: the CLI's region opens on [`attention`], which
//! names the command that answers; a screen of the application frames itself with
//! [`needing`], which names the run by its handle and the key that goes to it.

use std::time::Duration;

use surco::motion::Motion;
use surco::words::Standing;
use yunta_core::step::{Choice, NextStep};
use yunta_core::RunId;
use yunta_replay::frame::{NodeFrame, NodeStanding, NodeTimes, RunFrame, RunPhase, RunTimes};

use crate::command;
use crate::engine;
use crate::pause;

mod gist;
mod heard;
mod nodes;
mod said;
mod spent;
mod still;

pub use gist::gist;
pub use heard::Heard;
pub use nodes::nodes;
pub(crate) use nodes::{listed_as, time_taken};
pub use spent::spent;
pub use still::still;

use crate::children::{child_row, children_of};

use surco::components::{Attention, Headline, Ribbon, Stretch};
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
/// Any other standing draws none, since the run's gist says that nothing needs
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

/// The key a screen of the application goes to what needs the person with, and what it
/// does there.
const GO_THERE: (&str, &str) = ("a", "go there");

/// The attention row a screen of the application gives `frame`, a run that needs the
/// person, beside what the clock adds to it (`times`): its handle, what holds it in its
/// gist's words, then the key that goes there.
/// Where a line is typed (`typed`), every key types, `a` among them, so the row names no
/// key. Not [`attention`], the row the CLI's region opens with, which names the command
/// that answers.
pub fn needing(frame: &RunFrame, times: &RunTimes, typed: bool) -> Attention {
    let asks = crate::state::gist(frame, times).unwrap_or_default();
    let (key, does) = GO_THERE;
    let row = match typed {
        true => Attention::unkeyed(asks),
        false => Attention::key(asks, key, does),
    };
    row.of(frame.run_id.handle())
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

/// The rows one working node takes: what it is and how long it has worked and
/// gone quiet as `times` measures it, what its tasks are doing, what one of
/// its sessions said last, and what its last calls reached for.
///
/// A running node's spin turns a frame per event it says, and once it goes
/// long enough without one its row says how long. What a session said moves
/// neither: words are not evidence.
pub fn node_rows(
    frame: &RunFrame,
    node: &NodeFrame,
    times: &NodeTimes,
    said: Option<&yunta_core::port::Said>,
    look: &Look,
) -> Vec<Line> {
    let glyphs = look.glyphs;
    // A `parallel` group's children sit one step under the group, the
    // same step a child run sits under the node that bore it.
    let under = match node.group {
        Some(_) => indent(CHILD_DEPTH),
        None => String::new(),
    };
    let detail = format!("{under}{}", indent(DETAIL_DEPTH));
    let mut rows = vec![own_row(node, times, glyphs, look.motion).under(&under)];
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
/// the runner it resolved through with the candidate behind it, the model its open
/// sessions reported where the candidate names none, each time as `times` measures
/// it. When a row is cut to fit, what it runs on goes first.
fn own_row(node: &NodeFrame, times: &NodeTimes, glyphs: Glyphs, motion: Motion) -> Line {
    let standing = crate::state::NodeDisplay::from(node).standing;
    let running = standing == Standing::Running;
    let quiet = times
        .last_event_age
        .filter(|age| running && *age >= QUIET_AFTER);
    let mut word = standing.word().to_string();
    let mut rest = Vec::new();
    if let Some(elapsed) = times.elapsed {
        match running {
            true => word.push_str(&format!(" for {}", duration(elapsed))),
            false => rest.push(duration(elapsed)),
        }
    }
    if let Some(age) = quiet {
        rest.push(format!("quiet {}", duration(age)));
    }
    if let Some(runner) = &node.runner {
        let reported = node.sessions.iter().find_map(|open| open.model.as_ref());
        let model = runner.chosen.model.as_ref().or(reported);
        let candidate = crate::runner::candidate(&runner.chosen.adapter, model);
        let mut chose = format!("{} {candidate}", runner.runner);
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
    let sep = glyphs.spaced_sep();
    match rest.is_empty() {
        true => row,
        false => row.push(Tone::Muted, format!("{sep}{}", rest.join(&sep))),
    }
}

/// The rows the CLI's region watches a moving run by: the row that asks for the person where
/// something does, and each working node's rows with what its session said last. The region
/// closes them on what the run comes to and what it spent, the run's [`gist()`] and
/// [`spent()`]. Not the run's chronicle, which says what happened, in order; nor what stands
/// across a run's screen in the application, which says the run by its [`headline`], its
/// [`ribbon`] and its gist, and its nodes beside its pane ([`nodes()`]).
pub struct Rows {
    /// The row that asks for the person, or says whom the run's engine asks at its own
    /// terminal; absent while nothing needs them.
    pub attention: Option<Line>,
    /// Each working node's rows, in the workflow's order.
    pub nodes: Vec<Line>,
}

/// The rows of `frame`, the run `run_id`, with what the clock adds to it (`times`) and
/// what `heard` kept of what its sessions said. `answerable` says whether the run stopped
/// on a menu of its own, which decides the command the attention row offers; a run whose
/// engine asks at its own terminal says so in its place.
pub fn rows(
    frame: &RunFrame,
    times: &RunTimes,
    run_id: &RunId,
    answerable: bool,
    heard: &Heard,
    look: &Look,
) -> Rows {
    let attention = match &times.asking {
        Some(asking) => asked(asking, look),
        None => attention(frame, run_id, answerable, look),
    };
    let nodes = working(frame)
        .into_iter()
        .flat_map(|node| {
            let said = heard.of(run_id, &node.id);
            node_rows(frame, node, &times.node(&node.id), said, look)
        })
        .collect();
    Rows { attention, nodes }
}

/// The run's signature at the top of a screen that watches it: each node's stretch in the
/// workflow's order, named by [`stretch_names`], and, where the view moves, the plow along the
/// running node's stretch a cell for each event it has had.
pub fn ribbon(frame: &RunFrame, motion: Motion) -> Ribbon {
    let ids: Vec<&str> = frame.nodes.iter().map(|node| node.id.as_str()).collect();
    let stretches = frame
        .nodes
        .iter()
        .zip(stretch_names(&ids))
        .map(|(node, name)| Stretch::new(name, node.standing));
    let ribbon = Ribbon::new(stretches);
    let running = frame
        .nodes
        .iter()
        .find(|node| node.standing == Standing::Running);
    match (motion, running) {
        (Motion::Moves, Some(node)) => ribbon.plowed(node.events),
        (Motion::Moves, None) | (Motion::Still, _) => ribbon,
    }
}

/// The name over each stretch of a ribbon, for `ids`, a workflow's node ids in the order it
/// declares them: the part of an id before `@`, else its first word split at `-`, where no
/// stretch before it in that order has that name; else its last word, where none has that;
/// else its whole id. A ribbon names its stretches short so that a workflow's whole row of
/// them fits a screen: fragua's fifteen take 95 cells. Not a node's id, which a node's own
/// row and the trail say whole.
pub fn stretch_names(ids: &[&str]) -> Vec<String> {
    let mut names: Vec<String> = Vec::with_capacity(ids.len());
    for &id in ids {
        let first = match id.split_once('@') {
            Some((before, _)) => before,
            None => id.split('-').next().unwrap_or(id),
        };
        let last = id.rsplit('-').next().unwrap_or(id);
        let free = |name: &&str| !names.iter().any(|taken| taken == name);
        let name = [first, last].into_iter().find(free).unwrap_or(id);
        names.push(name.to_string());
    }
    names
}

/// The headline across a watched run's screen: the run's mark, its workflow and mode, its
/// word, how long it has stood there, and its handle at the far end. An open run has stood
/// there `for` as long as a person sees it hold: since it was asked at a terminal, since its
/// last event where it parked or went quiet, since it started where it moves. A closed one
/// took what it took `in` its whole span. The handle gives way first on a row too narrow
/// for the whole headline, since the screen's trail names the run too. Not
/// [`Run::headline`](crate::page::Run::headline), a printed page's, which keeps its handle,
/// says how long ago a closed run stopped and that a stalled one went quiet. Each time is as
/// `times` measures it.
pub fn headline(frame: &RunFrame, times: &RunTimes) -> Headline {
    let word = frame.standing;
    let headline = Headline::new(
        word.mark(),
        crate::state::called(&frame.workflow, &frame.mode),
        word.word(),
    );
    stood(frame, times)
        .into_iter()
        .fold(headline, Headline::fact)
        .edge_said_elsewhere(frame.run_id.handle())
}

/// How long the run `frame` has stood where it is as `times` measures it, as its headline
/// says it: `for` as long as a person sees an open run hold, `in` the whole span of one that
/// closed.
pub(crate) fn stood(frame: &RunFrame, times: &RunTimes) -> Option<String> {
    let open = matches!(
        frame.phase,
        RunPhase::Created | RunPhase::Running | RunPhase::Waiting { .. }
    );
    match open {
        true => crate::page::run::stood_open(frame, times)
            .map(|stood| crate::page::run::facts::stood_for(duration(stood))),
        false => times.elapsed.map(crate::page::run::facts::closed_in),
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
    let apart = look.glyphs.spaced_sep();
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
