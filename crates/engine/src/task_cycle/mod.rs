//! The task cycle — the part of the tasks cycle
//! that actually runs a task through pre-check, dispatch, post-check and
//! scope check. `yunta_engine::register` validates a tasks document before
//! any of this; this module is what happens once a task is `ready`.
//!
//! The engine, never the agent, decides `done`: [`run_task`] always
//! re-runs every criterion after dispatch, regardless of what the
//! session reported — an agent that claims success with red criteria
//! still leaves the task not-done.

mod attempt;
mod criteria;
mod outcome;
mod session;
mod stream;

use std::path::PathBuf;
use yunta_core::ScopeGlob;

use thiserror::Error;
use tokio_util::sync::CancellationToken;
use yunta_core::events::TaskLedger;
use yunta_core::port::{Adapter, Budget, PermissionProfile};
use yunta_core::{AdapterError, Task, TaskId};
use yunta_storage::StorageError;

pub use outcome::{
    surprises, AttemptRecord, BlockedCause, DispatchOutcome, Surprise, TaskCycleReport, TaskOutcome,
};

use crate::process::Supervision;
use crate::scope::ScopeCheckError;
use attempt::{run_one_attempt, AttemptParams, AttemptStep};

pub use criteria::{post_check, pre_check, Memo, Memoized};
pub(crate) use session::dispatch_session;
pub(crate) use session::Dispatched;
pub use session::{DispatchError, RunToolsNeed, SessionObserver, SessionSetup};

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
    #[error("task `{task}`'s session could not hold the run tools its node needs")]
    RunTools {
        task: TaskId,
        #[source]
        source: crate::run::runner_resolve::RunToolsSetupError,
    },
    #[error(transparent)]
    ScopeCheck(#[from] ScopeCheckError),
    #[error("failed to evaluate task `{task}`'s scope expansion request: {source}")]
    ScopeExpansion {
        task: TaskId,
        #[source]
        source: crate::scope_expansion::ScopeExpansionError,
    },
    /// A memoized command a caller ran that belongs to no task — a
    /// `baseline_compare` asking the same suite the criteria ask.
    #[error("failed to run `{cmd}`")]
    MemoizedCommand {
        cmd: String,
        #[source]
        source: crate::process::SpawnError,
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
    pub already_granted_paths: &'a [ScopeGlob],
}

/// The resources and retry policy one task's attempts run under —
/// distinct from [`ScopeGovernance`] (what the session may touch) and
/// from the cross-cutting `audit`/`cancel`/`setup` surfaces (who's
/// watching and how it stops).
pub struct AttemptEnv<'a> {
    pub adapter: &'a dyn Adapter,
    /// The loop node these task sessions belong to. A task session is
    /// the node's session: it runs on the node's runner and writes the
    /// node's declared files.
    pub node: &'a yunta_core::Node,
    /// The tree this task works in and the tree it started from. Every
    /// attempt runs in the same checkout and is judged against the same
    /// starting point, which is what makes one attempt answerable for
    /// what an earlier one of its own left behind.
    pub unit: &'a crate::worktree::Unit,
    pub max_retries: u32,
    pub budget: Budget,
    pub memo: &'a Memo,
    /// The run's tasks, as its log leaves them — what the pre-check
    /// reads to run the cheap criteria before the expensive ones,
    /// derived from the same log every wake derives its state from.
    pub history: &'a TaskLedger,
    /// What every subprocess of the cycle is born under: the run's
    /// registry, the node's token, the run's `subprocess_vars` and the
    /// run's clock. It reaches the spawn by parameter, so a criterion
    /// runs under the same governance as the session before it.
    pub supervision: Supervision<'a>,
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
        node,
        unit,
        max_retries,
        budget,
        memo,
        history,
        supervision,
    } = env;
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
                outcome: TaskOutcome::Blocked {
                    cause: BlockedCause::CommandDenied { rule },
                },
                needs_human_decision: false,
            });
        }
    }

    // A cycle whose token already fired has nothing to verify: every
    // subprocess the pre-check would run is governed by that same token,
    // so it would only produce "killed before it could answer" for the
    // caller to read as a verdict. A `join: any` sibling winning between
    // the batch starting and this task's first check is exactly that
    // case: the task was cut, not judged.
    if supervision.cancel.is_cancelled() {
        return Ok(TaskCycleReport {
            task_id: task.id.clone(),
            staged: last_staged.clone(),
            pre_check: Vec::new(),
            attempts: Vec::new(),
            outcome: TaskOutcome::Interrupted,
            needs_human_decision: false,
        });
    }

    let pre_runs = pre_check(task, &unit.worktree, memo, history, supervision).await?;

    // The pre-check validates the criteria before any work: a non-guard
    // that already passes, or a guard already red, means the criteria
    // are wrong, not the task. Nothing prejudged — the empty verdict —
    // proceeds to the attempts; anything found blocks the task naming
    // every one of them.
    if let Some(found) = yunta_core::NonEmpty::new(surprises(task, &pre_runs)) {
        return Ok(TaskCycleReport {
            task_id: task.id.clone(),
            staged: last_staged.clone(),
            pre_check: pre_runs,
            attempts: Vec::new(),
            outcome: TaskOutcome::Blocked {
                cause: BlockedCause::PreCheck(found),
            },
            needs_human_decision: false,
        });
    }

    let params = AttemptParams {
        task,
        instruction,
        adapter,
        node,
        unit,
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
            cause: BlockedCause::Unmet {
                attempts: max_retries + 1,
            },
        },
    })
}
