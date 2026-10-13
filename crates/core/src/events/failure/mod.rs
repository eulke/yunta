//! Why a node failed, as data.
//!
//! The one payload field that is not a plain value: a node fails with a
//! sentence the engine states, with declared artifacts that did not close,
//! or with a fact the log records (a command's exit, a session's refusal,
//! an account's limit, a loop's stops, a red check, a child run's failure).
//! Keeping every case in one type is what lets the log record the facts and
//! every surface produce its own prose from them, instead of the engine
//! writing prose once and three surfaces taking it apart again.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::diagnostic::{ArtifactFailure, Report};
use crate::events::TaskStop;
use crate::glob::{InvalidScopeGlob, ScopeGlob};
use crate::hash::ContentHash;
use crate::ids::{AdapterId, RunId};
use crate::protected::Protection;

mod cause;
mod exit;
mod records;
mod remedy;
mod sentences;

pub use cause::{FailureCause, RefusalCause, SessionRefusal};
pub use exit::{
    AccountLimit, CommandExit, CommandOrigin, LimitSource, SessionDeath, SessionEnd, SessionExit,
    Unanswered, STOPPED_EXIT_CODE, TAIL_LINES,
};
pub use records::{OwedScope, RequestedScope, Unchanged, UnpassableTask};
pub use remedy::{Facts, Outlook, Precondition, Remedy, Stuck};
pub use sentences::{loop_overrun_failure, PAST_THE_CAP};

/// Why a node failed.
///
/// Untagged, with `Message` last: a payload carrying `artifacts:` reads
/// as [`Failure::Artifacts`], one carrying `died:` as
/// [`Failure::SessionDied`], one carrying `exited:` as
/// [`Failure::Exited`], one carrying `outside_scope:` as
/// [`Failure::ScopeViolated`], one carrying `requested_scope:` as
/// [`Failure::ScopeRequested`], one carrying `owed:` as
/// [`Failure::ScopeOwed`], one carrying `unset:` as [`Failure::Unset`],
/// one carrying `unchanged:` as [`Failure::Unchanged`], one carrying
/// `denied_paths:` as [`Failure::PathsDenied`], one carrying
/// `unpassable:` as [`Failure::PlanUnpassable`], one carrying
/// `session_refused:` as [`Failure::SessionRefused`], one carrying
/// `limit:` as [`Failure::Limit`], one carrying `stopped:` as
/// [`Failure::TasksStopped`], one carrying `red:` as [`Failure::Red`],
/// one carrying `child_run:` as [`Failure::ChildFailed`], and a log
/// written before failures were data carries `outcome:` alone and reads
/// back as [`Failure::Message`]. That tolerance is the rule for what is
/// persisted and versioned, and it is why no reader needs to know which
/// version wrote the line it is looking at.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum Failure {
    /// Declared artifacts that did not close, each saying what went
    /// wrong with it: the file, the content of the document, or the run
    /// that owes the artifact and holds none of it.
    Artifacts { artifacts: Vec<ArtifactFailure> },
    /// The session the node was working in ended without a terminal
    /// event, and how its process went.
    SessionDied { died: SessionDeath },
    /// The node's diff reached paths no glob of its scope allows. Named
    /// one by one, because what a person does next — widen the scope,
    /// or change those files in the run's tree — is about exactly them.
    ScopeViolated { outside_scope: Vec<PathBuf> },
    /// The node's session asked for more scope than it has: its work is
    /// not done until a person answers, and the answer is theirs.
    ScopeRequested { requested_scope: RequestedScope },
    /// Tasks of a loop asked for scope beyond their own, and nobody was
    /// there to answer: each request is owed a person's decision, and
    /// nothing else of the loop can run until it gets one.
    ScopeOwed { owed: Vec<OwedScope> },
    /// A config key the node cannot run without, left unset in the
    /// config the run froze when it was created — so no attempt of this
    /// run can go differently.
    Unset { unset: crate::config::ConfigKey },
    /// A check that judges the run's tree was asked to run again on the
    /// very tree an earlier attempt failed on: it is refused before
    /// running, since the same command on the same tree answers the same.
    Unchanged { unchanged: Unchanged },
    /// The work reached paths no session of the run may write, each with
    /// the reason it is protected: `reasons` lines up with `denied_paths`
    /// by index. No failure widens them: a person grants an instruction file
    /// only to a session that asks for it, so the work goes, or the project
    /// changes its config.
    PathsDenied {
        denied_paths: Vec<PathBuf>,
        #[serde(default)]
        reasons: Vec<Protection>,
    },
    /// A command the node ran exited non-zero, and what it printed last
    /// is the reason a person reads.
    Exited { exited: CommandExit },
    /// Tasks whose declared criterion no work of the plan can pass: the
    /// planner rewrites the plan. Its key, `unpassable:`, is one no other
    /// variant has.
    PlanUnpassable { unpassable: Vec<UnpassableTask> },
    /// The session refused the request: the adapter's message, and the
    /// cause it read from it.
    SessionRefused { session_refused: SessionRefusal },
    /// The account's limit, or the forge's, stopped the run. `resets_at` is
    /// absolute: a forge's retry-after is added to the failure's own time.
    Limit { limit: AccountLimit },
    /// Tasks of a loop stopped before the loop could finish: each names
    /// the task and why it stopped.
    TasksStopped { stopped: Vec<TaskStop> },
    /// A judging check came out red on the tree it judged.
    Red { red: RedCheck },
    /// A composed workflow whose run ended in failure; `child_failure` is
    /// the failure its own log records, when it records one.
    ChildFailed {
        child_run: RunId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        child_failure: Option<Box<Failure>>,
    },
    /// A failure the engine states in one sentence.
    Message { outcome: String },
}

