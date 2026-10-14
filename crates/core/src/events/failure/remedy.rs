//! What can change a failure: the options a menu offers, what each one
//! waits for, and the outlook the facts in the log give. Pure over the
//! failure and [`Facts`]; the engine reads the facts from the log, and
//! render writes every word of the result.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::events::{Surprise, TaskCause, TaskStop};
use crate::reserved::ReservedOption;
use crate::NonEmpty;

use super::{AccountLimit, Failure, RefusalCause};

/// One option a failure's menu offers, with the input it waits for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Remedy {
    pub option: ReservedOption,
    /// The input a `retry` waits for; always set on one. Other options may leave it `None`.
    pub precondition: Option<Precondition>,
}

/// What must change before a retry can end differently: the tree, something outside it, or the named input.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Precondition {
    /// The run's tree has to change first.
    TreeChanged,
    /// Something outside the run's tree has to change first.
    OutsideTreeChanged,
    /// The credential the session was refused for has to be fixed.
    CredentialFixed,
    /// The model the session asked for has to be one the account can use.
    ModelAvailable,
    /// The service accepts requests again, by account or by throttle.
    LimitLifted,
    /// The service the session talks to has to answer.
    ServiceReachable,
    /// The command has to run: it is installed and executable.
    CommandRunnable { cmd: String },
}

/// Why a failure offers no remedy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Stuck {
    /// The config the run froze at birth lacks what this needs; a new run under a config that declares it is the way out.
    FrozenConfig,
    /// A file the run froze at birth no longer holds the bytes it froze: the
    /// run reads its copy, so a new run is the way out.
    FrozenInput,
    /// The project's permissions deny the work.
    ProjectDenies,
    /// The scope needs a grant this run will not give.
    NoGrant,
    /// The same check on the same tree answers the same: the tree has to change first.
    TreeUnchanged,
    /// A person's decision is owed before the node can go on.
    Owed,
    /// The refusal says the same to every attempt.
    Final,
    /// A declared criterion no work of the plan can pass: the plan has to change.
    CriteriaWrong,
}

/// What a failure's menu can do about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outlook {
    /// The run pauses until the reset at `at`; a wake at or after that instant runs the node.
    WaitUntil { at: DateTime<Utc> },
    /// The options a person may choose from, each with its input.
    Choose(Vec<Remedy>),
    /// No option helps, and why.
    Stuck(Stuck),
}

/// What the log holds beside the failure: its clock, the node's tasks, grants and tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Facts {
    /// When the failure happened; a reset is read against it.
    pub failed_at: DateTime<Utc>,
    /// Whether the node's tasks hold work a continuation can pick up.
    pub continuable: bool,
    /// Whether the run may still grant the scope the failure asks for.
    pub may_grant: bool,
    /// Whether the run's tree moved since the node's last failed attempt.
    pub tree_moved: bool,
}

impl Failure {
    /// What can change this failure. Pure over the failure and `facts`, which come from the log.
    pub fn remedies(&self, facts: &Facts) -> Outlook {
        match self {
            Failure::Unset { .. } => Outlook::Stuck(Stuck::FrozenConfig),
            Failure::FrozenInput { .. } => Outlook::Stuck(Stuck::FrozenInput),
            Failure::PathsDenied { .. } => Outlook::Stuck(Stuck::ProjectDenies),
            Failure::ScopeOwed { .. } => owed_scope_outlook(facts),
            Failure::SessionRefused { session_refused } => {
                cause_outlook(&session_refused.cause, facts)
            }
            Failure::Limit { limit } => limit_outlook(limit, facts),
            Failure::PlanUnpassable { .. } => Outlook::Stuck(Stuck::CriteriaWrong),
            Failure::Unchanged { unchanged }
                if matches!(unchanged.failure.as_ref(), Failure::PlanUnpassable { .. }) =>
            {
                Outlook::Stuck(Stuck::CriteriaWrong)
            }
            Failure::TasksStopped { stopped } => stopped_outlook(stopped, facts),
            Failure::Red { .. } | Failure::Artifacts { .. } | Failure::Unchanged { .. }
                if !facts.tree_moved =>
            {
                Outlook::Stuck(Stuck::TreeUnchanged)
            }
            Failure::Red { .. } | Failure::Artifacts { .. } => retry(Precondition::TreeChanged),
            Failure::Unchanged { unchanged } => unchanged.failure.remedies(facts),
            Failure::ScopeViolated { .. } | Failure::ScopeRequested { .. } if facts.may_grant => {
                Outlook::Choose(vec![Remedy {
                    option: ReservedOption::Grant,
                    precondition: None,
                }])
            }
            Failure::ScopeViolated { .. } | Failure::ScopeRequested { .. } => {
                Outlook::Stuck(Stuck::NoGrant)
            }
            Failure::Exited { .. } => retry(if facts.tree_moved {
                Precondition::TreeChanged
            } else {
                Precondition::OutsideTreeChanged
            }),
            Failure::SessionNotStopped { .. } => retry(Precondition::OutsideTreeChanged),
            Failure::ChildFailed {
                child_failure: Some(child),
                ..
            } => child.remedies(facts),
            Failure::ChildFailed {
                child_failure: None,
                ..
            }
            | Failure::SessionDied { .. }
            | Failure::Message { .. } => retry(Precondition::OutsideTreeChanged),
        }
    }
}

