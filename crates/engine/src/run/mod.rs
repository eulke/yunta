//! Run creation and execution.
//!
//! `create_run` freezes the anatomy on disk (run.dir, `manifest.yaml`,
//! `run_created`); `execute_run` drives the run forward and is also
//! `yunta resume` — it replays the log, asks [`schedule::next_action`]
//! what's next, and executes until the answer is terminal. Crash,
//! restart and Ctrl-C are the same case: whatever the log says happened,
//! happened; everything else re-runs (`restart_node`).
//!
//! **`events.jsonl` export**: the Contrato says only "at close" without
//! enumerating which terminal states count — a real gap, documented
//! rather than guessed silently. This recorte exports on every terminal
//! `RunReport` the schedule loop already recognizes — `Finish` and
//! `Pause` alike, via [`RunCtx::export_events_jsonl`] — unconditionally,
//! independent of whether the workflow declares `on_finish:` at all (the
//! Contrato's phrasing reads as two actions conjoined at close, not one
//! gated on the other). The `ScheduleStep::Broken` path exports too,
//! best-effort before its `Err` — a corrupt log is exactly the one a
//! forensic reader most wants on disk. `on_finish.distill` is a
//! deterministic transform — see `distill.rs`; the close sequence is
//! distill → `run_finished` → export → cleanup.

mod budget;
mod check_exec;
mod context_resolve;
mod distill;
mod escalation;
mod executor_exec;
mod gate_exec;
mod loop_exec;
mod node_exec;
mod promote;
mod questions_exec;
mod schedule;
mod step;
mod steps;
mod workflow_exec;

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use thiserror::Error;
use tokio_util::sync::CancellationToken;
use yunta_adapters::{Adapter, Forge, ForgeError};
use yunta_core::events::{
    EventDraft, EventPayload, Finding, FindingPostedPayload, FindingSeverity, RunCreatedPayload,
    RunPausedPayload, RunResumedPayload, StoredEvent,
};
use yunta_core::{
    AdapterError, AdapterId, Clock, FindingId, IdSource, Manifest, ModeName, NodeId, Pid, RunId,
    Seq,
};
use yunta_storage::{AsyncStorage, StorageError};

use crate::human_interaction::HumanInteraction;
use crate::replay::{derive, RunState};
use crate::scope::ScopeCheckError;
use crate::task_cycle::{Memo, TaskCycleError};
pub use budget::session_token_budget;
pub use escalation::{current_escalation, resolve_gate, ResolveGateError};
pub use promote::{create_promotion_successor, Predecessor, PromotionSuccessor, RunRoots};
use schedule::ScheduleStep;

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

