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
//! other). `ScheduleStep::Broken`'s early `Err` return is deliberately
//! excluded: that path never reaches a `RunReport` at all today, and a
//! corrupt-log export is its own design question, not silently folded
//! into this one. `on_finish.distill` itself stays unimplemented — its
//! mechanism (agent session? deterministic transform?) isn't documented
//! anywhere in Notion, and per CLAUDE.md a mechanism that can't be
//! reasoned about mock-testability for shouldn't be built on a guess.

mod budget;
mod check_exec;
mod context_resolve;
mod executor_exec;
mod gate_exec;
mod loop_exec;
mod node_exec;
mod questions_exec;
mod schedule;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use thiserror::Error;
use tokio_util::sync::CancellationToken;
use yunta_adapters::{Adapter, Forge};
use yunta_core::events::{
    Event, EventPayload, GateOption, GateWaitingPayload, NodeReroutedPayload,
    PromotionSignaledPayload, RunCreatedPayload, RunFinishedPayload, RunMetrics, RunPausedPayload,
    RunResumedPayload, TerminalState,
};
use yunta_core::{Clock, Manifest, NodeId, RunId, YuntaError};
use yunta_storage::{Storage, StorageError};

use crate::human_interaction::HumanInteraction;
use crate::replay::{derive, RunState};
use crate::scope::ScopeCheckError;
use crate::stats::cptv;
use crate::task_cycle::{Memo, TaskCycleError};
pub use budget::session_token_budget;
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
        if self
            .budget_lifted
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Ok(yunta_adapters::Budget::default());
        }
        let Some(cap) = self
            .manifest
            .config
            .limits
            .as_ref()
            .and_then(|limits| limits.max_tokens_per_run)
        else {
            return Ok(yunta_adapters::Budget::default());
        };
        let state = derive(&self.load_events()?);
        let non_terminal = flatten(&self.manifest.workflow.nodes)
            .into_iter()
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
            ..Default::default()
        })
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

/// Every node in declaration order, `parallel` children included — the
/// same local convention `progress.rs`/`stats.rs` each already follow.
fn flatten(nodes: &[yunta_core::Node]) -> Vec<&yunta_core::Node> {
    let mut flat = Vec::new();
    for node in nodes {
        flat.push(node);
        if let yunta_core::NodeKind::Parallel {
            nodes: children, ..
        } = &node.kind
        {
            flat.extend(flatten(children));
        }
    }
    flat
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

/// Creates the run's anatomy (§2): run.dir with `artifacts/` and
/// `scratch/`, the frozen `manifest.yaml`, and the `run_created` event.
/// Returns the run directory.
///
/// `mode` (§10.1/D44) is frozen into `run_created.mode` right here and
/// never re-resolved again — a resume reads the same name back off the
/// log. `"default"` — the caller's choice when nothing else applies,
/// same sentinel `stats.rs`'s own `mode_of` already falls back to for a
/// pre-T9.1 log — always passes: a workflow declaring no `modes:` at
/// all has nothing to validate a name against, and every node stays
/// schedulable, exactly pre-T9.1 behavior. A workflow that *does*
/// declare `modes:` rejects any other unrecognized name.
pub fn create_run(
    run_id: &RunId,
    manifest: &Manifest,
    runs_root: &Path,
    storage: &Storage,
    clock: &dyn Clock,
    mode: &str,
    promoted_from: Option<&RunId>,
) -> Result<PathBuf, RunError> {
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
            yunta_schema: None,
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
    // DI-08: the root of every per-node token this invocation hands out.
    // `None` (tests, callers with no signal source) gets a token nothing
    // ever fires — the pre-DI-08 behavior exactly.
    let root_cancel = cancel.cloned().unwrap_or_default();
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
    let mode_name = match &events[0].payload {
        EventPayload::RunCreated(p) => p.mode.clone(),
        _ => "default".to_string(),
    };
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
                return Err(RunError::Broken { diagnostic });
            }
            ScheduleStep::Finish => {
                let state = derive(&events);
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
                // failed. `retry`/`abort` are the two outcomes any
                // exhausted re-route offers; §10.2's own "esto excede el
                // modo" adds a third, `promote`, exactly when there's
                // somewhere later in `modes:`'s own declaration order to
                // promote *to* — never invented when there isn't.
                let suggested_mode = schedule::next_mode_after(&manifest.workflow, &mode_name);
                let mut options = vec![
                    GateOption {
                        id: "retry".to_string(),
                        label: format!("Re-route to `{goto}` once more"),
                        tradeoff: format!(
                            "Uses one extra correction attempt beyond the declared \
                             max_reroutes ({max_reroutes}); escalates again if `{goto}` \
                             doesn't fix it"
                        ),
                    },
                    GateOption {
                        id: "abort".to_string(),
                        label: "Abort the run".to_string(),
                        tradeoff: "Stops here; nothing further executes".to_string(),
                    },
                ];
                if let Some(next_mode) = &suggested_mode {
                    options.push(GateOption {
                        id: "promote".to_string(),
                        label: format!("Promote to mode `{next_mode}`"),
                        tradeoff: format!(
                            "Closes this run (`run_finished: promoted`) and starts a \
                             successor in `{next_mode}`, inheriting this run's artifacts; \
                             §10.2 — there's no mechanism to demote back to `{mode_name}`"
                        ),
                    });
                }
                let escalation = GateWaitingPayload {
                    summary: format!(
                        "node `{node}` failed and its {max_reroutes} re-route(s) to `{goto}` \
                         are exhausted: {cause}"
                    ),
                    evidence: cause.clone(),
                    options,
                    external_ref: None,
                };
                let resolution = ctx.human_interaction.resolve(&escalation).await;
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
                ctx.emit(Some(&node), EventPayload::GateWaiting(escalation))?;
                ctx.emit(Some(&node), EventPayload::GateResolved(resolution.clone()))?;
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
                for result in futures::future::join_all(executions).await {
                    result?;
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
