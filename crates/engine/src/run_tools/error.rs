//! Why a run tool call could not be honored, rendered once at the MCP
//! boundary as the text the session reads back.

use yunta_core::NodeId;

/// Why a tool call could not be honored — rendered once, at the MCP
/// boundary, as the text the session gets back.
#[derive(Debug, thiserror::Error)]
pub(crate) enum RunToolError {
    /// A refusal a session acts on: the numbered problems to fix, in the
    /// words the document's own diagnostics use. Never a deserializer's
    /// account of itself.
    #[error("{text}")]
    Refused { text: String },
    /// The call carries a value that the log cannot keep out of the typed
    /// record it would become, so nothing was recorded.
    #[error(
        "nothing was recorded: a value this call carries cannot be kept out of the {kind} record"
    )]
    Unredactable { kind: String },
    #[error(
        "invalid submission — requires `document` (an object), the {names} document this node \
         declares: {detail}"
    )]
    InvalidSubmission { names: String, detail: String },
    #[error(
        "invalid request — requires paths (list) and reason, with an optional \
         proposed_criterion {{cmd}} or evidence (one of your task's criteria)"
    )]
    InvalidRequest {
        #[source]
        source: serde_json::Error,
    },
    #[error(
        "invalid declaration — requires `kind`: `departure`, with `from` (one of \
         `{{\"shape\": name}}`, `{{\"decision\": id}}`, `{{\"change\": at}}`, \
         `{{\"criterion\": cmd}}` or `\"outcome\"`), `planned`, `instead` and `why`; or \
         `unpassable`, with `criterion` and `why`"
    )]
    InvalidDeparture {
        #[source]
        source: serde_json::Error,
    },
    #[error(
        "invalid answer — requires `node` and `id`, the finding answered as its node reported \
         it, `answer` (`fixed` or `declined`) and `why`"
    )]
    InvalidAnswer {
        #[source]
        source: serde_json::Error,
    },
    #[error("an answer says why — `why` is empty")]
    EmptyAnswer,
    #[error(
        "`{id}` is a finding this node reported: change it with `{update}`, or take it back \
         with `{withdraw}`, saying why"
    )]
    OwnFinding {
        id: yunta_core::FindingId,
        update: String,
        withdraw: String,
    },
    #[error("node `{node}`'s finding `{id}` is settled — {settled}; there is nothing to answer")]
    SettledFinding {
        node: NodeId,
        id: yunta_core::FindingId,
        settled: String,
    },
    #[error("node `{node}` reported no finding `{id}` on this run")]
    NoSuchFinding {
        node: NodeId,
        id: yunta_core::FindingId,
    },
    #[error("node `{node}` took its finding `{id}` back — {reason}; there is nothing to answer")]
    WithdrawnFinding {
        node: NodeId,
        id: yunta_core::FindingId,
        reason: String,
    },
    #[error("a declaration says what it is — `{field}` is empty")]
    EmptyDeparture { field: &'static str },
    #[error("the plan holds no {from} — it holds {known}")]
    NotInThePlan { from: String, known: String },
    #[error(
        "`{cmd}` is a command the run measured green before any work, which holds every task \
         — it is not the plan's to depart from: keep what passed passing"
    )]
    SuiteDeparture { cmd: String },
    #[error(
        "this session's node is not in a `coordination: blackboard` group — the blackboard is \
         never mounted outside one"
    )]
    NotInBlackboardGroup,
    #[error(
        "this session has no scope an answer could widen — it works no task, and its node \
         either declares no scope or may not be granted more on this run"
    )]
    NoScopeToWiden,
    #[error("this tool judges a node's own scope, and this session's node declares none")]
    NoNodeScope,
    #[error("this attempt's start is not on the log, so there is no tree to judge its work from")]
    NoStartingTree,
    #[error(
        "a scope expansion request under `{pending}` is already waiting for this attempt's end, \
         and only requests under the same criterion join it"
    )]
    CriterionConflict { pending: String },
    #[error(
        "`{criterion}` does not fail on the work (exit {exit}), so it cannot be declared \
         unpassable — only a criterion that is red on the work can be"
    )]
    NotRedOnTheWork { criterion: String, exit: i32 },
    #[error("the criterion could not be run to see whether it is red on the work")]
    Criterion {
        #[source]
        source: crate::scope_expansion::ScopeExpansionError,
    },
    #[error("the scope decision could not be recorded on the run's log")]
    Record {
        #[source]
        source: crate::run::RunError,
    },
    #[error(
        "`{tool}` answers about a task, and this session works none — only a loop's task \
         sessions are served it"
    )]
    NotATaskSession { tool: String },
    #[error("the criteria could not be run where the engine runs them: {detail}")]
    Handover { detail: String },
    #[error("the run's plan could not be read")]
    Plan {
        #[source]
        source: crate::artifacts::HeldError,
    },
    #[error("the task's work could not be checked")]
    Check {
        #[source]
        source: crate::task_cycle::TaskCycleError,
    },
    #[error(
        "this check stopped: the checkout changed, and the check asked after it judges what \
         the checkout holds now — its answer is the one that counts"
    )]
    Superseded,
    /// The run stopped while the call was being answered. Nothing about the
    /// call was refused, so it gets no refusal text and no refusal is
    /// recorded for it.
    #[error("cancelled by user")]
    Cancelled,
    #[error("the run's log cannot be reached")]
    Storage {
        #[source]
        source: yunta_storage::StorageError,
    },
    #[error("the answer cannot be rendered as JSON")]
    Render {
        #[source]
        source: serde_json::Error,
    },
    /// A request file that could not be read, parsed or written: its own sentence names the path.
    #[error(transparent)]
    Request {
        source: crate::scope_expansion::RequestError,
    },
    #[error("node `{node}` declares no artifacts, so there is nothing to check")]
    NoArtifacts { node: NodeId },
    #[error("`{name}` is not an artifact this node declares; it declares {declared}")]
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

    /// Why this call was refused, in the closed word the log keeps. A call
    /// the run stopped was refused by nothing, so it has no word.
    pub(super) fn refusal(&self) -> Option<yunta_core::events::RunToolRefusal> {
        use yunta_core::events::RunToolRefusal as R;
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
