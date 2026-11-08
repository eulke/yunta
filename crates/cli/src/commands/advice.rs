//! What a surface says beside a run that has stopped: what the run is
//! waiting on.
//!
//! Every one of these is printed, at some point, beside a run a person
//! still has to act on: in the live region's attention row, in the block
//! that closes a run out, in `yunta status` and in the JSON a program
//! reads. They are said once here so a reader who met a phrase on one
//! surface meets the same phrase on the next. What a run offers next is
//! replay's (`RunFrame::actions`), written by `yunta_render::command`.

use yunta_core::events::{NodeWait, PauseReason};
use yunta_replay::frame::WaitingOn;

/// What holds a run whose log says it is moving and whose engine is gone.
pub(crate) const STALLED: &str = "no process is driving it: the engine that ran it is gone";

/// What holds a run whose engine is asking a person at its terminal: a
/// question asked live is on no log until it is answered, so the only
/// place to answer it is where it is asked. With `at`, the sentence says
/// how long it has been asking; a page whose headline says it leaves it
/// out.
pub(crate) fn asking(
    prompt: &yunta_replay::registry::Prompt,
    at: Option<chrono::DateTime<chrono::Utc>>,
) -> String {
    let who = prompt
        .node
        .as_ref()
        .map_or_else(|| "the run".to_string(), |node| format!("node `{node}`"));
    let waited = at.map(|at| {
        let waited = (at - prompt.since).to_std().unwrap_or_default();
        format!(" for {}", surco::figures::duration(waited))
    });
    format!(
        "{who} is asking at its terminal (pid {}){}; answer it there",
        prompt.pid,
        waited.unwrap_or_default()
    )
}

/// Whom a live engine is asking, on the one line a row has room for: the
/// node, and that the answer is given at its terminal.
pub(crate) fn asking_on(prompt: &yunta_replay::registry::Prompt) -> String {
    match &prompt.node {
        Some(node) => format!("node `{node}` asks at its terminal"),
        None => "the run asks at its terminal".to_string(),
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
        WaitingOn::Run { reason } => surco::text::one_line(&yunta_render::pause::sentence(reason)),
    }
}

/// What a parked run is waiting on, as a page that quotes the record
/// beside it says it: an escalation's claim alone — the page quotes its
/// record as a failed node's evidence or in the decision's account — and
/// the sentence the engine recorded for any other pause.
pub(crate) fn parked_claim(on: &WaitingOn) -> String {
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
    crate::commands::status::decision::claim_words(&(**escalation).clone().into_payload())
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
        | WaitingOn::Run { reason } => {
            surco::text::one_line(&yunta_render::pause::sentence(reason))
        }
        WaitingOn::Node { reason: None, .. } => parked_on(on),
    }
}
