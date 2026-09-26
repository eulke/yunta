//! What one task cycle produced, as data: what the pre-check found
//! wrong before any work started, how each attempt's session ended, and
//! why the cycle stopped.
//!
//! Every one of these is a fact with a `Display` of its own, so the
//! sentence a reader sees is produced once, here, and no site building
//! a report can say it differently.

use std::path::PathBuf;

use yunta_core::events::{SessionDeath, SessionExit, TokenUsage};
use yunta_core::{Seq, TaskId};

use crate::scope::ScopeCheckResult;

use super::CriterionRun;

/// A criterion the pre-check found wrong before any work: "esta fase
/// valida al validador" — a non-guard that already passes, or a guard
/// that is already red, means the criteria themselves need fixing, not
/// the task nobody has started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Surprise {
    TrivialCriterion { cmd: String },
    BrokenGuard { cmd: String },
}

impl std::fmt::Display for Surprise {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Surprise::TrivialCriterion { cmd } => write!(
                f,
                "criterion `{cmd}` already passes before any work — the criteria need \
                 fixing, not the task"
            ),
            Surprise::BrokenGuard { cmd } => {
                write!(f, "guard `{cmd}` is already red before any work started")
            }
        }
    }
}

/// Everything the pre-check found wrong, in the order the task declares
/// its criteria; empty when every non-guard is red and every guard
/// green, which is the normal case.
///
/// A function of what ran and nothing else: the learned order decides
/// when the evidence arrives, never what is verified or what is
/// reported (D177), and a replay derives the same verdict from
/// `criteria_checked`.
pub fn surprises(task: &yunta_core::Task, runs: &[CriterionRun]) -> Vec<Surprise> {
    task.criteria
        .iter()
        .filter_map(|criterion| {
            // By command *and* kind: one task may declare the same
            // command twice, once as a guard and once not, and those
            // are two different questions about it.
            let is_guard = criterion.r#type == Some(yunta_core::events::CriterionType::Guard);
            let run = runs
                .iter()
                .find(|run| run.cmd == criterion.cmd && run.is_guard == is_guard)?;
            match (run.is_guard, run.exit_code) {
                (true, code) if code != 0 => Some(Surprise::BrokenGuard {
                    cmd: run.cmd.clone(),
                }),
                (false, 0) => Some(Surprise::TrivialCriterion {
                    cmd: run.cmd.clone(),
                }),
                _ => None,
            }
        })
        .collect()
}

/// Why a task stopped without being done. One type for every answer the
/// cycle gives, so the sentence a reader sees is produced once, here,
/// from the fact rather than from a `format!` at each site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockedCause {
    /// The criteria were wrong before any work started.
    PreCheck(yunta_core::NonEmpty<Surprise>),
    /// Every attempt ran and the criteria are still red, or the work
    /// left the scope the task declared.
    Unmet { attempts: u32 },
    /// A scope-expansion request is owed a human decision, and no
    /// further session spends budget while one is owed.
    ScopeDecisionOwed,
    /// The session reported a failure nothing will retry, and the
    /// criteria the engine checked itself are still red.
    NonRetryable,
    /// A criterion's own command is one the run's permissions refuse,
    /// so the task cannot be verified at all. `rule` is the refusal the
    /// permission check wrote, naming the pattern and the field.
    CommandDenied { rule: String },
    /// The session an attempt worked in ended without a terminal event.
    /// It left no work behind and said why, and another attempt would
    /// open the same session against the same configuration.
    SessionDied(SessionDeath),
}

