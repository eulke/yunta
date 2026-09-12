//! The human decisions a loop raises: the scope expansions its tasks
//! requested, put to the gate escalation and recorded as granted or
//! denied.

use yunta_core::events::{
    Decider, EventPayload, Fact, Finding, FindingPostedPayload, FindingSeverity,
    GateResolvedPayload, ProposedCriterionPrecheck, ScopeExpansionDeniedPayload,
    ScopeExpansionGrantedPayload, ScopeExpansionRequestedPayload, TaskStatus,
    TaskStatusChangedPayload, TokenUsage,
};
use yunta_core::{FindingId, Node};

use crate::reserved::{offers, ReservedOption};
use crate::run::node_close::fail_with_tokens;
use crate::run::node_exec::NodeEnd;
use crate::run::{RunCtx, RunError};

/// Resolves each escalated scope-expansion request through the run's human
/// interaction surface, once the whole batch is on the log: a grant or a
/// denial (with its finding) is recorded, and a `was_blocked` task returns to
/// `Pending` for its retry. Returns the node's end when any request goes
/// unresolved (no live surface) — the run pauses owing that decision.
pub(super) async fn resolve_escalations(
    ctx: &RunCtx<'_>,
    node: &Node,
    pending_escalations: Vec<PendingEscalation>,
    scope_expansion: Option<&yunta_core::ScopeExpansion>,
    expansions_granted_this_run: &mut u32,
    tokens: TokenUsage,
) -> Result<Option<NodeEnd>, RunError> {
    // The whole batch integrates before anything is asked (serial integration
    // order still holds — this only stops the *next* batch from being
    // dispatched while a decision is owed): a pending `ask`/exhausted-cap
    // request must reach a human before the run spends any further budget,
    // never be bypassed just because the task that raised it happened to
    // succeed on its own declared scope. With a live surface the human
    // decides right here; only what stays unresolved pauses the run.
    let mode = scope_expansion.map(|se| se.mode).unwrap_or_default();
    let max_per_run = scope_expansion.and_then(|se| se.max_per_run);
    let mut unresolved: Vec<yunta_core::TaskId> = Vec::new();
    for pending in pending_escalations {
        let escalation =
            expansion_escalation(&pending, mode, max_per_run, *expansions_granted_this_run);
        let Some(choice) = ctx.ask_human(&escalation).await? else {
            unresolved.push(pending.task_id);
            continue;
        };
        // Same convention as every other gate: waiting and resolved land
        // together, only once actually resolved — an unresolved question
        // re-asks on resume instead of remembering a decision nobody made.
        ctx.emit(Some(&node.id), EventPayload::GateWaiting(escalation))
            .await?;
        let resolved_seq = ctx
            .emit(
                Some(&node.id),
                EventPayload::GateResolved(GateResolvedPayload::Chosen(choice.clone())),
            )
            .await?;
        let decided_by = Decider::Person { id: choice.by };
        if ReservedOption::of(&choice.option) == Some(ReservedOption::Grant) {
            *expansions_granted_this_run += 1;
            ctx.emit(
                Some(&node.id),
                EventPayload::ScopeExpansionGranted(ScopeExpansionGrantedPayload {
                    task_id: pending.task_id.clone(),
                    decided_by,
                    mode,
                    count_this_run: *expansions_granted_this_run,
                    paths: pending.outcome.request.paths.clone(),
                }),
            )
            .await?;
        } else {
            // Anything that isn't an explicit grant denies — the conservative
            // reading of an ambiguous resolution, and every denial converts to
            // a finding, same as the rule-mode path.
            let reason = choice
                .free_text
                .unwrap_or_else(|| "denied by a human at the gate".to_string());
            ctx.emit(
                Some(&node.id),
                EventPayload::ScopeExpansionDenied(ScopeExpansionDeniedPayload {
                    task_id: pending.task_id.clone(),
                    decided_by,
                    mode,
                    count_this_run: *expansions_granted_this_run,
                    denial_reason: Some(reason.clone()),
                }),
            )
            .await?;
            ctx.emit(
                Some(&node.id),
                EventPayload::FindingPosted(FindingPostedPayload {
                    finding: Finding {
                        id: FindingId::try_from(format!(
                            "scope-expansion-{}-{}",
                            pending.task_id, pending.attempt_no
                        ))?,
                        severity: FindingSeverity::Minor,
                        title: format!("scope expansion denied for task `{}`", pending.task_id),
                        location: pending.outcome.request.paths.join(", "),
                        detail: format!(
                            "{reason} — agent's stated reason: {}",
                            pending.outcome.request.reason
                        ),
                        proposed_criterion: pending
                            .outcome
                            .request
                            .proposed_criterion
                            .clone()
                            .map(Into::into),
                    },
                }),
            )
            .await?;
        }
        // Granted or denied, the task gets its retry: with the widened scope
        // (from the log's own granted paths), or within the original one — a
        // denial never kills the task, it re-runs inside what was declared.
        if pending.was_blocked {
            ctx.emit(
                Some(&node.id),
                EventPayload::TaskStatusChanged(TaskStatusChangedPayload {
                    task_id: pending.task_id.clone(),
                    new_status: TaskStatus::Pending,
                    caused_by: resolved_seq,
                }),
            )
            .await?;
        }
    }
    if !unresolved.is_empty() {
        let mut diagnostic =
            "a scope expansion request needs a human decision before this run can continue"
                .to_string();
        for task_id in &unresolved {
            diagnostic.push_str(&format!(
                "; task `{task_id}` has a scope expansion request awaiting a human decision"
            ));
        }
        return Ok(Some(
            fail_with_tokens(ctx, node, diagnostic, false, tokens).await?,
        ));
    }
    Ok(None)
}

