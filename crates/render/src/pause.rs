//! The one line a reader meets for a parked run, said from the fact the
//! log holds.
//!
//! Every surface that says why a run stopped (a listing's row, the status
//! heading, the line a resume echoes) calls [`sentence`], so none of them
//! can say a different thing. The fact is stored as data; the words are
//! produced here, where a reader is told.

use yunta_core::events::{loop_overrun_failure, node_failed_line, PauseReason, PAST_THE_CAP};
use yunta_core::text;
use yunta_core::units::{Bytes, Tokens};
use yunta_core::{NodeId, RunId};

use crate::escalation;

/// The one line a reader sees for a parked run. Every surface prints
/// this, so none of them can say something different.
pub fn sentence(pause: &PauseReason) -> String {
    match pause {
        PauseReason::Escalation(waiting) => escalation::sentence(waiting),
        PauseReason::Cancelled => "cancelled by user".to_string(),
        PauseReason::CancelledAfterCrash => "cancelled after crash".to_string(),
        PauseReason::BudgetExhausted { spent, cap } => budget(*spent, *cap),
        PauseReason::NoRoom {
            available,
            need,
            keep_free,
        } => no_room(*available, *need, *keep_free),
        PauseReason::LoopOverrun { node, cap } => loop_overrun_failure(node, *cap),
        PauseReason::ExternalGate { url } => format!("waiting on external gate: {url}"),
        PauseReason::UncertainOrphans { nodes } => uncertain(nodes),
        // The claim; what a command printed travels as the escalation's
        // evidence, not inside the one line a pause is.
        PauseReason::NodeFailed { node, failure } => node_failed_line(node, failure),
        PauseReason::Blocked { node, on } => blocked(node, on),
        PauseReason::GateAborted { node, free_text } => gate_aborted(node, free_text.as_deref()),
        PauseReason::ChildPaused { node, run, child } => child_paused(node, run, child),
        PauseReason::Questions { node, pending } => {
            format!(
                "node `{node}` {}",
                text::asked_questions(pending.as_slice())
            )
        }
        PauseReason::AnswersRefused { node, report } => {
            format!("node `{node}`'s answers were refused: {report}")
        }
        PauseReason::Recorded { sentence: said } => said.clone(),
    }
}

/// Why a run stops once it has spent the tokens its limit allows.
fn budget(spent: u64, cap: u64) -> String {
    format!(
        "budget: run spent {} with `limits.max_tokens_per_run: {cap}` — {PAST_THE_CAP}",
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
        "no room: the disk has {} free{building}, where {} stays free — make room \
         (`yunta gc`, close a parked run, or lower `storage.pool.keep_free`) and resume",
        Bytes(available),
        Bytes(keep_free)
    )
}

/// Why a run stops when it cannot tell how `nodes` ended.
fn uncertain(nodes: &[NodeId]) -> String {
    format!(
        "{} running with no terminal event when the engine last stopped: {} — \
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

/// A gate resolved to abort, with whatever the person wrote alongside it.
fn gate_aborted(node: &NodeId, free_text: Option<&str>) -> String {
    text::detailed(
        format!("node `{node}`'s gate was resolved to abort"),
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
mod tests {
    use yunta_core::events::PauseReason;
    use yunta_core::{NodeId, RunId};

    use super::sentence;

    #[test]
    fn a_pause_a_person_chose_reads_as_the_choice() {
        assert_eq!(sentence(&PauseReason::Cancelled), "cancelled by user");
    }

    #[test]
    fn a_loop_that_overran_names_its_limit_and_the_way_past_it() {
        let overrun = PauseReason::LoopOverrun {
            node: NodeId::from("fix-lint"),
            cap: 3,
        };
        assert_eq!(
            sentence(&overrun),
            "loop `fix-lint` exceeded `limits.max_loop_iterations` (3) — resume with an \
             interactive surface to continue past the cap or abort"
        );
    }

    #[test]
    fn a_blocked_run_names_what_its_node_waits_on() {
        let blocked = PauseReason::Blocked {
            node: NodeId::from("lint"),
            on: vec![NodeId::from("build"), NodeId::from("fix-lint")],
        };
        assert_eq!(
            sentence(&blocked),
            "no node is runnable: `lint` waits on `build`, `fix-lint`, which this run left \
             unresolved"
        );
    }

    #[test]
    fn a_parked_child_names_its_run_and_carries_its_own_line() {
        let parked = PauseReason::ChildPaused {
            node: NodeId::from("grill"),
            run: RunId::from("child"),
            child: Box::new(PauseReason::Cancelled),
        };
        assert_eq!(
            sentence(&parked),
            "node `grill`'s child run is parked: child run `child` paused, and resuming this \
             run resumes it:\n  cancelled by user"
        );
    }
}
