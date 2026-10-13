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

use yunta_core::events::{GateWaitingPayload, Stuck};
use yunta_engine::{NodeWait, RunPhase, WaitingOn};

use crate::render::state::RunWord;

/// What holds a run whose log says it is moving and whose engine is gone.
pub(crate) const STALLED: &str = "no process is driving it: the engine that ran it is gone";

/// What holds a run whose engine is asking a person at its terminal: a
/// question asked live is on no log until it is answered, so the only
/// place to answer it is where it is asked.
pub(crate) fn asking(prompt: &yunta_engine::Prompt, at: chrono::DateTime<chrono::Utc>) -> String {
    let who = prompt
        .node
        .as_ref()
        .map_or_else(|| "the run".to_string(), |node| format!("node `{node}`"));
    let waited = (at - prompt.since).to_std().unwrap_or_default();
    format!(
        "{who} is asking at its terminal (pid {}) for {}; answer it there",
        prompt.pid,
        yunta_core::units::duration(waited)
    )
}

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
        WaitingOn::Run { reason } => {
            yunta_core::text::one_line(&yunta_render::pause::sentence(reason))
        }
    }
}

/// Whether [`parked_in_full`] says the sentence the engine recorded —
/// a decision's claim with the record beside it — rather than only the
/// node it waits on: a surface that printed it never prints that record
/// twice.
pub(crate) fn parked_says_why(on: &WaitingOn) -> bool {
    matches!(
        on,
        WaitingOn::Node {
            reason: Some(_),
            ..
        } | WaitingOn::Run { .. }
    )
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
            yunta_core::text::one_line(&yunta_render::pause::sentence(reason))
        }
        WaitingOn::Node { reason: None, .. } => parked_on(on),
    }
}

/// Answers a run parked on a decision, with the option left for the
/// reader to pick off the menu printed above it.
pub(crate) fn resolve_gate(run: &str) -> String {
    format!("yunta resolve-gate {run} <option>")
}

