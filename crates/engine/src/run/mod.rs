//! Run creation and execution.
//!
//! `create_run` freezes the anatomy on disk (run.dir, `manifest.yaml`,
//! `run_created`, and the baseline the run is measured against);
//! `execute_run` drives the run forward and is also
//! `yunta resume` — it replays the log, asks [`schedule::decide`]
//! what's next, and executes until the answer is terminal. Crash,
//! restart and Ctrl-C are the same case: whatever the log says happened,
//! happened; everything else re-runs (`restart_node`).
//!
//! **`events.jsonl` export**: the run exports its whole log on every
//! terminal `RunReport` the schedule loop recognizes — `Finish` and
//! `Pause` alike, via [`RunCtx::export_events_jsonl`] — unconditionally,
//! independent of whether the workflow declares `on_finish:` at all. The
//! `ScheduleStep::Broken` path exports too, best-effort before its
//! `Err` — a corrupt log is exactly the one a forensic reader most wants
//! on disk. `on_finish.distill` is a deterministic transform — see
//! `distill.rs`; the close sequence is distill → `run_finished` →
//! export → cleanup.

mod aside;
mod asking;
pub mod baseline;
mod bash_exec;
mod budget;
pub(crate) mod capability;
mod check_exec;
mod cleanup;
mod close;
mod console_gate;
mod context_resolve;
mod continuation;
mod create;
mod ctx;
pub(crate) mod denied;
mod distill;
mod escalation;
mod exec;
mod executor_exec;
mod finding_proofs;
mod flawed;
mod gate_exec;
pub(crate) mod gate_findings;
mod hooks_exec;
mod internal_gate;
mod invocation;
mod loop_exec;
mod node_artifacts;
mod node_close;
pub(crate) mod node_exec;
mod node_scope;
mod node_start;
mod owed_scope;
mod parallel_exec;
mod promote;
mod prompt_exec;
mod pull_request_exec;
mod questions_exec;
pub(crate) mod runner_resolve;
pub mod schedule;
pub(crate) mod session_plan;
mod session_policy;
mod shared_tree;
mod step;
mod steps;
mod wake;
mod workflow_exec;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use thiserror::Error;
use tokio_util::sync::CancellationToken;
use yunta_core::events::PauseReason;
use yunta_core::port::{Adapter, Forge, ForgeError};
use yunta_core::{
    AdapterError, AdapterId, Clock, IdSource, Manifest, ModeName, NodeId, RunId, WorkflowName,
};
use yunta_storage::{AsyncStorage, StorageError};

use crate::human_interaction::HumanInteraction;
use crate::observer::RunObserver;
use crate::replay::RunState;
use crate::scope::ScopeCheckError;
use crate::task_cycle::TaskCycleError;
pub use baseline::{BirthBaseline, Owed as OwedMeasurement};
pub use budget::session_token_budget;
pub use close::{close_run, CloseRunError};
pub use create::{create_run, run_redactor, BirthArtifact, BirthOrigin, CreateRunParams};
pub(crate) use ctx::RunCtx;
pub use escalation::{awaits_decision, current_escalation, resolve_gate, ResolveGateError};
pub(crate) use exec::execute_run_at_depth;
pub use exec::record_pause_after_crash;
pub(in crate::run) use exec::{find_node, pause};
pub use promote::{
    create_promotion_successor, CallerInfra, Predecessor, PromotionSuccessor, RunRoots,
};

/// A frozen `manifest.yaml` that cannot be read back.
#[derive(Debug, Error)]
pub enum ManifestReadError {
    #[error("cannot read `{path}`")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot read the manifest at `{path}`")]
    Parse {
        path: PathBuf,
        #[source]
        source: yunta_core::persisted::PersistedError,
    },
}