/// Why a judging check came out red: a coverage figure under its
/// threshold or none at all, a command that exited non-zero, or a baseline
/// suite that passed when it was measured and fails now.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RedCheck {
    /// The coverage the run measured, when it measured one, and the
    /// threshold it fell short of. `cmd` is the command that measured it.
    Coverage {
        cmd: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        measured: Option<f64>,
        threshold: f64,
    },
    /// The command that exited non-zero, with its exit code, the line it
    /// concluded with and the run object holding everything it printed.
    Command {
        cmd: String,
        exit_code: i32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        conclusion: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<ContentHash>,
    },
    /// The baseline suite passed when the lineage measured it and fails now;
    /// `tail` is what it printed last, which says why.
    Regression {
        cmd: String,
        exit_code: i32,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tail: Vec<String>,
    },
}

impl Failure {
    pub fn message(text: impl Into<String>) -> Self {
        Failure::Message {
            outcome: text.into(),
        }
    }

    pub fn artifacts(failures: Vec<ArtifactFailure>) -> Self {
        Failure::Artifacts {
            artifacts: failures,
        }
    }

    /// A session of `adapter` that ended without a terminal event.
    /// `exit` is what its process left behind, absent for a session
    /// with no process of its own.
    pub fn session_died(adapter: AdapterId, exit: Option<SessionExit>) -> Self {
        Failure::SessionDied {
            died: SessionDeath { adapter, exit },
        }
    }

    /// A diff that reached `outside_scope`, which the node's scope does
    /// not allow.
    pub fn scope_violated(outside_scope: Vec<PathBuf>) -> Self {
        Failure::ScopeViolated { outside_scope }
    }

    /// A node whose kind cannot run without `key`, which the run's config
    /// leaves unset.
    pub fn unset(key: crate::config::ConfigKey) -> Self {
        Failure::Unset { unset: key }
    }

    /// An attempt refused because nothing changed in the run's tree
    /// since attempt `since` failed on it with `failure`. A refusal of a
    /// refusal names the attempt that last ran, and what it failed with.
    pub fn unchanged(since: u32, failure: Failure) -> Self {
        match failure {
            Failure::Unchanged { unchanged } => Failure::Unchanged { unchanged },
            failure => Failure::Unchanged {
                unchanged: Unchanged {
                    since,
                    failure: Box::new(failure),
                },
            },
        }
    }