impl std::fmt::Display for BlockedCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // One line per surprise: a task whose criteria are wrong in
            // two ways is wrong in two ways, and a reader fixes both.
            BlockedCause::PreCheck(found) => {
                let said: Vec<String> = found.as_slice().iter().map(ToString::to_string).collect();
                write!(f, "{}", said.join("\n"))
            }
            BlockedCause::Unmet { attempts } => write!(
                f,
                "criteria still red or scope violated after {attempts} attempt(s)"
            ),
            BlockedCause::ScopeDecisionOwed => {
                write!(f, "a scope expansion request needs a human decision")
            }
            BlockedCause::NonRetryable => write!(
                f,
                "the session reported a non-retryable failure and the criteria are still red"
            ),
            BlockedCause::CommandDenied { rule } => write!(f, "{rule}"),
            BlockedCause::SessionDied(died) => write!(f, "{died}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchOutcome {
    Completed {
        summary: String,
    },
    Failed {
        message: String,
        retryable: bool,
    },
    /// No terminal event at all — the engine synthesizes this, the
    /// adapter never emits it, and asks the session that fell silent how
    /// its process ended. `None` for a session with no process of its
    /// own.
    Crashed {
        exit: Option<SessionExit>,
    },
    /// The dispatch's own `CancellationToken` fired — a
    /// `join: any` sibling won, or the user cancelled the run. The
    /// session was cut (interrupt→kill); the *caller* decides what the
    /// cancellation means, because only it knows which token fired.
    Cancelled,
    /// The engine cut the session via `interrupt` → `kill`:
    /// the token count from `Usage` events or the wall-clock timeout
    /// demanded it, independent of whether the adapter itself honored
    /// `SessionRequest.budget`.
    BudgetExceeded {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttemptRecord {
    pub attempt: u32,
    pub dispatch: DispatchOutcome,
    /// Tokens this attempt's session consumed, from its `Usage` events.
    pub tokens: TokenUsage,
    pub post_check: Vec<CriterionRun>,
    pub scope: ScopeCheckResult,
    pub succeeded: bool,
    /// The agent's own expansion request this attempt, if
    /// it wrote one, and what the engine decided — `None` when no request
    /// file was found, the ordinary case. The caller (`loop_exec.rs`) owns
    /// emitting `scope_expansion_requested`/`granted`/`denied` and the
    /// finding conversion from this; `run_task` only decides and
    /// widens `scope` for this attempt's own check when granted.
    pub scope_expansion: Option<crate::scope_expansion::ScopeExpansionOutcome>,
    /// A write the session's fence said it would have stopped and the
    /// diff carries anyway. `run_task` has no `RunCtx` to record it on,
    /// so it hands the breach to the caller that already records this
    /// attempt's `scope_checked`.
    pub fence_breach: Option<crate::scope::Breach>,
    /// The sequence number the log gave this attempt's post-check, which
    /// the cycle records the moment it runs. `None` without an observer.
    pub recorded: Option<Seq>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaskOutcome {
    Done,
    Blocked {
        cause: BlockedCause,
    },
    /// The cycle's cancellation token fired mid-attempt — the
    /// session was cut (interrupt→kill) and the cycle stopped without a
    /// verdict. What that means for the task's status is the caller's
    /// call, not this cycle's.
    Interrupted,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TaskCycleReport {
    pub task_id: TaskId,
    pub pre_check: Vec<CriterionRun>,
    pub attempts: Vec<AttemptRecord>,
    pub outcome: TaskOutcome,
    /// `true` when any attempt's scope-expansion request escalated (
    /// `ask` mode, or `max_per_run` already exhausted) — neither is a
    /// verdict `run_task` can render alone, so the cycle stops retrying
    /// and the caller (`loop_exec.rs`) puts the decision to
    /// `HumanInteraction` — pausing only when no live surface answers —
    /// rather than burning further sessions while one is owed.
    pub needs_human_decision: bool,
    /// What the adapter declared it wrote into the task's worktree for
    /// its own mechanics during the last attempt — what the scope
    /// check at integration leaves out, exactly as the cycle's own
    /// check did.
    pub staged: Vec<PathBuf>,
    /// The sequence number of the last check this cycle recorded — what
    /// the status change closing the cycle cites. `None` only for a
    /// cycle run without an observer, which records nothing.
    pub last_check: Option<Seq>,
}