/// The outlook of one retry waiting for `precondition`: the only option a cause with a named input offers.
fn retry(precondition: Precondition) -> Outlook {
    Outlook::Choose(vec![retry_remedy(precondition)])
}

/// The one remedy a retry waiting for `precondition` is.
fn retry_remedy(precondition: Precondition) -> Remedy {
    Remedy {
        option: ReservedOption::Retry,
        precondition: Some(precondition),
    }
}

/// The answers a person gives an owed scope request: a grant when the run
/// may give one, and a denial, which always stays open. Neither changes
/// the tree, so neither is a retry.
fn owed_scope_outlook(facts: &Facts) -> Outlook {
    let grant = facts.may_grant.then_some(Remedy {
        option: ReservedOption::Grant,
        precondition: None,
    });
    let deny = Remedy {
        option: ReservedOption::Deny,
        precondition: None,
    };
    Outlook::Choose(grant.into_iter().chain([deny]).collect())
}

/// What a refusal leaves open: a credential, a model or the service can be
/// fixed, and a final refusal stays final.
fn cause_outlook(cause: &RefusalCause, _facts: &Facts) -> Outlook {
    match cause {
        RefusalCause::Authentication => retry(Precondition::CredentialFixed),
        RefusalCause::InvalidModel => retry(Precondition::ModelAvailable),
        // The automatic wait for the service has ended, so a person decides when to try again.
        RefusalCause::Unreachable => retry(Precondition::ServiceReachable),
        RefusalCause::Final => Outlook::Stuck(Stuck::Final),
    }
}

/// A limit whose reset lies after the failure waits for it. One with no
/// reset, or one already past, waits for the service to accept requests again.
fn limit_outlook(limit: &AccountLimit, facts: &Facts) -> Outlook {
    match limit.resets_at {
        Some(at) if at > facts.failed_at => Outlook::WaitUntil { at },
        _ => retry(Precondition::LimitLifted),
    }
}

/// The outlook of a loop whose tasks stopped. A wait takes precedence, and
/// it lasts until the latest reset it waits for. Otherwise the continuation
/// comes first, as the menu lists it, then the remedies of every stop, and
/// only a loop no stop offers any remedy for is stuck.
fn stopped_outlook(stopped: &[TaskStop], facts: &Facts) -> Outlook {
    let mut remedies: Vec<Remedy> = Vec::new();
    if continuable(stopped, facts) {
        remedies.push(Remedy {
            option: ReservedOption::ContinueWork,
            precondition: None,
        });
    }
    let mut stuck: Option<Stuck> = None;
    let mut wait: Option<DateTime<Utc>> = None;
    for stop in stopped {
        match stop_outlook(&stop.cause, facts) {
            Outlook::Choose(found) => {
                for remedy in found {
                    if !remedies.contains(&remedy) {
                        remedies.push(remedy);
                    }
                }
            }
            Outlook::Stuck(reason) => stuck = stuck.or(Some(reason)),
            Outlook::WaitUntil { at } => wait = Some(wait.map_or(at, |latest| latest.max(at))),
        }
    }
    if let Some(at) = wait {
        return Outlook::WaitUntil { at };
    }
    match (remedies.is_empty(), stuck) {
        (false, _) => Outlook::Choose(remedies),
        (true, Some(reason)) => Outlook::Stuck(reason),
        // No stop explains the loop, so it reads as a failure with no named cause.
        (true, None) => retry(Precondition::OutsideTreeChanged),
    }
}

