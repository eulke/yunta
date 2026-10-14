//! One session's stream, folded into the log as it arrives.
//!
//! What an adapter reports while a session runs reaches the audit trail
//! here — appended as the stream carries it, so a concurrent `status` sees
//! the live session rather than a summary written at the end. The terminal
//! outcome the fold returns is what ends the read loop.

use std::time::Duration;

use futures::stream::BoxStream;
use futures::StreamExt;
use tokio_util::sync::CancellationToken;
use yunta_core::events::{
    EventPayload, Failure, FailureCause, RefusalCause, RunToolFailedPayload, RunToolFailureCause,
    SessionEvent, Timed, TokenUsage,
};
use yunta_core::port::{Adapter, AgentEvent, AgentOutcome, AgentSession, Budget};
use yunta_storage::StorageError;

use super::dispatch::{DispatchError, Opening};
use super::session::{note_summary, SessionObserver};
use super::DispatchOutcome;
use crate::run_tools::{Arrivals, RunToolError, SessionRails};

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

/// One streamed `AgentEvent` folds into the dispatch state the read loop
/// shares: the session's terms, and the totals its events are recorded
/// against. Grouped so the read loop hands them over as a unit.
pub(super) struct AgentEventCtx<'a> {
    pub(super) adapter: &'a dyn Adapter,
    pub(super) requested_agent: &'a Option<yunta_core::AgentName>,
    pub(super) requested_effort: Option<yunta_core::Effort>,
    pub(super) max_tokens: Option<u64>,
    /// Whether the adapter counts the token cap while the session runs.
    pub(super) counts_cap_mid_session: bool,
    pub(super) audit: Option<(&'a dyn SessionObserver, &'a yunta_core::NodeId)>,
    pub(super) tokens: &'a mut TokenUsage,
    /// The session the stream opened, once it has. A run tool failure
    /// before that belongs to no session and is not recorded.
    pub(super) opened: &'a mut Option<yunta_core::SessionId>,
    /// How much of this session's writes its adapter's fence covered, as the
    /// opening reported it.
    pub(super) fence: &'a mut Option<yunta_core::fence::Coverage>,
    /// The confinement the session was opened under, which `fence` is read beside.
    pub(super) fence_confinement: &'a mut Option<yunta_core::events::ConfinementRecord>,
    /// Set when the session failed because it lost its service.
    pub(super) cut_off: &'a mut bool,
    /// The task this session works, when a loop opened it.
    pub(super) task: Option<&'a yunta_core::TaskId>,
    /// The session this one resumes, when it does.
    pub(super) continues: Option<&'a yunta_core::SessionId>,
    /// The calls the session's run tools took in, which say whether a
    /// rejection the adapter reports reached the server.
    pub(super) arrivals: &'a Arrivals,
    /// The rail the session's CLI asks through, which hears the session the
    /// stream opens.
    pub(super) rails: Option<&'a SessionRails>,
    /// What the session was opened to reach, recorded with the session.
    pub(super) confinement: yunta_core::events::ConfinementRecord,
}

/// What one session's stream is read against: the adapter its events come
/// from, where they are recorded, and the terms the session was opened on.
pub(super) struct Terms<'a> {
    pub(super) adapter: &'a dyn Adapter,
    pub(super) audit: Option<(&'a dyn SessionObserver, &'a yunta_core::NodeId)>,
    pub(super) agent: Option<yunta_core::AgentName>,
    pub(super) effort: Option<yunta_core::Effort>,
    pub(super) budget: Budget,
    pub(super) confinement: yunta_core::events::ConfinementRecord,
    /// Whether the adapter counts the token cap while the session runs, as
    /// `counts_tokens_mid_session` answers it once for the session.
    pub(super) counts_cap_mid_session: bool,
}

/// What a session's stream folds into while it is read.
#[derive(Default)]
pub(super) struct Totals {
    pub(super) tokens: TokenUsage,
    pub(super) opened: Option<yunta_core::SessionId>,
    /// How much of the session's writes its adapter's fence covered, as the
    /// opening reported it.
    pub(super) fence: Option<yunta_core::fence::Coverage>,
    /// The confinement the session was opened under, which `fence` is read beside.
    pub(super) confinement: Option<yunta_core::events::ConfinementRecord>,
    pub(super) cut_off: bool,
}

