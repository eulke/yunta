//! One session's stream, folded into the log as it arrives.
//!
//! What an adapter reports while a session runs reaches the audit trail
//! here — appended as the stream carries it, so a concurrent `status`
//! sees the live session rather than a summary written at the end. The
//! terminal outcome the fold returns is what ends the read loop.

use yunta_core::events::{EventPayload, SessionEvent, TokenUsage};
use yunta_core::port::{Adapter, AgentEvent, AgentOutcome};
use yunta_storage::StorageError;

use super::session::{note_summary, DispatchError, SessionObserver};
use super::DispatchOutcome;

/// The server is up and the endpoint reached the session; what did not
/// survive is the client's own reading of the tool list, which neither
/// side can work around from here — every tool this node needs to hand
/// its documents over is simply absent. It goes on the log as the
/// session opens rather than at the close it dooms, so the cause sits
/// next to the moment it happened instead of one failed close away,
/// where the only visible symptom is a document nobody delivered.
/// The record of a session that opened holding none of the run tools
/// the engine gave it a server for.
fn run_tools_unreachable(adapter: &yunta_core::AdapterId) -> EventPayload {
    EventPayload::Session(SessionEvent::CapabilityDegraded(
        yunta_core::events::CapabilityDegradedPayload::new(
            yunta_core::Capability::RunTools,
            adapter.clone(),
            yunta_core::events::Policy::NoRunTools,
        ),
    ))
}
/// One streamed `AgentEvent` and the dispatch state [`apply_agent_event`]
/// folds it into — grouped so the read loop hands them over as a unit.
pub(super) struct AgentEventCtx<'a> {
    pub(super) event: AgentEvent,
    pub(super) adapter: &'a dyn Adapter,
    pub(super) requested_agent: &'a Option<yunta_core::AgentName>,
    /// Whether the engine gave this session a per-run tool server.
    pub(super) run_tools_offered: bool,
    pub(super) max_tokens: Option<u64>,
    pub(super) audit: Option<(&'a dyn SessionObserver, &'a yunta_core::NodeId)>,
    pub(super) tokens: &'a mut TokenUsage,
    /// The session the stream opened, once it has. A write refused
    /// before that belongs to no session and is not recorded.
    pub(super) opened: &'a mut Option<yunta_core::SessionId>,
    /// How much of this session's writes its adapter's fence covered,
    /// as the opening reported it.
    pub(super) fence: &'a mut Option<yunta_core::fence::Coverage>,
}

/// Appends one streamed event to the session's audit trail
/// (`agent_session_opened`/`agent_message`, emitted as the stream arrives so
/// a concurrent `status` sees the live session) and folds a `Usage` event
/// into the running token total. Returns the terminal outcome that ends the
/// stream — `Completed`, `Failed`, or a budget stop — or `None` to keep
/// reading. A failed audit append is never swallowed: its storage cause
/// ends the dispatch, so the node fails with the cause rather than the trail
/// losing an event nobody can recover.
pub(super) async fn apply_agent_event(
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
        opened,
        fence: covered,
    } = ctx;
    match event {
        AgentEvent::SessionOpened {
            session_id,
            model,
            fence,
        } => {
            // The session this attempt is running: what a write it
            // refuses is recorded against.
            *opened = Some(session_id.clone());
            covered.clone_from(&fence);
            emit_audit(
                audit,
                EventPayload::Session(SessionEvent::Opened(
                    yunta_core::events::AgentSessionOpenedPayload {
                        session_id,
                        agent: requested_agent.clone(),
                        model,
                        capabilities: adapter.capabilities(),
                        fence,
                    },
                )),
            )
            .await
            .map_err(DispatchError::Audit)?;
        }
        AgentEvent::WriteRefused { target } => {
            // A refusal belongs to the session that refused it. A CLI
            // that reported one before opening a session reported
            // something this adapter cannot place, and nothing is
            // recorded for it.
            if let Some(session_id) = opened.clone() {
                emit_audit(
                    audit,
                    EventPayload::Session(SessionEvent::WriteRefused(
                        yunta_core::events::WriteRefusedPayload::new(session_id, target),
                    )),
                )
                .await
                .map_err(DispatchError::Audit)?;
            }
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
