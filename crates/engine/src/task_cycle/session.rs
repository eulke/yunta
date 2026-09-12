//! One agent session for a task: opening it, folding its events into
//! the log as they arrive, and closing it when the attempt ends or is
//! interrupted.

use std::path::PathBuf;
use std::time::Duration;

use futures::StreamExt;
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use yunta_adapters::{Adapter, AgentEvent, AgentOutcome, SessionRequest};
use yunta_core::events::{EventPayload, TokenUsage};
use yunta_core::AdapterError;
use yunta_storage::StorageError;

use super::DispatchOutcome;

/// Everything about *how* one node's sessions open, resolved
/// once by the engine and threaded through the cycle: the mounted
/// skills, the adapter's opaque settings, and the env — which is ONLY
/// the declared secret names present in the engine's own environment
/// (values never touch the log, nothing undeclared leaks).
#[derive(Clone)]
pub struct SessionSetup {
    pub skills: Vec<PathBuf>,
    pub adapter_settings: serde_json::Map<String, serde_json::Value>,
    pub env: std::collections::HashMap<String, yunta_core::Secret<String>>,
    /// The per-run MCP host plus the loop node's own id, present
    /// ONLY when the resolved adapter declared `run_tools` (the caller
    /// gates on the capability — this module never re-checks it). Each
    /// task attempt opens its own fresh listener+credential from it —
    /// per session, never reused.
    pub run_tools: Option<crate::run_tools::RunToolsAccess>,
    /// The run's own directory, so a task session can be told where its
    /// adapter may drop scaffolding — outside the worktree, whose diff
    /// the scope check reads.
    pub run_dir: PathBuf,
    /// The loop node these task sessions belong to. It names each
    /// session's own scratch directory, so concurrent attempts of one
    /// node never write over each other's scaffolding.
    pub node: yunta_core::NodeId,
}

impl SessionSetup {
    /// A setup that carries nothing but the node its sessions belong
    /// to: no skills, no settings, no secrets and no per-run tools.
    pub fn bare(node: yunta_core::NodeId) -> Self {
        Self {
            skills: Vec::new(),
            adapter_settings: serde_json::Map::new(),
            env: std::collections::HashMap::new(),
            run_tools: None,
            run_dir: PathBuf::new(),
            node,
        }
    }

    /// The env a session may see: declared names, present values.
    pub fn secrets_env(
        config: &yunta_core::ConfigLayer,
    ) -> std::collections::HashMap<String, yunta_core::Secret<String>> {
        config
            .secrets
            .iter()
            .filter_map(|name| {
                std::env::var(name)
                    .ok()
                    .map(|value| (name.clone(), yunta_core::Secret::from(value)))
            })
            .collect()
    }
}

/// What a session dispatch needs from its surrounding run,
/// abstracted so `run_task` stays callable without a full run context
/// (its own integration tests): append the session's audit events, and
/// expose the process registry for pgid bookkeeping. `RunCtx` is the one
/// real implementor.
#[async_trait::async_trait]
pub trait SessionObserver: Sync {
    /// Appends one session audit event to the run's log. The storage
    /// cause travels back on failure so the dispatch fails the node
    /// rather than dropping the event — a lost audit event thins the
    /// trail `status` and replay read.
    async fn emit_session_event(
        &self,
        node_id: &yunta_core::NodeId,
        payload: EventPayload,
    ) -> Result<(), StorageError>;
    fn process_registry(&self) -> Option<&crate::process_registry::ProcessRegistry>;
}

