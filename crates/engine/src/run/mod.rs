//! Run creation and execution (T4.1/T4.5 recorte, Contrato §2/§8.1).
//!
//! `create_run` freezes the anatomy on disk (run.dir, `manifest.yaml`,
//! `run_created`); `execute_run` drives the run forward and is also
//! `yunta resume` — it replays the log, asks [`schedule::next_action`]
//! what's next, and executes until the answer is terminal. Crash,
//! restart and Ctrl-C are the same case: whatever the log says happened,
//! happened; everything else re-runs (§8.1, `restart_node`).
//!
//! **`events.jsonl` export (§8.3, T5.8)**: the Contrato says only "al
//! cierre" without enumerating which terminal states count — a real gap,
//! documented in `docs/m0-status.md`'s T5.8 entry rather than guessed
//! silently. This recorte exports on every terminal `RunReport` the
//! schedule loop already recognizes — `Finish` and `Pause` alike, via
//! [`RunCtx::export_events_jsonl`] — unconditionally, independent of
//! whether the workflow declares `on_finish:` at all (D20/§8.3's own
//! phrasing reads as two actions conjoined at close, not one gated on the
//! other). Since DI-21 the `ScheduleStep::Broken` path exports too,
//! best-effort before its `Err` — a corrupt log is exactly the one a
//! forensic reader most wants on disk. `on_finish.distill` is DI-24's deterministic
//! transform (ADR D107) — see `distill.rs`; the close sequence is
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
mod workflow_exec;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use thiserror::Error;
use tokio_util::sync::CancellationToken;
use yunta_adapters::{Adapter, Forge};
use yunta_core::events::{
    Event, EventPayload, NodeReroutedPayload, PromotionSignaledPayload, RunCreatedPayload,
    RunFinishedPayload, RunMetrics, RunPausedPayload, RunResumedPayload, TerminalState,
};
use yunta_core::{Clock, Manifest, NodeId, RunId, YuntaError};
use yunta_storage::{Storage, StorageError};

use crate::human_interaction::HumanInteraction;
use crate::replay::{derive, RunState};
use crate::scope::ScopeCheckError;
use crate::stats::cptv;
use crate::task_cycle::{Memo, TaskCycleError};
pub use budget::session_token_budget;
pub use escalation::{current_escalation, resolve_gate, ResolveGateError};
pub use promote::{create_promotion_successor, PromotionSuccessor};
pub use schedule::mode_included_nodes;
use schedule::ScheduleStep;

#[derive(Debug, Error)]
pub enum RunError {
    #[error(transparent)]
    Storage(#[from] StorageError),

    #[error("run `{run_id}` has no events — create it before executing it")]
    UnknownRun { run_id: RunId },

    #[error("run is broken: {diagnostic}")]
    Broken { diagnostic: String },

    /// §10.1/D44: `create_run`'s own guard — `check` validates every
    /// mode's *internal* coherence, but never sees which one a run
    /// actually asks for, so this is where an unknown `--mode` name is
    /// caught, before anything is written.
    #[error("workflow `{workflow}` declares no mode `{mode}` — declared modes: {declared}")]
    UnknownMode {
        workflow: String,
        mode: String,
        declared: String,
    },

    #[error("failed to {context}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
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
        source: YuntaError,
    },

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
    pub adapters: &'a HashMap<String, Arc<dyn Adapter>>,
    pub storage: &'a Storage,
    pub clock: &'a dyn Clock,
    pub max_task_retries: u32,
    /// Criteria memoization (§5.4, T5.9) — one cache per `execute_run`
    /// call, never persisted: a resume simply starts cold, which is safe
    /// (over-verifying) rather than risking a stale cross-run hit.
    pub memo: Memo,
    /// The one surface every §5.3 escalation goes through (T7.2, DI-01):
    /// exhausted re-routes and scope-expansion `ask` alike — on the ctx
    /// so the deep execution paths (loop_exec) reach it without threading
    /// one more parameter through every layer.
    pub human_interaction: &'a dyn HumanInteraction,
    /// §8.3/DI-05: a human's `continue` past the token cap, held for
    /// this invocation only — in memory, never derived from the log, so
    /// every resume asks again before spending new money. Atomic because
    /// concurrent batch members read it while the scheduler loop writes.
    pub budget_lifted: std::sync::atomic::AtomicBool,
    /// DI-08: `run.dir/scratch/engine.json`, so a separate process can
    /// find this run's live process tree. `None` when the file could not
    /// be written — the run proceeds, degraded loudly (A4's external
    /// paths lose their map, the internal ones never needed it).
    pub process_registry: Option<crate::process_registry::ProcessRegistry>,
    /// DI-08/DI-11: the invocation's root cancellation (Ctrl-C, `yunta
    /// cancel`). Execution paths consult it to tell a user cancellation
    /// (leave the node orphaned — resume re-treats it per
    /// `on_interrupt`, §8.1) apart from a `join: any` sibling race
    /// (record the loss as failed so the group can close).
    pub root_cancel: CancellationToken,
    /// T9.3: the forge this invocation was given — on the ctx so a
    /// `kind: workflow` node can hand it down to its child run (whose
    /// own gates are as real as the parent's).
    pub forge: Option<&'a dyn Forge>,
    /// T9.3: how many `kind: workflow` levels above this run (0 = the
    /// root invocation) — compared against
    /// `limits.max_workflow_depth` before a child is born.
    pub depth: u32,
    /// T8.2: the per-run MCP host every session listener of this run
    /// shares (its own reopened storage handle — listeners outlive any
    /// borrow of ours). `None` when the reopen failed at run start:
    /// sessions run without an endpoint, degraded loudly where a node
    /// actually needed one.
    pub run_tools_host: Option<Arc<crate::run_tools::RunToolsHost>>,
}