/// Reads a run's frozen manifest back from its `manifest.yaml`, keeping
/// whatever a newer binary wrote beside what this one knows.
///
/// A manifest stamped with a schema this binary does not read is
/// refused naming both versions — a run interpreted under a shape its
/// own creator did not write is a run whose history means something
/// else. Everything below that reads, and what this binary did not
/// understand comes back on the document for a caller to report.
pub async fn read_manifest(
    path: &Path,
) -> Result<yunta_core::persisted::PersistedDoc<Manifest>, ManifestReadError> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|source| ManifestReadError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    yunta_core::persisted::PersistedDoc::read(&bytes).map_err(|source| ManifestReadError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

#[derive(Debug, Error)]
pub enum RunError {
    #[error(transparent)]
    Storage(#[from] StorageError),

    /// A declared secret too short to keep out of the log. The run is
    /// refused before it takes a checkout or writes anything, and the
    /// error names the variable.
    #[error(transparent)]
    Secret(#[from] yunta_core::ShortSecret),

    #[error("run `{run_id}` has no events — create it before executing it")]
    UnknownRun { run_id: RunId },

    #[error("run is broken: {diagnostic}")]
    Broken { diagnostic: String },

    /// An escalation the engine built could not be put together: its claim
    /// repeats the record it is audited against. The error keeps the fact
    /// that repeats as data, and the surface that shows the error draws it.
    #[error("run is broken: {context}: {source}")]
    Escalation {
        context: String,
        source: yunta_core::events::EscalationError,
    },

    /// The invocation's token fired while the run was being born, or
    /// while it was preparing the tree a run works on. A birth has no
    /// log of its own to record a pause on, so it says so to its
    /// caller — a `kind: workflow` node turns it into the parent's
    /// `run_paused { cancelled by user }`, and a CLI into its own exit.
    #[error("cancelled by user")]
    Cancelled,

    /// The disk has no room for what the run is about to build: the run
    /// stops and says how to make some, rather than filling the disk.
    #[error(transparent)]
    NoRoom(crate::worktree::WorktreeError),

    /// A `HumanInteraction` surface returned an answer the escalation it
    /// was shown does not accept — an option it never offered, or one
    /// that asks for words given none: a decision nobody made, refused
    /// rather than recorded as the gate's outcome.
    #[error("the human surface's answer was refused: {refused}, for: {summary}")]
    RefusedAnswer {
        refused: yunta_core::events::Refusal,
        summary: String,
    },

    /// `create_run`'s own guard — `check` validates every
    /// mode's *internal* coherence, but never sees which one a run
    /// actually asks for, so this is where an unknown `--mode` name is
    /// caught, before anything is written.
    #[error("workflow `{workflow}` declares no mode `{mode}` — declared modes: {declared}")]
    UnknownMode {
        workflow: WorkflowName,
        mode: ModeName,
        declared: String,
    },

    /// `create_run`'s other guard: a run is born once, whole. A
    /// directory already at the run's path belongs to another run, or
    /// to a birth that stopped before its `run_created` — either way
    /// the id is not free.
    #[error(
        "run directory `{path}` already exists — a run id names one birth; a directory with \
         no run in the log is left from a birth that stopped early and is safe to delete"
    )]
    RunDirExists { path: PathBuf },

    /// An identifier the engine composed breaks its own rule — an
    /// invariant of the composition, reported rather than assumed.
    #[error("an identifier the engine composed is not valid: {source}")]
    Id {
        #[from]
        source: yunta_core::InvalidId,
    },

    #[error("failed to {context}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to {context}")]
    Forge {
        context: String,
        #[source]
        source: ForgeError,
    },

    #[error(transparent)]
    Git(crate::git::GitError),

    #[error("failed to serialize the manifest for `{path}`: {detail}")]
    ManifestWrite { path: PathBuf, detail: String },

    /// An interpreted artifact the run needs and cannot read. The
    /// report names the file and every problem it has, so the run states
    /// what is wrong with the document rather than what a deserializer
    /// made of it.
    #[error("{0}")]
    UnreadableArtifact(#[from] yunta_core::diagnostic::Report),

    #[error("adapter failed to spawn a session for node `{node}`")]
    Spawn {
        node: NodeId,
        #[source]
        source: AdapterError,
    },

    #[error(transparent)]
    Process(#[from] crate::process::SpawnError),

    #[error(transparent)]
    TaskCycle(TaskCycleError),

    #[error(transparent)]
    ScopeCheck(ScopeCheckError),

    #[error(transparent)]
    EventsExport(#[from] crate::events_export::EventsExportError),

    #[error(transparent)]
    Worktree(crate::worktree::WorktreeError),

    /// An artifact the run acquired that could not become a fact of the
    /// run: its bytes, its acceptance or its view did not land.
    #[error(transparent)]
    Artifact(#[from] crate::artifacts::AcceptError),

    /// An artifact the run's log holds whose bytes the run cannot hand
    /// over — the object is gone, or no longer hashes to what was
    /// accepted.
    #[error(transparent)]
    Object(#[from] crate::artifacts::ObjectError),
}

/// A git the caller's token stopped is not a failure of anything: it is
/// the invocation being cancelled, and it says so wherever it surfaces.
/// One conversion, so no call site has to remember to ask. A lock the git
/// waited on is the worktree's, unless the token stopped that wait.
impl From<crate::git::GitError> for RunError {
    fn from(git: crate::git::GitError) -> Self {
        if git.cancelled() {
            return RunError::Cancelled;
        }
        match git.cause {
            crate::git::Cause::Lock(lock) => RunError::from(*lock),
            _ => RunError::Git(git),
        }
    }
}

/// A lock wait the token stopped is the run's cancellation; any other lock
/// the run could not take is a worktree failure, which names its holder.
impl From<crate::lock::LockError> for RunError {
    fn from(lock: crate::lock::LockError) -> Self {
        match lock {
            crate::lock::LockError::Cancelled { .. } => RunError::Cancelled,
            lock => RunError::Worktree(crate::worktree::WorktreeError::Lock(lock)),
        }
    }
}

impl From<crate::worktree::WorktreeError> for RunError {
    fn from(worktree: crate::worktree::WorktreeError) -> Self {
        match (worktree.cancelled(), &worktree) {
            (true, _) => RunError::Cancelled,
            (false, crate::worktree::WorktreeError::NoRoom { .. }) => RunError::NoRoom(worktree),
            (false, _) => RunError::Worktree(worktree),
        }
    }
}

/// A scope check is cancelled when the git or the checkout it read was
/// stopped; any other failure of the check stays a scope failure.
impl From<ScopeCheckError> for RunError {
    fn from(error: ScopeCheckError) -> Self {
        match error {
            ScopeCheckError::Git(git) => RunError::from(git),
            error if error.cancelled() => RunError::Cancelled,
            error => RunError::ScopeCheck(error),
        }
    }
}

/// A task cycle is cancelled when a git or a checkout it asked was stopped,
/// whichever variant carries that error; any other failure stays the task's.
impl From<TaskCycleError> for RunError {
    fn from(error: TaskCycleError) -> Self {
        match error {
            TaskCycleError::Git(git) => RunError::from(git),
            error if cancelled_within(&error) => RunError::Cancelled,
            error => RunError::TaskCycle(error),
        }
    }
}

/// Whether the token stopped a git or a checkout that a task cycle's error
/// carries.
fn cancelled_within(error: &TaskCycleError) -> bool {
    match error {
        TaskCycleError::ScopeCheck(scope) => scope.cancelled(),
        TaskCycleError::ScopeExpansion { source, .. } => source.cancelled(),
        TaskCycleError::Carry { source, .. }
        | TaskCycleError::Spec { source, .. }
        | TaskCycleError::TreeContent(source) => source.cancelled(),
        _ => false,
    }
}

/// How `execute_run` came back: everything done, waiting on a human, or
/// promoted onward.
#[derive(Debug, Clone, PartialEq)]
pub enum RunTerminal {
    Finished,
    /// The pause the run stopped on, as its log records it.
    Paused {
        reason: PauseReason,
    },
    /// A node failed and the run's `defaults.on_failure` closed the run
    /// as failed rather than pausing it (`abort` at the first failure,
    /// `continue` once the rest of the graph has run). `run_finished`
    /// (`terminal_state: Failed`) is on the log; `reason` names the
    /// causing node, in the words a `NodeFailed` pause gives it.
    Failed {
        reason: String,
    },
    /// This run's own gate accepted promotion — `run_finished`
    /// (`terminal_state: Promoted`) is already on *this* log, closing it
    /// for good (nothing reopens a finished run, the same guarantee the
    /// SHA-drift recheck leans on). Creating and starting the
    /// successor — a fresh run, its own `run_id`, in `suggested_mode`,
    /// `promoted_from` this one — is the caller's job: it needs the
    /// original repo checkout (`cwd`) to prepare a worktree, which
    /// `execute_run` was never given (only ever an *existing* worktree).
    Promoted {
        suggested_mode: ModeName,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunReport {
    pub terminal: RunTerminal,
    /// The run's state as this invocation left its log: its close or its
    /// pause included.
    pub state: RunState,
}

impl RunReport {
    /// Where the run works as it stopped: the checkout its log names last
    /// — where it was born, woke or moved — or `given`, the tree it was
    /// handed, for a run whose log names none.
    pub fn tree(&self, given: &Path) -> std::path::PathBuf {
        self.state.run.checkout().unwrap_or(given).to_path_buf()
    }
}

/// Everything [`execute_run`] needs from its caller, grouped: the run's
/// own identity/manifest/paths, the environment it executes against
/// (adapters, storage, clock, ids), and the cross-cutting surfaces
/// (human_interaction, forge, cancel) every deep execution path can
/// reach through `RunCtx` once this is unpacked into one.
pub struct RunEnv<'a> {
    pub run_id: &'a RunId,
    pub manifest: &'a Manifest,
    pub run_dir: &'a Path,
    pub worktree: &'a Path,
    pub adapters: &'a HashMap<AdapterId, Arc<dyn Adapter>>,
    pub storage: &'a AsyncStorage,
    pub clock: Arc<dyn Clock>,
    /// Mints the ids of the runs this one gives birth to — its
    /// children and its promotion successor.
    pub ids: &'a dyn IdSource,
    pub human_interaction: &'a dyn HumanInteraction,
    pub forge: Option<&'a dyn Forge>,
    /// What stops this run: the invocation's own token, which the shell
    /// that started it owns. Every subprocess the run spawns is born
    /// under it, so a Ctrl-C reaches the whole tree.
    pub cancel: &'a CancellationToken,
    /// The ambient environment this run executes in, captured once at the
    /// caller's boundary rather than read from the process below it: the
    /// user state root the user knowledge layer resolves against, and the
    /// variables layered onto every subprocess. `None` means no user layer
    /// and no injected variables — the shape most tests want.
    pub ambient: Option<&'a yunta_core::Env>,
    /// The hook a CLI runs to ask the judge about one write: this
    /// binary's own path, resolved once by the shell that started the
    /// run. `None` in a harness with no binary to run — an adapter
    /// whose fence needs it then fails the session rather than opening
    /// one that writes freely.
    pub rail_command: Option<yunta_core::rail::RailCommand>,
    /// Where the values of the variables `secrets:` names come from. The
    /// config names them; only the shell that started the run may read
    /// their values, so the engine asks here and never the process. A run
    /// that declares no secret passes a source that binds nothing.
    pub secrets: Arc<dyn yunta_core::SecretSource>,
    /// Where every event this invocation appends is mirrored as it is
    /// written — this run's, its `kind: workflow` children's and its
    /// promotion successors' alike. Display only: it derives nothing and
    /// decides nothing.
    ///
    /// An owned [`Arc`] rather than a borrow, unlike `human_interaction`
    /// and `forge` beside it, because [`RunToolsHost`] outlives every
    /// borrow of the invocation and needs its own handle; `clock` in this
    /// same struct is an `Arc<dyn Clock>` for exactly that reason. The
    /// `Option` is load-bearing too, not nullability sugar: it is what
    /// lets the append helper skip the payload clone entirely when
    /// nobody is watching.
    ///
    /// [`RunToolsHost`]: crate::RunToolsHost
    pub observer: Option<Arc<dyn RunObserver>>,
}

/// The failure a command that exited `code` becomes: what it printed
/// kept, redacted, in the run's objects, and its last lines on the
/// failure itself, stdout included — a compiler or a test runner says
/// why on stdout as often as on stderr.
///
/// `what` names the command in the error a failed write reports.
async fn command_exited(
    ctx: &RunCtx<'_>,
    outcome: &crate::process::Outcome,
    code: i32,
    origin: Option<yunta_core::events::CommandOrigin>,
    what: &str,
) -> Result<yunta_core::events::Failure, RunError> {
    let printed = crate::process::CommandOutput::of(outcome);
    // A command that printed nothing leaves nothing to keep, and no
    // pointer to an empty file a reader would open to find nothing.
    let output = match printed.bytes().is_empty() {
        true => None,
        false => Some(
            crate::artifacts::store::ObjectStore::at(ctx.run_dir)
                .put_redacted(printed.bytes(), &ctx.redactor)
                .await
                .map_err(|source| RunError::Io {
                    context: format!("keep what {what} printed"),
                    source,
                })?,
        ),
    };
    Ok(yunta_core::events::Failure::exited(
        yunta_core::events::CommandExit {
            origin,
            code,
            tail: printed.tail(),
            output,
        },
    ))
}

/// Drives a run until it finishes or pauses. Serving `yunta run` and
/// `yunta resume` with the same function is the point: the log decides
/// what remains, never in-process state.
pub async fn execute_run(env: RunEnv<'_>) -> Result<RunReport, RunError> {
    execute_run_at_depth(env, 0).await
}
