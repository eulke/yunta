//! What a surface says beside a run that has stopped: what the run is
//! waiting on, and the commands a person runs to move it.
//!
//! Every one of them is printed, at some point, beside a run a person
//! still has to act on: in the live region's attention row, in the block
//! that closes a run out, in `yunta status`, in the JSON a program
//! reads, and in the diagnostic a command hands back when it could not
//! finish its own job. They are spelled once here so a reader who met a
//! phrase on one surface meets the same phrase on the next, and so
//! renaming a subcommand is a change to this file rather than a hunt
//! through prose.
//!
//! Each command carries what the run is called by — its handle on a
//! line a person reads, its whole id in a document a program reads —
//! and nothing else a reader would have to invent — a gate's option stays `<option>`, because an option
//! id printed as an example gets pasted, and no id printed beside a run
//! is ever one that run's own menu does not offer.

use yunta_engine::{NodeWait, RunPhase, WaitingOn};

use crate::render::state::RunWord;

/// What holds a run whose log says it is moving and whose engine is gone.
pub(crate) const STALLED: &str = "no process is driving it: the engine that ran it is gone";

/// What a run is parked on, or `None` for a run nobody has to touch —
/// the one answer that decides whether a surface says anything about a
/// decision at all.
pub(crate) fn parked(phase: &RunPhase) -> Option<&WaitingOn> {
    match phase {
        RunPhase::Waiting { on } => Some(on),
        RunPhase::Created
        | RunPhase::Running
        | RunPhase::Finished
        | RunPhase::Failed { .. }
        | RunPhase::Cancelled
        | RunPhase::Promoted { .. }
        | RunPhase::Broken { .. } => None,
    }
}

/// What a parked run is waiting on, on the one line a row has room for:
/// the node, and the forge handle it was published under when it has
/// one. A row in a listing carries one of these per run, so it names the
/// node and stops there.
///
/// A pause the log recorded under no node has no node to name, and says
/// its own reason instead, collapsed onto that line.
pub(crate) fn parked_on(on: &WaitingOn) -> String {
    match on {
        WaitingOn::Node { node, on, .. } => match on {
            NodeWait::Gate {
                external_ref: Some(handle),
            } => format!("node `{node}`, published at {handle}"),
            NodeWait::Gate { external_ref: None } | NodeWait::Questions { .. } => {
                format!("node `{node}`")
            }
        },
        WaitingOn::Run { reason } => yunta_core::text::one_line(reason),
    }
}

/// What a parked run is waiting on, at the length a page has room for:
/// the sentence the engine recorded when it stopped the run, which names
/// what a node id cannot — which questions are still unanswered, which
/// cap a budget hit, which review a forge is holding.
///
/// A node parked while the run itself keeps moving has no such sentence
/// on the log, and reads as [`parked_on`].
pub(crate) fn parked_in_full(on: &WaitingOn) -> String {
    match on {
        WaitingOn::Node {
            reason: Some(reason),
            ..
        }
        | WaitingOn::Run { reason } => yunta_core::text::one_line(reason),
        WaitingOn::Node { reason: None, .. } => parked_on(on),
    }
}

/// Answers a run parked on a decision, with the option left for the
/// reader to pick off the menu printed above it.
pub(crate) fn resolve_gate(run: &str) -> String {
    format!("yunta resolve-gate {run} <option>")
}

/// Hands a run back to the engine, once whatever stopped it is settled
/// somewhere else — a budget, a scope, an answers file, a review on a
/// forge.
pub(crate) fn resume(run: &str) -> String {
    format!("yunta resume {run}")
}

/// Stops a run and the whole process tree under it.
pub(crate) fn cancel(run: &str) -> String {
    format!("yunta cancel {run}")
}

/// Closes a stopped run nobody is going to continue.
pub(crate) fn close(run: &str) -> String {
    format!("yunta close {run}")
}

/// Shows where a run stands, derived from its own log.
pub(crate) fn status(run: &str) -> String {
    format!("yunta status {run}")
}

/// Gathers a finished run's manifest, log and artifacts into the bundle
/// that certifies it.
pub(crate) fn receipt(run: &str) -> String {
    format!("yunta receipt {run}")
}

/// Walks a run's event hash chain, which is what says where a log stopped
/// being readable.
pub(crate) fn verify(run: &str) -> String {
    format!("yunta verify {run}")
}

/// What a person types next for a run its word describes, each command
/// with what it does. `menu` says whether a run that needs the person
/// stopped on a menu, whose options carry their own commands.
pub(crate) fn after(word: RunWord, run: &str, menu: bool) -> Vec<(String, &'static str)> {
    match word {
        RunWord::Stalled => vec![
            (resume(run), "continues it from its log"),
            (close(run), "closes it for good"),
        ],
        RunWord::NeedsYou if !menu => vec![
            (resume(run), "hands it back once that is settled"),
            (close(run), "closes it for good"),
        ],
        RunWord::NeedsYou => vec![(close(run), "closes it for good")],
        RunWord::Finished | RunWord::Reported => vec![(receipt(run), "certifies what it did")],
        RunWord::Broken => vec![(verify(run), "says where its log stops reading")],
        RunWord::Created | RunWord::Running => {
            vec![(cancel(run), "stops it and everything under it")]
        }
        RunWord::Failed | RunWord::Cancelled | RunWord::Promoted => Vec::new(),
    }
}
