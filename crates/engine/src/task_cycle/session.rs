//! One agent session for a task: opening it, folding its events into
//! the log as they arrive, and closing it when the attempt ends or is
//! interrupted.

use std::path::PathBuf;
use std::time::Duration;

use futures::StreamExt;
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use yunta_core::events::{EventPayload, TokenUsage};
use yunta_core::port::{Adapter, AgentEvent, AgentOutcome, SessionRequest};
use yunta_core::AdapterError;
use yunta_storage::StorageError;

use super::DispatchOutcome;
use yunta_core::events::SessionEvent;

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
    /// The runner this node resolved to: every task session of the node
    /// runs on its model and, when it names one, its agent. The runner
    /// is resolved once for the whole loop, so no attempt can drift onto
    /// another model than the one the log recorded.
    pub chosen: yunta_core::RunnerCandidate,
    /// Where the files this node declares belong, when it declares any.
    /// A task session writes the loop node's own artifacts, so it is
    /// told the same directory the node closes on.
    pub artifact_dir: Option<PathBuf>,
    /// What makes this node's run tools mandatory rather than an offer:
    /// a `coordination: blackboard` group whose semantics the engine
    /// never emulates, or an interpreted artifact that has no other way
    /// in. A listener that fails to bind for such a node fails the node;
    /// for any other node it degrades and the session runs on.
    pub run_tools_required: Option<RunToolsNeed>,
}

/// Why a node cannot proceed without the run tools.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunToolsNeed {
    /// The node is in a `coordination: blackboard` group.
    Blackboard,
    /// The node declares an interpreted artifact a session hands over
    /// through the tools.
    TypedArtifact(yunta_core::ArtifactKind),
}

impl SessionSetup {
    /// A setup that carries nothing but the node its sessions belong
    /// to and the runner they run on: no skills, no settings, no
    /// secrets, no per-run tools and no declared files.
    pub fn bare(node: yunta_core::NodeId, chosen: yunta_core::RunnerCandidate) -> Self {
        Self {
            skills: Vec::new(),
            adapter_settings: serde_json::Map::new(),
            env: std::collections::HashMap::new(),
            run_tools: None,
            run_dir: PathBuf::new(),
            node,
            chosen,
            artifact_dir: None,
            run_tools_required: None,
        }
    }

    /// The env a session may see: the names the config declares, bound
    /// to whatever `source` has for them. A name nothing binds simply
    /// does not reach the session — a secret the run cannot produce is
    /// absent, never empty.
    pub fn secrets_env(
        config: &yunta_core::ConfigLayer,
        source: Option<&dyn yunta_core::SecretSource>,
    ) -> std::collections::HashMap<String, yunta_core::Secret<String>> {
        let Some(source) = source else {
            return std::collections::HashMap::new();
        };
        config
            .secrets
            .iter()
            .filter_map(|name| source.get(name).map(|value| (name.clone(), value)))
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

/// The record of a session that opened holding none of the run tools
/// the engine gave it a server for.
///
/// The server is up and the endpoint reached the session; what did not
/// survive is the client's own reading of the tool list, which neither
/// side can work around from here — every tool this node needs to hand
/// its documents over is simply absent. It goes on the log as the
/// session opens rather than at the close it dooms, so the cause sits
/// next to the moment it happened instead of one failed close away,
/// where the only visible symptom is a document nobody delivered.
fn run_tools_unreachable(adapter: &yunta_core::AdapterId) -> EventPayload {
    EventPayload::Session(SessionEvent::CapabilityDegraded(
        yunta_core::events::CapabilityDegradedPayload::new(
            yunta_core::Capability::RunTools,
            adapter.clone(),
            yunta_core::events::Policy::NoRunTools,
        ),
    ))
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
    // Whether this session was handed a per-run tool server at all: a
    // session that holds none of those tools only means something went
    // wrong if it was given a server to hold them from.
    let run_tools_offered = request.run_tools_endpoint.is_some();
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
                run_tools_offered,
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
    /// Whether the engine gave this session a per-run tool server.
    run_tools_offered: bool,
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
        run_tools_offered,
        max_tokens,
        audit,
        tokens,
    } = ctx;
    match event {
        AgentEvent::SessionOpened { session_id, model } => {
            emit_audit(
                audit,
                EventPayload::Session(SessionEvent::Opened(
                    yunta_core::events::AgentSessionOpenedPayload {
                        session_id,
                        agent: requested_agent.clone(),
                        model,
                        capabilities: adapter.capabilities(),
                    },
                )),
            )
            .await
            .map_err(DispatchError::Audit)?;
        }
        AgentEvent::RunToolsMounted { count } => {
            if run_tools_offered && count == 0 {
                emit_audit(audit, run_tools_unreachable(adapter.id()))
                    .await
                    .map_err(DispatchError::Audit)?;
            }
        }
        AgentEvent::ToolUse { name, target } => {
            emit_audit(
                audit,
                EventPayload::Session(SessionEvent::Message(
                    yunta_core::events::AgentMessagePayload {
                        message_type: yunta_core::events::AgentMessageType::ToolUse,
                        tool_name: Some(name),
                        target: Some(target),
                        input_tokens: None,
                        output_tokens: None,
                        cached_input_tokens: None,
                        text: None,
                    },
                )),
            )
            .await
            .map_err(DispatchError::Audit)?;
        }
        AgentEvent::Note { text } => {
            emit_audit(
                audit,
                EventPayload::Session(SessionEvent::Message(
                    yunta_core::events::AgentMessagePayload {
                        message_type: yunta_core::events::AgentMessageType::Note,
                        tool_name: None,
                        target: None,
                        input_tokens: None,
                        output_tokens: None,
                        cached_input_tokens: None,
                        // A mechanical size+digest summary, never the content —
                        // the log must not be able to carry a secret the note
                        // contained.
                        text: Some(note_summary(&text)),
                    },
                )),
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
                EventPayload::Session(SessionEvent::Message(
                    yunta_core::events::AgentMessagePayload {
                        message_type: yunta_core::events::AgentMessageType::Usage,
                        tool_name: None,
                        target: None,
                        input_tokens,
                        output_tokens,
                        cached_input_tokens,
                        text: None,
                    },
                )),
            )
            .await
            .map_err(DispatchError::Audit)?;
            // A count the CLI did not report adds nothing: a session
            // that said nothing about its input has not said zero.
            tokens.input += input_tokens.unwrap_or(0);
            tokens.output += output_tokens.unwrap_or(0);
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
                // Whatever the adapter caught travels with the sentence
                // it states: the cause is read here or it is lost.
                message: yunta_core::describe(&error),
                retryable,
            }));
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