/// Everything node execution needs, borrowed once. Also owns the small
/// emit helper so every event gets its timestamp from the same injected
/// clock and its seq from storage.
pub(crate) struct RunCtx<'a> {
    pub run_id: &'a RunId,
    pub manifest: &'a Manifest,
    pub run_dir: &'a Path,
    pub worktree: &'a Path,
    pub adapters: &'a HashMap<AdapterId, Arc<dyn Adapter>>,
    pub storage: &'a AsyncStorage,
    pub clock: Arc<dyn Clock>,
    pub ids: &'a dyn IdSource,
    pub max_task_retries: u32,
    /// Criteria memoization — one cache per `execute_run`
    /// call, never persisted: a resume simply starts cold, which is safe
    /// (over-verifying) rather than risking a stale cross-run hit.
    pub memo: Memo,
    /// The one surface every escalation goes through:
    /// exhausted re-routes and scope-expansion `ask` alike — on the ctx
    /// so the deep execution paths (loop_exec) reach it without threading
    /// one more parameter through every layer.
    pub human_interaction: &'a dyn HumanInteraction,
    /// A human's `continue` past the token cap, held for
    /// this invocation only — in memory, never derived from the log, so
    /// every resume asks again before spending new money. Atomic because
    /// concurrent batch members read it while the scheduler loop writes.
    pub budget_lifted: std::sync::atomic::AtomicBool,
    /// `run.dir/scratch/engine.json`, so a separate process can
    /// find this run's live process tree. `None` when the file could not
    /// be written — the run proceeds, degraded loudly (an external
    /// cancellation loses its map to this run's processes; internal
    /// paths never needed it).
    pub process_registry: Option<crate::process_registry::ProcessRegistry>,
    /// The invocation's root cancellation (Ctrl-C, `yunta
    /// cancel`). Execution paths consult it to tell a user cancellation
    /// (leave the node orphaned — resume re-treats it per
    /// `on_interrupt`) apart from a `join: any` sibling race
    /// (record the loss as failed so the group can close).
    pub root_cancel: CancellationToken,
    pub(crate) adapter_override: Option<&'a AdapterId>,
    /// The forge this invocation was given — on the ctx so a
    /// `kind: workflow` node can hand it down to its child run (whose
    /// own gates are as real as the parent's).
    pub forge: Option<&'a dyn Forge>,
    /// How many `kind: workflow` levels above this run (0 = the
    /// root invocation) — compared against
    /// `limits.max_workflow_depth` before a child is born.
    pub depth: u32,
    /// The per-run MCP host every session listener of this run
    /// shares — it holds its own handle on the log, since listeners
    /// outlive any borrow of ours.
    pub run_tools_host: Arc<crate::run_tools::RunToolsHost>,
}

impl RunCtx<'_> {
    /// The supervision every subprocess of this run gets: its registry,
    /// and `cancel` — a node's own token, or the run's root token.
    pub(crate) fn supervision<'a>(
        &'a self,
        cancel: &'a CancellationToken,
    ) -> crate::process::Supervision<'a> {
        crate::process::Supervision {
            registry: self.process_registry.as_ref(),
            cancel: Some(cancel),
        }
    }

    /// Appends one event and returns the seq storage assigned to it. The
    /// timestamp is read from the run's clock here, before the hop to
    /// the blocking thread that writes it.
    pub(crate) async fn emit(
        &self,
        node_id: Option<&NodeId>,
        payload: EventPayload,
    ) -> Result<Seq, RunError> {
        let draft = EventDraft {
            run_id: self.run_id.clone(),
            node_id: node_id.cloned(),
            payload,
        };
        let at = self.clock.now();
        Ok(self.storage.append(draft, at).await?)
    }

    pub(crate) async fn load_events(&self) -> Result<Vec<StoredEvent>, RunError> {
        Ok(self.storage.events_for_run(self.run_id.clone()).await?)
    }

    /// Exports the run's whole log to `run.dir/events.jsonl` — called
    /// at every close this recorte recognizes (`Finish` and `Pause`; see
    /// this module's own doc comment on the trigger decision). Re-exports
    /// in full each time, same "regenerate from the log" principle
    /// `progress.md` already follows — a run that pauses, resumes, and
    /// later finishes just gets the file rewritten with the fuller log,
    /// never appended to.
    pub(crate) async fn export_events_jsonl(&self) -> Result<(), RunError> {
        let events = self.load_events().await?;
        let jsonl = crate::events_export::render_events_jsonl(&events)?;
        tokio::fs::write(self.run_dir.join("events.jsonl"), jsonl)
            .await
            .map_err(|source| RunError::Io {
                context: "write events.jsonl".to_string(),
                source,
            })
    }

    /// Records an engine-authored degradation as a `finding_posted`:
    /// something the engine itself could not do (a git step, a cleanup,
    /// its own bookkeeping file), on the run's log in the same
    /// vocabulary an agent's findings use — never a `tracing` warning
    /// that leaves the log silent. `node` is the node it concerns, or
    /// `None` for a run-level degradation.
    pub(crate) async fn engine_finding(
        &self,
        node: Option<&NodeId>,
        id: &str,
        severity: FindingSeverity,
        title: String,
        location: String,
        detail: String,
    ) -> Result<(), RunError> {
        self.emit(
            node,
            EventPayload::FindingPosted(FindingPostedPayload {
                finding: Finding {
                    id: FindingId::try_from(id.to_string())?,
                    severity,
                    title,
                    location,
                    detail,
                    proposed_criterion: None,
                },
            }),
        )
        .await?;
        Ok(())
    }

    /// The [`Budget`] for one agent session: an equal
    /// share of the remaining run cap
    /// ([`budget::session_token_budget`]'s policy). Unlimited — exactly
    /// the pre-limits behavior — when no cap is declared, or when a
    /// human already answered `continue` this invocation (their lift
    /// must not resurface as a zero-token session budget). `timeout`
    /// stays `None`: `defaults.timeout_minutes` is resolved separately,
    /// outside this function's scope.
    pub(crate) async fn session_budget(&self) -> Result<yunta_adapters::Budget, RunError> {
        // `defaults.timeout_minutes` applies on every path —
        // the wall clock is orthogonal to the token cap and to a
        // human's `continue`.
        let timeout = self.manifest.config.resolved_session_timeout();
        if self
            .budget_lifted
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Ok(yunta_adapters::Budget {
                timeout,
                ..Default::default()
            });
        }
        let Some(cap) = self
            .manifest
            .config
            .limits
            .as_ref()
            .and_then(|limits| limits.max_tokens_per_run)
        else {
            return Ok(yunta_adapters::Budget {
                timeout,
                ..Default::default()
            });
        };
        let state = derive(&self.load_events().await?);
        let non_terminal = self
            .manifest
            .workflow
            .iter_nodes()
            .filter(|node| {
                !matches!(
                    state.nodes.get(&node.id),
                    Some(crate::replay::NodeState::Finished { .. })
                )
            })
            .count();
        Ok(yunta_adapters::Budget {
            max_tokens: Some(budget::session_token_budget(
                cap,
                state.total_tokens.total(),
                non_terminal,
            )),
            timeout,
            ..Default::default()
        })
    }

    /// The opaque `adapter_settings` the config declares for `adapter`
    /// — passed through to the request untouched.
    pub(crate) fn adapter_settings(
        &self,
        adapter: &AdapterId,
    ) -> serde_json::Map<String, serde_json::Value> {
        self.manifest
            .config
            .adapters
            .as_ref()
            .and_then(|adapters| adapters.get(adapter))
            .and_then(|settings| settings.adapter_settings.clone())
            .unwrap_or_default()
    }
}

