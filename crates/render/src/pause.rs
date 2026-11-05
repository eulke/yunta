//! The one line a reader meets for a parked run, said from the fact the
//! log holds.
//!
//! Every surface that says why a run stopped (a listing's row, the status
//! heading, the line a resume echoes) calls [`sentence`], so none of them
//! can say a different thing. The fact is stored as data; the words are
//! produced here, where a reader is told.

use surco::figures::{Bytes, Tokens};
use surco::text;
use surco::words::Standing;
use yunta_core::events::PauseReason;
use yunta_core::{NodeId, QuestionId, RunId};

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
        make_room("yunta gc")
    )
}

/// What makes room on a disk the checkouts fill: `gc`, the command that
/// reclaims, among the other ways.
pub fn make_room(gc: &str) -> String {
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
mod tests {
    use surco::words::Standing;
    use yunta_core::events::{Failure, PauseReason};
    use yunta_core::{NodeId, QuestionId, RunId};

    use super::{sentence, standing};

    #[test]
    fn a_node_that_waits_for_a_reset_names_the_moment_it_resumes() {
        let waiting = PauseReason::AwaitingReset {
            node: NodeId::from("review"),
            at: yunta_testkit_core::fixed_now(),
        };
        assert_eq!(
            sentence(&waiting),
            "node `review` waits for its limit to reset at 2026-01-01 00:00 UTC, and the run \
             resumes on its own then"
        );
    }

    #[test]
    fn a_failed_node_pauses_on_the_headline_of_its_failure() {
        let failed = PauseReason::NodeFailed {
            node: NodeId::from("lint"),
            failure: Failure::Message {
                outcome: "exit 1".into(),
            },
        };
        assert_eq!(sentence(&failed), "node `lint` failed: exit 1");
    }

    #[test]
    fn a_pause_a_person_chose_reads_as_the_choice() {
        assert_eq!(sentence(&PauseReason::Cancelled), "interrupted by a person");
    }

    /// A person's pause at a gate says the decision is still open, and adds any note.
    #[test]
    fn a_person_paused_the_run_with_its_decision_open() {
        let paused = PauseReason::GateAborted {
            node: NodeId::from("approve-plan"),
            free_text: None,
        };
        assert_eq!(
            sentence(&paused),
            "a person paused the run at node `approve-plan`, its decision still open"
        );
        let with_note = PauseReason::GateAborted {
            node: NodeId::from("approve-plan"),
            free_text: Some("not this quarter".to_string()),
        };
        assert_eq!(
            sentence(&with_note),
            "a person paused the run at node `approve-plan`, its decision still open: \
             not this quarter"
        );
    }

    /// A closed pull request names the node, leaves its decision open, and gives the URL.
    #[test]
    fn a_closed_pull_request_reads_as_a_pause_with_its_decision_open() {
        let closed = PauseReason::PullRequestClosed {
            node: NodeId::from("approve"),
            url: "https://mock.forge/pr/1".to_string(),
        };
        assert_eq!(
            sentence(&closed),
            "node `approve`'s pull request was closed, its decision still open \
             (https://mock.forge/pr/1)"
        );
    }

    /// A person's gate pause and a closed pull request both have the stopped standing.
    #[test]
    fn both_pauses_read_as_stopped() {
        let gate = PauseReason::GateAborted {
            node: NodeId::from("approve"),
            free_text: None,
        };
        let closed = PauseReason::PullRequestClosed {
            node: NodeId::from("approve"),
            url: "https://mock.forge/pr/1".to_string(),
        };
        assert_eq!(standing(&gate), Standing::Stopped);
        assert_eq!(standing(&closed), Standing::Stopped);
    }

    #[test]
    fn a_loop_that_overran_names_its_limit() {
        let overrun = PauseReason::LoopOverrun {
            node: NodeId::from("fix-lint"),
            cap: 3,
        };
        assert_eq!(
            sentence(&overrun),
            "loop `fix-lint` exceeded `limits.max_loop_iterations` (3)"
        );
    }

    /// What goes on from here is the decision under it, on every interface, so the sentence
    /// says what happened and nothing about who can answer.
    #[test]
    fn a_run_on_its_token_cap_says_what_it_spent_and_its_limit() {
        assert_eq!(
            sentence(&PauseReason::BudgetExhausted {
                spent: 100,
                cap: 100
            }),
            "its token budget is spent (100 tokens with `limits.max_tokens_per_run: 100`)"
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
             run resumes it:\n  interrupted by a person"
        );
    }

    /// The one sentence every surface says a node's unanswered questions
    /// with — the node's own label, the run's pause, the chronicle.
    #[test]
    fn questions_awaiting_an_answer_are_said_one_way() {
        assert_eq!(
            super::asked_questions(&[QuestionId::from("q-scope")]),
            "asked 1 question: `q-scope`"
        );
        assert_eq!(
            super::asked_questions(&[QuestionId::from("q-scope"), QuestionId::from("q-api")]),
            "asked 2 questions: `q-scope`, `q-api`"
        );
    }
}