/// Reads the session's stream to its end: each event is folded into the log
/// and the totals as it arrives, until the stream ends, a stop it asked for
/// ends it, a budget cuts it off, or the attempt is cancelled. Returns the
/// outcome the stream stopped on, if it stopped on one, and whether the read
/// was cancelled.
pub(super) async fn read_stream(
    session: &mut dyn AgentSession,
    cancel: &CancellationToken,
    terms: &Terms<'_>,
    opening: Opening<'_>,
    continues: Option<&yunta_core::SessionId>,
    totals: &mut Totals,
) -> Result<(Option<DispatchOutcome>, bool), DispatchError> {
    // A session with no run tools has no call that reached a server.
    let no_run_tools = Arrivals::default();
    let deadline = terms
        .budget
        .timeout
        .map(|timeout| (tokio::time::Instant::now() + timeout, timeout));
    let mut ctx = AgentEventCtx {
        adapter: terms.adapter,
        requested_agent: &terms.agent,
        requested_effort: terms.effort,
        max_tokens: terms.budget.max_tokens,
        counts_cap_mid_session: terms.counts_cap_mid_session,
        audit: terms.audit,
        tokens: &mut totals.tokens,
        opened: &mut totals.opened,
        fence: &mut totals.fence,
        fence_confinement: &mut totals.confinement,
        cut_off: &mut totals.cut_off,
        task: opening.task,
        continues,
        arrivals: opening.arrivals.unwrap_or(&no_run_tools),
        rails: opening.rails,
        confinement: terms.confinement.clone(),
    };
    let mut stream = session.events();
    loop {
        let event = match next_event(&mut stream, cancel, deadline).await {
            Next::Event(event) => event,
            Next::Ended => return Ok((None, false)),
            Next::Cancelled => return Ok((None, true)),
            Next::TimedOut(timeout) => return Ok((Some(timed_out(timeout)), false)),
        };
        if let Some(outcome) = apply_agent_event(&mut ctx, event).await? {
            return Ok((Some(outcome), false));
        }
    }
}

/// What a read waits on for its next event, or how the wait ended when no
/// event came.
enum Next {
    Event(AgentEvent),
    Ended,
    TimedOut(Duration),
    Cancelled,
}

/// Waits for the stream's next event, or for the attempt to be cancelled or
/// the session's deadline to pass, whichever comes first.
async fn next_event(
    stream: &mut BoxStream<'_, AgentEvent>,
    cancel: &CancellationToken,
    deadline: Option<(tokio::time::Instant, Duration)>,
) -> Next {
    // A node outside a `join: any` race passes a token nothing ever
    // cancels, so the cancel branch simply never wins for it, under either
    // of the two selects below.
    match deadline {
        Some((deadline_at, timeout)) => tokio::select! {
            _ = cancel.cancelled() => Next::Cancelled,
            result = tokio::time::timeout_at(deadline_at, stream.next()) => match result {
                Ok(Some(event)) => Next::Event(event),
                Ok(None) => Next::Ended,
                Err(_) => Next::TimedOut(timeout),
            },
        },
        None => tokio::select! {
            _ = cancel.cancelled() => Next::Cancelled,
            next = stream.next() => next.map_or(Next::Ended, Next::Event),
        },
    }
}

/// The stop a session that ran past its timeout earns: a budget stop naming
/// the timeout it ran past.
fn timed_out(timeout: Duration) -> DispatchOutcome {
    DispatchOutcome::BudgetExceeded {
        cause: Failure::TimedOut {
            after_ms: yunta_core::units::whole_millis(timeout),
            timed: Timed::Session,
        },
    }
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
    ctx: &mut AgentEventCtx<'_>,
    event: AgentEvent,
) -> Result<Option<DispatchOutcome>, DispatchError> {
    match event {
        AgentEvent::SessionOpened {
            session_id,
            model,
            fence,
        } => session_opened(ctx, session_id, model, fence).await?,
        AgentEvent::RunToolRejected { tool } => rejected_tool(ctx, tool).await?,
        AgentEvent::ToolUse { name, target } => tool_use(ctx, name, target).await?,
        AgentEvent::Note { text } => note(ctx, text).await?,
        AgentEvent::Usage {
            input_tokens,
            output_tokens,
            cached_input_tokens,
        } => return usage(ctx, input_tokens, output_tokens, cached_input_tokens).await,
        AgentEvent::Completed {
            result: AgentOutcome { summary },
        } => return Ok(Some(completed(ctx, summary))),
        AgentEvent::Failed { error, cause } => {
            *ctx.cut_off = cause == FailureCause::Refused(RefusalCause::Unreachable);
            // The error travels whole: what the adapter caught under its
            // message is read where the failure is reported, or it is lost.
            return Ok(Some(DispatchOutcome::Failed { error, cause }));
        }
    }
    Ok(None)
}

/// Records a session's opening: the session it is, the coverage its fence
/// reports, and the confinement it was opened under.
async fn session_opened(
    ctx: &mut AgentEventCtx<'_>,
    session_id: yunta_core::SessionId,
    model: Option<yunta_core::ModelName>,
    fence: Option<yunta_core::fence::Coverage>,
) -> Result<(), DispatchError> {
    // The session this attempt is running: what a write it refuses is
    // recorded against.
    *ctx.opened = Some(session_id.clone());
    ctx.fence.clone_from(&fence);
    *ctx.fence_confinement = Some(ctx.confinement.clone());
    // The rail answers for the session this stream opened last.
    if let Some(rails) = ctx.rails {
        rails.own(session_id.clone());
    }
    let payload = yunta_core::events::AgentSessionOpenedPayload {
        session_id,
        agent: ctx.requested_agent.clone(),
        model,
        effort: ctx.requested_effort,
        capabilities: ctx.adapter.capabilities(),
        fence,
        task_id: ctx.task.cloned(),
        continues: ctx.continues.cloned(),
        confinement: Some(ctx.confinement.clone()),
    };
    emit_audit(
        ctx.audit,
        EventPayload::Session(SessionEvent::Opened(payload)),
    )
    .await
    .map_err(DispatchError::Audit)
}

