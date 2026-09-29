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
    TrivialCriterion {
        cmd: String,
    },
    /// A guard already red on the tree the task starts from: what broke
    /// it came before the task. `proves` is what the guard is there to
    /// show, when the task or the run says so.
    BrokenGuard {
        cmd: String,
        proves: Option<String>,
    },
    /// A criterion that never answered: its command could not be found
    /// or executed where the engine runs criteria, or it was stopped.
    /// No work on the tree changes that.
    Unrunnable {
        run: CriterionRun,
    },
}

impl std::fmt::Display for Surprise {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Surprise::TrivialCriterion { cmd } => write!(
                f,
                "criterion `{cmd}` already passes before any work — the criteria need \
                 fixing, not the task"
            ),
            Surprise::BrokenGuard { cmd, proves } => {
                write!(f, "guard `{cmd}` is already red before any work started")?;
                match proves {
                    Some(proves) => write!(f, " — it is there to show that {proves}"),
                    None => Ok(()),
                }
            }
            Surprise::Unrunnable { run } => write!(
                f,
                "criterion `{}` could not run: exit {} — the criteria need fixing, or \
                 the environment the engine runs them in does",
                run.cmd,
                run.exit_described()
            ),
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
            if run.could_not_run().is_some() {
                return Some(Surprise::Unrunnable { run: run.clone() });
            }
            match (run.is_guard, run.exit_code) {
                (true, code) if code != 0 => Some(Surprise::BrokenGuard {
                    cmd: run.cmd.clone(),
                    proves: criterion.proves.clone(),
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
    /// left the scope the task declared. `red` and `outside` are what the
    /// last attempt left: the criteria still failing, and the paths it
    /// changed outside the scope.
    Unmet {
        attempts: u32,
        red: Vec<CriterionRun>,
        outside: Vec<PathBuf>,
    },
    /// A scope-expansion request is owed a human decision, and no
    /// further session spends budget while one is owed.
    ScopeDecisionOwed,
    /// The session reported a failure nothing will retry, and the
    /// criteria the engine checked itself are still red.
    NonRetryable,
    /// After an attempt, a criterion never answered: its command could
    /// not be found or executed where the engine runs criteria. Another
    /// attempt would change the tree, never that.
    Unrunnable { runs: Vec<CriterionRun> },
    /// A person chose to continue from the work the task's last attempt
    /// left, and that work no longer applies on the run's tree: these are
    /// the paths it stopped on.
    CarriedWorkNoLongerApplies { paths: Vec<PathBuf> },
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
            BlockedCause::Unmet {
                attempts,
                red,
                outside,
            } => write_unmet(f, *attempts, red, outside),
            BlockedCause::ScopeDecisionOwed => {
                write!(f, "a scope expansion request needs a human decision")
            }
            BlockedCause::NonRetryable => write!(
                f,
                "the session reported a non-retryable failure and the criteria are still red"
            ),
            BlockedCause::Unrunnable { runs } => write!(
                f,
                "a criterion could not run, and another attempt would not change that: {}",
                exits(runs, "exits")
            ),
            BlockedCause::CarriedWorkNoLongerApplies { paths } => {
                let listed: Vec<String> = paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect();
                write!(
                    f,
                    "the work its last attempt left no longer applies on the run's tree \
                     (conflicts in {}); run it again from scratch",
                    listed.join(", ")
                )
            }
            BlockedCause::CommandDenied { rule } => write!(f, "{rule}"),
            BlockedCause::SessionDied(died) => write!(f, "{died}"),
        }
    }
}

/// Each run as "`cmd` <verb> <exit code and what it means>", and the
/// last line it printed, in order: a person deciding about the task reads
/// why it fails without opening what the run kept of it.
fn exits(runs: &[CriterionRun], verb: &str) -> String {
    runs.iter()
        .map(|run| {
            yunta_core::text::aside(
                format!("`{}` {verb} {}", run.cmd, run.exit_described()),
                &run.said().unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// What an exhausted task's last attempt left: the criteria still red,
/// and the paths it changed outside the scope.
fn write_unmet(
    f: &mut std::fmt::Formatter<'_>,
    attempts: u32,
    red: &[CriterionRun],
    outside: &[PathBuf],
) -> std::fmt::Result {
    write!(f, "not done after {attempts} attempt(s)")?;
    let outside: Vec<String> = outside
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    match (red.is_empty(), outside.is_empty()) {
        (false, false) => write!(
            f,
            ": {}, and the work changed {} outside its scope",
            exits(red, "still exits"),
            outside.join(", ")
        ),
        (false, true) => write!(f, ": {}", exits(red, "still exits")),
        (true, false) => write!(
            f,
            ": the criteria pass, but the work changed {} outside its scope",
            outside.join(", ")
        ),
        (true, true) => Ok(()),
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
    /// The session this attempt ran in, once it opened one — what an
    /// attempt after an engine grant resumes.
    pub session: Option<yunta_core::SessionId>,
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
