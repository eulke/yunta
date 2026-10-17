//! Why a node failed, as data.
//!
//! Every cause is a variant that carries the facts it is about: a command's
//! exit, a session's refusal, an account's limit, a loop's stops, a red
//! check, a child run's failure, and what a person decided. Keeping every
//! case in one type is what lets the log record the facts and every surface
//! write its own words from them.

use std::path::PathBuf;

use serde::{Deserialize, Deserializer, Serialize};

use crate::diagnostic::ArtifactFailure;
use crate::events::{Approval, ArtifactId, FindingSeverity, LoggedWords, TaskStop};
use crate::ids::{
    AdapterId, AgentName, FindingId, InputName, NodeId, OptionId, Responder, RunId, RunnerName,
    SkillName, TaskId,
};
use crate::protected::Protection;
use crate::shown::Flaw;
use crate::template::TemplateError;
use crate::workflow::JoinPolicy;
use crate::{Capability, CommitSha, Unbuildable};

mod accessors;
mod cause;
mod denied;
mod exit;
mod records;
mod remedy;
mod step;

pub use cause::{FailureCause, RefusalCause, SessionRefusal};
pub use denied::{DeniedBy, DeniedCommand, DeniedRule};
pub use exit::{
    AccountLimit, CommandExit, CommandOrigin, LimitSource, SessionDeath, SessionEnd, SessionExit,
    Unanswered, STOPPED_EXIT_CODE, TAIL_LINES,
};
pub use records::{
    CapabilityUse, ChildRefusal, FrozenRead, Interruption, MountError, NameProblem, ObjectProblem,
    OwedScope, RedCheck, RequestedScope, RunnerRefusal, Timed, Unchanged, UnpassableTask,
};
pub use remedy::{Facts, Outlook, Precondition, Remedy, Stuck};
pub use step::{EngineError, EngineStep};

