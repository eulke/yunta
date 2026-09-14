//! Run-level token budget: `limits.max_tokens_per_run`
//! against the log-derived `total_tokens`, checked before every
//! token-spending scheduler step. Exhaustion is never a silent pause —
//! it's an escalation (`continue`/`abort`) first, and only degrades
//! to `run_paused { reason: budget… }` when no surface can answer.
//!
//! Authorization is **per invocation, in memory** — deliberately not
//! derived from the log: identifying which historical `gate_resolved`
//! was a budget escalation would take string-matching or a schema
//! marker, and semantically every new invocation spends new money. A
//! resume re-asks; the decision it gets is still audited in the log as
//! a run-level gate pair (`node_id: None`).

use yunta_core::events::{
    Escalation, EscalationError, EventPayload, Fact, GateResolvedPayload, PauseReason,
};
use yunta_core::NonEmpty;

use super::{RunCtx, RunError};
use crate::reserved::{offers, ReservedOption};
use yunta_core::events::GateEvent;

/// What the invocation does after the human (or their absence) weighs in.
pub enum BudgetDecision {
    /// The cap is lifted for the rest of *this invocation* only.
    Continue,
    /// Pause with this reason — chosen `abort`, or no surface to ask.
    Pause { reason: PauseReason },
}

/// The one escalation flow every exhausted limit shares: it puts the
/// caller's escalation object to the run's human-interaction surface and
/// records a `gate_waiting`/`gate_resolved` pair only once actually resolved
/// (no resolution records nothing, so a resume re-asks, the convention every
/// node gate follows). `Continue` lifts the cap for this invocation only;
/// `abort`, or no surface to ask, pauses with the caller's reason. `node_id` scopes the audited pair: the
/// run budget is run-level (`None`), a loop overrun is the loop node's.
pub async fn escalate(
    ctx: &RunCtx<'_>,
    node_id: Option<&yunta_core::NodeId>,
    escalation: Escalation,
    pause_reason: PauseReason,
) -> Result<BudgetDecision, RunError> {
    match ctx.ask_human(&escalation).await? {
        Some(choice) => {
            let continues = ReservedOption::of(&choice.option) == Some(ReservedOption::Continue);
            ctx.emit(
                node_id,
                EventPayload::Gates(GateEvent::Waiting(escalation.into_payload())),
            )
            .await?;
            ctx.emit(
                node_id,
                EventPayload::Gates(GateEvent::Resolved(GateResolvedPayload::Chosen(choice))),
            )
            .await?;
            if continues {
                Ok(BudgetDecision::Continue)
            } else {
                Ok(BudgetDecision::Pause {
                    reason: pause_reason,
                })
            }
        }
        None => Ok(BudgetDecision::Pause {
            reason: pause_reason,
        }),
    }
}

/// The escalation object and pause reason for an exhausted run token budget:
/// mechanical summary and evidence, options with their tradeoffs spelled out
/// — never a bare "over budget, y/n?".
pub fn over_budget_escalation(
    ctx: &RunCtx<'_>,
    spent: u64,
    cap: u64,
) -> Result<(Escalation, PauseReason), EscalationError> {
    let escalation = Escalation::new(
        format!("run `{}` exhausted its token budget", ctx.run_id),
        vec![
            Fact::labelled("limits.max_tokens_per_run", cap.to_string()),
            Fact::labelled(
                "total input+output tokens derived from the event log",
                spent.to_string(),
            ),
        ]
        .into(),
        NonEmpty::from((
            offers::continue_past_tokens(),
            vec![offers::abort_on_tokens()],
        )),
    )?;
    let reason = PauseReason::BudgetExhausted { spent, cap };
    Ok((escalation, reason))
}

/// The escalation object and pause reason for a loop that hit
/// `limits.max_loop_iterations` — the same continue/abort mechanism as the
/// token cap, and the only net under a tasks document whose state oscillates
/// forever. Node-scoped, unlike the run budget: [`escalate`] records its
/// pair on the loop node.
pub fn loop_overrun_escalation(
    node_id: &yunta_core::NodeId,
    iteration: u32,
    cap: u32,
) -> Result<(Escalation, PauseReason), EscalationError> {
    // The claim is that the loop wants another iteration and the cap
    // stops it; which cap and how many iterations is the record below,
    // stated once.
    let escalation = Escalation::new(
        format!("loop `{node_id}` needs another iteration and its cap is spent"),
        vec![
            Fact::labelled(
                "iterations already run this invocation",
                (iteration - 1).to_string(),
            ),
            Fact::labelled("limits.max_loop_iterations", cap.to_string()),
            Fact::bare("the tasks document still has ready tasks"),
        ]
        .into(),
        NonEmpty::from((
            offers::continue_past_iterations(),
            vec![offers::abort_on_iterations()],
        )),
    )?;
    let reason = PauseReason::LoopOverrun {
        node: node_id.clone(),
        cap,
    };
    Ok((escalation, reason))
}

/// Pure session-budget policy: one agent session may
/// spend at most an equal share of the run cap, bounded by what actually
/// remains — `min(remaining, cap / non_terminal_nodes)`. Deliberately
/// simple: it never predicts which nodes are cheap, it only guarantees
/// no single session can spend the whole run's remaining budget when
/// other nodes still have work coming.
pub fn session_token_budget(cap: u64, spent: u64, non_terminal_nodes: usize) -> u64 {
    let remaining = cap.saturating_sub(spent);
    remaining.min(cap / non_terminal_nodes.max(1) as u64)
}
