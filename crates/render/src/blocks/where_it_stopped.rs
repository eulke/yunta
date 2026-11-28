//! Why a run stands with nothing to answer here, as its standstill's pane says it: what it
//! waits on, in the log's words, then what moves it again; or where it stopped, and why.
//!
//! A pause no option here answers says what it waits on and that it is settled where it
//! was raised, then handed back. No room on the disk says what is free against what is
//! needed: what must stay free and what going on takes. Nodes left half done say each node
//! as it was left, and the branch their work stands on: what to check before going on. A
//! stop says each node it held as it was left, then why, in the log's words; a run no
//! process drives says the nodes at work when it went quiet, then that nothing drives it.
//! A node as it was left is its mark and its name, as the nodes beside the pane list it,
//! then its attempt and how long it worked.
//!
//! The facts of a block stand on labelled rows, each label in one column as wide as the
//! widest of them and two cells, as Surco lays labelled rows out. Where a pane is short of
//! rows, what the run waits on gives way whole ([`WhereItStopped::least`]): what stopped it
//! and what moves it again stay.

use std::time::Duration;

use surco::components::{Labelled, LabelledRow};
use surco::doc::Block;
use surco::figures::{duration, Bytes};
use surco::glyphs::{Glyphs, Mark};
use surco::line::{Line, Tone};
use surco::words::Standing;
use yunta_core::events::PauseReason;
use yunta_core::{Isolation, Manifest, NodeId};
use yunta_replay::frame::{NodeFrame, NodeStanding, RunFrame, RunTimes};

use super::how_it_stands::WAITS_ON;
use crate::page::run::facts;
use crate::watched::listed_as;

/// What a pause no option here answers asks of a person instead.
const NO_OPTION_HERE: &str =
    "no option answers it here: settle it where it was raised, then hand it back";

/// The words over the nodes a stop held, or that were at work when no process drove on.
const WHERE_IT_STOPPED: &str = "where it stopped";

/// The words over the nodes the engine left half done.
const LEFT_HALF_DONE: &str = "left half done";

/// The label of why a run stopped.
const WHY: &str = "why";

/// The label of what the disk has free.
const FREE: &str = "free";

/// The label of what must be free for the run to go on.
const NEEDED: &str = "needed";

/// Why a run stands with nothing to answer here, as its standstill's pane says it. Not
/// [`super::stop`], why a task stopped, nor a pause's one line ([`crate::pause::sentence`]),
/// which this says under its heading.
pub enum WhereItStopped {
    /// A pause no option here answers: what it waits on.
    Unanswered { waits_on: String },
    /// No room on the disk: what it waits on, then what the disk has free against what must
    /// be free for the run to go on, in bytes.
    NoRoom {
        waits_on: String,
        free: u64,
        needed: u64,
    },
    /// Nodes left half done: what it waits on, then each node as it was left, and the branch
    /// their work stands on.
    HalfDone {
        waits_on: String,
        left: Vec<AsLeft>,
        branch: String,
    },
    /// A stop, or no process driving the run: each node it held as it was left, and why.
    Stopped { held: Vec<AsLeft>, why: String },
}

/// A node as a standstill left it: the mark the nodes beside the pane give it, its name, the
/// attempt it was on and how long it worked. Not a [`NodeFrame`], all its log says of it.
pub struct AsLeft {
    mark: Mark,
    node: NodeId,
    attempt: u32,
    worked: Option<Duration>,
}

impl WhereItStopped {
    /// Why the run framed as `frame`, of the run created as `manifest` froze it, stands with
    /// nothing to answer here: no process driving it where it stalled, else the pause it
    /// stands at, each node it held saying how long it worked as `times` measures it. None
    /// for a run that neither stalled nor stands at a pause.
    pub fn of(frame: &RunFrame, times: &RunTimes, manifest: &Manifest) -> Option<WhereItStopped> {
        if frame.standing == Standing::Stalled {
            let at_work = frame
                .nodes
                .iter()
                .filter(|node| node.standing == Standing::Running);
            return Some(WhereItStopped::Stopped {
                held: at_work.filter_map(|node| AsLeft::of(node, times)).collect(),
                why: crate::engine::STALLED.to_string(),
            });
        }
        let pause = frame.phase.waiting_on()?.reason()?;
        Some(WhereItStopped::at(pause, frame, times, manifest))
    }

    /// Why the run framed as `frame`, of the run created as `manifest` froze it, stands at
    /// `pause` with nothing to answer here: what it waits on, then, by the pause, what moves
    /// it again, or where it stopped and why.
    fn at(
        pause: &PauseReason,
        frame: &RunFrame,
        times: &RunTimes,
        manifest: &Manifest,
    ) -> WhereItStopped {
        let waits_on = crate::pause::sentence(pause);
        match pause {
            PauseReason::NoRoom {
                available,
                need,
                keep_free,
            } => WhereItStopped::NoRoom {
                waits_on,
                free: *available,
                needed: need.saturating_add(*keep_free),
            },
            PauseReason::UncertainOrphans { nodes } | PauseReason::Blocked { on: nodes, .. } => {
                let left = nodes.iter().filter_map(|id| {
                    let node = frame.nodes.iter().find(|node| node.id == *id)?;
                    AsLeft::of(node, times)
                });
                WhereItStopped::HalfDone {
                    waits_on,
                    left: left.collect(),
                    branch: branch(frame, manifest),
                }
            }
            PauseReason::Cancelled
            | PauseReason::CancelledAfterCrash
            | PauseReason::GateAborted { .. }
            | PauseReason::Recorded { .. } => {
                let held = frame.nodes.iter().filter(|node| held_by_the_pause(node));
                WhereItStopped::Stopped {
                    held: held.filter_map(|node| AsLeft::of(node, times)).collect(),
                    why: waits_on,
                }
            }
            PauseReason::Escalation(_)
            | PauseReason::ChildPaused { .. }
            | PauseReason::NodeFailed { .. }
            | PauseReason::BudgetExhausted { .. }
            | PauseReason::LoopOverrun { .. }
            | PauseReason::ExternalGate { .. }
            | PauseReason::PullRequestClosed { .. }
            | PauseReason::AwaitingReset { .. }
            | PauseReason::Questions { .. }
            | PauseReason::AnswersRefused { .. } => WhereItStopped::Unanswered { waits_on },
        }
    }