impl RunCtx<'_> {
    /// Appends one event and returns the seq storage assigned to it.
    pub(crate) fn emit(
        &self,
        node_id: Option<&NodeId>,
        payload: EventPayload,
    ) -> Result<u64, RunError> {
        let event = Event {
            run_id: self.run_id.clone(),
            seq: 0, // storage assigns the real monotonic seq
            timestamp: self.clock.now(),
            node_id: node_id.cloned(),
            payload,
        };
        Ok(self.storage.append_event(&event)?)
    }

    pub(crate) fn load_events(&self) -> Result<Vec<Event>, RunError> {
        Ok(self.storage.events_for_run(self.run_id)?)
    }

    /// Exports the run's whole log to `run.dir/events.jsonl` (§8.3, T5.8)
    /// — called at every close this recorte recognizes (`Finish` and
    /// `Pause`; see this module's own doc comment on the trigger
    /// decision). Re-exports in full each time, same "regenerate from the
    /// log" principle `progress.md` (T5.5) already follows — a run that
    /// pauses, resumes, and later finishes just gets the file rewritten
    /// with the fuller log, never appended to.
    pub(crate) fn export_events_jsonl(&self) -> Result<(), RunError> {
        let events = self.load_events()?;
        let jsonl = crate::events_export::render_events_jsonl(&events)?;
        std::fs::write(self.run_dir.join("events.jsonl"), jsonl).map_err(|source| RunError::Io {
            context: "write events.jsonl".to_string(),
            source,
        })
    }

    /// The [`Budget`] for one agent session (§8.3/T3.3, DI-05): an equal
    /// share of the remaining run cap
    /// ([`budget::session_token_budget`]'s policy). Unlimited — exactly
    /// the pre-limits behavior — when no cap is declared, or when a
    /// human already answered `continue` this invocation (their lift
    /// must not resurface as a zero-token session budget). `timeout`
    /// stays `None`: `defaults.timeout_minutes` is outside T1.2's cut.
    pub(crate) fn session_budget(&self) -> Result<yunta_adapters::Budget, RunError> {
        // `defaults.timeout_minutes` (DI-13) applies on every path —
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
        let state = derive(&self.load_events()?);
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
                budget::tokens_spent(state.total_tokens),
                non_terminal,
            )),
            timeout,
            ..Default::default()
        })
    }

    /// The opaque `adapter_settings` the config declares for `adapter`
    /// (DI-13) — passed through to the request untouched.
    pub(crate) fn adapter_settings(
        &self,
        adapter: &str,
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

/// DI-09: `RunCtx` is the one real [`SessionObserver`] — audit events
/// land in the run's own log as they arrive, so a concurrent `status`
/// sees the live session. A failed append warns instead of aborting the
/// stream: the run's next mandatory event hits the same storage and
/// fails the run properly if it's really down.
impl crate::task_cycle::SessionObserver for RunCtx<'_> {
    fn emit_session_event(&self, node_id: &NodeId, payload: EventPayload) {
        if let Err(e) = self.emit(Some(node_id), payload) {
            tracing::warn!(error = %e, "failed to append a session audit event");
        }
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
    /// §10.2/D22: this run's own gate accepted promotion — `run_finished`
    /// (`terminal_state: Promoted`) is already on *this* log, closing it
    /// for good (I3: nothing reopens a finished run, same guarantee
    /// T7.7's own SHA-drift recheck leans on). Creating and starting the
    /// successor — a fresh run, its own `run_id`, in `suggested_mode`,
    /// `promoted_from` this one — is the caller's job: it needs the
    /// original repo checkout (`cwd`) to prepare a worktree, which
    /// `execute_run` was never given (only ever an *existing* worktree).
    Promoted {
        suggested_mode: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunReport {
    pub terminal: RunTerminal,
    pub state: RunState,
}

/// What [`create_run`] freezes (DI-19): the run's identity and its
/// declared birth facts, bundled — `storage`/`clock` stay separate
/// arguments because they are the caller's *infrastructure*, not this
/// run's data.
pub struct CreateRunParams<'a> {
    pub run_id: &'a RunId,
    pub manifest: &'a Manifest,
    pub runs_root: &'a Path,
    /// §10.1/D44 — frozen into `run_created.mode` and never re-resolved.
    pub mode: &'a str,
    /// §10.2/D22 — the predecessor this run inherits from, if any.
    pub promoted_from: Option<&'a RunId>,
}

/// Creates the run's anatomy (§2): run.dir with `artifacts/` and
/// `scratch/`, the frozen `manifest.yaml`, and the `run_created` event.
/// Returns the run directory.
///
/// `mode` (§10.1/D44) is frozen into `run_created.mode` right here and
/// never re-resolved again — a resume reads the same name back off the
/// log. `"default"` — the caller's choice when nothing else applies,
/// same sentinel `events::run_mode` falls back to for a pre-T9.1 log —
/// always passes: a workflow declaring no `modes:` at all has nothing
/// to validate a name against, and every node stays schedulable,
/// exactly pre-T9.1 behavior. A workflow that *does* declare `modes:`
/// rejects any other unrecognized name.
pub fn create_run(
    params: CreateRunParams<'_>,
    storage: &Storage,
    clock: &dyn Clock,
) -> Result<PathBuf, RunError> {
    let CreateRunParams {
        run_id,
        manifest,
        runs_root,
        mode,
        promoted_from,
    } = params;
    if mode != "default" {
        match &manifest.workflow.modes {
            Some(modes) if !modes.contains_key(mode) => {
                return Err(RunError::UnknownMode {
                    workflow: manifest.workflow.name.clone(),
                    mode: mode.to_string(),
                    declared: modes.keys().cloned().collect::<Vec<_>>().join(", "),
                });
            }
            Some(_) => {}
            None => {
                return Err(RunError::UnknownMode {
                    workflow: manifest.workflow.name.clone(),
                    mode: mode.to_string(),
                    declared: "(none — this workflow declares no modes:)".to_string(),
                });
            }
        }
    }

    let run_dir = runs_root.join(run_id.as_str());
    for dir in [
        run_dir.clone(),
        run_dir.join("artifacts"),
        run_dir.join("scratch"),
    ] {
        std::fs::create_dir_all(&dir).map_err(|source| RunError::Io {
            context: format!("create run directory `{}`", dir.display()),
            source,
        })?;
    }

    let manifest_path = run_dir.join("manifest.yaml");
    let yaml = serde_yaml::to_string(manifest).map_err(|e| RunError::ManifestWrite {
        path: manifest_path.clone(),
        detail: e.to_string(),
    })?;
    std::fs::write(&manifest_path, yaml).map_err(|source| RunError::Io {
        context: format!("write `{}`", manifest_path.display()),
        source,
    })?;

    let event = Event {
        run_id: run_id.clone(),
        seq: 0,
        timestamp: clock.now(),
        node_id: None,
        payload: EventPayload::RunCreated(RunCreatedPayload {
            manifest_hash: manifest.manifest_hash(),
            inputs: HashMap::new(), // `inputs:` schema is T1.5, out of M-0
            mode: mode.to_string(),
            promoted_from: promoted_from.cloned(),
            // §2.1/DI-13: resolved once here — declared range as
            // written, or the binary's own schema when absent (the
            // reference text's "se infiere del binario").
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
    storage.append_event(&event)?;

    Ok(run_dir)
}

/// Drives a run until it finishes or pauses. Serving `yunta run` and
/// `yunta resume` with the same function is the point: the log decides
/// what remains, never in-process state (I2).
#[allow(clippy::too_many_arguments)]
pub async fn execute_run(
    run_id: &RunId,
    manifest: &Manifest,
    run_dir: &Path,
    worktree: &Path,
    adapters: &HashMap<String, Arc<dyn Adapter>>,
    storage: &Storage,
    clock: &dyn Clock,
    max_task_retries: u32,
    human_interaction: &dyn HumanInteraction,
    forge: Option<&dyn Forge>,
    cancel: Option<&CancellationToken>,
) -> Result<RunReport, RunError> {
    execute_run_at_depth(
        run_id,
        manifest,
        run_dir,
        worktree,
        adapters,
        storage,
        clock,
        max_task_retries,
        human_interaction,
        forge,
        cancel,
        0,
    )
    .await
}

/// [`execute_run`] with an explicit composition depth (T9.3):
/// `workflow_exec` re-enters here for each child run, one level deeper —
/// the recursion the Contrato's "resume del padre retoma hijos
/// huérfanos recursivamente" (§12) is made of.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute_run_at_depth(
    run_id: &RunId,
    manifest: &Manifest,
    run_dir: &Path,
    worktree: &Path,
    adapters: &HashMap<String, Arc<dyn Adapter>>,
    storage: &Storage,
    clock: &dyn Clock,
    max_task_retries: u32,
    human_interaction: &dyn HumanInteraction,
    forge: Option<&dyn Forge>,
    cancel: Option<&CancellationToken>,
    depth: u32,
) -> Result<RunReport, RunError> {
    // DI-08: the root of every per-node token this invocation hands out.
    // `None` (tests, callers with no signal source) gets a token nothing
    // ever fires — the pre-DI-08 behavior exactly.
    let root_cancel = cancel.cloned().unwrap_or_default();
    let root_cancel_for_ctx = root_cancel.clone();
    let ctx = RunCtx {
        run_id,
        manifest,
        run_dir,
        worktree,
        adapters,
        storage,
        clock,
        max_task_retries,
        memo: Memo::new(manifest.config_hash.clone()),
        human_interaction,
        budget_lifted: std::sync::atomic::AtomicBool::new(false),
        process_registry: match crate::process_registry::ProcessRegistry::create(
            run_dir,
            std::process::id(),
            clock.now().to_rfc3339(),
        ) {
            Ok(registry) => Some(registry),
            Err(e) => {
                tracing::warn!(error = %e, "cannot write engine.json — `yunta cancel` will                      not see this invocation's processes");
                None
            }
        },
        root_cancel: root_cancel_for_ctx,
        forge,
        depth,
        // T8.2: one host per execute_run invocation; every session
        // listener reopens nothing — they share this handle's clone of
        // the storage connection path. A failed reopen degrades here,
        // once, loudly; nodes that *need* the endpoint (a blackboard
        // group) fail individually with their own diagnostic.
        run_tools_host: match storage.reopen() {
            Ok(own) => Some(Arc::new(crate::run_tools::RunToolsHost::new(
                own,
                run_id.clone(),
                &manifest.workflow,
            ))),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "cannot reopen storage for the per-run MCP host — sessions run without \
                     run tools"
                );
                None
            }
        },
    };

    let events = ctx.load_events()?;
    if events.is_empty() {
        return Err(RunError::UnknownRun {
            run_id: run_id.clone(),
        });
    }
    if events
        .iter()
        .any(|e| matches!(e.payload, EventPayload::RunFinished(_)))
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
        // on this run — this one is a resume (§8.1).
        ctx.emit(
            None,
            EventPayload::RunResumed(RunResumedPayload {
                resume_policy_applied: Some("restart_node".to_string()),
            }),
        )?;
    }

    // §5.6/T7.7: "al despertar" means once per invocation, not once per
    // scheduling iteration — checked here, before the loop, so both the
    // very first `yunta run` call and every later `yunta resume` do this
    // exactly once. Placed *after* the "already `run_finished`" early
    // return above, deliberately: a genuinely finished run is immutable
    // (I2/§2 — nothing reopens it, ever), so a stale approval only
    // matters, and is only ever rechecked, while the run still has
    // unresolved work of its own keeping it open.
    gate_exec::recheck_approved_gates(&ctx, forge).await?;

    // §10.1/D44: the mode is frozen once, in `run_created` (`events[0]`
    // — never absent, checked above), and never re-resolved — a resume
    // reads the same name back off the log rather than re-deriving it,
    // same "resolved once, reused forever" discipline runner resolution
    // already follows (§13.1).
    let mode_name = yunta_core::events::run_mode(&events).to_string();
    let mode_nodes = schedule::mode_included_nodes(&manifest.workflow, &mode_name);

    loop {
        // DI-08: a Ctrl-C (or any root cancellation) between scheduler
        // steps pauses here; one that lands mid-batch is honored by the
        // per-node child tokens below, whose failed nodes land in the
        // log first and then reach this same check.
        if root_cancel.is_cancelled() {
            ctx.emit(
                None,
                EventPayload::RunPaused(RunPausedPayload {
                    reason: "cancelled by user".to_string(),
                }),
            )?;
            ctx.export_events_jsonl()?;
            return Ok(RunReport {
                terminal: RunTerminal::Paused {
                    reason: "cancelled by user".to_string(),
                },
                state: derive(&ctx.load_events()?),
            });
        }
        let events = ctx.load_events()?;
        match schedule::next_step(
            &manifest.workflow,
            &events,
            manifest.max_parallel_nodes,
            manifest.config.resolved_on_interrupt(),
            mode_nodes.as_ref(),
        ) {
            ScheduleStep::Broken { diagnostic } => {
                // DI-21: a corrupt log is exactly the one you most want
                // exported — each event serializes on its own, so a
                // broken *sequence* doesn't stop the forensic copy.
                // Best-effort by design: if the export itself fails, the
                // original diagnostic wins (never masked by an IO error
                // about its own post-mortem).
                if let Err(export_error) = ctx.export_events_jsonl() {
                    tracing::warn!(
                        error = %export_error,
                        "could not export events.jsonl for the broken run"
                    );
                }
                return Err(RunError::Broken { diagnostic });
            }
            ScheduleStep::Finish => {
                // §8.3/DI-24: distill before `run_finished` — nothing is
                // emitted after the close event (I3), and its findings
                // are events.
                distill::run_distill(&ctx, &mode_name).await?;
                let state = derive(&ctx.load_events()?);
                ctx.emit(
                    None,
                    EventPayload::RunFinished(RunFinishedPayload {
                        terminal_state: TerminalState::Done,
                        metrics: RunMetrics {
                            cptv: cptv(&state),
                            tokens: state.total_tokens,
                        },
                    }),
                )?;
                ctx.export_events_jsonl()?;
                // §8.3/DI-13: `on_finish.cleanup: worktree` — after the
                // export, only at a real Finish (a paused run expects a
                // resume in that tree; a promoted one seeds its
                // successor's worktree from it). A cleanup failure warns
                // and never un-finishes the run the log already closed.
                let wants_cleanup = manifest.workflow.on_finish.iter().any(|step| {
                    matches!(
                        step,
                        yunta_core::OnFinishStep::Cleanup {
                            cleanup: yunta_core::CleanupTarget::Worktree
                        }
                    )
                });
                if wants_cleanup && manifest.isolation == yunta_core::Isolation::Worktree {
                    match crate::worktree::cleanup_worktree(
                        ctx.worktree,
                        &format!("yunta/{run_id}"),
                    )
                    .await
                    {
                        Ok(crate::worktree::WorktreeCleanup::Removed) => {}
                        Ok(crate::worktree::WorktreeCleanup::NotALinkedWorktree) => {
                            tracing::warn!(
                                worktree = %ctx.worktree.display(),
                                "on_finish.cleanup: worktree skipped — the run's tree is not \
                                 a linked git worktree"
                            );
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "on_finish.cleanup: worktree failed");
                        }
                    }
                }
                return Ok(RunReport {
                    terminal: RunTerminal::Finished,
                    state,
                });
            }
            ScheduleStep::Pause { reason } => {
                ctx.emit(
                    None,
                    EventPayload::RunPaused(RunPausedPayload {
                        reason: reason.clone(),
                    }),
                )?;
                ctx.export_events_jsonl()?;
                return Ok(RunReport {
                    terminal: RunTerminal::Paused { reason },
                    state: derive(&ctx.load_events()?),
                });
            }
            ScheduleStep::Reroute {
                from,
                to,
                attempt,
                max_reroutes,
                cause,
            } => {
                ctx.emit(
                    Some(&from),
                    EventPayload::NodeRerouted(NodeReroutedPayload {
                        to_node: to,
                        cause,
                        attempt,
                        max_reroutes,
                    }),
                )?;
            }
            ScheduleStep::GateExhaustedReroutes {
                node,
                goto,
                max_reroutes,
                cause,
            } => {
                // T7.2/§5.3: the engine assembles the escalation (summary
                // + mechanical evidence from the log) — never the node
                // that failed, which has no further say once it's
                // failed. Shared with `current_escalation` (M8/T8.1) so
                // a `resolve_gate` MCP call, running in a process that
                // never paused this run, reconstructs the identical
                // object instead of a second copy that could drift.
                let suggested_mode = schedule::next_mode_after(&manifest.workflow, &mode_name);
                let escalation = escalation::build_reroute_escalation(
                    &manifest.workflow,
                    &mode_name,
                    &node,
                    &goto,
                    max_reroutes,
                    &cause,
                );
                // DI-27: a decision `resolve_gate` pre-seeded onto the
                // log while this run was parked is consumed here, by
                // this same consequence code — never re-asked, and its
                // §5.3 pair is already recorded so it is never
                // re-emitted. The option is re-validated against the
                // re-derived menu: a mismatch means ask normally.
                let pre_seeded = escalation::pre_seeded_resolution(&events, &node).filter(|r| {
                    r.chosen_option
                        .as_deref()
                        .is_some_and(|chosen| escalation.options.iter().any(|o| o.id == chosen))
                });
                let already_recorded = pre_seeded.is_some();
                let resolution = match pre_seeded {
                    Some(resolution) => Some(resolution),
                    None => ctx.human_interaction.resolve(&escalation).await,
                };
                let Some(resolution) = resolution else {
                    // No live surface to ask (headless, no TTY, `yunta
                    // test`) — the pre-T7.2 behavior: pause and let a
                    // later `yunta resume` (or a future MCP client)
                    // carry the decision instead.
                    ctx.emit(
                        None,
                        EventPayload::RunPaused(RunPausedPayload {
                            reason: escalation.summary.clone(),
                        }),
                    )?;
                    ctx.export_events_jsonl()?;
                    return Ok(RunReport {
                        terminal: RunTerminal::Paused {
                            reason: escalation.summary,
                        },
                        state: derive(&ctx.load_events()?),
                    });
                };
                if !already_recorded {
                    ctx.emit(Some(&node), EventPayload::GateWaiting(escalation))?;
                    ctx.emit(Some(&node), EventPayload::GateResolved(resolution.clone()))?;
                }
                if resolution.chosen_option.as_deref() == Some("retry") {
                    ctx.emit(
                        Some(&node),
                        EventPayload::NodeRerouted(NodeReroutedPayload {
                            to_node: goto,
                            cause,
                            attempt: max_reroutes + 1,
                            max_reroutes,
                        }),
                    )?;
                } else if resolution.chosen_option.as_deref() == Some("promote") {
                    // `suggested_mode` must be `Some` here — `"promote"`
                    // only ever appeared as an option when it was.
                    let next_mode = suggested_mode.expect("promote option implies a next mode");
                    ctx.emit(
                        None,
                        EventPayload::PromotionSignaled(PromotionSignaledPayload {
                            reason: format!(
                                "node `{node}` exhausted its re-routes to `{goto}`: {cause}"
                            ),
                            evidence: cause,
                            suggested_mode: next_mode.clone(),
                        }),
                    )?;
                    // §8.3/DI-24: a promotion is a real close — the
                    // short attempt's knowledge is knowledge, and the
                    // successor inherits it through the repo layer.
                    distill::run_distill(&ctx, &mode_name).await?;
                    // DI-10/§10.2: findings without an artifact (D80
                    // denials) live only on this log — derive them into
                    // an inheritable artifact so the successor's copied
                    // context carries them. No findings, no file.
                    let events_for_close = ctx.load_events()?;
                    let inherited = crate::findings::inherited_findings(&events_for_close);
                    if !inherited.is_empty() {
                        let file = yunta_core::events::FindingsFile {
                            findings: inherited,
                        };
                        let yaml = serde_yaml::to_string(&file).map_err(|e| RunError::Broken {
                            diagnostic: format!("failed to serialize inherited findings: {e}"),
                        })?;
                        let path = ctx.run_dir.join("artifacts/findings-inherited.yaml");
                        std::fs::write(&path, yaml).map_err(|source| RunError::Io {
                            context: format!("write `{}`", path.display()),
                            source,
                        })?;
                    }
                    let state = derive(&events_for_close);
                    ctx.emit(
                        None,
                        EventPayload::RunFinished(RunFinishedPayload {
                            terminal_state: TerminalState::Promoted,
                            metrics: RunMetrics {
                                cptv: cptv(&state),
                                tokens: state.total_tokens,
                            },
                        }),
                    )?;
                    ctx.export_events_jsonl()?;
                    return Ok(RunReport {
                        terminal: RunTerminal::Promoted {
                            suggested_mode: next_mode,
                        },
                        state: derive(&ctx.load_events()?),
                    });
                } else {
                    let reason = format!(
                        "node `{node}`'s gate was resolved to abort{}",
                        resolution
                            .free_text
                            .as_deref()
                            .map(|text| format!(": {text}"))
                            .unwrap_or_default()
                    );
                    ctx.emit(
                        None,
                        EventPayload::RunPaused(RunPausedPayload {
                            reason: reason.clone(),
                        }),
                    )?;
                    ctx.export_events_jsonl()?;
                    return Ok(RunReport {
                        terminal: RunTerminal::Paused { reason },
                        state: derive(&ctx.load_events()?),
                    });
                }
            }
            ScheduleStep::Execute(batch) => {
                // §8.3/DI-05: the budget check guards exactly the steps
                // that spend tokens — a run whose remaining work is gates
                // and questions finishes without ever tripping it.
                if !ctx.budget_lifted.load(std::sync::atomic::Ordering::Relaxed) {
                    if let Some(cap) = manifest
                        .config
                        .limits
                        .as_ref()
                        .and_then(|limits| limits.max_tokens_per_run)
                    {
                        let spent = budget::tokens_spent(derive(&events).total_tokens);
                        if spent >= cap {
                            match budget::authorize_over_budget(&ctx, spent, cap).await? {
                                budget::BudgetDecision::Continue => ctx
                                    .budget_lifted
                                    .store(true, std::sync::atomic::Ordering::Relaxed),
                                budget::BudgetDecision::Pause { reason } => {
                                    ctx.emit(
                                        None,
                                        EventPayload::RunPaused(RunPausedPayload {
                                            reason: reason.clone(),
                                        }),
                                    )?;
                                    ctx.export_events_jsonl()?;
                                    return Ok(RunReport {
                                        terminal: RunTerminal::Paused { reason },
                                        state: derive(&ctx.load_events()?),
                                    });
                                }
                            }
                        }
                    }
                }
                // A batch runs to completion together (every member reaches
                // a terminal per-node state) before the next iteration
                // decides what comes next — the same simplification
                // `kind: parallel`'s `join: all` makes explicit (§5.8),
                // here implicit for scheduler-formed batches. Top-level DAG
                // fan-out never interrupts a still-running sibling the
                // moment one fails — that's `join: any`'s own semantics
                // (T4.6), scoped to a named `parallel` group, not implicit
                // `max_parallel_nodes` batches — so each node gets a token
                // nothing ever cancels.
                let cancel_for_batch = root_cancel.child_token();
                let executions = batch.into_iter().map(|(node_id, attempt)| {
                    let ctx = &ctx;
                    let cancel_for_batch = cancel_for_batch.clone();
                    let node = &manifest.workflow;
                    async move {
                        let node = node
                            .nodes
                            .iter()
                            .find(|n| n.id == node_id)
                            .ok_or_else(|| RunError::Broken {
                                diagnostic: format!(
                                    "scheduler chose node `{node_id}` which the manifest's workflow does not define"
                                ),
                            })?;
                        node_exec::execute_node(ctx, node, attempt, &cancel_for_batch).await
                    }
                });
                // T9.3: a workflow node whose child run paused can't
                // close its node (§12: the parent waits on the child's
                // *terminal* state) — after the whole batch lands, the
                // parent pauses too, naming the child. A root
                // cancellation takes precedence: the loop-top check
                // handles it as "cancelled by user".
                let mut child_paused: Option<String> = None;
                for result in futures::future::join_all(executions).await {
                    if let node_exec::NodeEnd::ChildPaused { reason } = result? {
                        child_paused.get_or_insert(reason);
                    }
                }
                if let Some(reason) = child_paused {
                    if !root_cancel.is_cancelled() {
                        ctx.emit(
                            None,
                            EventPayload::RunPaused(RunPausedPayload {
                                reason: reason.clone(),
                            }),
                        )?;
                        ctx.export_events_jsonl()?;
                        return Ok(RunReport {
                            terminal: RunTerminal::Paused { reason },
                            state: derive(&ctx.load_events()?),
                        });
                    }
                }
            }
            ScheduleStep::PublishGate { node } => {
                let node = find_node(&manifest.workflow, &node)?;
                let yunta_core::NodeKind::Gate {
                    assignee,
                    external: Some(external),
                    ..
                } = &node.kind
                else {
                    return Err(RunError::Broken {
                        diagnostic: format!(
                            "scheduler chose node `{}` as an external gate to publish, but it isn't one",
                            node.id
                        ),
                    });
                };
                let step = gate_exec::publish_gate(
                    &ctx,
                    node,
                    assignee,
                    external,
                    forge,
                    human_interaction,
                )
                .await?;
                if let gate_exec::GateStep::StillWaiting { reason } = step {
                    return Ok(RunReport {
                        terminal: RunTerminal::Paused { reason },
                        state: derive(&ctx.load_events()?),
                    });
                }
            }
            ScheduleStep::PollGate { node, external_ref } => {
                let node = find_node(&manifest.workflow, &node)?;
                let step =
                    gate_exec::poll_gate(&ctx, node, &external_ref, forge, human_interaction)
                        .await?;
                if let gate_exec::GateStep::StillWaiting { reason } = step {
                    return Ok(RunReport {
                        terminal: RunTerminal::Paused { reason },
                        state: derive(&ctx.load_events()?),
                    });
                }
            }
            ScheduleStep::ResolveInternalGate { node } => {
                let node = find_node(&manifest.workflow, &node)?;
                let yunta_core::NodeKind::Gate {
                    assignee,
                    message,
                    options,
                    on,
                    external: None,
                } = &node.kind
                else {
                    return Err(RunError::Broken {
                        diagnostic: format!(
                            "scheduler chose node `{}` as an internal gate, but it isn't one",
                            node.id
                        ),
                    });
                };
                let step = gate_exec::resolve_internal_gate(
                    &ctx,
                    node,
                    assignee,
                    message.as_deref(),
                    options,
                    on,
                )
                .await?;
                if let gate_exec::GateStep::StillWaiting { reason } = step {
                    ctx.emit(
                        None,
                        EventPayload::RunPaused(RunPausedPayload {
                            reason: reason.clone(),
                        }),
                    )?;
                    ctx.export_events_jsonl()?;
                    return Ok(RunReport {
                        terminal: RunTerminal::Paused { reason },
                        state: derive(&ctx.load_events()?),
                    });
                }
            }
            ScheduleStep::AskQuestions { node } => {
                let node = find_node(&manifest.workflow, &node)?;
                match questions_exec::execute_ask(&ctx, node).await? {
                    questions_exec::AskOutcome::Answered => {}
                    questions_exec::AskOutcome::Pause { reason } => {
                        ctx.emit(
                            None,
                            EventPayload::RunPaused(RunPausedPayload {
                                reason: reason.clone(),
                            }),
                        )?;
                        ctx.export_events_jsonl()?;
                        return Ok(RunReport {
                            terminal: RunTerminal::Paused { reason },
                            state: derive(&ctx.load_events()?),
                        });
                    }
                }
            }
        }
    }
}

fn find_node<'a>(
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
