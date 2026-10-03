//! Why a task's cycle could not go on: what failed, and for which task.

use thiserror::Error;
use yunta_core::{AdapterError, TaskId};
use yunta_storage::StorageError;

use crate::scope::ScopeCheckError;

/// What stopped a task's cycle.
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
    #[error("failed to put the work task `{task}` left back into its checkout")]
    Carry {
        task: TaskId,
        #[source]
        source: Box<crate::worktree::WorktreeError>,
    },
    #[error("failed to lay task `{task}`'s tests over the tree its work starts from")]
    Spec {
        task: TaskId,
        #[source]
        source: Box<crate::worktree::WorktreeError>,
    },
    #[error("failed to evaluate task `{task}`'s scope expansion request: {source}")]
    ScopeExpansion {
        task: TaskId,
        #[source]
        source: crate::scope_expansion::ScopeExpansionError,
    },
    /// A memoized command a caller ran that belongs to no task — a
    /// `baseline_compare` asking the same suite the criteria ask.
    #[error("failed to keep what task `{task}`'s criteria printed")]
    KeepOutput {
        task: TaskId,
        #[source]
        source: std::io::Error,
    },
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
    #[error("failed to stage what the working tree holds for memoization")]
    TreeIndex(#[source] std::io::Error),
    #[error("failed to read what the working tree holds for memoization")]
    TreeContent(#[source] Box<crate::worktree::WorktreeError>),
}