/// `RunCtx` is the one real [`SessionObserver`] — audit events
/// land in the run's own log as they arrive, so a concurrent `status`
/// sees the live session. A failed append warns instead of aborting the
/// stream: the run's next mandatory event hits the same storage and
/// fails the run properly if it's really down.
#[async_trait::async_trait]
impl crate::task_cycle::SessionObserver for RunCtx<'_> {
    async fn emit_session_event(
        &self,
        node_id: &NodeId,
        payload: EventPayload,
    ) -> Result<(), StorageError> {
        // A session audit event that cannot be appended is not
        // dropped: it would silently thin the trail `status` and replay
        // read (a lost `agent_session_opened` even changes what a resume
        // finds), so the storage cause travels back to the dispatch and
        // fails the node — the same storage the run's next mandatory
        // event would hit anyway, surfaced now instead of masked.
        let draft = EventDraft {
            run_id: self.run_id.clone(),
            node_id: Some(node_id.clone()),
            payload,
        };
        let at = self.clock.now();
        self.storage.append(draft, at).await.map(|_| ())
    }

    fn process_registry(&self) -> Option<&crate::process_registry::ProcessRegistry> {
        self.process_registry.as_ref()
    }
}

/// How `execute_run` came back: everything done, waiting on a human, or
/// promoted onward.
#[derive(Debug, Clone, PartialEq)]
pub enum RunTerminal {
    Finished,
    Paused {
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

/// A file a run carries from birth, under its `artifacts/`: what a
/// parent mounts into a child, or a successor inherits from its
/// predecessor. `name` is relative to `artifacts/` and may nest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BirthArtifact {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// What [`create_run`] freezes: the run's identity and its
/// declared birth facts, bundled — `storage`/`clock` stay separate
/// arguments because they are the caller's *infrastructure*, not this
/// run's data.
pub struct CreateRunParams<'a> {
    pub run_id: &'a RunId,
    pub manifest: &'a Manifest,
    pub runs_root: &'a Path,
    /// Frozen into `run_created.mode` and never re-resolved.
    pub mode: &'a ModeName,
    /// The predecessor this run inherits from, if any.
    pub promoted_from: Option<&'a RunId>,
    /// Files under `artifacts/` from birth, written before
    /// `run_created` — a run that exists in the log has them.
    pub artifacts: &'a [BirthArtifact],
}

/// Creates the run's anatomy: run.dir with `artifacts/` and
/// `scratch/`, the frozen `manifest.yaml`, the birth artifacts, and
/// the `run_created` event — in that order, so the log names a run
/// only once its directory is complete. Returns the run directory.
///
/// The run directory must not exist: a run is born once, and an id is
/// never reused ([`RunError::RunDirExists`]).
///
/// `mode` is frozen into `run_created.mode` right here and
/// never re-resolved again — a resume reads the same name back off the
/// log. `"default"` — the caller's choice when nothing else applies,
/// same sentinel `events::run_mode` falls back to for a log with no
/// mode recorded — always passes: a workflow declaring no `modes:` at
/// all has nothing to validate a name against, and every node stays
/// schedulable, exactly the behavior before modes existed. A workflow
/// that *does* declare `modes:` rejects any other unrecognized name.
pub async fn create_run(
    params: CreateRunParams<'_>,
    storage: &AsyncStorage,
    clock: &dyn Clock,
) -> Result<PathBuf, RunError> {
    let CreateRunParams {
        run_id,
        manifest,
        runs_root,
        mode,
        promoted_from,
        artifacts,
    } = params;
    if *mode != ModeName::default() {
        match &manifest.workflow.modes {
            Some(modes) if !modes.contains_key(mode) => {
                return Err(RunError::UnknownMode {
                    workflow: manifest.workflow.name.clone(),
                    mode: mode.clone(),
                    declared: modes
                        .keys()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", "),
                });
            }
            Some(_) => {}
            None => {
                return Err(RunError::UnknownMode {
                    workflow: manifest.workflow.name.clone(),
                    mode: mode.clone(),
                    declared: "(none — this workflow declares no modes:)".to_string(),
                });
            }
        }
    }

