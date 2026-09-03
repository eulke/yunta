//! [`RunCtx`] — everything node execution borrows once, and the small
//! helpers (`emit`, `load_events`, `export_events_jsonl`, `engine_finding`)
//! that give every event its timestamp from the one injected clock and its
//! seq from storage.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use yunta_adapters::{Adapter, Forge};
use yunta_core::events::{
    EventDraft, EventPayload, Finding, FindingPostedPayload, FindingSeverity, StoredEvent,
};
use yunta_core::{AdapterId, Clock, FindingId, IdSource, Manifest, NodeId, RunId, Seq};
use yunta_storage::{AsyncStorage, StorageError};

use crate::human_interaction::HumanInteraction;
use crate::replay::derive;
use crate::task_cycle::Memo;

use super::{budget, RunError};

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
