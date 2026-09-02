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

use yunta_core::events::{EventPayload, GateOption, GateWaitingPayload};

use super::{RunCtx, RunError};

/// What the invocation does after the human (or their absence) weighs in.
pub enum BudgetDecision {
    /// The cap is lifted for the rest of *this invocation* only.
    Continue,
    /// Pause with this reason — chosen `abort`, an unrecognized option,
    /// or no surface to ask.
    Pause { reason: String },
}

/// Escalates an exhausted run budget. A resolution — either way —
/// is recorded as a run-level `gate_waiting`/`gate_resolved` pair; no
/// resolution records nothing, so a resume re-asks (the same convention
/// every node gate follows).
pub async fn authorize_over_budget(
    ctx: &RunCtx<'_>,
    spent: u64,
    cap: u64,
) -> Result<BudgetDecision, RunError> {
    let reason = format!(
        "budget: run spent {spent} tokens with `limits.max_tokens_per_run: {cap}` — \
         resume with an interactive surface to continue past the cap or abort"
    );
    let escalation = escalation(ctx, spent, cap);
    match ctx.human_interaction.resolve(&escalation).await {
        Some(resolution) => {
            let chosen = resolution.chosen_option.clone();
            ctx.emit(None, EventPayload::GateWaiting(escalation))
                .await?;
            ctx.emit(None, EventPayload::GateResolved(resolution))
                .await?;
            if chosen.as_deref() == Some("continue") {
                Ok(BudgetDecision::Continue)
            } else {
                Ok(BudgetDecision::Pause { reason })
            }
        }
        None => Ok(BudgetDecision::Pause { reason }),
    }
}

/// The escalation object: mechanical summary and evidence, options with their
/// tradeoffs spelled out — never a bare "over budget, y/n?".
fn escalation(ctx: &RunCtx<'_>, spent: u64, cap: u64) -> GateWaitingPayload {
    GateWaitingPayload {
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
                id: "continue".to_string(),
                label: "Continue past the cap".to_string(),
                tradeoff: "Lifts the cap for this invocation only; a later resume \
                           will ask again before spending more"
                    .to_string(),
            },
            GateOption {
                id: "abort".to_string(),
                label: "Pause the run".to_string(),
                tradeoff: "The run pauses with reason `budget`; a resume re-asks".to_string(),
            },
        ],
        external_ref: None,
    }
}

/// Escalates a loop that hit `limits.max_loop_iterations` — the
/// same continue/abort mechanism as the token cap, and the only net under a
/// ledger whose state oscillates forever. Node-scoped, unlike the run
/// budget: the pair is recorded on the loop node as a synchronous internal
/// pair invisible to derived state, and the same per-invocation rule
/// applies — `Continue` lifts the cap only for the
/// `execute_loop` call that asked.
pub async fn authorize_loop_overrun(
    ctx: &RunCtx<'_>,
    node_id: &yunta_core::NodeId,
    iteration: u32,
    cap: u32,
) -> Result<BudgetDecision, RunError> {
    let reason = format!(
        "loop `{node_id}` exceeded `limits.max_loop_iterations` ({cap}) — resume with an \
         interactive surface to continue past the cap or abort"
    );
    let escalation = GateWaitingPayload {
        summary: format!(
            "loop `{node_id}` needs iteration {iteration} but `limits.max_loop_iterations` \
             is {cap}"
        ),
        evidence: format!(
            "iterations already run this invocation: {}; limits.max_loop_iterations: {cap}; \
             the ledger still has ready tasks",
            iteration - 1
        ),
        options: vec![
            GateOption {
                id: "continue".to_string(),
                label: "Keep iterating".to_string(),
                tradeoff: "Lifts the cap for this invocation only; a later resume \
                           will ask again"
                    .to_string(),
            },
            GateOption {
                id: "abort".to_string(),
                label: "Fail the loop node".to_string(),
                tradeoff: "The node fails naming the limit and the run pauses; a \
                           resume re-runs the loop and re-asks"
                    .to_string(),
            },
        ],
        external_ref: None,
    };
    match ctx.human_interaction.resolve(&escalation).await {
        Some(resolution) => {
            let chosen = resolution.chosen_option.clone();
            ctx.emit(Some(node_id), EventPayload::GateWaiting(escalation))
                .await?;
            ctx.emit(Some(node_id), EventPayload::GateResolved(resolution))
                .await?;
            if chosen.as_deref() == Some("continue") {
                Ok(BudgetDecision::Continue)
            } else {
                Ok(BudgetDecision::Pause { reason })
            }
        }
        None => Ok(BudgetDecision::Pause { reason }),
    }
}

/// `input + output` — cached reads are informational (a subset of
/// input), never double-counted.
pub fn tokens_spent(totals: yunta_core::events::TokenUsage) -> u64 {
    totals.input + totals.output
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
