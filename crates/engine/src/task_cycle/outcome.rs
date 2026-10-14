//! What one task cycle produced, as data: what the pre-check found wrong
//! before any work started, how each attempt's session ended, and why the
//! cycle stopped.
//!
//! Every one of these is a fact the log can hold. The sentences a reader
//! sees are written from them by the render crate, and the words an agent
//! reads are written by the run tools, so no prose about them lives here.

use yunta_core::events::{
    CriterionResult, Failure, FailureCause, SessionExit, Surprise, TaskCause, TokenUsage,
    Unanswered,
};
use yunta_core::expansion::Decision;
use yunta_core::port::AgentError;
use yunta_core::{ScopeGlob, Seq, TaskId};

use crate::process::Printed;
use crate::scope::ScopeCheckResult;

/// What one criterion's command printed and answered, on the tree it ran on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriterionRun {
    pub cmd: String,
    pub exit_code: i32,
    pub is_guard: bool,
    /// Whether this result came from the memoization cache instead of
    /// an actual execution — `criteria_checked` records it so receipt/replay
    /// show what ran versus what was reused, nothing verified in silence.
    pub reused: bool,
    /// Wall-clock milliseconds the execution took — what the
    /// learned ordering feeds on. `None` when `reused` (nothing ran).
    pub duration_ms: Option<u64>,
    /// What the command printed on this tree: in this run of it, or, for
    /// a red answer the cache reused, in the run that gave it. `None` for
    /// a green answer reused, and for a check the engine states rather
    /// than runs.
    pub output: Option<Printed>,
    /// The tree the answer is for — what the checkout held — and, for a
    /// command that runs `git`, the commit it stood on. `None` for a
    /// check the engine states rather than runs.
    pub tree: Option<yunta_core::TreeId>,
    pub head: Option<String>,
}

impl CriterionRun {
    /// Why this criterion never answered, if it did not.
    pub fn could_not_run(&self) -> Option<Unanswered> {
        super::criteria::could_not_run(self.exit_code)
    }

    /// The line it concluded with, if it printed anything: see
    /// [`Printed::concluded`].
    pub fn conclusion(&self) -> Option<String> {
        self.output.as_ref().and_then(Printed::concluded)
    }

    /// How its exit code reads to an agent: the code, and what it means
    /// when the command never answered.
    pub fn exit_described(&self) -> String {
        match self.could_not_run() {
            Some(why) => format!(
                "{} ({})",
                self.exit_code,
                crate::run_tools::words::unanswered_words(why)
            ),
            None => self.exit_code.to_string(),
        }
    }
}

/// The pre-check's findings, read from what the criteria ran: a non-guard
/// that already passes, a guard already red, or a criterion that never
/// answered. Empty when every non-guard is red and every guard green.
///
/// A function of what ran and nothing else: the learned order decides
/// when the evidence arrives, never what is verified or what is
/// reported, and a replay derives the same verdict from
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
                return Some(Surprise::Unrunnable {
                    cmd: run.cmd.clone(),
                    exit_code: run.exit_code,
                });
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

#[derive(Debug, Clone, PartialEq)]
pub enum DispatchOutcome {
    Completed {
        summary: String,
    },
    /// The session ended in a failure the adapter stated, and the cause it
    /// gave. The error keeps whatever the adapter caught under its message.
    Failed {
        error: AgentError,
        cause: FailureCause,
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
    /// the token count from `Usage` events or the timeout
    /// demanded it, independent of whether the adapter itself honored
    /// `SessionRequest.budget`. `cause` is the budget the session ran past.
    BudgetExceeded {
        cause: Failure,
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
    /// What the post-check answered, one result per criterion of
    /// `post_check`, as the log holds it. Kept whether or not a log
    /// records it, so a cause quotes the same answers either way.
    pub post_results: Vec<CriterionResult>,
    pub scope: ScopeCheckResult,
    pub succeeded: bool,
    /// The request the session left, as the judgement decided it on this
    /// attempt's tree — `None` when it left none, the ordinary case. The
    /// caller emits the request's events from it.
    pub scope_expansion: Option<crate::scope_expansion::ScopeExpansionOutcome>,
    /// The paths of an escalation this attempt closed with, which nobody
    /// decides once the task is done. `None` for every other attempt.
    pub withdrawn: Option<Vec<ScopeGlob>>,
    /// A write the session's fence said it would have stopped and the
    /// diff carries anyway. `run_task` has no `RunCtx` to record it on,
    /// so it hands the breach to the caller that already records this
    /// attempt's `scope_checked`.
    pub fence_breach: Option<crate::scope::Breach>,
    /// The sequence number the log gave this attempt's post-check, which
    /// the cycle records the moment it runs. `None` without an observer.
    pub recorded: Option<Seq>,
}

impl AttemptRecord {
    /// Whether the request this attempt left waits for a person's decision.
    pub(super) fn escalated(&self) -> bool {
        matches!(
            self.scope_expansion.as_ref().map(|o| &o.decision),
            Some(Decision::Escalate { .. })
        )
    }

    /// Whether the request this attempt left is settled, so its file goes: a
    /// grant or a refusal answers it, and a withdrawn one has nobody left to.
    pub(super) fn request_settled(&self) -> bool {
        self.withdrawn.is_some()
            || self
                .scope_expansion
                .as_ref()
                .is_some_and(|o| !matches!(o.decision, Decision::Escalate { .. }))
    }

    /// Whether a fresh session takes the task on after this attempt, which
    /// left its request pending under a red criterion: one may close the task
    /// without what it asked for, and the request is then withdrawn. One does,
    /// once per cycle. When `earlier` already left a request pending, a person
    /// decides it, and no further session spends the run while they do.
    pub(super) fn fresh_session_owed(&self, earlier: &[AttemptRecord]) -> bool {
        let red = self.post_check.iter().any(|run| run.exit_code != 0);
        self.escalated() && red && !earlier.iter().any(AttemptRecord::escalated)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaskOutcome {
    Done,
    /// Why the task stopped short of done, as the cycle found it.
    Blocked {
        cause: TaskCause,
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
    pub staged: Vec<std::path::PathBuf>,
    /// The sequence number of the last check this cycle recorded — what
    /// the status change closing the cycle cites. `None` only for a
    /// cycle run without an observer, which records nothing.
    pub last_check: Option<Seq>,
}

impl TaskCycleReport {
    /// A cycle a cancellation cut before any criterion answered: nothing
    /// was run and nothing found wanting. `last_check` is the empty
    /// pre-check it recorded, which the status change closing it cites.
    pub(super) fn cut(task_id: TaskId, last_check: Option<Seq>) -> Self {
        TaskCycleReport {
            task_id,
            pre_check: Vec::new(),
            attempts: Vec::new(),
            outcome: TaskOutcome::Interrupted,
            needs_human_decision: false,
            staged: Vec::new(),
            last_check,
        }
    }
}