/// Why a node failed.
///
/// Untagged: each variant is read by the keys it requires, and each requires
/// at least one key no other variant has. A payload that names none of them
/// is refused rather than read as some variant. `Message` reads a log line
/// that carries `outcome:` alone, a failure written as a sentence, so it
/// comes last. A field the wire always names but may leave `null` is read
/// with `deserialize_with`, since serde reads a missing `Option` as `None`,
/// which would let a payload match a variant it does not belong to.
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
    /// A frozen input no longer matches the hash the run was born with.
    FrozenInput { frozen_input: String },
    /// The engine cannot end a session it asked to stop; its process group may
    /// still run. `session_not_stopped` is what the stop reported; `release_failed`,
    /// what taking back what the session set up reported, when that failed too.
    SessionNotStopped {
        session_not_stopped: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        release_failed: Option<String>,
    },
    /// A session or an executor past its wall-clock budget: `after_ms` is the
    /// budget, and `timed` says whose it was.
    TimedOut {
        after_ms: u64,
        #[serde(default)]
        timed: Timed,
    },
    /// A task that spends more tokens than its budget.
    TokenBudget { used: u64, max: u64 },
    /// A template that does not render, and why. `input` is the child's input
    /// the template was rendering, when it was rendering one.
    Template {
        template: TemplateError,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        input: Option<InputName>,
    },
    /// An artifact the run cannot hand over; `problem` says whether its
    /// object is missing or holds other bytes.
    ArtifactUnreadable {
        artifact: ArtifactId,
        problem: ObjectProblem,
    },
    /// A name the engine refuses for an artifact; `problem` says which rule it breaks.
    ArtifactName { name: String, problem: NameProblem },
    /// A mount that cannot bring its artifact into the node; `error` says why.
    /// Boxed for the same reason as the child refusal: a failure is copied
    /// through every frame that holds one.
    MountUnreachable { error: Box<MountError> },
    /// A gate whose person chose `option`, which routes the run to `target`;
    /// `said` is what the person wrote with the choice, when they wrote any.
    GateChose {
        node: NodeId,
        option: OptionId,
        target: NodeId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        said: Option<String>,
    },
    /// A person accepted that the tests a task is held to are wrong; `owed_by` owes the fix.
    TestsAccepted { task: TaskId, owed_by: NodeId },
    /// A composed workflow the run could not start. The refusal is boxed: it
    /// is the largest payload a failure carries, and every frame that holds a
    /// failure pays for it.
    ChildRefused {
        workflow: String,
        refusal: Box<ChildRefusal>,
    },
    /// A composed workflow that would nest past the depth the run allows;
    /// `depth` is the level it would reach, and `workflow` the composed one.
    NestedTooDeep {
        depth: u32,
        max_depth: u32,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        workflow: String,
    },
    /// An artifact an external gate hands over that no node of this run produces.
    GateArtifactMissing {
        node: NodeId,
        gate_artifact: ArtifactId,
    },
    /// A capability the frozen adapter does not declare, which a node needs;
    /// `used_for` says what for, and `agent` names the agent that asked, when one did.
    CapabilityMissing {
        node: NodeId,
        capability: Capability,
        adapter: AdapterId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        used_for: Option<CapabilityUse>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent: Option<AgentName>,
    },
    /// An engine step failed on its own; `engine_error` names the step, and
    /// what its failing call reported of itself.
    EngineFailed { engine_error: EngineError },
    /// A command the project's permissions deny; `denied_rule` is the command and
    /// the list that denies it.
    CommandDenied { denied_rule: DeniedRule },
    /// A skill the node names that the run does not hold; `searched` lists the roots it looked in.
    SkillMissing {
        missing_skill: SkillName,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        searched: Vec<PathBuf>,
    },
    /// A loop that reached the iteration cap the run declares; `loop_cap` is that cap.
    LoopCapped { node: NodeId, loop_cap: u32 },
    /// A session the engine cannot confine as the run asks; `confinement` says why.
    Confinement { confinement: Unbuildable },
    /// A frozen read that cannot give the node what it asked for; `frozen_read` says why.
    FrozenRead { frozen_read: FrozenRead },
    /// A parallel group that failed under its join: for `join: all`, the failed
    /// children with the first first; for `join: any`, every child.
    ChildrenFailed {
        join: JoinPolicy,
        children_failed: Vec<NodeId>,
    },
    /// A child that ran with no terminal event when the engine last stopped,
    /// under `on_interrupt: fail_if_uncertain`.
    Uncertain { uncertain: NodeId },
    /// A gate a person rejected from the console, on the approval it asked for.
    Rejected {
        rejected_by: Responder,
        on: Approval,
    },
    /// A plan the planner did not publish, sent back with the flaws it has.
    SentBack { sent_back: Vec<Flaw> },
    /// A findings gate with findings at or above its severity.
    FindingsOpen {
        at_or_above: FindingSeverity,
        open_findings: Vec<FindingId>,
    },
    /// The loser of a `join: any` race, which a sibling in its group finished ahead of.
    Interrupted { interrupted_by: Interruption },
    /// A reviewer requested changes on an external gate's pull request, at `reviewed_sha`.
    ChangesRequested {
        requested_by: String,
        reviewed_sha: CommitSha,
        comments: usize,
    },
    /// A pull request closed without approval; `closed_pull_request` is its
    /// reference when the forge gave one.
    PullRequestClosed {
        #[serde(deserialize_with = "required_option")]
        closed_pull_request: Option<String>,
    },
    /// A loop with no task ready and not every task done; `not_ready` names
    /// the tasks that are waiting.
    LoopStalled { not_ready: Vec<TaskId> },
    /// A loop whose run holds no tasks document; `tasks_for` is the loop's node.
    NoTasksDocument { tasks_for: NodeId },
    /// A node's runner that did not resolve to an adapter, and why.
    RunnerUnresolved {
        runner: RunnerName,
        runner_refusal: RunnerRefusal,
    },
    /// Reads a log line that carries `outcome:` alone; no writer creates it.
    /// The words are the sentence an older log stored, kept as written.
    Message { outcome: LoggedWords },
}

/// Reads a field the wire always names, `null` included. It is the one way a
/// variant's only key can be required: serde reads a missing `Option` as `None`.
fn required_option<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer)
}

/// A template that renders a node's own text: no child's input is named.
impl From<TemplateError> for Failure {
    fn from(template: TemplateError) -> Self {
        Failure::Template {
            template,
            input: None,
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

    #[test]
    fn a_denial_read_from_a_log_without_reasons_names_its_paths() {
        let failure: Failure =
            serde_json::from_value(serde_json::json!({"denied_paths": ["a.txt", ".claude/x"]}))
                .expect("a log written before reasons existed reads back");
        assert_eq!(
            failure,
            Failure::PathsDenied {
                denied_paths: vec!["a.txt".into(), ".claude/x".into()],
                reasons: Vec::new(),
            }
        );
    }

    #[test]
    fn a_payload_naming_no_failure_is_refused_rather_than_read_as_one() {
        for payload in [serde_json::json!({}), serde_json::json!({"unrelated": 1})] {
            assert!(
                serde_json::from_value::<Failure>(payload.clone()).is_err(),
                "{payload}"
            );
        }
    }

    #[test]
    fn a_closed_pull_request_needs_its_key_even_when_the_reference_is_null() {
        let closed: Failure =
            serde_json::from_value(serde_json::json!({"closed_pull_request": null}))
                .expect("a null reference reads back");
        assert_eq!(
            closed,
            Failure::PullRequestClosed {
                closed_pull_request: None,
            }
        );
        let message: Failure = serde_json::from_value(serde_json::json!({"outcome": "closed"}))
            .expect("an outcome alone reads back");
        assert_eq!(
            message,
            Failure::Message {
                outcome: "closed".into(),
            }
        );
    }
}
