//! Where the folded run goes, and the two layouts of one chronicle.
//!
//! A surface does not choose words, render's `chronicle` does, and it
//! does not choose what happened, the engine's own derivation does.
//! What a surface chooses is what to keep and where to put it, and
//! those are the two answers here.

use surco::glyphs::Glyphs;
use surco::line::Line;
use surco::look::Look;
use surco::width::{indent, CHILD_DEPTH};
use yunta_core::events::{children, findings, gates, node, run, session};
use yunta_core::RunId;
use yunta_render::chronicle;
use yunta_replay::frame::{Happening, Moment};

use super::lines::Lines;
use super::region::Region;

/// Where the folded run goes.
pub(super) enum Draw {
    Lines(Lines),
    Live(Box<Region>),
}

impl Draw {
    /// Puts one moment of the run on this surface.
    ///
    /// The two differ in what they keep, never in what they call it: an
    /// append-only reader has no region to have watched, so every
    /// moment is written; a watched terminal keeps above its region
    /// what closed something or asked something of a person, because
    /// the region already shows the rest while it is true.
    ///
    /// `run` is the run, for the command a quoted failure names to read
    /// the rest of what it printed.
    pub(super) fn record(&mut self, moment: &Moment, run: &RunId, glyphs: Glyphs) {
        match self {
            Draw::Lines(lines) => lines.moment(moment, run, glyphs),
            Draw::Live(region) => {
                if kept(&moment.happening) {
                    let rows = graduation(moment, run, &region.look());
                    region.record(&rows);
                }
            }
        }
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
            chronicle::says_anything(happening)
                && !matches!(state, yunta_core::events::NodeState::Running { .. })
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

/// The rows a settled node leaves behind: its own line, what it printed
/// when it failed, and the children it bore indented under it.
///
/// The children come with it because they leave the region with it. A
/// node in the region carries its own tree; a node that graduated
/// carries it into the history, where the run's composition stays
/// readable after the node that composed it is gone.
fn graduation(moment: &Moment, run: &RunId, look: &Look) -> Vec<Line> {
    let mut rows = vec![chronicle::line(moment, look)];
    rows.extend(crate::output::blocks_lines(
        &chronicle::evidence(moment, run),
        look,
    ));
    let Happening::Node(node::happening::Happening::Reached { children, .. }) = &moment.happening
    else {
        return rows;
    };
    let under = indent(CHILD_DEPTH);
    rows.extend(children.iter().map(|child| {
        Line::new()
            .plain(under.as_str())
            .plain(yunta_render::children::child_row(child, look.glyphs))
    }));
    rows
}

#[cfg(test)]
mod tests;