/// What one task's stop leaves open, read from its cause and the facts.
fn stop_outlook(cause: &TaskCause, facts: &Facts) -> Outlook {
    match cause {
        TaskCause::DeviationOwed { .. } | TaskCause::ScopeDecisionOwed => {
            Outlook::Stuck(Stuck::Owed)
        }
        TaskCause::PreCheck { found } => pre_check_outlook(found, facts),
        TaskCause::CriterionUnpassable { .. } => Outlook::Stuck(Stuck::CriteriaWrong),
        TaskCause::CommandDenied { .. } => Outlook::Stuck(Stuck::ProjectDenies),
        TaskCause::Unrunnable { runs } => match runs.first() {
            Some(run) => retry(Precondition::CommandRunnable {
                cmd: run.cmd.clone(),
            }),
            // A stop that names no run names no command to fix, so the outside decides.
            None => retry(Precondition::OutsideTreeChanged),
        },
        TaskCause::SessionRefused(refusal) => cause_outlook(&refusal.cause, facts),
        TaskCause::SessionLimited(limit) => limit_outlook(limit, facts),
        TaskCause::SessionDied(_) => retry(Precondition::OutsideTreeChanged),
        TaskCause::Unmet { .. } if facts.tree_moved => retry(Precondition::TreeChanged),
        TaskCause::Unmet { .. } => Outlook::Stuck(Stuck::TreeUnchanged),
        // The work the task carried over conflicts with the tree as it stands, and only a
        // change to that tree clears the conflict: a stop the tree has not moved since
        // answers the same way, so it is stuck on the tree, as a continuation is.
        TaskCause::CarriedWorkNoLongerApplies { .. } if facts.tree_moved => {
            retry(Precondition::TreeChanged)
        }
        TaskCause::CarriedWorkNoLongerApplies { .. } => Outlook::Stuck(Stuck::TreeUnchanged),
    }
}

/// What a pre-check that found criteria wrong leaves open. A criterion that
/// already passes before any work is wrong in itself, so only the plan can
/// fix it, and its presence keeps the outlook stuck whatever else was found.
/// Otherwise a retry is offered for each distinct command that cannot run, in
/// the order the check names them, since making a command runnable is what a
/// retry can change. A guard red before any work adds one retry after them,
/// waiting for what the guard reads: the tree once the tree moved, else
/// something outside it.
fn pre_check_outlook(found: &NonEmpty<Surprise>, facts: &Facts) -> Outlook {
    let mut remedies: Vec<Remedy> = Vec::new();
    let mut guard_red = false;
    for surprise in found.as_slice() {
        match surprise {
            Surprise::TrivialCriterion { .. } => return Outlook::Stuck(Stuck::CriteriaWrong),
            Surprise::Unrunnable { cmd, .. } => {
                let remedy = retry_remedy(Precondition::CommandRunnable { cmd: cmd.clone() });
                if !remedies.contains(&remedy) {
                    remedies.push(remedy);
                }
            }
            Surprise::BrokenGuard { .. } => guard_red = true,
        }
    }
    if guard_red {
        let precondition = if facts.tree_moved {
            Precondition::TreeChanged
        } else {
            Precondition::OutsideTreeChanged
        };
        remedies.push(retry_remedy(precondition));
    }
    Outlook::Choose(remedies)
}

/// Whether the loop's work can go on from where its tasks stopped: the node
/// holds work a continuation picks up, and no stop needs its command, its
/// criteria or its project's permissions changed first. A stop that carried
/// work over offers the continuation only once the tree moved, because the
/// same work over the same tree fails again.
fn continuable(stopped: &[TaskStop], facts: &Facts) -> bool {
    let blocked = stopped.iter().any(|stop| {
        matches!(
            stop.cause,
            TaskCause::Unrunnable { .. }
                | TaskCause::PreCheck { .. }
                | TaskCause::CommandDenied { .. }
        )
    });
    let carried = stopped
        .iter()
        .any(|stop| matches!(stop.cause, TaskCause::CarriedWorkNoLongerApplies { .. }));
    facts.continuable && !blocked && (facts.tree_moved || !carried)
}

#[cfg(test)]
mod tests;
