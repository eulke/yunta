//! Run-level token budget (§8.3, DI-05): `limits.max_tokens_per_run`
//! against the log-derived `total_tokens`, checked before every
//! token-spending scheduler step. Exhaustion is never a silent pause —
//! it's a §5.3 escalation (`continue`/`abort`) first, and only degrades
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

/// Escalates an exhausted run budget (§5.3). A resolution — either way —
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
            ctx.emit(None, EventPayload::GateWaiting(escalation))?;
            ctx.emit(None, EventPayload::GateResolved(resolution))?;
            if chosen.as_deref() == Some("continue") {
                Ok(BudgetDecision::Continue)
            } else {
                Ok(BudgetDecision::Pause { reason })
            }
        }
        None => Ok(BudgetDecision::Pause { reason }),
    }
}

/// The §5.3 object: mechanical summary and evidence, options with their
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

/// `input + output` — cached reads are informational (a subset of
/// input), never double-counted.
pub fn tokens_spent(totals: yunta_core::events::TokenUsage) -> u64 {
    totals.input + totals.output
}

/// §8.3/T3.3 session-budget policy (DI-05), pure: one agent session may
/// spend at most an equal share of the run cap, bounded by what actually
/// remains — `min(remaining, cap / non_terminal_nodes)`. Deliberately
/// simple: it never predicts which nodes are cheap, it only guarantees
/// no single session can spend the whole run's remaining budget when
/// other nodes still have work coming.
pub fn session_token_budget(cap: u64, spent: u64, non_terminal_nodes: usize) -> u64 {
    let remaining = cap.saturating_sub(spent);
    remaining.min(cap / non_terminal_nodes.max(1) as u64)
}
