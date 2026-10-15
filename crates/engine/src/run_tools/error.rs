//! Why a run tool call could not be honored. The variant is the cause and
//! its fields are the data; the text the session reads back is written
//! once, by `words::tool_error`, at the MCP boundary.

use yunta_core::NodeId;

/// Why a tool call could not be honored. The display is a code: the words
/// a session reads are `words::tool_error`'s.
#[derive(Debug, thiserror::Error)]
pub(crate) enum RunToolError {
    /// A refusal a session acts on: the numbered problems to fix, in the
    /// words the document's own diagnostics use. Never a deserializer's
    /// account of itself.
    #[error("refused")]
    Refused { text: String },
    /// The call carries a value that the log cannot keep out of the typed
    /// record it would become, so nothing was recorded.
    #[error("unredactable")]
    Unredactable { kind: String },
    #[error("invalid_submission")]
    InvalidSubmission { names: String, detail: String },
    #[error("invalid_request")]
    InvalidRequest {
        #[source]
        source: serde_json::Error,
    },
    #[error("invalid_departure")]
    InvalidDeparture {
        #[source]
        source: serde_json::Error,
    },
    #[error("invalid_answer")]
    InvalidAnswer {
        #[source]
        source: serde_json::Error,
    },
    #[error("empty_answer")]
    EmptyAnswer,
    #[error("own_finding")]
    OwnFinding {
        id: yunta_core::FindingId,
        update: String,
        withdraw: String,
    },
    #[error("settled_finding")]
    SettledFinding {
        node: NodeId,
        id: yunta_core::FindingId,
        settled: String,
    },
    #[error("no_such_finding")]
    NoSuchFinding {
        node: NodeId,
        id: yunta_core::FindingId,
    },
    #[error("withdrawn_finding")]
    WithdrawnFinding {
        node: NodeId,
        id: yunta_core::FindingId,
        reason: String,
    },
    #[error("empty_departure")]
    EmptyDeparture { field: &'static str },
    #[error("not_in_the_plan")]
    NotInThePlan { from: String, known: String },
    #[error("suite_departure")]
    SuiteDeparture { cmd: String },
    #[error("not_in_blackboard_group")]
    NotInBlackboardGroup,
    #[error("no_scope_to_widen")]
    NoScopeToWiden,
    #[error("no_node_scope")]
    NoNodeScope,
    #[error("no_starting_tree")]
    NoStartingTree,
    #[error("criterion_conflict")]
    CriterionConflict { pending: String },
    #[error("not_red_on_the_work")]
    NotRedOnTheWork { criterion: String, exit: i32 },
    #[error("criterion")]
    Criterion {
        #[source]
        source: crate::scope_expansion::ScopeExpansionError,
    },
    #[error("record")]
    Record {
        #[source]
        source: crate::run::RunError,
    },
    #[error("not_a_task_session")]
    NotATaskSession { tool: String },
    #[error("handover")]
    Handover { detail: String },
    #[error("plan")]
    Plan {
        #[source]
        source: crate::artifacts::HeldError,
    },
    #[error("check")]
    Check {
        #[source]
        source: crate::task_cycle::TaskCycleError,
    },
    #[error("superseded")]
    Superseded,
    /// The run stopped while the call was being answered. Nothing about the
    /// call was refused, so it gets no refusal text and no refusal is
    /// recorded for it.
    #[error("cancelled")]
    Cancelled,
    #[error("storage")]
    Storage {
        #[source]
        source: yunta_storage::StorageError,
    },
    #[error("render")]
    Render {
        #[source]
        source: serde_json::Error,
    },
    /// A request file that could not be read, parsed or written: its own sentence names the path.
    #[error(transparent)]
    Request {
        source: crate::scope_expansion::RequestError,
    },
    #[error("no_artifacts")]
    NoArtifacts { node: NodeId },
    #[error("undeclared_artifact")]
    UndeclaredArtifact { name: String, declared: String },
    #[error(transparent)]
    Accept {
        source: crate::artifacts::AcceptError,
    },
}

impl From<crate::artifacts::AcceptError> for RunToolError {
    /// A submission whose record the redaction refused is the call's own
    /// refusal, as it is on every other tool write; any other failure of an
    /// acceptance is the engine's.
    fn from(source: crate::artifacts::AcceptError) -> Self {
        match source {
            crate::artifacts::AcceptError::Log {
                source: yunta_storage::StorageError::Unredactable(unredactable),
                ..
            } => RunToolError::Unredactable {
                kind: unredactable.kind,
            },
            source => RunToolError::Accept { source },
        }
    }
}

impl RunToolError {
    /// The error a handover gives for a git call that did not answer: a call
    /// the run stopped is the run's cancellation, and any other failure means
    /// the document could not be proven, and says what git said.
    pub(super) fn handover_git(failed: crate::git::GitError) -> Self {
        match failed.cancelled() {
            true => RunToolError::Cancelled,
            false => RunToolError::Handover {
                detail: failed.detail(),
            },
        }
    }

