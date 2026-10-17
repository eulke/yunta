//! Why a task stopped: the cause a `blocked` status carries, the stop
//! that names the task it belongs to, and what a criterion's first run
//! told the engine before any work began.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::events::node::payloads::CriterionResult;
use crate::events::tasks::payloads::{CriterionUnpassablePayload, DeviationDeclaredPayload};
use crate::events::{AccountLimit, DeniedRule, SessionDeath, SessionRefusal};
use crate::ids::TaskId;
use crate::NonEmpty;

/// Why a task stopped short of done. A `blocked` status carries one, and
/// the words a person reads for it are written by the render crate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskCause {
    /// The criteria were wrong before any work started: what the
    /// pre-check found, one entry per criterion.
    PreCheck { found: NonEmpty<Surprise> },
    /// Every attempt ran and the criteria are still red, or the work left
    /// the scope the task declared. `red` and `outside` are what the last
    /// attempt left: the criteria still failing, and the paths it changed
    /// outside the scope.
    Unmet {
        attempts: u32,
        red: Vec<CriterionResult>,
        outside: Vec<PathBuf>,
    },
    /// After an attempt, a criterion never answered: its command could not
    /// be found or executed, or it was stopped before it answered. Another
    /// attempt would change the tree, never that.
    Unrunnable { runs: Vec<CriterionResult> },
    /// The session refused the request, with the adapter's message and the
    /// cause it gave. The cycle ends here: no other session runs the task.
    SessionRefused(SessionRefusal),
    /// The account's limit ended the session; the node waits for its reset.
    SessionLimited(AccountLimit),
    /// The session an attempt worked in ended without a terminal event, and
    /// how its process went.
    SessionDied(SessionDeath),
    /// A person chose to continue from the work the task's last attempt
    /// left, and that work no longer applies on the run's tree: these are
    /// the paths it conflicts on.
    CarriedWorkNoLongerApplies { paths: Vec<PathBuf> },
    /// A criterion's own command is one the run's permissions refuse, so
    /// the task cannot be verified at all. `rule` is the command and the
    /// list that refused it.
    CommandDenied { rule: DeniedRule },
    /// A scope-expansion request is owed a human decision, and no further
    /// session spends budget while one is owed.
    ScopeDecisionOwed,
    /// The session declared that its work departs from the plan, and a
    /// person answers each departure before the task closes.
    DeviationOwed {
        deviations: NonEmpty<DeviationDeclaredPayload>,
    },
    /// The session declared a criterion impossible on the plan's work. No
    /// further session retries it, and the planner rewrites the tasks.
    CriterionUnpassable {
        declared: CriterionUnpassablePayload,
    },
}

/// A task that stopped, and why: one entry of the list a loop's failure
/// carries for the tasks that kept it from closing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TaskStop {
    pub task_id: TaskId,
    pub cause: TaskCause,
}

/// What a criterion's first run told the engine before any work began.
/// Each case says the criteria are wrong, not the task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Surprise {
    /// A non-guard criterion that already passes before any work: the
    /// criteria need fixing, not the task.
    TrivialCriterion { cmd: String },
    /// A guard already red on the tree the task starts from: what broke it
    /// came before the task. `proves` is what the guard is there to show,
    /// when the task or the run says so.
    BrokenGuard { cmd: String, proves: Option<String> },
    /// A criterion that never answered; `Unanswered::of(exit_code)` says why.
    Unrunnable { cmd: String, exit_code: i32 },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{DeniedBy, DeniedCommand, LimitSource, RefusalCause};
    use crate::ids::AdapterId;

    /// The command a permission refused, and the pattern of the list that refused it.
    fn denied_rule() -> DeniedRule {
        DeniedRule::Command(DeniedCommand {
            command: "cargo test".to_string(),
            by: DeniedBy::Deny {
                pattern: "cargo *".to_string(),
            },
        })
    }

    /// A criterion whose command was never found: its run answered with 127.
    fn unrunnable_criterion() -> CriterionResult {
        CriterionResult {
            cmd: "yunta-no-such-tool".to_string(),
            exit_code: 127,
            r#type: None,
            reused: false,
            duration_ms: None,
            output: None,
            tail: Vec::new(),
            tree: None,
            head: None,
            conclusion: None,
        }
    }

    /// One cause of every shape a task can stop on.
    fn every_cause() -> Vec<TaskCause> {
        let refusal = SessionRefusal {
            adapter: AdapterId::from_static("mock"),
            cause: RefusalCause::Authentication,
            message: "Invalid API key".to_string(),
        };
        let unrunnable = unrunnable_criterion();
        vec![
            TaskCause::PreCheck {
                found: NonEmpty::from((
                    Surprise::Unrunnable {
                        cmd: "yunta-no-such-tool".to_string(),
                        exit_code: 127,
                    },
                    Vec::new(),
                )),
            },
            TaskCause::Unmet {
                attempts: 2,
                red: vec![unrunnable.clone()],
                outside: vec![PathBuf::from("notes.md")],
            },
            TaskCause::Unrunnable {
                runs: vec![unrunnable],
            },
            TaskCause::SessionRefused(refusal),
            TaskCause::SessionLimited(AccountLimit {
                source: LimitSource::Forge,
                resets_at: None,
                message: "rate limited".to_string(),
            }),
            TaskCause::SessionDied(SessionDeath {
                adapter: AdapterId::from_static("mock"),
                exit: None,
            }),
            TaskCause::CarriedWorkNoLongerApplies {
                paths: vec![PathBuf::from("src/lib.rs")],
            },
            TaskCause::CommandDenied {
                rule: denied_rule(),
            },
            TaskCause::ScopeDecisionOwed,
        ]
    }

    #[test]
    fn every_cause_reads_back_as_the_stop_it_was_written_as() {
        for cause in every_cause() {
            let written = serde_json::to_value(&cause).unwrap();
            let read: TaskCause = serde_json::from_value(written.clone()).unwrap();
            assert_eq!(read, cause, "{written}");
        }
    }

    #[test]
    fn a_stop_is_tagged_by_its_snake_case_name() {
        let written = serde_json::to_value(TaskCause::ScopeDecisionOwed).unwrap();
        assert_eq!(
            written,
            serde_json::json!({ "type": "scope_decision_owed" })
        );
    }

    #[test]
    fn every_surprise_reads_back_as_the_surprise_it_was_written_as() {
        let surprises = [
            Surprise::TrivialCriterion {
                cmd: "true".to_string(),
            },
            Surprise::BrokenGuard {
                cmd: "test -f seed".to_string(),
                proves: Some("the seed exists".to_string()),
            },
            Surprise::Unrunnable {
                cmd: "yunta-no-such-tool".to_string(),
                exit_code: 127,
            },
        ];
        for surprise in surprises {
            let written = serde_json::to_value(&surprise).unwrap();
            let read: Surprise = serde_json::from_value(written.clone()).unwrap();
            assert_eq!(read, surprise, "{written}");
        }
    }
}