/// Puts a run's decision to a person: on a terminal, its documents and
/// its menu; off one, a command per option.
pub(crate) fn decide(run: &str) -> String {
    format!("yunta resolve-gate {run}")
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

/// Shows one node of a run whole: its failure, the end of what it
/// printed and where all of it is.
pub(crate) fn status_node(run: &str, node: &str) -> String {
    format!("yunta status {run} --node {node}")
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
/// with what it does. `decision` is the decision a run that needs the
/// person stopped on, when the log rebuilds one: its options carry their
/// own commands, and a failure that no retry can change until the tree
/// moves says so beside the command that hands the run back.
pub(crate) fn after(
    word: RunWord,
    run: &str,
    decision: Option<&GateWaitingPayload>,
) -> Vec<(String, &'static str)> {
    match word {
        RunWord::Stalled => vec![
            (resume(run), "continues it from its log"),
            (close(run), "closes it for good"),
        ],
        RunWord::Waiting => after_reset(run),
        RunWord::NeedsYou => match decision {
            None => vec![
                (resume(run), "hands it back once that is settled"),
                (close(run), "closes it for good"),
            ],
            Some(decision) => {
                let mut steps = Vec::new();
                if waits_for_tree(decision) {
                    steps.push((resume(run), "offers a retry once the run's tree changed"));
                }
                steps.push((close(run), "closes it for good"));
                steps
            }
        },
        RunWord::Finished | RunWord::Reported => vec![(receipt(run), "certifies what it did")],
        RunWord::Broken => vec![(verify(run), "says where its log stops reading")],
        RunWord::Created | RunWord::Running => {
            vec![(cancel(run), "stops it and everything under it")]
        }
        RunWord::Failed | RunWord::Cancelled | RunWord::Promoted => Vec::new(),
    }
}

/// What a person types next for a run paused on a limit's reset. The resume
/// that hands it back waits for the reset before it takes anything, so the
/// words say when it moves rather than asking a person to settle something
/// first.
pub(crate) fn after_reset(run: &str) -> Vec<(String, &'static str)> {
    vec![
        (resume(run), "waits for the reset, then drives on"),
        (close(run), "closes it for good"),
    ]
}

/// Whether the failure a decision is about offers no retry until the run's
/// tree changes. The same check on the same tree answers the same, so the
/// menu says that much and the way back is a resume once the tree moved.
fn waits_for_tree(decision: &GateWaitingPayload) -> bool {
    decision
        .failure
        .as_ref()
        .and_then(|menu| menu.stuck.as_ref())
        == Some(&Stuck::TreeUnchanged)
}

/// The node a run paused on a limit's reset is held for, and the moment
/// the run resumes at by itself. `None` for a run in any other state.
pub(crate) fn reset_pause(
    phase: &RunPhase,
) -> Option<(&yunta_core::NodeId, chrono::DateTime<chrono::Utc>)> {
    match phase {
        RunPhase::Waiting {
            on: WaitingOn::Run { reason },
        } => reason.reset_wait(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use yunta_core::events::{Escalation, Evidence, Failure, FailureMenu, GateOption, PauseReason};
    use yunta_core::{NodeId, NonEmpty, OptionId};

    use super::*;

    #[test]
    fn a_run_paused_on_a_reset_names_its_node_and_the_moment_it_resumes() {
        let at = chrono::Utc
            .with_ymd_and_hms(2026, 10, 13, 12, 0, 0)
            .unwrap();
        let phase = RunPhase::Waiting {
            on: WaitingOn::Run {
                reason: PauseReason::AwaitingReset {
                    node: NodeId::from_static("build"),
                    at,
                },
            },
        };
        let (node, resumes) = reset_pause(&phase).expect("a run paused on a reset");
        assert_eq!((node.as_str(), resumes), ("build", at));
        assert!(reset_pause(&RunPhase::Running).is_none());
    }

    /// A decision on a failed node whose menu gives `stuck` as the reason no
    /// remedy is offered, with nothing but abort on it.
    fn decision_stuck_on(stuck: Stuck) -> GateWaitingPayload {
        let menu = FailureMenu {
            attempt: 1,
            failure: Failure::message("exit 1"),
            remedies: Vec::new(),
            stuck: Some(stuck),
            reroute: None,
        };
        let abort = GateOption {
            id: OptionId::from_static("abort"),
            label: Some("Abort".to_string()),
            tradeoff: None,
            asks: None,
        };
        Escalation::new(
            "node `build` failed",
            Evidence::none(),
            NonEmpty::from((abort, Vec::new())),
        )
        .expect("the claim does not repeat its record")
        .with_failure(menu)
        .into_payload()
    }

    #[test]
    fn a_failure_stuck_on_an_unchanged_tree_lists_the_resume_that_retries_once_it_changed() {
        let stuck = decision_stuck_on(Stuck::TreeUnchanged);
        assert_eq!(
            after(RunWord::NeedsYou, "run-1", Some(&stuck)),
            vec![
                (
                    "yunta resume run-1".to_string(),
                    "offers a retry once the run's tree changed",
                ),
                ("yunta close run-1".to_string(), "closes it for good"),
            ]
        );
        let denied = decision_stuck_on(Stuck::ProjectDenies);
        assert_eq!(
            after(RunWord::NeedsYou, "run-1", Some(&denied)),
            vec![("yunta close run-1".to_string(), "closes it for good")]
        );
    }

    #[test]
    fn a_run_that_needs_a_person_without_a_decision_is_handed_back_once_settled() {
        assert_eq!(
            after(RunWord::NeedsYou, "run-1", None),
            vec![
                (
                    "yunta resume run-1".to_string(),
                    "hands it back once that is settled",
                ),
                ("yunta close run-1".to_string(), "closes it for good"),
            ]
        );
    }

    #[test]
    fn a_run_waiting_on_a_reset_is_offered_the_reset_words() {
        assert_eq!(after(RunWord::Waiting, "run-1", None), after_reset("run-1"));
    }
}
