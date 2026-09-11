//! The task cycle — the part of the ledger cycle
//! that actually runs a task through pre-check, dispatch, post-check and
//! scope check. `yunta_engine::register` validates a ledger before
//! any of this; this module is what happens once a task is `ready`.
//!
//! The engine, never the agent, decides `done`: [`run_task`] always
//! re-runs every criterion after dispatch, regardless of what the
//! session reported — an agent that claims success with red criteria
//! still leaves the task not-done.

mod attempt;
mod criteria;
mod session;

use std::path::{Path, PathBuf};

use thiserror::Error;
use tokio_util::sync::CancellationToken;
use yunta_adapters::{Adapter, Budget, PermissionProfile};
use yunta_core::events::TokenUsage;
use yunta_core::{AdapterError, Task, TaskId};
use yunta_storage::StorageError;

use crate::process::Supervision;
use crate::scope::{ScopeCheckError, ScopeCheckResult};
use attempt::{run_one_attempt, AttemptParams, AttemptStep};

pub use criteria::{post_check, pre_check, Memo};
pub(crate) use session::dispatch_session;
pub use session::{DispatchError, SessionObserver, SessionSetup};

#[derive(Debug, Error)]
pub enum TaskCycleError {
    #[error("failed to run criterion `{cmd}` for task `{task}`")]
    Criterion {
        task: TaskId,
        cmd: String,
        #[source]
        source: crate::process::SpawnError,
    },
    #[error("adapter failed to spawn a session for task `{task}`")]
    Spawn {
        task: TaskId,
        #[source]
        source: AdapterError,
    },
    #[error("failed to append a session audit event for task `{task}`")]
    Audit {
        task: TaskId,
        #[source]
        source: StorageError,
    },
    #[error(transparent)]
    ScopeCheck(#[from] ScopeCheckError),
    #[error("failed to evaluate task `{task}`'s scope expansion request: {source}")]
    ScopeExpansion {
        task: TaskId,
        #[source]
        source: crate::scope_expansion::ScopeExpansionError,
    },
    #[error(
        "failed to compute the working tree's hash for memoization: {}",
        crate::git::failed(.args, .cwd, .detail)
    )]
    TreeHash {
        args: String,
        cwd: std::path::PathBuf,
        detail: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriterionRun {
    pub cmd: String,
    pub exit_code: i32,
    pub is_guard: bool,
    /// Whether this result came from the memoization cache instead of
    /// an actual execution — `criteria_checked` records it so recibo/replay
    /// show what ran versus what was reused, nothing verified in silence.
    pub reused: bool,
    /// Wall-clock milliseconds the execution took — what the
    /// learned ordering feeds on. `None` when `reused` (nothing ran).
    pub duration_ms: Option<u64>,
}

/// The pre-check's verdict: "esta fase valida al
/// validador" — a non-guard criterion that already passes, or a guard
/// that's already red, means the criteria themselves are wrong, not that
/// the (not-yet-started) work is wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreCheckOutcome {
    Red,
    TrivialCriterion { cmd: String },
    BrokenGuard { cmd: String },
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
    /// adapter never emits it.
    Crashed,
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
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaskOutcome {
    Done,
    Blocked {
        reason: String,
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
}

/// Default retry cap ("cap configurable, default 2").
pub const DEFAULT_MAX_RETRIES: u32 = 2;

/// The permission/scope policy a task cycle enforces:
/// `permissions` is the merged model every criterion command is checked
/// against before anything runs — a violating criterion blocks the whole
/// task citing the rule (a policy outcome in the report, never an engine
/// abort), scanned here at the cycle's single entry point so the
/// standalone [`pre_check`]/[`post_check`] helpers stay pure building
/// blocks. `profile` is the node's own rung of the same permission
/// ladder, forwarded to every session this cycle opens. `scope_expansion`
/// carries the loop node's own settings (absent means the schema's
/// own default, `deny`); `grants` is the batch's shared
/// [`crate::scope_expansion::GrantLedger`] — `max_per_run` is
/// run-scoped, not task-scoped, and the ledger's atomic cap window is
/// what makes the count exact when several batch members request at
/// once. `already_granted_paths` are the paths every *prior*
/// `scope_expansion_granted` on the log authorized for this task — a
/// human grant lands between attempts, so the retry's effective scope
/// must include them from the very first diff it evaluates.
pub struct ScopeGovernance<'a> {
    pub permissions: Option<&'a yunta_core::PermissionsConfig>,
    pub profile: PermissionProfile,
    pub scope_expansion: Option<&'a yunta_core::ScopeExpansion>,
    /// The run's `limits.max_expansion_files` ceiling, resolved once by
    /// the caller — a `rules`-mode request touching more files than this
    /// is denied.
    pub max_expansion_files: usize,
    pub grants: &'a crate::scope_expansion::GrantLedger,
    pub already_granted_paths: &'a [String],
}

/// The resources and retry policy one task's attempts run under —
/// distinct from [`ScopeGovernance`] (what the session may touch) and
/// from the cross-cutting `audit`/`cancel`/`setup` surfaces (who's
/// watching and how it stops).
pub struct AttemptEnv<'a> {
    pub adapter: &'a dyn Adapter,
    pub cwd: &'a Path,
    pub max_retries: u32,
    pub budget: Budget,
    pub memo: &'a Memo,
    /// Where every criterion's process registers for the run.
    pub registry: Option<&'a crate::process_registry::ProcessRegistry>,
}

