//! The words for one moment, chosen once.
//!
//! Every surface that says what happened says it from here: the
//! append-only lines a pipe gets, and the history a watched terminal
//! keeps above its region. They differ in what they *keep* and in how
//! they lay it out, never in what a thing is called — so a reader who
//! followed a run on a terminal and a reader who read the same run out
//! of a CI log met the same sentences.
//!
//! Every state word comes from `render::state::NodeDisplay`, every
//! run or child close from `view::closed_as`, every capability from
//! `Capability::as_str`. Nothing here formats a domain type with
//! `Debug`: a reader is owed a word, not a Rust identifier.

mod baseline_words;
mod capability_words;
mod format_words;
mod rail_words;
mod scope_words;
mod words;

use yunta_core::events::{children, findings, gates, node, run, session};
use yunta_core::text::{aside, one_line};
use yunta_engine::{Happening, Moment};

use super::view;
use crate::commands::advice;
use crate::render::blocks::{Drawn, FailureDetail, Whole};
use crate::render::ink::{Line, Tone};
use crate::render::{indent, Glyphs, Look, Mark, CHILD_DEPTH};
use words::carried;

/// One moment as a surface says it, before any layout decides where it
/// goes.
pub(super) struct Said {
    /// What this moment is marked with, when it is about something
    /// reaching a state, asking a person or changing course; `None` for a
    /// moment that only reports.
    pub(super) mark: Option<Mark>,
    pub(super) text: String,
}

/// The words for `moment`. One match per domain, and the only ones.
pub(super) fn say(moment: &Moment, glyphs: Glyphs) -> Said {
    let subject = match &moment.node {
        Some(node) => node.to_string(),
        None => "run".to_string(),
    };
    let (mark, carried) = carried(&moment.happening, glyphs.sep());
    Said {
        mark,
        text: aside(subject, &one_line(&carried)),
    }
}

/// Whether a terminal a person is watching keeps this above its region.
///
/// What it keeps is what closed something, what asked something of a
/// person, and the failures a person can act on: the rest is the run
/// working, and the region already shows that while it is true. A run's
/// own close is the one thing that closes and is not kept — the block
/// that reports it is its record, and a line above the region would say
/// it twice.
pub(super) fn kept(happening: &Happening) -> bool {
    match happening {
        Happening::Run(run::happening::Happening::Paused { .. })
        | Happening::Run(run::happening::Happening::Resumed { .. })
        | Happening::Run(run::happening::Happening::PromotionSignaled { .. })
        | Happening::Run(run::happening::Happening::HostSuspended { .. }) => true,
        Happening::Run(_) => false,
        Happening::Node(node::happening::Happening::Reached { state, .. }) => {
            !matches!(state, yunta_core::events::NodeState::Running { .. })
        }
        Happening::Node(node::happening::Happening::Rerouted(_)) => true,
        Happening::Node(_) => false,
        Happening::Session(session::happening::Happening::Degraded { .. })
        | Happening::Session(session::happening::Happening::RailFailed { .. })
        | Happening::Session(session::happening::Happening::RunToolFailed { .. })
        | Happening::Session(session::happening::Happening::RunToolRefused { .. })
        | Happening::Session(session::happening::Happening::CutOff { .. })
        | Happening::Session(session::happening::Happening::Reconnected { .. }) => true,
        // A formatter the bound stopped is the project's to fix; one that
        // finished is the run working.
        Happening::Session(session::happening::Happening::Formatted { timed_out, .. }) => {
            *timed_out
        }
        Happening::Session(_) => false,
        Happening::Gates(gates::happening::Happening::Escalated(_))
        | Happening::Gates(gates::happening::Happening::Resolved(_))
        | Happening::Gates(gates::happening::Happening::Asked { .. })
        | Happening::Gates(gates::happening::Happening::Answered { .. }) => true,
        // What asks the person is on their screen already; the line is
        // the answer's.
        Happening::Gates(gates::happening::Happening::AskingOpened) => false,
        Happening::Findings(findings::happening::Happening::Finding { change, .. }) => matches!(
            change,
            findings::happening::Change::Posted
                | findings::happening::Change::Withdrawn { .. }
                | findings::happening::Change::Answered { .. }
                | findings::happening::Change::Proved { .. }
                | findings::happening::Change::Settled { .. }
        ),
        Happening::Children(children::happening::Happening::Closed { .. }) => true,
        Happening::Children(_) => false,
        Happening::Unknown { .. } => true,
        Happening::Tasks(_) | Happening::Scope(_) | Happening::Artifacts(_) => false,
    }
}

/// The line one moment is said on, marked when it carries a mark.
pub(super) fn line(moment: &Moment, look: &Look) -> Line {
    let said = say(moment, look.glyphs);
    let line = match said.mark {
        Some(mark) => Line::new()
            .push(Tone::of(mark), look.glyphs.mark(mark).to_string())
            .plain(" "),
        None => Line::new(),
    };
    line.plain(said.text)
}

/// The rows a settled node leaves behind: its own line, what it printed
/// when it failed, and the children it bore indented under it.
///
/// The children come with it because they leave the region with it. A
/// node in the region carries its own tree; a node that graduated
/// carries it into the history, where the run's composition stays
/// readable after the node that composed it is gone.
pub(super) fn graduation(moment: &Moment, run: &str, look: &Look) -> Vec<Line> {
    let mut rows = vec![line(moment, look)];
    rows.extend(evidence(moment, run, look));
    let Happening::Node(node::happening::Happening::Reached { children, .. }) = &moment.happening
    else {
        return rows;
    };
    let under = indent(CHILD_DEPTH);
    rows.extend(children.iter().map(|child| {
        Line::new()
            .plain(under.as_str())
            .plain(view::child_row(child, look.glyphs))
    }));
    rows
}

/// What a node that failed printed, quoted under the line that says it
/// failed, and the command that shows all of it — the reason is read
/// where the failure is, not one command away. `run` is what the run is
/// called by. Nothing for any other moment.
pub(super) fn evidence(moment: &Moment, run: &str, look: &Look) -> Vec<Line> {
    let Happening::Node(node::happening::Happening::Reached {
        state: yunta_core::events::NodeState::Failed { failure, .. },
        ..
    }) = &moment.happening
    else {
        return Vec::new();
    };
    let whole = failure.output().map(|_| {
        Whole::Command(match &moment.node {
            Some(node) => advice::status_node(run, node.as_str()),
            None => advice::status(run),
        })
    });
    FailureDetail { failure, whole }.lines(look)
}

#[cfg(test)]
mod tests;