/// One `Escalate`d request waiting for the human's verdict.
pub(super) struct PendingEscalation {
    pub(super) task_id: yunta_core::TaskId,
    pub(super) attempt_no: u32,
    pub(super) outcome: crate::scope_expansion::ScopeExpansionOutcome,
    /// Whether the task ended `Blocked` on this escalation (vs. `Done`
    /// with the question still owed) — decides whether resolution sets
    /// it back to `Pending` for a retry.
    pub(super) was_blocked: bool,
}

/// Builds the escalation object for a scope-expansion request — the
/// engine assembles summary + mechanical evidence from what it already
/// verified (the request, the proposed criterion's own pre-check exit,
/// the cap state); the agent's only contribution is the reason it wrote
/// into the request itself.
fn expansion_escalation(
    pending: &PendingEscalation,
    mode: yunta_core::ScopeExpansionMode,
    max_per_run: Option<u32>,
    granted_so_far: u32,
) -> yunta_core::events::GateWaitingPayload {
    let request = &pending.outcome.request;
    let precheck = match pending.outcome.precheck_exit {
        Some(code) => format!("proposed criterion `{}` pre-check exit: {code}", {
            request
                .proposed_criterion
                .as_ref()
                .map(|c| c.cmd.as_str())
                .unwrap_or("?")
        }),
        None => "no criterion proposed".to_string(),
    };
    let cap = match max_per_run {
        Some(cap) => format!("{granted_so_far}/{cap} grant(s) used"),
        None => format!("{granted_so_far} grant(s) so far, no cap declared"),
    };
    let mode_name = match mode {
        yunta_core::ScopeExpansionMode::Rules => "rules",
        yunta_core::ScopeExpansionMode::Ask => "ask",
        yunta_core::ScopeExpansionMode::Deny => "deny",
    };
    yunta_core::events::GateWaitingPayload {
        summary: format!(
            "task `{}` requests scope expansion: {}",
            pending.task_id, request.reason
        ),
        evidence: vec![
            Fact::labelled("paths", request.paths.join(", ")),
            Fact::bare(precheck),
            Fact::labelled("mode", mode_name),
            Fact::bare(cap),
        ]
        .into(),
        options: vec![offers::grant(&request.paths.join(", ")), offers::deny()],
        external_ref: None,
    }
}

