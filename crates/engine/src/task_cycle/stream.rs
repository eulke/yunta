//! One session's stream, folded into the log as it arrives.
//!
//! What an adapter reports while a session runs reaches the audit trail
//! here — appended as the stream carries it, so a concurrent `status`
//! sees the live session rather than a summary written at the end. The
//! terminal outcome the fold returns is what ends the read loop.

use yunta_core::events::{
    EventPayload, FailureCause, RefusalCause, RunToolFailedPayload, RunToolFailureCause,
    SessionEvent, TokenUsage,
};
use yunta_core::port::{Adapter, AgentEvent, AgentOutcome};
use yunta_storage::StorageError;

use super::dispatch::DispatchError;
use super::session::{note_summary, SessionObserver};
use super::DispatchOutcome;
use crate::run_tools::Arrivals;

/// Records that a session's run tools never reached it: one that was
/// offered its tools, opened, and never listed them or called one, which
/// both CLIs do before their first turn.
pub(crate) async fn record_unlisted_tools(
    audit: Option<(&dyn SessionObserver, &yunta_core::NodeId)>,
    adapter: &yunta_core::AdapterId,
    run_tools: Option<&crate::run_tools::RunToolsSession>,
    dispatched: &super::Dispatched,
) -> Result<(), yunta_storage::StorageError> {
    let unlisted = run_tools.is_some_and(|session| !session.listed());
    if unlisted && !dispatched.sessions.is_empty() {
        emit_audit(audit, crate::run_tools::run_tools_unreachable(adapter)).await?;
    }
    Ok(())
}

/// The writes the sessions a dispatch opened were refused, as the log holds
/// them. Read only while a criterion of the attempt is red, since a refused
/// write explains only a failure; a dispatch with no observer has no log to
/// read, so it reads none.
pub(crate) async fn read_refusals(
    audit: Option<(&dyn SessionObserver, &yunta_core::NodeId)>,
    sessions: &[yunta_core::SessionId],
    post: &[super::CriterionRun],
) -> Result<Vec<yunta_core::events::ToolTarget>, StorageError> {
    let red = post
        .iter()
        .any(|run| run.exit_code != 0 && run.could_not_run().is_none());
    match audit.filter(|_| red) {
        Some((observer, _)) => observer.refused_writes(sessions).await,
        None => Ok(Vec::new()),
    }
}

/// One streamed `AgentEvent` and the dispatch state [`apply_agent_event`]
/// folds it into — grouped so the read loop hands them over as a unit.
pub(super) struct AgentEventCtx<'a> {
    pub(super) event: AgentEvent,
    pub(super) adapter: &'a dyn Adapter,
    pub(super) requested_agent: &'a Option<yunta_core::AgentName>,
    pub(super) requested_effort: Option<yunta_core::Effort>,
    pub(super) max_tokens: Option<u64>,
    pub(super) audit: Option<(&'a dyn SessionObserver, &'a yunta_core::NodeId)>,
    pub(super) tokens: &'a mut TokenUsage,
    /// The session the stream opened, once it has. A run tool failure
    /// before that belongs to no session and is not recorded.
    pub(super) opened: &'a mut Option<yunta_core::SessionId>,
    /// How much of this session's writes its adapter's fence covered,
    /// as the opening reported it.
    pub(super) fence: &'a mut Option<yunta_core::fence::Coverage>,
    /// Set when the session failed because it lost its service.
    pub(super) cut_off: &'a mut bool,
    /// The task this session works, when a loop opened it.
    pub(super) task: Option<&'a yunta_core::TaskId>,
    /// The session this one resumes, when it does.
    pub(super) continues: Option<&'a yunta_core::SessionId>,
    /// The calls the session's run tools took in, which say whether a
    /// rejection the adapter reports reached the server.
    pub(super) arrivals: &'a Arrivals,
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
        requested_effort,
        max_tokens,
        audit,
        tokens,
        opened,
        fence: covered,
        cut_off,
        task,
        continues,
        arrivals,
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
                        effort: requested_effort,
                        capabilities: adapter.capabilities(),
                        fence,
                        task_id: task.cloned(),
                        continues: continues.cloned(),
                    },
                )),
            )
            .await
            .map_err(DispatchError::Audit)?;
        }
        AgentEvent::RunToolRejected { tool } => {
            // A rejection of a call the server received is a failed call. One
            // the server never received was blocked before it got there.
            let cause = match arrivals.take_arrival(tool) {
                true => RunToolFailureCause::CallFailed,
                false => RunToolFailureCause::ApprovalBlocked,
            };
            if let Some(session_id) = opened.clone() {
                emit_audit(
                    audit,
                    EventPayload::Session(SessionEvent::RunToolFailed(RunToolFailedPayload {
                        session_id,
                        tool,
                        cause,
                    })),
                )
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
        AgentEvent::Failed { error, cause } => {
            *cut_off = cause == FailureCause::Refused(RefusalCause::Unreachable);
            // The error travels whole: what the adapter caught under its
            // message is read where the failure is reported, or it is lost.
            return Ok(Some(DispatchOutcome::Failed { error, cause }));
        }
    }
    Ok(None)
}

/// Appends one event to the session's audit trail, or does nothing when the
/// dispatch runs without an observer (a standalone `run_task` in a test).
pub(super) async fn emit_audit(
    audit: Option<(&dyn SessionObserver, &yunta_core::NodeId)>,
    payload: EventPayload,
) -> Result<(), StorageError> {
    match audit {
        Some((observer, node_id)) => observer.record(node_id, payload).await.map(|_| ()),
        None => Ok(()),
    }
}
