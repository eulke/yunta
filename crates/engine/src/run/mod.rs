//! Run creation and execution.
//!
//! `create_run` freezes the anatomy on disk (run.dir, `manifest.yaml`,
//! `run_created`); `execute_run` drives the run forward and is also
//! `yunta resume` — it replays the log, asks [`schedule::next_action`]
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

mod bash_exec;
mod budget;
mod check_exec;
mod context_resolve;
mod create;
mod ctx;
mod distill;
mod escalation;
mod exec;
mod executor_exec;
mod gate_exec;
mod hooks_exec;
mod loop_exec;
mod node_close;
mod node_exec;
mod parallel_exec;
mod promote;
mod prompt_exec;
mod questions_exec;
mod runner_resolve;
mod schedule;
mod step;
mod steps;
mod workflow_exec;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use thiserror::Error;
use tokio_util::sync::CancellationToken;
use yunta_adapters::{Adapter, Forge, ForgeError};
use yunta_core::{AdapterError, AdapterId, Clock, IdSource, Manifest, ModeName, NodeId, RunId};
use yunta_storage::{AsyncStorage, StorageError};

use crate::human_interaction::HumanInteraction;
use crate::replay::RunState;
use crate::scope::ScopeCheckError;
use crate::task_cycle::TaskCycleError;
pub use budget::session_token_budget;
pub use create::{create_run, BirthArtifact, CreateRunParams};
pub(crate) use ctx::RunCtx;
pub use escalation::{current_escalation, resolve_gate, ResolveGateError};
pub(crate) use exec::execute_run_at_depth;
pub use exec::record_pause_after_crash;
pub(in crate::run) use exec::{find_node, pause, record_pause};
pub use promote::{create_promotion_successor, Predecessor, PromotionSuccessor, RunRoots};

/// A frozen `manifest.yaml` that cannot be read back.
#[derive(Debug, Error)]
pub enum ManifestReadError {
    #[error("cannot read `{path}`")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("`{path}` is not a manifest")]
    Parse {
        path: PathBuf,
        #[source]
        source: yunta_core::yaml::YamlError,
    },
}

/// Reads a run's frozen manifest back from its `manifest.yaml`.
pub fn read_manifest(path: &Path) -> Result<Manifest, ManifestReadError> {
    let text = std::fs::read_to_string(path).map_err(|source| ManifestReadError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    yunta_core::yaml::parse(&text).map_err(|source| ManifestReadError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

#[derive(Debug, Error)]
pub enum RunError {
    #[error(transparent)]
    Storage(#[from] StorageError),

    #[error("run `{run_id}` has no events — create it before executing it")]
    UnknownRun { run_id: RunId },

    #[error("run is broken: {diagnostic}")]
    Broken { diagnostic: String },

    /// `create_run`'s own guard — `check` validates every
    /// mode's *internal* coherence, but never sees which one a run
    /// actually asks for, so this is where an unknown `--mode` name is
    /// caught, before anything is written.
    #[error("workflow `{workflow}` declares no mode `{mode}` — declared modes: {declared}")]
    UnknownMode {
        workflow: String,
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

    #[error("git failed to {context}: {detail}")]
    Git { context: String, detail: String },

    #[error("failed to serialize the manifest for `{path}`: {detail}")]
    ManifestWrite { path: PathBuf, detail: String },

    #[error("task ledger `{path}` no longer parses: {detail}")]
    CorruptLedger { path: PathBuf, detail: String },

    #[error("adapter failed to spawn a session for node `{node}`")]
    Spawn {
        node: NodeId,
        #[source]
        source: AdapterError,
    },

    #[error(transparent)]
    Process(#[from] crate::process::SpawnError),

    #[error(transparent)]
    TaskCycle(#[from] TaskCycleError),

    #[error(transparent)]
    ScopeCheck(#[from] ScopeCheckError),

    #[error(transparent)]
    EventsExport(#[from] crate::events_export::EventsExportError),

    #[error(transparent)]
    Worktree(#[from] crate::worktree::WorktreeError),
}

/// How `execute_run` came back: everything done, waiting on a human, or
/// promoted onward.
#[derive(Debug, Clone, PartialEq)]
pub enum RunTerminal {
    Finished,
    Paused {
        reason: String,
    },
    /// A node failed and the run's `defaults.on_failure` closed the run
    /// as failed rather than pausing it (`abort` at the first failure,
    /// `continue` once the rest of the graph has run). `run_finished`
    /// (`terminal_state: Failed`) is on the log; `reason` names the
    /// causing node, the same shape a `Paused` reason takes.
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
    pub state: RunState,
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
    pub max_task_retries: u32,
    pub human_interaction: &'a dyn HumanInteraction,
    pub forge: Option<&'a dyn Forge>,
    pub cancel: Option<&'a CancellationToken>,
    /// `yunta run --adapter <id>`: every role resolves to its candidate
    /// on this adapter, or fails naming what it tried. Invocation-scoped,
    /// never frozen: the log's `runner_resolved` records the discards.
    pub adapter_override: Option<&'a AdapterId>,
    /// The ambient environment this run executes in, captured once at the
    /// caller's boundary rather than read from the process below it: the
    /// user state root the user knowledge layer resolves against, and the
    /// variables layered onto every subprocess. `None` means no user layer
    /// and no injected variables — the shape most tests want.
    pub ambient: Option<&'a yunta_core::Env>,
}

/// Drives a run until it finishes or pauses. Serving `yunta run` and
/// `yunta resume` with the same function is the point: the log decides
/// what remains, never in-process state.
pub async fn execute_run(env: RunEnv<'_>) -> Result<RunReport, RunError> {
    execute_run_at_depth(env, 0).await
}