/// How [`dispatch_session`] failed: the adapter refused, or a session
/// audit event could not be appended. The two have different owners —
/// the adapter boundary versus the run's own storage — so callers route
/// each to its own node/run failure.
#[derive(Debug, Error)]
pub enum DispatchError {
    #[error(transparent)]
    Adapter(#[from] AdapterError),
    #[error("failed to append a session audit event")]
    Audit(#[source] StorageError),
}

/// The only shape of a note the log ever carries: its size and
/// a content-hash prefix — enough to audit a claimed note against,
/// never enough to reconstruct or leak it.
fn note_summary(text: &str) -> String {
    let hash = yunta_core::sha256_hex(text.as_bytes()).to_string();
    format!("{} bytes, sha256 {}", text.len(), &hash[..12])
}

/// Grace period between `interrupt` and the follow-up `kill` once a
/// budget has been exceeded — long enough for a session that closes
/// cleanly on interrupt to actually do so, short enough that a session
/// that ignores it doesn't stall the attempt.
const INTERRUPT_GRACE_PERIOD: Duration = Duration::from_millis(200);

/// Spawns one session from `request`, drains it to a terminal outcome
/// and reports the tokens it consumed. Shared by the task cycle and by
/// prompt-node execution: the request differs, the enforcement
/// does not.
///
/// The adapter passes `request.budget` along if its CLI supports it,
/// but enforcement is the engine's job either way — this counts `Usage`
/// and races the wall-clock deadline independent of that, and cuts the
/// session with `interrupt` → grace → `kill` when either budget is
/// exceeded.
pub(crate) async fn dispatch_session(
    adapter: &dyn Adapter,
    request: SessionRequest,
    cancel: &CancellationToken,
    audit: Option<(&dyn SessionObserver, &yunta_core::NodeId)>,
    resume: Option<&yunta_core::SessionId>,
) -> Result<(DispatchOutcome, TokenUsage), DispatchError> {
    let budget = request.budget;
    let requested_agent = request.agent.clone();
    // `Some` continues an interrupted conversation instead of
    // opening a new one — the caller already verified the capability.
    let mut session = match resume {
        Some(session_id) => adapter.resume(session_id, request).await?,
        None => adapter.spawn(request).await?,
    };
    // On the map for a separate `yunta cancel` while it lives.
    let _pgid_registration = crate::process_registry::register(
        audit.and_then(|(observer, _)| observer.process_registry()),
        session.pgid(),
    );
    // Carries the timeout `Duration` alongside its computed `Instant` so
    // the timeout-exceeded branch can report it without re-deriving it
    // from `budget.timeout`.
    let deadline = budget
        .timeout
        .map(|timeout| (tokio::time::Instant::now() + timeout, timeout));
    let mut tokens = TokenUsage::default();
    let mut terminal = None;
    let mut cancelled = false;

    {
        let mut stream = session.events();
        loop {
            // A node outside a `join: any` race passes a token nothing
            // ever cancels, so that branch simply never wins for it —
            // same `select!` shape either way.
            let next = if let Some((deadline_at, timeout)) = deadline {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        cancelled = true;
                        None
                    }
                    result = tokio::time::timeout_at(deadline_at, stream.next()) => {
                        match result {
                            Ok(next) => next,
                            Err(_) => {
                                terminal = Some(DispatchOutcome::BudgetExceeded {
                                    reason: format!("exceeded timeout of {timeout:?}"),
                                });
                                break;
                            }
                        }
                    }
                }
            } else {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        cancelled = true;
                        None
                    }
                    next = stream.next() => next,
                }
            };
            if cancelled {
                break;
            }

            let Some(event) = next else { break };

            if let Some(outcome) = apply_agent_event(AgentEventCtx {
                event,
                adapter,
                requested_agent: &requested_agent,
                max_tokens: budget.max_tokens,
                audit,
                tokens: &mut tokens,
            })
            .await?
            {
                terminal = Some(outcome);
                break;
            }
        }
    } // the stream's borrow of `session` ends here — interrupt/kill need &mut self too.

    if cancelled || matches!(terminal, Some(DispatchOutcome::BudgetExceeded { .. })) {
        // Never leave anything running: ordered termination first,
        // then forceful — mock has nothing to distinguish them, but a
        // real adapter's session may still close cleanly on interrupt.
        let _ = session.interrupt().await;
        tokio::time::sleep(INTERRUPT_GRACE_PERIOD).await;
        let _ = session.kill().await;
    }

    if cancelled {
        return Ok((DispatchOutcome::Cancelled, tokens));
    }

    Ok((terminal.unwrap_or(DispatchOutcome::Crashed), tokens))
}

/// One streamed `AgentEvent` and the dispatch state [`apply_agent_event`]
/// folds it into — grouped so the read loop hands them over as a unit.
struct AgentEventCtx<'a> {
    event: AgentEvent,
    adapter: &'a dyn Adapter,
    requested_agent: &'a Option<yunta_core::AgentName>,
    max_tokens: Option<u64>,
    audit: Option<(&'a dyn SessionObserver, &'a yunta_core::NodeId)>,
    tokens: &'a mut TokenUsage,
}