    let run_dir = runs_root.join(run_id.as_str());
    tokio::fs::create_dir_all(runs_root)
        .await
        .map_err(|source| RunError::Io {
            context: format!("create runs root `{}`", runs_root.display()),
            source,
        })?;
    match tokio::fs::create_dir(&run_dir).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(RunError::RunDirExists { path: run_dir });
        }
        Err(source) => {
            return Err(RunError::Io {
                context: format!("create run directory `{}`", run_dir.display()),
                source,
            });
        }
    }
    let artifacts_dir = run_dir.join("artifacts");
    for dir in [artifacts_dir.clone(), run_dir.join("scratch")] {
        tokio::fs::create_dir(&dir)
            .await
            .map_err(|source| RunError::Io {
                context: format!("create run directory `{}`", dir.display()),
                source,
            })?;
    }

    let manifest_path = run_dir.join("manifest.yaml");
    let yaml = yunta_core::yaml::to_string(manifest).map_err(|e| RunError::ManifestWrite {
        path: manifest_path.clone(),
        detail: e.to_string(),
    })?;
    tokio::fs::write(&manifest_path, yaml)
        .await
        .map_err(|source| RunError::Io {
            context: format!("write `{}`", manifest_path.display()),
            source,
        })?;

    for artifact in artifacts {
        let dest = artifacts_dir.join(&artifact.name);
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|source| RunError::Io {
                    context: format!("create `{}`", parent.display()),
                    source,
                })?;
        }
        tokio::fs::write(&dest, &artifact.bytes)
            .await
            .map_err(|source| RunError::Io {
                context: format!("write birth artifact `{}`", dest.display()),
                source,
            })?;
    }

    let event = EventDraft {
        run_id: run_id.clone(),
        node_id: None,
        payload: EventPayload::RunCreated(RunCreatedPayload {
            manifest_hash: manifest.manifest_hash(),
            // Every declared input as the manifest froze it — provided
            // or defaulted, already validated: what the run used.
            inputs: manifest
                .inputs
                .iter()
                .map(|(name, value)| (name.clone(), serde_json::Value::String(value.clone())))
                .collect(),
            mode: mode.clone(),
            promoted_from: promoted_from.cloned(),
            // Resolved once here — declared range as
            // written, or the binary's own schema when absent (the
            // reference text's "inferred from the binary").
            yunta_schema: Some(
                manifest
                    .workflow
                    .yunta_schema
                    .clone()
                    .unwrap_or_else(|| format!("={}", yunta_core::YUNTA_SCHEMA)),
            ),
            base_branch: manifest.base_branch.clone(),
            base_commit: manifest.base_commit.clone(),
        }),
    };
    let at = clock.now();
    storage.append(event, at).await?;

    Ok(run_dir)
}

