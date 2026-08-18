//! The task cycle (T5.2, Contrato §5.2) — the part of the ledger cycle
//! that actually runs a task through pre-check, dispatch, post-check and
//! scope check. `yunta_engine::register` (T5.1) validates a ledger before
//! any of this; this module is what happens once a task is `ready`.
//!
//! The engine, never the agent, decides `done` (I5): [`run_task`] always
//! re-runs every criterion after dispatch, regardless of what the
//! session reported — an agent that claims success with red criteria
//! still leaves the task not-done.

use std::path::Path;

use futures::StreamExt;
use thiserror::Error;
use yunta_adapters::{
    Adapter, AgentEvent, AgentOutcome, Budget, PermissionProfile, SessionRequest,
};
use yunta_core::events::CriterionType;
use yunta_core::{Task, TaskId, YuntaError};

use crate::scope::{scope_check, ScopeCheckError, ScopeCheckResult};

#[derive(Debug, Error)]
pub enum TaskCycleError {
    #[error("failed to run criterion `{cmd}` for task `{task}`")]
    Criterion {
        task: TaskId,
        cmd: String,
        #[source]
        source: std::io::Error,
    },
    #[error("adapter failed to spawn a session for task `{task}`")]
    Spawn {
        task: TaskId,
        #[source]
        source: YuntaError,
    },
    #[error(transparent)]
    ScopeCheck(#[from] ScopeCheckError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriterionRun {
    pub cmd: String,
    pub exit_code: i32,
    pub is_guard: bool,
}

/// The pre-check's verdict (§5.2 step 2): "esta fase valida al
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
    /// No terminal event at all (O2) — the engine synthesizes this, the
    /// adapter never emits it.
    Crashed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptRecord {
    pub attempt: u32,
    pub dispatch: DispatchOutcome,
    pub post_check: Vec<CriterionRun>,
    pub scope: ScopeCheckResult,
    pub succeeded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskOutcome {
    Done,
    Blocked { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskCycleReport {
    pub task_id: TaskId,
    pub pre_check: Vec<CriterionRun>,
    pub attempts: Vec<AttemptRecord>,
    pub outcome: TaskOutcome,
}

/// Default retry cap (§5.2: "cap configurable, default 2").
pub const DEFAULT_MAX_RETRIES: u32 = 2;

async fn run_criterion(task_id: &TaskId, cwd: &Path, cmd: &str) -> Result<i32, TaskCycleError> {
    let status = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(cwd)
        .status()
        .await
        .map_err(|source| TaskCycleError::Criterion {
            task: task_id.clone(),
            cmd: cmd.to_string(),
            source,
        })?;
    Ok(status.code().unwrap_or(-1))
}

async fn run_all_criteria(task: &Task, cwd: &Path) -> Result<Vec<CriterionRun>, TaskCycleError> {
    let mut runs = Vec::with_capacity(task.criteria.len());
    for criterion in &task.criteria {
        let exit_code = run_criterion(&task.id, cwd, &criterion.cmd).await?;
        runs.push(CriterionRun {
            cmd: criterion.cmd.clone(),
            exit_code,
            is_guard: criterion.r#type == Some(CriterionType::Guard),
        });
    }
    Ok(runs)
}

/// Pre-check in rojo (§5.2 step 2): every non-`guard` criterion must
/// fail, every `guard` must pass. Runs every criterion regardless — the
/// report should show all of them, not stop at the first surprise.
pub async fn pre_check(
    task: &Task,
    cwd: &Path,
) -> Result<(Vec<CriterionRun>, PreCheckOutcome), TaskCycleError> {
    let runs = run_all_criteria(task, cwd).await?;

    let mut outcome = PreCheckOutcome::Red;
    for run in &runs {
        if matches!(outcome, PreCheckOutcome::Red) {
            if run.is_guard && run.exit_code != 0 {
                outcome = PreCheckOutcome::BrokenGuard {
                    cmd: run.cmd.clone(),
                };
            } else if !run.is_guard && run.exit_code == 0 {
                outcome = PreCheckOutcome::TrivialCriterion {
                    cmd: run.cmd.clone(),
                };
            }
        }
    }

    Ok((runs, outcome))
}

/// Post-check (§5.2 step 4): every criterion, guard or not, must now
/// pass.
pub async fn post_check(task: &Task, cwd: &Path) -> Result<Vec<CriterionRun>, TaskCycleError> {
    run_all_criteria(task, cwd).await
}

async fn dispatch(
    task: &Task,
    adapter: &dyn Adapter,
    cwd: &Path,
) -> Result<DispatchOutcome, TaskCycleError> {
    let request = SessionRequest {
        prompt: format!(
            "Read task `{}` ({}) from the ledger and implement it within its declared scope.",
            task.id, task.title
        ),
        cwd: cwd.to_path_buf(),
        model: None,
        agent: None,
        permissions: PermissionProfile::Edit,
        env: Default::default(),
        edit_constraints: Some(task.scope.clone()),
        budget: Budget::default(),
        adapter_settings: Default::default(),
    };

    let mut session = adapter
        .spawn(request)
        .await
        .map_err(|source| TaskCycleError::Spawn {
            task: task.id.clone(),
            source,
        })?;

    let mut terminal = None;
    {
        let mut stream = session.events();
        while let Some(event) = stream.next().await {
            match event {
                AgentEvent::Completed {
                    result: AgentOutcome { summary },
                } => terminal = Some(DispatchOutcome::Completed { summary }),
                AgentEvent::Failed { error, retryable } => {
                    terminal = Some(DispatchOutcome::Failed {
                        message: error.message,
                        retryable,
                    })
                }
                _ => {}
            }
        }
    }

    Ok(terminal.unwrap_or(DispatchOutcome::Crashed))
}

/// Runs a task through the full cycle (§5.2): pre-check once, then
/// dispatch → post-check → scope-check per attempt, retrying with a
/// fresh session up to `max_retries` times before `Blocked`.
///
/// Never trusts the session's own outcome (I5): `succeeded` on each
/// attempt is decided entirely by re-running criteria and the scope
/// diff, regardless of whether the session reported `Completed`.
pub async fn run_task(
    task: &Task,
    adapter: &dyn Adapter,
    cwd: &Path,
    max_retries: u32,
) -> Result<TaskCycleReport, TaskCycleError> {
    let (pre_runs, pre_outcome) = pre_check(task, cwd).await?;

    if !matches!(pre_outcome, PreCheckOutcome::Red) {
        let reason = match pre_outcome {
            PreCheckOutcome::TrivialCriterion { cmd } => format!(
                "criterion `{cmd}` already passes before any work — the criteria need fixing, not the task"
            ),
            PreCheckOutcome::BrokenGuard { cmd } => {
                format!("guard `{cmd}` is already red before any work started")
            }
            PreCheckOutcome::Red => unreachable!(),
        };
        return Ok(TaskCycleReport {
            task_id: task.id.clone(),
            pre_check: pre_runs,
            attempts: Vec::new(),
            outcome: TaskOutcome::Blocked { reason },
        });
    }

    let mut attempts = Vec::new();
    for attempt in 1..=(max_retries + 1) {
        let dispatch_outcome = dispatch(task, adapter, cwd).await?;
        let post_runs = post_check(task, cwd).await?;
        let scope = scope_check(cwd, &task.scope).await?;

        let criteria_green = post_runs.iter().all(|r| r.exit_code == 0);
        let succeeded = criteria_green && scope.violations.is_empty();

        attempts.push(AttemptRecord {
            attempt,
            dispatch: dispatch_outcome,
            post_check: post_runs,
            scope,
            succeeded,
        });

        if succeeded {
            return Ok(TaskCycleReport {
                task_id: task.id.clone(),
                pre_check: pre_runs,
                attempts,
                outcome: TaskOutcome::Done,
            });
        }
    }

    Ok(TaskCycleReport {
        task_id: task.id.clone(),
        pre_check: pre_runs,
        attempts,
        outcome: TaskOutcome::Blocked {
            reason: format!(
                "criteria still red or scope violated after {} attempt(s)",
                max_retries + 1
            ),
        },
    })
}
