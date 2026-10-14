//! Why a task's cycle could not go on: what failed, and for which task.

use thiserror::Error;
use yunta_core::events::TokenUsage;
use yunta_core::{AdapterError, TaskId};
use yunta_storage::StorageError;

use crate::git::GitError;
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
    /// The engine asked the task's session to stop and could not stop it;
    /// `tokens` is what the session reported before the stop failed.
    #[error("the engine could not stop task `{task}`'s session: {detail}")]
    NotStopped {
        task: TaskId,
        detail: String,
        /// Boxed, as in `DispatchError::NotStopped`: the largest part of the variant.
        release: Option<Box<AdapterError>>,
        tokens: TokenUsage,
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
    #[error("task `{task}`'s session could not be confined as its node asks")]
    Confinement {
        task: TaskId,
        #[source]
        source: yunta_core::Unbuildable,
    },
    /// A session stages its paths once, so a record already taken is an
    /// engine fault: the cycle names it rather than dropping it.
    #[error("task `{task}`'s session staged its paths twice")]
    AlreadyStaged { task: TaskId },
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
    #[error(transparent)]
    Git(#[from] GitError),
    #[error("failed to stage what the working tree holds for memoization")]
    TreeIndex(#[source] std::io::Error),
    #[error("failed to read what the working tree holds for memoization")]
    TreeContent(#[source] Box<crate::worktree::WorktreeError>),
}

impl TaskCycleError {
    /// The same error, with `earlier` added to the spend its stopped session
    /// reported: the attempts before the one that could not stop its session
    /// spent tokens that the node which ran the cycle still owes.
    pub(super) fn with_earlier_spend(self, earlier: TokenUsage) -> Self {
        match self {
            TaskCycleError::NotStopped {
                task,
                detail,
                release,
                tokens,
            } => TaskCycleError::NotStopped {
                task,
                detail,
                release,
                tokens: earlier + tokens,
            },
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stop_that_fails_after_earlier_attempts_carries_their_spend() {
        let spent = TokenUsage {
            input: 10,
            output: 4,
            cached: None,
        };
        let earlier = TokenUsage {
            input: 3,
            output: 1,
            cached: None,
        };
        let stopped = TaskCycleError::NotStopped {
            task: TaskId::from_static("write-tests"),
            detail: "kill was refused".to_string(),
            release: None,
            tokens: spent,
        };
        match stopped.with_earlier_spend(earlier) {
            TaskCycleError::NotStopped { tokens, .. } => assert_eq!(
                tokens,
                TokenUsage {
                    input: 13,
                    output: 5,
                    cached: None,
                }
            ),
            other => panic!("the stop stays a stop: {other}"),
        }
    }

    #[test]
    fn an_error_that_is_not_a_stop_passes_through_unchanged() {
        let unchanged = TaskCycleError::Spawn {
            task: TaskId::from_static("write-tests"),
            source: AdapterError::Adapter {
                adapter: yunta_core::AdapterId::from_static("mock"),
                message: "refused".to_string(),
            },
        };
        let earlier = TokenUsage {
            input: 3,
            output: 1,
            cached: None,
        };
        assert!(matches!(
            unchanged.with_earlier_spend(earlier),
            TaskCycleError::Spawn { .. }
        ));
    }
}