/// Records a run tool the CLI rejected: as a failed call when the server
/// received it, as a blocked one when it did not. Nothing is recorded before
/// the session opens, since no session holds the tool before that.
async fn rejected_tool(
    ctx: &mut AgentEventCtx<'_>,
    tool: yunta_core::RunTool,
) -> Result<(), DispatchError> {
    // A rejection of a call the server received is a failed call. One
    // the server never received was blocked before it got there.
    let cause = match ctx.arrivals.take_arrival(tool) {
        true => RunToolFailureCause::CallFailed,
        false => RunToolFailureCause::ApprovalBlocked,
    };
    if let Some(session_id) = ctx.opened.clone() {
        emit_audit(
            ctx.audit,
            EventPayload::Session(SessionEvent::RunToolFailed(RunToolFailedPayload {
                session_id,
                tool,
                cause,
            })),
        )
        .await
        .map_err(DispatchError::Audit)?;
    }
    Ok(())
}

/// Records a tool the session called and what the call acted on.
async fn tool_use(
    ctx: &AgentEventCtx<'_>,
    name: String,
    target: yunta_core::events::ToolTarget,
) -> Result<(), DispatchError> {
    emit_audit(
        ctx.audit,
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
    .map_err(DispatchError::Audit)
}

/// Records a note the session wrote, as its size and digest and never its
/// content.
async fn note(ctx: &AgentEventCtx<'_>, text: String) -> Result<(), DispatchError> {
    emit_audit(
        ctx.audit,
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
    .map_err(DispatchError::Audit)
}

/// Records what the CLI reported it spent and folds it into the running
/// total. When the adapter counts the cap mid-session, a total past the cap
/// ends the stream here.
async fn usage(
    ctx: &mut AgentEventCtx<'_>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cached_input_tokens: Option<u64>,
) -> Result<Option<DispatchOutcome>, DispatchError> {
    emit_audit(
        ctx.audit,
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
    // A count the CLI did not report adds nothing: a session that said
    // nothing about its input has not said zero.
    ctx.tokens.input += input_tokens.unwrap_or(0);
    ctx.tokens.output += output_tokens.unwrap_or(0);
    if let Some(cached) = cached_input_tokens {
        ctx.tokens.cached = Some(ctx.tokens.cached.unwrap_or(0) + cached);
    }
    if !ctx.counts_cap_mid_session {
        return Ok(None);
    }
    Ok(over_cap(ctx.max_tokens, ctx.tokens.total()))
}

/// What a session that completed its turn ends on: `Completed`, unless the
/// adapter does not count the cap mid-session and the session spent past it.
fn completed(ctx: &AgentEventCtx<'_>, summary: String) -> DispatchOutcome {
    if ctx.counts_cap_mid_session {
        return DispatchOutcome::Completed { summary };
    }
    over_cap(ctx.max_tokens, ctx.tokens.total()).unwrap_or(DispatchOutcome::Completed { summary })
}

/// The stop a token total earns against the cap: `BudgetExceeded` once the
/// total is above it. Nothing while the total is within the cap or no cap is
/// set. Both places the cap is read give this one reason.
fn over_cap(max_tokens: Option<u64>, used: u64) -> Option<DispatchOutcome> {
    let max = max_tokens.filter(|max| used > *max)?;
    Some(DispatchOutcome::BudgetExceeded {
        cause: Failure::TokenBudget { used, max },
    })
}

/// Whether the adapter counts the token cap while the session runs, so the
/// stream stops the session the moment the total passes the cap. An adapter
/// that does not is read once, when the session closes.
pub(super) fn counts_tokens_mid_session(adapter: &dyn Adapter) -> bool {
    adapter
        .capabilities()
        .declares(yunta_core::Capability::MidSessionTokenCap)
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

/// Writes the summary of the calls a session's rail answered, once the session
/// has ended. A summary the log does not take is kept with the rail's
/// unrecorded error, since the session's outcome is decided already.
pub(super) async fn record_rail_summary(terms: &Terms<'_>, rails: Option<&SessionRails>) {
    let Some(rails) = rails else {
        return;
    };
    let Some(summary) = rails.take_summary() else {
        return;
    };
    let payload = EventPayload::Session(SessionEvent::RailSummary(summary));
    if let Err(error) = emit_audit(terms.audit, payload).await {
        rails.left_unrecorded(RunToolError::from(error));
    }
}
