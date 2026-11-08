//! The one line a reader meets for a parked run, said from the fact the
//! log holds, and what a parked run waits on at the length each place has
//! room for.
//!
//! Every surface that says why a run stopped (a listing's row, the status
//! heading, the line a resume echoes, the live region's attention row, the
//! block that closes a run out, the JSON a program reads) says it from here,
//! through [`sentence`] and the words beside it, so a reader who met a phrase
//! on one surface meets the same phrase on the next. The fact is stored as
//! data; the words are produced here, where a reader is told.
//!
//! What holds a run whose log says it moves is no pause: [`crate::engine`]
//! says that, from what the run's registry says of its engine.

use surco::figures::{Bytes, Tokens};
use surco::text;
use surco::words::Standing;
use yunta_core::events::{NodeWait, PauseReason};
use yunta_core::step::NextStep;
use yunta_core::{NodeId, QuestionId, RunId};
use yunta_replay::frame::WaitingOn;

use crate::escalation;

/// How a limit's reset moment reads: the day and the minute, in UTC, as a
/// person reads a log's clock.
pub(crate) const RESET_FORMAT: &str = "%Y-%m-%d %H:%M UTC";

/// What a run a person interrupted says of the stop: what the log knows,
/// whoever reads it, a teammate or an agent among them.
pub const INTERRUPTED: &str = "interrupted by a person";

/// What a loop's iteration cap says once the loop has passed it: the limit and
/// its value. The loop's pause and its failure both read it, so a parked run names
/// the limit to whichever surface reads it, and the way on is the decision under it.
pub(crate) fn exceeded_loop_cap(cap: u32) -> String {
    format!("exceeded `limits.max_loop_iterations` ({cap})")
}

/// The one sentence for questions awaiting an answer: `asked 2
/// questions: `q-scope`, `q-api``.
///
/// Every surface that says a node asked says it with these bytes — the
/// chronicle, the node's own label, the reason a run parked — so a
/// reader never meets the same fact spelled two ways.
pub fn asked_questions(asked: &[QuestionId]) -> String {
    format!(
        "asked {}: {}",
        text::counted(asked.len(), "question"),
        text::listed(asked.iter().map(QuestionId::as_str))
    )
}

/// The word a run paused this way is called by: `waiting` for a limit's
/// reset, which the run resumes on its own, `stopped` for a stop a person
/// made, and `needs you` for every other stop.
pub fn standing(pause: &PauseReason) -> Standing {
    match pause {
        _ if pause.reset_wait().is_some() => Standing::Waiting,
        _ if pause.by_a_person() => Standing::Stopped,
        _ => Standing::NeedsYou,
    }
}

/// The chronicle's words for a pause: the word it is called by, then the
/// sentence the pause is.
pub fn said(pause: &PauseReason) -> String {
    format!("{}: {}", standing(pause), sentence(pause))
}

/// The one line a reader sees for a parked run. Every surface prints
/// this, so none of them can say something different.
pub fn sentence(pause: &PauseReason) -> String {
    match pause {
        PauseReason::Escalation(waiting) => escalation::sentence(waiting),
        PauseReason::Cancelled => INTERRUPTED.to_string(),
        PauseReason::CancelledAfterCrash => {
            "its engine had died, and a cancel recorded the stop".to_string()
        }
        PauseReason::BudgetExhausted { spent, cap } => budget(*spent, *cap),
        PauseReason::NoRoom {
            available,
            need,
            keep_free,
        } => no_room(*available, *need, *keep_free),
        PauseReason::LoopOverrun { node, cap } => {
            format!("loop `{node}` {}", exceeded_loop_cap(*cap))
        }
        PauseReason::ExternalGate { url } => format!("waiting on external gate: {url}"),
        PauseReason::UncertainOrphans { nodes } => uncertain(nodes),
        // The claim; what a command printed travels as the escalation's
        // evidence, not inside the one line a pause is.
        PauseReason::NodeFailed { node, failure } => {
            format!(
                "node `{node}` failed: {}",
                crate::failure::headline(failure)
            )
        }
        PauseReason::AwaitingReset { node, at } => format!(
            "node `{node}` waits for its limit to reset at {}, and the run resumes on its own \
             then",
            at.format(RESET_FORMAT)
        ),
        PauseReason::Blocked { node, on } => blocked(node, on),
        PauseReason::GateAborted { node, free_text } => gate_aborted(node, free_text.as_deref()),
        PauseReason::PullRequestClosed { node, url } => {
            format!("node `{node}`'s pull request was closed, its decision still open ({url})")
        }
        PauseReason::ChildPaused { node, run, child } => child_paused(node, run, child),
        PauseReason::Questions { node, pending } => {
            format!("node `{node}` {}", asked_questions(pending.as_slice()))
        }
        PauseReason::AnswersRefused { node, report } => {
            format!("node `{node}`'s answers were refused: {report}")
        }
        PauseReason::Recorded { sentence: said } => said.as_str().to_string(),
    }
}

