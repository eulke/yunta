//! Why a run is parked, as a fact.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::events::gates::escalation::Escalation;
use crate::events::{Failure, LoggedWords};
use crate::ids::{NodeId, QuestionId, RunId};

/// Why a run is parked, as a fact rather than a sentence.
///
/// The log stores this fact. The line a reader sees for a parked run is
/// composed from it at the border, by the render crate, so every surface
/// that shows the pause says it the same way.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PauseReason {
    /// A decision is waiting on a person, with the escalation that asks
    /// it. Serde goes through `GateWaitingPayload`; a read does not
    /// re-check the claim.
    Escalation(Box<Escalation>),
    /// A person stopped the run while it was running.
    Cancelled,
    /// A person stopped a run whose engine was already dead: the
    /// orphaned process groups were killed from outside and the pause
    /// written on the engine's behalf, so the log says who ended the
    /// run and that nothing of it was still alive to ask.
    CancelledAfterCrash,
    /// The run reached `limits.max_tokens_per_run`.
    BudgetExhausted { spent: u64, cap: u64 },
    /// The disk has `available` bytes free, and going on would take
    /// `need` more where `keep_free` must stay free.
    NoRoom {
        available: u64,
        need: u64,
        keep_free: u64,
    },
    /// A loop reached `limits.max_loop_iterations` with tasks still
    /// ready.
    LoopOverrun { node: NodeId, cap: u32 },
    /// A gate is published and the forge has not answered it.
    ExternalGate { url: String },
    /// A resume found nodes still running whose fate it cannot tell.
    UncertainOrphans { nodes: Vec<NodeId> },
    /// A node failed and nothing re-routed it.
    NodeFailed { node: NodeId, failure: Failure },
    /// The account's limit resets at `at`; the run resumes detached then.
    AwaitingReset { node: NodeId, at: DateTime<Utc> },
    /// Nothing is runnable: `node` waits on `on`, which this pass left
    /// unresolved.
    Blocked { node: NodeId, on: Vec<NodeId> },
    /// A gate was resolved to abort, with whatever the person wrote.
    GateAborted {
        node: NodeId,
        free_text: Option<String>,
    },
    /// A child run paused on its own log; the parent names it and keeps
    /// the child's pause.
    ChildPaused {
        node: NodeId,
        run: RunId,
        child: Box<PauseReason>,
    },
    /// A node asked questions and no surface could answer them.
    Questions {
        node: NodeId,
        pending: crate::NonEmpty<QuestionId>,
    },
    /// A surface answered and the questions refused the reply, with
    /// every way it failed to answer them.
    AnswersRefused {
        node: NodeId,
        report: crate::diagnostic::Report,
    },
    /// Prose a log carries as one sentence; shown as written.
    Recorded { sentence: LoggedWords },
}

impl PauseReason {
    /// The node a limit's reset holds the run for, and the moment the run
    /// resumes at by itself. A pause of that kind is a wait nobody answers,
    /// not a stop a person has to. `None` for every other pause.
    pub fn reset_wait(&self) -> Option<(&NodeId, DateTime<Utc>)> {
        match self {
            PauseReason::AwaitingReset { node, at } => Some((node, *at)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{AuthorGate, Claim, GateOption, Leads, Menu};
    use crate::ids::OptionId;

    /// Every reason a run can park on reads back as the fact it was
    /// written as, including the ones that nest a payload or a pause.
    #[test]
    fn every_pause_reads_back_as_the_fact_it_was_written_as() {
        let option = GateOption::leading(
            OptionId::from_static("approve"),
            Leads::On {
                next: Vec::new(),
                settles: false,
            },
        );
        let escalation = Escalation::new(
            Claim::Written("the gate waits".to_string()),
            Menu::Gate(AuthorGate {
                assignee: "lead".into(),
            }),
            crate::NonEmpty::from((option, Vec::new())),
        );
        let pauses = [
            PauseReason::Escalation(Box::new(escalation)),
            PauseReason::Cancelled,
            PauseReason::AwaitingReset {
                node: NodeId::from_static("limited"),
                at: chrono::DateTime::UNIX_EPOCH,
            },
            PauseReason::BudgetExhausted { spent: 5, cap: 4 },
            PauseReason::UncertainOrphans {
                nodes: vec![NodeId::from_static("review")],
            },
            PauseReason::ChildPaused {
                node: NodeId::from_static("child"),
                run: RunId::from_static("run-1"),
                child: Box::new(PauseReason::Cancelled),
            },
            PauseReason::Questions {
                node: NodeId::from_static("ask"),
                pending: crate::NonEmpty::from((QuestionId::from_static("q-1"), Vec::new())),
            },
            PauseReason::Recorded {
                sentence: "waiting".into(),
            },
        ];
        for pause in pauses {
            let written = serde_json::to_value(&pause).unwrap();
            let read: PauseReason = serde_json::from_value(written.clone()).unwrap();
            assert_eq!(read, pause, "{written}");
        }
    }

    #[test]
    fn only_a_limit_reset_is_a_wait_nobody_answers() {
        let node = NodeId::from_static("limited");
        let at = DateTime::UNIX_EPOCH;
        let reset = PauseReason::AwaitingReset {
            node: node.clone(),
            at,
        };
        assert_eq!(reset.reset_wait(), Some((&node, at)));
        let budget = PauseReason::BudgetExhausted { spent: 5, cap: 4 };
        assert!(budget.reset_wait().is_none());
    }
}