/// Appends one streamed event to the session's audit trail
/// (`agent_session_opened`/`agent_message`, emitted as the stream arrives so
/// a concurrent `status` sees the live session) and folds a `Usage` event
/// into the running token total. Returns the terminal outcome that ends the
/// stream — `Completed`, `Failed`, or a budget stop — or `None` to keep
/// reading. A failed audit append is never swallowed: its storage cause
/// ends the dispatch, so the node fails with the cause rather than the trail
/// losing an event nobody can recover.
async fn apply_agent_event(
    ctx: AgentEventCtx<'_>,
) -> Result<Option<DispatchOutcome>, DispatchError> {
    let AgentEventCtx {
        event,
        adapter,
        requested_agent,
        max_tokens,
        audit,
        tokens,
    } = ctx;
    match event {
        AgentEvent::SessionOpened { session_id, model } => {
            emit_audit(
                audit,
                EventPayload::AgentSessionOpened(yunta_core::events::AgentSessionOpenedPayload {
                    session_id,
                    agent: requested_agent.clone(),
                    model,
                    capabilities: adapter.capabilities(),
                }),
            )
            .await
            .map_err(DispatchError::Audit)?;
        }
        AgentEvent::ToolUse {
            name,
            target_digest,
        } => {
            emit_audit(
                audit,
                EventPayload::AgentMessage(yunta_core::events::AgentMessagePayload {
                    message_type: yunta_core::events::AgentMessageType::ToolUse,
                    tool_name: Some(name),
                    target_digest: Some(target_digest),
                    input_tokens: None,
                    output_tokens: None,
                    cached_input_tokens: None,
                    text: None,
                }),
            )
            .await
            .map_err(DispatchError::Audit)?;
        }
        AgentEvent::Note { text } => {
            emit_audit(
                audit,
                EventPayload::AgentMessage(yunta_core::events::AgentMessagePayload {
                    message_type: yunta_core::events::AgentMessageType::Note,
                    tool_name: None,
                    target_digest: None,
                    input_tokens: None,
                    output_tokens: None,
                    cached_input_tokens: None,
                    // A mechanical size+digest summary, never the content —
                    // the log must not be able to carry a secret the note
                    // contained.
                    text: Some(note_summary(&text)),
                }),
            )
            .await
            .map_err(DispatchError::Audit)?;
        }
        AgentEvent::Usage {
            input_tokens,
            output_tokens,
            cached_input_tokens,
        } => {
            emit_audit(
                audit,
                EventPayload::AgentMessage(yunta_core::events::AgentMessagePayload {
                    message_type: yunta_core::events::AgentMessageType::Usage,
                    tool_name: None,
                    target_digest: None,
                    input_tokens: Some(input_tokens),
                    output_tokens: Some(output_tokens),
                    cached_input_tokens,
                    text: None,
                }),
            )
            .await
            .map_err(DispatchError::Audit)?;
            tokens.input += input_tokens;
            tokens.output += output_tokens;
            if let Some(cached) = cached_input_tokens {
                tokens.cached = Some(tokens.cached.unwrap_or(0) + cached);
            }
            let tokens_used = tokens.total();
            if let Some(max_tokens) = max_tokens {
                if tokens_used > max_tokens {
                    return Ok(Some(DispatchOutcome::BudgetExceeded {
                        reason: format!("exceeded max_tokens {max_tokens} ({tokens_used} used)"),
                    }));
                }
            }
        }
        AgentEvent::Completed {
            result: AgentOutcome { summary },
        } => return Ok(Some(DispatchOutcome::Completed { summary })),
        AgentEvent::Failed { error, retryable } => {
            return Ok(Some(DispatchOutcome::Failed {
                message: error.message,
                retryable,
            }))
        }
    }
    Ok(None)
}

/// Appends one event to the session's audit trail, or does nothing when the
/// dispatch runs without an observer (a standalone `run_task` in a test).
async fn emit_audit(
    audit: Option<(&dyn SessionObserver, &yunta_core::NodeId)>,
    payload: EventPayload,
) -> Result<(), StorageError> {
    match audit {
        Some((observer, node_id)) => observer.emit_session_event(node_id, payload).await,
        None => Ok(()),
    }
}