    /// A command that exited non-zero, and what it left behind.
    pub fn exited(exited: CommandExit) -> Self {
        Failure::Exited { exited }
    }

    /// The last lines the failing command printed, as the log holds them
    /// here. Empty for a failure that is not about a command, for a command
    /// whose output the log holds as a run object, and for one that printed
    /// nothing. A child run's failure hands over its own command's lines,
    /// which a person reads under this one.
    pub fn tail(&self) -> &[String] {
        match self {
            Failure::Exited { exited } => &exited.tail,
            Failure::Unchanged { unchanged } => unchanged.failure.tail(),
            Failure::ChildFailed {
                child_failure: Some(child),
                ..
            } => child.tail(),
            Failure::Red {
                red: RedCheck::Regression { tail, .. },
            } => tail,
            Failure::Artifacts { .. }
            | Failure::SessionDied { .. }
            | Failure::ScopeViolated { .. }
            | Failure::ScopeRequested { .. }
            | Failure::ScopeOwed { .. }
            | Failure::Unset { .. }
            | Failure::PathsDenied { .. }
            | Failure::PlanUnpassable { .. }
            | Failure::SessionRefused { .. }
            | Failure::Limit { .. }
            | Failure::TasksStopped { .. }
            | Failure::Red { .. }
            | Failure::ChildFailed { .. }
            | Failure::Message { .. } => &[],
        }
    }

    /// The run object holding everything the failing command printed.
    pub fn output(&self) -> Option<&ContentHash> {
        match self {
            Failure::Exited { exited } => exited.output.as_ref(),
            Failure::Red { red } => match red {
                RedCheck::Command { output, .. } => output.as_ref(),
                RedCheck::Coverage { .. } | RedCheck::Regression { .. } => None,
            },
            Failure::Unchanged { unchanged } => unchanged.failure.output(),
            Failure::ChildFailed {
                child_failure: Some(child),
                ..
            } => child.output(),
            Failure::Artifacts { .. }
            | Failure::SessionDied { .. }
            | Failure::ScopeViolated { .. }
            | Failure::ScopeRequested { .. }
            | Failure::ScopeOwed { .. }
            | Failure::Unset { .. }
            | Failure::PathsDenied { .. }
            | Failure::PlanUnpassable { .. }
            | Failure::SessionRefused { .. }
            | Failure::Limit { .. }
            | Failure::TasksStopped { .. }
            | Failure::ChildFailed { .. }
            | Failure::Message { .. } => None,
        }
    }

    /// Work that reached `paths`, which no session of the run may write,
    /// each path with the reason at the same index in `reasons`.
    pub fn paths_denied(paths: Vec<PathBuf>, reasons: Vec<Protection>) -> Self {
        Failure::PathsDenied {
            denied_paths: paths,
            reasons,
        }
    }

    /// A session that asked to be allowed `paths`, for `reason`.
    pub fn scope_requested(paths: Vec<ScopeGlob>, reason: impl Into<String>) -> Self {
        Failure::ScopeRequested {
            requested_scope: RequestedScope {
                paths,
                reason: reason.into(),
            },
        }
    }

    /// The paths the node wrote outside its scope. Empty for a failure
    /// that is not a violation of it. A child run's violations are its
    /// own, and do not widen this node's scope.
    pub fn outside_scope(&self) -> &[PathBuf] {
        match self {
            Failure::ScopeViolated { outside_scope } => outside_scope,
            Failure::Unchanged { unchanged } => unchanged.failure.outside_scope(),
            Failure::Artifacts { .. }
            | Failure::SessionDied { .. }
            | Failure::ScopeRequested { .. }
            | Failure::ScopeOwed { .. }
            | Failure::Unset { .. }
            | Failure::PathsDenied { .. }
            | Failure::Exited { .. }
            | Failure::PlanUnpassable { .. }
            | Failure::SessionRefused { .. }
            | Failure::Limit { .. }
            | Failure::TasksStopped { .. }
            | Failure::Red { .. }
            | Failure::ChildFailed { .. }
            | Failure::Message { .. } => &[],
        }
    }