/// Everything [`execute_run`] needs from its caller, grouped: the run's
/// own identity/manifest/paths, the environment it executes against
/// (adapters, storage, clock, ids), and the cross-cutting surfaces
/// (human_interaction, forge, cancel) every deep execution path can
/// reach through [`RunCtx`] once this is unpacked into one.
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
}

/// Drives a run until it finishes or pauses. Serving `yunta run` and
/// `yunta resume` with the same function is the point: the log decides
/// what remains, never in-process state.
pub async fn execute_run(env: RunEnv<'_>) -> Result<RunReport, RunError> {
    execute_run_at_depth(env, 0).await
}

/// Records the `run_paused` a post-crash `yunta cancel` writes when it
/// finds the engine already dead. The CLI never builds an `EventDraft`
/// itself: event construction and its clock stamp live here, so every
/// event on the log is emitted by the engine through one injected clock.
/// The run-level `run_paused` event — built in one place so every path that
/// stops a run (the scheduler's [`record_pause`], a crash's
/// [`record_pause_after_crash`]) writes the same event.
pub(super) fn run_paused(reason: &str) -> EventPayload {
    EventPayload::RunPaused(RunPausedPayload {
        reason: reason.to_string(),
    })
}

pub async fn record_pause_after_crash(
    storage: &AsyncStorage,
    run_id: &RunId,
    reason: &str,
    clock: &dyn Clock,
) -> Result<(), RunError> {
    storage
        .append(
            EventDraft {
                run_id: run_id.clone(),
                node_id: None,
                payload: run_paused(reason),
            },
            clock.now(),
        )
        .await?;
    Ok(())
}

/// Records the run's pause, exports the forensic `events.jsonl`, and returns
/// the paused report — the one place a run stops for a human to resume,
/// whatever asked for it (a cancellation, an exhausted budget, an
/// unanswered gate).
async fn pause(ctx: &RunCtx<'_>, reason: String) -> Result<RunReport, RunError> {
    record_pause(ctx, &reason).await?;
    Ok(RunReport {
        terminal: RunTerminal::Paused { reason },
        state: derive(&ctx.load_events().await?),
    })
}

/// Emits the run-level `run_paused` event and exports the forensic log — the
/// one place the pause event is written, shared by the scheduler's own pause
/// and the gate flow's.
pub(super) async fn record_pause(ctx: &RunCtx<'_>, reason: &str) -> Result<(), RunError> {
    ctx.emit(None, run_paused(reason)).await?;
    ctx.export_events_jsonl().await
}