    /// What it says, top to bottom, its marks drawn in `glyphs`: what the run waits on under
    /// its heading, then what moves it again; or where it stopped and why.
    pub fn blocks(&self, glyphs: Glyphs) -> Vec<Block> {
        let waits_on = match self {
            WhereItStopped::Unanswered { waits_on }
            | WhereItStopped::NoRoom { waits_on, .. }
            | WhereItStopped::HalfDone { waits_on, .. } => vec![
                Block::Heading(WAITS_ON.to_string()),
                Block::Lines(vec![Line::new().plain(waits_on.as_str())]),
            ],
            WhereItStopped::Stopped { .. } => Vec::new(),
        };
        waits_on.into_iter().chain(self.least(glyphs)).collect()
    }

    /// What still says it where its pane is short of rows: what the run waits on gives way
    /// whole, what stopped it and what moves it again stay.
    pub fn least(&self, glyphs: Glyphs) -> Vec<Block> {
        match self {
            WhereItStopped::Unanswered { .. } => {
                vec![Block::Rows(vec![Line::new().plain(NO_OPTION_HERE)])]
            }
            WhereItStopped::NoRoom { free, needed, .. } => vec![labelled(vec![
                (muted(FREE), Bytes(*free).to_string()),
                (muted(NEEDED), Bytes(*needed).to_string()),
            ])],
            WhereItStopped::HalfDone { left, branch, .. } => {
                let rows = left.iter().map(|left| left.row(glyphs));
                let branch = (muted(facts::BRANCH), branch.clone());
                vec![
                    Block::Heading(LEFT_HALF_DONE.to_string()),
                    labelled(rows.chain(std::iter::once(branch)).collect()),
                ]
            }
            WhereItStopped::Stopped { held, why } => {
                let rows = held.iter().map(|held| held.row(glyphs));
                let why = (muted(WHY), why.clone());
                vec![
                    Block::Heading(WHERE_IT_STOPPED.to_string()),
                    labelled(rows.chain(std::iter::once(why)).collect()),
                ]
            }
        }
    }
}

impl AsLeft {
    /// `node` as a standstill left it, how long it worked as `times` measures it: none for
    /// a node its log never started.
    fn of(node: &NodeFrame, times: &RunTimes) -> Option<AsLeft> {
        Some(AsLeft {
            mark: listed_as(node).mark(),
            node: node.id.clone(),
            attempt: node.attempt?,
            worked: times.node(&node.id).elapsed,
        })
    }

    /// Its labelled row: its mark and its name, then its attempt and how long it worked.
    fn row(&self, glyphs: Glyphs) -> (Line, String) {
        let label = Line::new()
            .push(Tone::of(self.mark), glyphs.mark(self.mark).to_string())
            .plain(" ")
            .plain(self.node.as_str());
        let attempt = format!("attempt {}", self.attempt);
        let value = match self.worked {
            Some(worked) => format!("{attempt}, after {}", duration(worked)),
            None => attempt,
        };
        (label, value)
    }
}

/// Whether `node` was at work when its run paused, and the pause holds it there.
fn held_by_the_pause(node: &NodeFrame) -> bool {
    match &node.state {
        NodeStanding::Paused { .. } => true,
        NodeStanding::ToGo
        | NodeStanding::Skipped
        | NodeStanding::LeftOut(_)
        | NodeStanding::Reached(_) => false,
    }
}

/// The branch the work of the run framed as `frame` stands on, as `manifest` froze where it
/// works: its own, off the branch it started from, where it works in a checkout of its own;
/// else the branch it started from, in the person's checkout.
fn branch(frame: &RunFrame, manifest: &Manifest) -> String {
    match manifest.isolation {
        Isolation::Worktree => facts::off(&frame.run_id.branch(), &manifest.base_branch),
        Isolation::None => manifest.base_branch.clone(),
    }
}

/// `label` as a row's label, muted.
fn muted(label: &str) -> Line {
    Line::new().push(Tone::Muted, label)
}

/// `rows`, each a label and its value, the labels in one column as Surco lays labelled rows
/// out: two cells past the widest of those that stand.
fn labelled(rows: Vec<(Line, String)>) -> Block {
    let rows = rows.into_iter().map(|(label, value)| LabelledRow {
        label,
        value: Some(value),
    });
    Block::Labelled(Labelled {
        rows: rows.collect(),
    })
}
