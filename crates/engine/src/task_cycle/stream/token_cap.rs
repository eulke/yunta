//! What a session spent, against its token cap.
//!
//! The cap is read in one of two places, by what the adapter declares: as
//! each usage arrives, when it counts the cap mid-session, or once when the
//! session completes its turn. Both give the one stop [`over_cap`] names.

use yunta_core::events::{
    AgentMessagePayload, AgentMessageType, EventPayload, Failure, SessionEvent,
};

use super::{emit_audit, AgentEventCtx};
use crate::task_cycle::dispatch::DispatchError;
use crate::task_cycle::DispatchOutcome;

/// Records what the CLI reported it spent and folds it into the running
/// total. When the adapter counts the cap mid-session, a total past the cap
/// ends the stream here.
pub(super) async fn usage(
    ctx: &mut AgentEventCtx<'_>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cached_input_tokens: Option<u64>,
) -> Result<Option<DispatchOutcome>, DispatchError> {
    emit_audit(
        ctx.audit,
        EventPayload::Session(SessionEvent::Message(AgentMessagePayload {
            message_type: AgentMessageType::Usage,
            tool_name: None,
            action: None,
            target: None,
            input_tokens,
            output_tokens,
            cached_input_tokens,
            note: None,
            text: None,
            session: None,
        })),
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
pub(super) fn completed(ctx: &AgentEventCtx<'_>, summary: String) -> DispatchOutcome {
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