/// [`execute_run`] with an explicit composition depth:
/// `workflow_exec` re-enters here for each child run, one level deeper —
/// the recursion the Contrato's "a parent's resume recursively resumes
/// orphaned children" is made of.
#[tracing::instrument(skip_all, fields(run_id = %env.run_id, depth))]
pub(crate) async fn execute_run_at_depth(
    env: RunEnv<'_>,
    depth: u32,
) -> Result<RunReport, RunError> {
    let RunEnv {
        run_id,
        manifest,
        run_dir,
        worktree,
        adapters,
        storage,
        clock,
        ids,
        max_task_retries,
        human_interaction,
        forge,
        cancel,
        adapter_override,
    } = env;
    // The root of every per-node token this invocation hands out.
    // `None` (tests, callers with no signal source) gets a token nothing
    // ever fires.
    let root_cancel = cancel.cloned().unwrap_or_default();
    let root_cancel_for_ctx = root_cancel.clone();
    // The registry is written before the ctx exists, so its failure
    // can't emit yet; it's carried past construction and recorded as a
    // finding once there's a log to record it on.
    let (registry, registry_error) = match crate::process_registry::ProcessRegistry::create(
        run_dir,
        Pid::current(),
        clock.now().to_rfc3339(),
    ) {
        Ok(registry) => (Some(registry), None),
        Err(e) => (None, Some(e)),
    };
    // The per-run MCP host outlives every borrow of this invocation, so
    // it owns a clone of the run's clock rather than borrowing it — the
    // one injected clock reaches the listener's own event appends.
    let clock_for_host = clock.clone();
    let ctx = RunCtx {
        run_id,
        manifest,
        run_dir,
        worktree,
        adapters,
        storage,
        clock,
        ids,
        max_task_retries,
        memo: Memo::new(manifest.config_hash.clone()),
        human_interaction,
        adapter_override,
        budget_lifted: std::sync::atomic::AtomicBool::new(false),
        process_registry: registry,
        root_cancel: root_cancel_for_ctx,
        forge,
        depth,
        // One host per execute_run invocation, shared by every
        // session listener; each of them reads and writes through the
        // host's own clone of the log handle.
        run_tools_host: Arc::new(crate::run_tools::RunToolsHost::new(
            storage.clone(),
            run_id.clone(),
            &manifest.workflow,
            clock_for_host,
        )),
    };

    let events = ctx.load_events().await?;
    if events.is_empty() {
        return Err(RunError::UnknownRun {
            run_id: run_id.clone(),
        });
    }
    if events
        .iter()
        .any(|e| matches!(e.payload(), Some(EventPayload::RunFinished(_))))
    {
        // Re-executing a finished run is a no-op, not an error — the log
        // already has its ending.
        return Ok(RunReport {
            terminal: RunTerminal::Finished,
            state: derive(&events),
        });
    }
    if events.len() > 1 {
        // Anything beyond run_created means a previous invocation worked
        // on this run — this one is a resume. A node the mode excludes
        // never ran, so the orphans are the same whichever nodes are
        // in the mode.
        let policies = schedule::resume_policies(
            manifest.workflow.nodes.iter(),
            &derive(&events),
            manifest.config.resolved_on_interrupt(),
        );
        let shared: BTreeSet<&str> = policies
            .iter()
            .map(|policy| policy.on_interrupt.as_str())
            .collect();
        let resume_policy_applied = match shared.iter().next() {
            Some(policy) if shared.len() == 1 => Some((*policy).to_string()),
            _ => None,
        };
        ctx.emit(
            None,
            EventPayload::RunResumed(RunResumedPayload {
                resume_policy_applied,
                policies,
            }),
        )
        .await?;
    }

    // "On wake" means once per invocation, not once per
    // scheduling iteration — checked here, before the loop, so both the
    // very first `yunta run` call and every later `yunta resume` do this
    // exactly once. Placed *after* the "already `run_finished`" early
    // return above, deliberately: a genuinely finished run is immutable
    // — nothing reopens it, ever — so a stale approval only
    // matters, and is only ever rechecked, while the run still has
    // unresolved work of its own keeping it open.
    if let Some(error) = registry_error {
        ctx.engine_finding(
            None,
            "engine-registry",
            FindingSeverity::Minor,
            "the process registry could not be written".to_string(),
            crate::process_registry::registry_path(run_dir)
                .display()
                .to_string(),
            format!("`yunta cancel` cannot see this invocation's process tree: {error}"),
        )
        .await?;
    }

    gate_exec::recheck_approved_gates(&ctx, forge).await?;

    // The mode is frozen once, in `run_created` (`events[0]`
    // — never absent, checked above), and never re-resolved — a resume
    // reads the same name back off the log rather than re-deriving it,
    // same "resolved once, reused forever" discipline runner resolution
    // already follows.
    let mode_name = yunta_core::events::run_mode(&events);
    let mode_nodes = crate::modes::mode_included_nodes(&manifest.workflow, &mode_name);

    loop {
        // A Ctrl-C (or any root cancellation) between scheduler
        // steps pauses here; one that lands mid-batch is honored by the
        // per-node child tokens below, whose failed nodes land in the
        // log first and then reach this same check.
        if root_cancel.is_cancelled() {
            return pause(&ctx, "cancelled by user".to_string()).await;
        }
        let events = ctx.load_events().await?;
        match schedule::next_step(
            &manifest.workflow,
            &events,
            manifest.max_parallel_nodes,
            manifest.config.resolved_on_interrupt(),
            mode_nodes.as_ref(),
        ) {
            ScheduleStep::Broken { diagnostic } => {
                return Err(steps::broken(&ctx, diagnostic).await)
            }
            ScheduleStep::Finish => return steps::finish(&ctx, &mode_name).await,
            ScheduleStep::Pause { reason } => return pause(&ctx, reason).await,
            ScheduleStep::Reroute {
                from,
                to,
                attempt,
                max_reroutes,
                cause,
            } => steps::reroute(&ctx, from, to, attempt, max_reroutes, cause).await?,
            ScheduleStep::GateExhaustedReroutes {
                node,
                goto,
                max_reroutes,
                cause,
            } => {
                if let Some(report) = steps::gate_exhausted(
                    &ctx,
                    &events,
                    &mode_name,
                    node,
                    goto,
                    max_reroutes,
                    cause,
                )
                .await?
                {
                    return Ok(report);
                }
            }
            ScheduleStep::Execute(batch) => {
                if let Some(report) = steps::execute_batch(&ctx, &events, batch).await? {
                    return Ok(report);
                }
            }
            ScheduleStep::PublishGate { node } => {
                if let Some(report) = steps::publish_gate(&ctx, node).await? {
                    return Ok(report);
                }
            }
            ScheduleStep::PollGate { node, external_ref } => {
                if let Some(report) = steps::poll_gate(&ctx, node, external_ref).await? {
                    return Ok(report);
                }
            }
            ScheduleStep::ResolveInternalGate { node } => {
                if let Some(report) = steps::resolve_internal_gate(&ctx, node).await? {
                    return Ok(report);
                }
            }
            ScheduleStep::AskQuestions { node } => {
                if let Some(report) = steps::ask_questions(&ctx, node).await? {
                    return Ok(report);
                }
            }
        }
    }
}

pub(super) fn find_node<'a>(
    workflow: &'a yunta_core::Workflow,
    node_id: &NodeId,
) -> Result<&'a yunta_core::Node, RunError> {
    workflow
        .nodes
        .iter()
        .find(|n| &n.id == node_id)
        .ok_or_else(|| RunError::Broken {
            diagnostic: format!(
                "scheduler chose node `{node_id}` which the manifest's workflow does not define"
            ),
        })
}
