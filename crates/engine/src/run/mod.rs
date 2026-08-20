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

mod check_exec;
mod context_resolve;
mod executor_exec;
mod gate_exec;
mod loop_exec;
mod node_exec;
mod schedule;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use thiserror::Error;
use tokio_util::sync::CancellationToken;
use yunta_adapters::{Adapter, Forge};
use yunta_core::events::{
    Event, EventPayload, GateOption, GateWaitingPayload, NodeReroutedPayload, RunCreatedPayload,
    RunFinishedPayload, RunMetrics, RunPausedPayload, RunResumedPayload, TerminalState,
};
use yunta_core::{Clock, Manifest, NodeId, RunId, YuntaError};
use yunta_storage::{Storage, StorageError};

use crate::human_interaction::HumanInteraction;
use crate::replay::{derive, RunState};
use crate::scope::ScopeCheckError;
use crate::stats::cptv;
use crate::task_cycle::{Memo, TaskCycleError};
use schedule::ScheduleStep;

#[derive(Debug, Error)]
pub enum RunError {
    #[error(transparent)]
    Storage(#[from] StorageError),

    #[error("run `{run_id}` has no events — create it before executing it")]
    UnknownRun { run_id: RunId },

    #[error("run is broken: {diagnostic}")]
    Broken { diagnostic: String },

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
}

/// How `execute_run` came back: everything done, or waiting on a human.
#[derive(Debug, Clone, PartialEq)]
pub enum RunTerminal {
    Finished,
    Paused { reason: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunReport {
    pub terminal: RunTerminal,
    pub state: RunState,
}

/// Creates the run's anatomy (§2): run.dir with `artifacts/` and
/// `scratch/`, the frozen `manifest.yaml`, and the `run_created` event.
/// Returns the run directory.
pub fn create_run(
    run_id: &RunId,
    manifest: &Manifest,
    runs_root: &Path,
    storage: &Storage,
    clock: &dyn Clock,
) -> Result<PathBuf, RunError> {
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
            mode: "default".to_string(), // modes are §10/M9, out of M-0
            promoted_from: None,
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
) -> Result<RunReport, RunError> {
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

    loop {
        let events = ctx.load_events()?;
        match schedule::next_step(
            &manifest.workflow,
            &events,
            manifest.max_parallel_nodes,
            manifest.config.resolved_on_interrupt(),
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
                // failed. `retry`/`abort` are exactly the two well-
                // defined outcomes at this pause: try the same
                // corrective node once more (human-authorized, since the
                // declared cap is spent), or stop here.
                let escalation = GateWaitingPayload {
                    summary: format!(
                        "node `{node}` failed and its {max_reroutes} re-route(s) to `{goto}` \
                         are exhausted: {cause}"
                    ),
                    evidence: cause.clone(),
                    options: vec![
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
                    ],
                    external_ref: None,
                };
                let resolution = human_interaction.resolve(&escalation).await;
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
                let executions = batch.into_iter().map(|(node_id, attempt)| {
                    let ctx = &ctx;
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
                        node_exec::execute_node(ctx, node, attempt, &CancellationToken::new())
                            .await
                    }
                });
                for result in futures::future::join_all(executions).await {
                    result?;
                }
            }
            ScheduleStep::PublishGate { node } => {
                let node = find_node(&manifest.workflow, &node)?;
                let yunta_core::NodeKind::Gate { assignee, external } = &node.kind else {
                    return Err(RunError::Broken {
                        diagnostic: format!(
                            "scheduler chose node `{}` as a gate to publish, but its kind isn't `gate`",
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