    /// Whether a wider scope is what this failure needs — what makes a
    /// grant a way forward rather than a guess.
    pub fn wants_scope(&self) -> bool {
        match self {
            Failure::ScopeViolated { outside_scope } => !outside_scope.is_empty(),
            Failure::ScopeRequested { requested_scope } => !requested_scope.paths.is_empty(),
            Failure::ScopeOwed { owed } => owed.iter().any(|owed| !owed.paths.is_empty()),
            Failure::Unchanged { unchanged } => unchanged.failure.wants_scope(),
            Failure::Artifacts { .. }
            | Failure::SessionDied { .. }
            | Failure::Unset { .. }
            | Failure::PathsDenied { .. }
            | Failure::Exited { .. }
            | Failure::PlanUnpassable { .. }
            | Failure::SessionRefused { .. }
            | Failure::Limit { .. }
            | Failure::TasksStopped { .. }
            | Failure::Red { .. }
            | Failure::ChildFailed { .. }
            | Failure::Message { .. } => false,
        }
    }

    /// What a grant would add to the node's scope for the work behind
    /// this failure to stand: each path it wrote outside, exactly, or
    /// what its session asked for. Empty for a failure a wider scope
    /// would not change.
    pub fn scope_wanted(&self) -> Result<Vec<ScopeGlob>, InvalidScopeGlob> {
        match self {
            Failure::ScopeRequested { requested_scope } => Ok(requested_scope.paths.clone()),
            // Each task's request is its own to grant: none of it widens
            // the loop's scope.
            Failure::ScopeOwed { .. } => Ok(Vec::new()),
            _ => self
                .outside_scope()
                .iter()
                .map(|path| ScopeGlob::exact(path))
                .collect(),
        }
    }

    /// Every report behind this failure, each carrying the document it
    /// is about. What a diagnostic is rendered from; a failure whose
    /// artifacts name no document yields none.
    pub fn reports(&self) -> impl Iterator<Item = &Report> {
        self.failures().filter_map(ArtifactFailure::report)
    }

    /// Every declared artifact that did not close. Empty for a failure
    /// that has nothing to do with artifacts.
    pub fn failures(&self) -> impl Iterator<Item = &ArtifactFailure> {
        match self {
            Failure::Artifacts { artifacts } => artifacts.iter(),
            Failure::Unchanged { unchanged } => unchanged.failure.failures(),
            // A dead session names no artifact, and neither does a scope, a
            // command or a sentence. A refused session, a limit, a loop's
            // stops, a check's figure and a child run's failure name no
            // document this node declared either: the count of documents
            // that did not close is about documents this node declared.
            Failure::SessionDied { .. }
            | Failure::ScopeViolated { .. }
            | Failure::ScopeRequested { .. }
            | Failure::ScopeOwed { .. }
            | Failure::Unset { .. }
            | Failure::PathsDenied { .. }
            | Failure::Exited { .. }
            | Failure::PlanUnpassable { .. }
            | Failure::SessionRefused { .. }
            | Failure::Limit { .. }
            | Failure::TasksStopped { .. }
            | Failure::Red { .. }
            | Failure::ChildFailed { .. }
            | Failure::Message { .. } => [].iter(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_regression_hands_over_the_tail_it_printed() {
        let failure = Failure::Red {
            red: RedCheck::Regression {
                cmd: "make test".into(),
                exit_code: 1,
                tail: vec!["2 tests failed".to_string()],
            },
        };
        assert_eq!(failure.tail(), ["2 tests failed"]);
    }

    #[test]
    fn a_command_whose_output_the_log_holds_by_hash_hands_over_no_lines() {
        let failure = Failure::Red {
            red: RedCheck::Command {
                cmd: "make test".into(),
                exit_code: 1,
                conclusion: None,
                output: None,
            },
        };
        assert!(failure.tail().is_empty());
    }
}
