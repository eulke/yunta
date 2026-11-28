//! See [`super`]. What holds a run that waits on a person's answer, classified once from the
//! pause or the failure that holds it, each kind with every form a surface says it in: the
//! line its run's row, its glance and its page's gist say ([`Hold::said`]), and the question
//! its decision's card asks ([`FailedHold::asked`]). A pause no kind here holds is said as the
//! log parked the run on it ([`super::parked_claim`]); a kind added here is said everywhere
//! at once.

use surco::text::counted;
use yunta_core::events::{Failure, OwedScope, PauseReason};
use yunta_core::{listed_globs, NodeId, ScopeGlob};

/// What holds a run parked on a person's answer, where it is more than the claim the log
/// parked it on: a round of questions, which a round's prompt answers, or a node's failure,
/// which a decision's card answers. Not the pause itself ([`PauseReason`]), which every
/// other surface says whole.
pub(crate) enum Hold<'a> {
    /// A round of `questions` that `node` asks.
    Round { node: &'a NodeId, questions: usize },
    /// A node's failure a decision's card answers.
    Failed(FailedHold<'a>),
}

/// What holds a run whose node failed, where its decision answers more than what now: a
/// check's red verdict, a loop's tasks asking to touch paths outside their scope, or a loop's
/// tasks whose sessions depart from the plan. Not a loop's spent cap, which its card asks
/// about and its row says as the log parked it.
pub(crate) enum FailedHold<'a> {
    /// A check's red verdict, which names what it judged.
    Red(&'a Failure),
    /// Each task's request to touch paths outside its scope.
    Scope(&'a [OwedScope]),
    /// How many tasks depart from the plan.
    Departs(usize),
}

impl<'a> Hold<'a> {
    /// What holds a run parked on `pause`, where a kind here holds it.
    pub(crate) fn of(pause: &'a PauseReason) -> Option<Self> {
        match pause {
            PauseReason::Questions { node, pending } => Some(Hold::Round {
                node,
                questions: pending.as_slice().len(),
            }),
            PauseReason::NodeFailed { failure, .. } => FailedHold::of(failure).map(Hold::Failed),
            _ => None,
        }
    }

    /// What it is said as on the run's row, its glance and its page, as what the person's
    /// answer settles: how many questions a round's node asks and that nothing is recorded
    /// until every one is answered; or what [`FailedHold::said`] says of a failure.
    pub(crate) fn said(&self) -> String {
        match self {
            Hold::Round { node, questions } => {
                let asked = format!("node `{node}` asks {}", counted(*questions, "question"));
                match questions {
                    1 => asked,
                    _ => format!("{asked}: {}", crate::prompt::round::NOTHING_KEPT),
                }
            }
            Hold::Failed(failed) => failed.said(),
        }
    }
}

impl<'a> FailedHold<'a> {
    /// What holds a run whose node failed with `failure`, where a kind here holds it.
    pub(crate) fn of(failure: &'a Failure) -> Option<Self> {
        match failure {
            Failure::Red { .. } => Some(FailedHold::Red(failure)),
            Failure::ScopeOwed { owed } => Some(FailedHold::Scope(owed)),
            _ => failure.departures_owed().map(FailedHold::Departs),
        }
    }

    /// What it is said as on the run's row, its glance and its page: a check's verdict alone,
    /// which names what it judged; that one task, or how many, ask to touch paths outside
    /// their scope; that one task, or how many, depart from what the plan decided.
    pub(crate) fn said(&self) -> String {
        match self {
            FailedHold::Red(failure) => crate::failure::headline(failure),
            FailedHold::Scope([one]) if one.paths.len() == 1 => {
                "a task asks to touch a path outside its scope".to_string()
            }
            FailedHold::Scope([_]) => "a task asks to touch paths outside its scope".to_string(),
            FailedHold::Scope(owed) => format!(
                "{} ask to touch paths outside their scope",
                counted(owed.len(), "task")
            ),
            FailedHold::Departs(1) => "a task departs from what the plan decided".to_string(),
            FailedHold::Departs(tasks) => format!(
                "{} depart from what the plan decided",
                counted(*tasks, "task")
            ),
        }
    }

    /// What its decision's card asks the person: what one task asks to touch, as the paths it
    /// names, and what several ask as its row says it; whether to accept what departs from the
    /// plan. None for a check's red verdict, whose card asks what now of the node that failed.
    pub(crate) fn asked(&self) -> Option<String> {
        match self {
            FailedHold::Red(_) => None,
            FailedHold::Scope([one]) => Some(asks_to_touch(&one.paths)),
            FailedHold::Scope(_) => Some(self.said()),
            FailedHold::Departs(tasks) => Some(departs(*tasks)),
        }
    }
}

/// What a card asks of a request to touch `paths` outside its scope: the paths it names.
pub(crate) fn asks_to_touch(paths: &[ScopeGlob]) -> String {
    format!("It asks to touch {}", listed_globs(paths))
}

/// What a card asks of a task, or of each of `tasks`, whose session departs from the plan.
pub(crate) fn departs(tasks: usize) -> String {
    match tasks {
        1 => "The task departs from its plan. Accept it?".to_string(),
        _ => format!(
            "{} depart from the plan. Accept them?",
            counted(tasks, "task")
        ),
    }
}
