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

use yunta_core::events::{EventPayload, GateOption, GateResolvedPayload, GateWaitingPayload};

use super::{RunCtx, RunError};
use crate::reserved::ReservedOption;

/// What the invocation does after the human (or their absence) weighs in.
pub enum BudgetDecision {
    /// The cap is lifted for the rest of *this invocation* only.
    Continue,
    /// Pause with this reason — chosen `abort`, or no surface to ask.
    Pause { reason: String },
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
    escalation: GateWaitingPayload,
    pause_reason: String,
) -> Result<BudgetDecision, RunError> {
    match ctx.ask_human(&escalation).await? {
        Some(choice) => {
            let continues = ReservedOption::of(&choice.option) == Some(ReservedOption::Continue);
            ctx.emit(node_id, EventPayload::GateWaiting(escalation))
                .await?;
            ctx.emit(
                node_id,
                EventPayload::GateResolved(GateResolvedPayload::Chosen(choice)),
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
) -> (GateWaitingPayload, String) {
    let escalation = GateWaitingPayload {
        summary: format!(
            "run `{}` exhausted its token budget: {spent} of {cap} tokens spent",
            ctx.run_id
        ),
        evidence: format!(
            "limits.max_tokens_per_run: {cap}; total input+output tokens derived \
             from the event log: {spent}"
        ),
        options: vec![
            GateOption {
                id: ReservedOption::Continue.id(),
                label: "Continue past the cap".to_string(),
                tradeoff: "Lifts the cap for this invocation only; a later resume \
                           will ask again before spending more"
                    .to_string(),
            },
            GateOption {
                id: ReservedOption::Abort.id(),
                label: "Pause the run".to_string(),
                tradeoff: "The run pauses with reason `budget`; a resume re-asks".to_string(),
            },
        ],
        external_ref: None,
    };
    let reason = format!(
        "budget: run spent {spent} tokens with `limits.max_tokens_per_run: {cap}` — \
         resume with an interactive surface to continue past the cap or abort"
    );
    (escalation, reason)
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
) -> (GateWaitingPayload, String) {
    let escalation = GateWaitingPayload {
        summary: format!(
            "loop `{node_id}` needs iteration {iteration} but `limits.max_loop_iterations` \
             is {cap}"
        ),
        evidence: format!(
            "iterations already run this invocation: {}; limits.max_loop_iterations: {cap}; \
             the tasks document still has ready tasks",
            iteration - 1
        ),
        options: vec![
            GateOption {
                id: ReservedOption::Continue.id(),
                label: "Keep iterating".to_string(),
                tradeoff: "Lifts the cap for this invocation only; a later resume \
                           will ask again"
                    .to_string(),
            },
            GateOption {
                id: ReservedOption::Abort.id(),
                label: "Fail the loop node".to_string(),
                tradeoff: "The node fails naming the limit and the run pauses; a \
                           resume re-runs the loop and re-asks"
                    .to_string(),
            },
        ],
        external_ref: None,
    };
    let reason = format!(
        "loop `{node_id}` exceeded `limits.max_loop_iterations` ({cap}) — resume with an \
         interactive surface to continue past the cap or abort"
    );
    (escalation, reason)
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