/// Emits the events one attempt's scope-expansion outcome requires:
/// always a `ScopeExpansionRequested`, then a `Granted` or `Denied` — never
/// both, and neither for `Escalate`, which has no event kind of its own
/// (nothing has been decided yet, so there's nothing to announce
/// beyond the request itself; the pause and its diagnostic already come
/// from `run_task`'s own `needs_human_decision`/`Blocked` outcome). Every
/// `Denied` also becomes a `FindingPosted`, using the agent's own
/// `reason`/`proposed_criterion` as the finding's evidence rather than the
/// engine inventing new wording. `decided_by` is always `Decider::Rule`
/// here — there is no gate node for a person to decide
/// through, so `ask` mode only ever reaches `Escalate`, never a rendered
/// verdict.
pub(super) async fn emit_scope_expansion_events(
    ctx: &RunCtx<'_>,
    node: &Node,
    task_id: &yunta_core::TaskId,
    attempt_number: u32,
    outcome: &crate::scope_expansion::ScopeExpansionOutcome,
    scope_expansion: Option<&yunta_core::ScopeExpansion>,
    granted_this_run: &mut u32,
) -> Result<(), RunError> {
    let mode = scope_expansion.map(|se| se.mode).unwrap_or_default();

    ctx.emit(
        Some(&node.id),
        EventPayload::ScopeExpansionRequested(ScopeExpansionRequestedPayload {
            task_id: task_id.clone(),
            paths: outcome.request.paths.clone(),
            reason: outcome.request.reason.clone(),
            proposed_criterion: outcome.request.proposed_criterion.clone().map(Into::into),
            proposed_criterion_precheck: outcome
                .precheck_exit
                .map(|exit_code| ProposedCriterionPrecheck { exit_code }),
        }),
    )
    .await?;

    match &outcome.decision {
        crate::scope_expansion::Decision::Granted => {
            *granted_this_run += 1;
            ctx.emit(
                Some(&node.id),
                EventPayload::ScopeExpansionGranted(ScopeExpansionGrantedPayload {
                    task_id: task_id.clone(),
                    decided_by: Decider::Rule,
                    mode,
                    count_this_run: *granted_this_run,
                    paths: outcome.request.paths.clone(),
                }),
            )
            .await?;
        }
        crate::scope_expansion::Decision::Denied(reason) => {
            ctx.emit(
                Some(&node.id),
                EventPayload::ScopeExpansionDenied(ScopeExpansionDeniedPayload {
                    task_id: task_id.clone(),
                    decided_by: Decider::Rule,
                    mode,
                    count_this_run: *granted_this_run,
                    denial_reason: Some(reason.clone()),
                }),
            )
            .await?;
            ctx.emit(
                Some(&node.id),
                EventPayload::FindingPosted(FindingPostedPayload {
                    finding: Finding {
                        id: FindingId::try_from(format!(
                            "scope-expansion-{task_id}-{attempt_number}"
                        ))?,
                        // Denied is a routine control-flow outcome, not
                        // evidence the run itself is broken — `Minor` by
                        // default, distinct from whatever severity the
                        // task's own criteria/scope failure separately
                        // carries.
                        severity: FindingSeverity::Minor,
                        title: format!("scope expansion denied for task `{task_id}`"),
                        location: outcome.request.paths.join(", "),
                        detail: format!(
                            "{reason} — agent's stated reason: {}",
                            outcome.request.reason
                        ),
                        proposed_criterion: outcome
                            .request
                            .proposed_criterion
                            .clone()
                            .map(Into::into),
                    },
                }),
            )
            .await?;
        }
        crate::scope_expansion::Decision::Escalate => {}
    }

    Ok(())
}