    /// Whether a stop cut this call short: the run's, its task's, or its
    /// session's own, which stops what the session's calls run. Nothing
    /// about such a call failed or was refused.
    pub(super) fn cancelled(&self) -> bool {
        match self {
            RunToolError::Cancelled => true,
            RunToolError::Check { source } => source.cancelled(),
            RunToolError::Criterion { source } => source.cancelled(),
            RunToolError::Record { source } => matches!(source, crate::run::RunError::Cancelled),
            _ => false,
        }
    }

    /// Why this call was refused, in the closed word the log keeps. A call
    /// a stop cut short was refused by nothing, so it has no word.
    pub(super) fn refusal(&self) -> Option<yunta_core::events::RunToolRefusal> {
        use yunta_core::events::RunToolRefusal as R;
        if self.cancelled() {
            return None;
        }
        let refusal = match self {
            RunToolError::Cancelled => return None,
            RunToolError::CriterionConflict { .. } => R::RequestPending,
            RunToolError::InvalidSubmission { .. }
            | RunToolError::InvalidRequest { .. }
            | RunToolError::InvalidDeparture { .. }
            | RunToolError::InvalidAnswer { .. }
            | RunToolError::EmptyAnswer
            | RunToolError::EmptyDeparture { .. }
            | RunToolError::NotInThePlan { .. }
            | RunToolError::SuiteDeparture { .. }
            | RunToolError::NoArtifacts { .. }
            | RunToolError::UndeclaredArtifact { .. } => R::InvalidArguments,
            RunToolError::NoScopeToWiden
            | RunToolError::NoNodeScope
            | RunToolError::NotATaskSession { .. }
            | RunToolError::NotInBlackboardGroup => R::NotOfferedHere,
            RunToolError::OwnFinding { .. }
            | RunToolError::SettledFinding { .. }
            | RunToolError::NoSuchFinding { .. }
            | RunToolError::WithdrawnFinding { .. } => R::FindingNotAnswerable,
            RunToolError::Refused { .. }
            | RunToolError::NotRedOnTheWork { .. }
            | RunToolError::Superseded
            | RunToolError::Unredactable { .. } => R::Refused,
            RunToolError::NoStartingTree
            | RunToolError::Criterion { .. }
            | RunToolError::Record { .. }
            | RunToolError::Handover { .. }
            | RunToolError::Plan { .. }
            | RunToolError::Check { .. }
            | RunToolError::Storage { .. }
            | RunToolError::Render { .. }
            | RunToolError::Request { .. }
            | RunToolError::Accept { .. } => R::EngineFailed,
        };
        Some(refusal)
    }
}

impl From<yunta_storage::StorageError> for RunToolError {
    /// A record the redaction refused is the call's own refusal; any other
    /// failure of the log is the engine's.
    fn from(source: yunta_storage::StorageError) -> Self {
        match source {
            yunta_storage::StorageError::Unredactable(unredactable) => RunToolError::Unredactable {
                kind: unredactable.kind,
            },
            source => RunToolError::Storage { source },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{Cause, GitError, Invocation, Stopped};
    use crate::task_cycle::TaskCycleError;
    use yunta_core::events::RunToolRefusal;

    /// A check whose git ended with `cause`.
    fn check_whose_git(cause: Cause) -> RunToolError {
        RunToolError::Check {
            source: TaskCycleError::Git(GitError {
                invocation: Invocation {
                    dir: std::path::PathBuf::from("/checkout"),
                    args: vec!["write-tree".to_string()],
                },
                cause,
            }),
        }
    }

    #[test]
    fn a_check_a_stop_cut_short_has_no_refusal_and_one_that_failed_does() {
        let stopped = check_whose_git(Cause::Stopped(Stopped::Cancelled));
        assert!(stopped.cancelled());
        assert_eq!(stopped.refusal(), None);

        let failed = check_whose_git(Cause::Exit {
            code: Some(128),
            stderr: "fatal: not a git repository".to_string(),
        });
        assert!(!failed.cancelled());
        assert_eq!(failed.refusal(), Some(RunToolRefusal::EngineFailed));
    }
}