/// Runs a task through the full cycle: pre-check once, then
/// dispatch → post-check → scope-check per attempt, retrying with a
/// fresh session up to `max_retries` times before `Blocked`.
///
/// Never trusts the session's own outcome: `succeeded` on each
/// attempt is decided entirely by re-running criteria and the scope
/// diff, regardless of whether the session reported `Completed`.
#[tracing::instrument(
    skip_all,
    fields(
        task = %task.id,
        node_id = audit.map(|(_, node)| node.as_str()).unwrap_or_default(),
    )
)]
pub async fn run_task(
    task: &Task,
    instruction: &str,
    env: AttemptEnv<'_>,
    governance: ScopeGovernance<'_>,
    audit: Option<(&dyn SessionObserver, &yunta_core::NodeId)>,
    cancel: &CancellationToken,
    setup: &SessionSetup,
) -> Result<TaskCycleReport, TaskCycleError> {
    // What the adapter declares it stages, per attempt; nothing before
    // a session opens.
    let mut last_staged: Vec<PathBuf> = Vec::new();
    let AttemptEnv {
        adapter,
        cwd,
        max_retries,
        budget,
        memo,
        registry,
    } = env;
    let supervision = Supervision {
        registry,
        cancel: Some(cancel),
        env: &[],
    };
    let ScopeGovernance {
        permissions,
        profile,
        scope_expansion,
        max_expansion_files,
        grants,
        already_granted_paths,
    } = governance;
    for criterion in &task.criteria {
        if let Some(rule) = crate::permissions::command_violation(&criterion.cmd, permissions) {
            return Ok(TaskCycleReport {
                task_id: task.id.clone(),
                staged: last_staged.clone(),
                pre_check: Vec::new(),
                attempts: Vec::new(),
                outcome: TaskOutcome::Blocked { reason: rule },
                needs_human_decision: false,
            });
        }
    }

    let (pre_runs, pre_outcome) = pre_check(task, cwd, memo, supervision).await?;

    // The pre-check validates the criteria before any work: a non-guard that
    // already passes, or a guard already red, means the criteria are wrong,
    // not the task. Only `Red` — nothing prejudged — proceeds to the
    // attempts; every other verdict blocks the task naming what to fix.
    let blocked_before_work = match pre_outcome {
        PreCheckOutcome::Red => None,
        PreCheckOutcome::TrivialCriterion { cmd } => Some(format!(
            "criterion `{cmd}` already passes before any work — the criteria need fixing, not the task"
        )),
        PreCheckOutcome::BrokenGuard { cmd } => {
            Some(format!("guard `{cmd}` is already red before any work started"))
        }
    };
    if let Some(reason) = blocked_before_work {
        return Ok(TaskCycleReport {
            task_id: task.id.clone(),
            staged: last_staged.clone(),
            pre_check: pre_runs,
            attempts: Vec::new(),
            outcome: TaskOutcome::Blocked { reason },
            needs_human_decision: false,
        });
    }

    let params = AttemptParams {
        task,
        instruction,
        adapter,
        cwd,
        budget,
        memo,
        profile,
        scope_expansion,
        max_expansion_files,
        grants,
        already_granted_paths,
        audit,
        cancel,
        setup,
        supervision,
    };
    let mut attempts = Vec::new();
    for attempt in 1..=(max_retries + 1) {
        let (staged, step) = run_one_attempt(&params, attempt).await?;
        last_staged = staged;
        match step {
            AttemptStep::Stop {
                record,
                outcome,
                needs_human_decision,
            } => {
                attempts.push(record);
                return Ok(TaskCycleReport {
                    task_id: task.id.clone(),
                    staged: last_staged,
                    pre_check: pre_runs,
                    attempts,
                    outcome,
                    needs_human_decision,
                });
            }
            AttemptStep::Again(record) => attempts.push(record),
        }
    }

    Ok(TaskCycleReport {
        task_id: task.id.clone(),
        staged: last_staged.clone(),
        pre_check: pre_runs,
        attempts,
        needs_human_decision: false,
        outcome: TaskOutcome::Blocked {
            reason: format!(
                "criteria still red or scope violated after {} attempt(s)",
                max_retries + 1
            ),
        },
    })
}