/// What a parked run is waiting on, on the one line a row has room for:
/// the node, and the forge handle it was published under when it has
/// one. A row in a listing carries one of these per run, so it names the
/// node and stops there.
///
/// A pause the log recorded under no node has no node to name, and says
/// its own reason instead, collapsed onto that line.
pub fn parked_on(on: &WaitingOn) -> String {
    match on {
        WaitingOn::Node { node, on, .. } => match on {
            NodeWait::Gate {
                external_ref: Some(handle),
            } => format!("node `{node}`, published at {handle}"),
            NodeWait::Gate { external_ref: None } | NodeWait::Questions { .. } => {
                format!("node `{node}`")
            }
        },
        WaitingOn::Run { reason } => text::one_line(&sentence(reason)),
    }
}

/// What a parked run is waiting on, as a page that quotes the record
/// beside it says it: an escalation's claim alone — the page quotes its
/// record as a failed node's evidence or in the decision's account — and
/// the sentence the engine recorded for any other pause.
pub fn parked_claim(on: &WaitingOn) -> String {
    let escalation = match on {
        WaitingOn::Node {
            reason: Some(PauseReason::Escalation(escalation)),
            ..
        }
        | WaitingOn::Run {
            reason: PauseReason::Escalation(escalation),
        } => escalation,
        WaitingOn::Node { .. } | WaitingOn::Run { .. } => return parked_in_full(on),
    };
    escalation::claim_words(escalation)
}

/// What a parked run is waiting on, at the length a page has room for:
/// the sentence the engine recorded when it stopped the run, which names
/// what a node id cannot — which questions are still unanswered, which
/// cap a budget hit, which review a forge is holding.
///
/// A node parked while the run itself keeps moving has no such sentence
/// on the log, and reads as [`parked_on`].
pub fn parked_in_full(on: &WaitingOn) -> String {
    match on {
        WaitingOn::Node {
            reason: Some(reason),
            ..
        }
        | WaitingOn::Run { reason } => text::one_line(&sentence(reason)),
        WaitingOn::Node { reason: None, .. } => parked_on(on),
    }
}

/// Why a run stops once it has spent the tokens its limit allows. The way on is the
/// decision under it.
fn budget(spent: u64, cap: u64) -> String {
    format!(
        "its token budget is spent ({} with `limits.max_tokens_per_run: {cap}`)",
        Tokens(spent).exact()
    )
}

/// Why a run stops for want of room on the disk, and how to make some.
fn no_room(available: u64, need: u64, keep_free: u64) -> String {
    let building = match need {
        0 => String::new(),
        need => format!(", and a build here takes about {} more", Bytes(need)),
    };
    format!(
        "no room on the disk, which has {} free where {} stays free{building}; {} and resume",
        Bytes(available),
        Bytes(keep_free),
        make_room()
    )
}

/// What makes room on a disk the checkouts fill: `gc`, the command that
/// reclaims, among the other ways.
pub fn make_room() -> String {
    let gc = crate::command::line(&NextStep::Gc);
    format!("make room (`{gc}`, close a parked run, or lower `storage.pool.keep_free`)")
}

/// Why a run stops when it cannot tell how `nodes` ended.
fn uncertain(nodes: &[NodeId]) -> String {
    format!(
        "{} running with no terminal event when the engine last stopped: {}; \
         `on_interrupt: fail_if_uncertain` refuses to guess how each ended; verify manually \
         before resuming",
        text::agreeing(nodes.len(), "a node was", "nodes were"),
        text::listed(nodes.iter().map(NodeId::as_str))
    )
}

/// Why a run stops when a node waits on nodes this pass left unresolved.
fn blocked(node: &NodeId, on: &[NodeId]) -> String {
    format!(
        "no node is runnable: `{node}` waits on {}, which this run left unresolved",
        text::listed(on.iter().map(NodeId::as_str))
    )
}

/// A gate a person paused the run at: its decision stays open, and whatever
/// the person wrote with the pause follows it.
fn gate_aborted(node: &NodeId, free_text: Option<&str>) -> String {
    text::detailed(
        format!("a person paused the run at node `{node}`, its decision still open"),
        free_text.unwrap_or_default(),
    )
}

/// A run this node bore is parked. The parent names the child run and
/// carries the child's own pause under its line rather than restating it.
fn child_paused(node: &NodeId, run: &RunId, child: &PauseReason) -> String {
    format!(
        "node `{node}`'s child run is parked: child run `{run}` paused, and resuming this \
         run resumes it:\n  {}",
        text::hanging(&sentence(child), "  ")
    )
}

#[cfg(test)]
mod tests;
