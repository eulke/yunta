//! The loop node's task cycle (T4.1 + T5.2 + T5.10 wiring, Contrato
//! §5.2/§5.5): each iteration forms a batch of up to `concurrency` `ready`
//! tasks (ledger declaration order), dispatches every member in its own
//! isolated worktree concurrently, then integrates them **serially, in
//! that same declaration order** — rebase onto the current tree,
//! re-verify criteria and scope there, and only then fast-forward the
//! run's shared worktree. `concurrency: 1` (the default) walks the exact
//! same path with a batch of one; §5.5/D65 are explicit that this needs
//! no special case.

use std::path::{Path, PathBuf};

use yunta_core::events::{
    CriteriaCheckedPayload, CriterionResult, CriterionType, Decider, Event, EventPayload, Finding,
    FindingPostedPayload, FindingSeverity, LoopIterationPayload, Phase, ProposedCriterionPrecheck,
    ScopeCheckedPayload, ScopeExpansionDeniedPayload, ScopeExpansionGrantedPayload,
    ScopeExpansionRequestedPayload, TaskStatus, TaskStatusChangedPayload, TokenUsage,
};
use yunta_core::{Isolation, Ledger, Node, NodeKind, PromptSource, Task};

use crate::replay::{derive, RunState};
use crate::scope::scope_check;
use crate::task_cycle::{post_check, run_task, CriterionRun, Memo, TaskCycleReport, TaskOutcome};
use crate::worktree::prepare_worktree;

use super::node_exec::{
    close_node, fail, fail_with_tokens, prompt_text, render_or_fail, resolve_node_runner, NodeEnd,
};
use super::{RunCtx, RunError};

pub(super) async fn execute_loop(
    ctx: &RunCtx<'_>,
    node: &Node,
    until: &str,
    prompt: &PromptSource,
) -> Result<NodeEnd, RunError> {
    if until != "all_tasks_complete" {
        return fail(
            ctx,
            node,
            format!("loop until `{until}` is not supported — M-0 only has `all_tasks_complete`"),
            false,
        );
    }
    let instruction = match render_or_fail(ctx, node, prompt_text(ctx, node, prompt))? {
        Ok(rendered) => rendered,
        Err(end) => return Ok(end),
    };
    let chosen = match resolve_node_runner(ctx, node)? {
        Ok(chosen) => chosen,
        Err(end) => return Ok(end),
    };
    let adapter = &ctx.adapters[&chosen.adapter];

    let Some(ledger) = load_registered_ledger(ctx)? else {
        return fail(
            ctx,
            node,
            "no task ledger has been registered before this loop — a previous node must \
             produce an artifact with `kind: task-ledger`"
                .to_string(),
            false,
        );
    };

    // Absent means the engine's own default, 1 — sequential, deliberately
    // not config-overridable (§5.5/D65: token spend multiplies with it,
    // so it's declared per-workflow, never inherited silently).
    let concurrency = match &node.kind {
        NodeKind::Loop { concurrency, .. } => concurrency.unwrap_or(1).max(1),
        _ => 1,
    };
    // Loop-scoped, never workflow/config-scoped (§6.2: a request is
    // task-specific, and only a loop node's own tasks can ever write one).
    let scope_expansion = match &node.kind {
        NodeKind::Loop {
            scope_expansion, ..
        } => scope_expansion.as_ref(),
        _ => None,
    };

    let mut tokens = TokenUsage::default();
    let mut iteration: u32 = 0;
    // §8.3/DI-05: the only net under a ledger whose state oscillates
    // forever. Checked only when a non-empty batch wants to run — the
    // closing empty-batch pass never trips it. `continue` lifts the cap
    // for this invocation only (same rule as the run token budget).
    let max_iterations = ctx.manifest.config.resolved_max_loop_iterations();
    let mut iterations_lifted = false;
    // Blocked reasons gathered this invocation, so the loop's own failure
    // can cite them (§6.1 for permission blocks; useful for every block).
    // A resume starts empty — the log carries each task's *status*, and
    // the generic tail below still names which tasks are blocked.
    let mut blocked_reasons: Vec<String> = Vec::new();
    loop {
        iteration += 1;
        // §6.2: a scope-expansion request left `Escalate`d (`ask` mode,
        // or `max_per_run` exhausted) is a decision owed to a human —
        // regardless of whether *this* attempt's own criteria happened
        // to succeed without needing the grant. Collected per batch and
        // resolved (DI-01) through `ctx.human_interaction` once the
        // whole batch has integrated; only what stays unresolved (no
        // live surface) pauses the run.
        let mut pending_escalations: Vec<PendingEscalation> = Vec::new();
        let events = ctx.load_events()?;
        let state = derive(&events);

        let batch = select_batch(&ledger, &state, concurrency);

        if batch.is_empty() {
            let all_done = ledger
                .tasks
                .iter()
                .all(|task| state.tasks.get(&task.id) == Some(&TaskStatus::Done));
            ctx.emit(
                Some(&node.id),
                EventPayload::LoopIteration(LoopIterationPayload {
                    iteration,
                    until_result: all_done,
                }),
            )?;
            if all_done {
                return close_node(
                    ctx,
                    node,
                    format!("{} task(s) done", ledger.tasks.len()),
                    tokens,
                )
                .await;
            }
            let mut diagnostic = "no task is ready and not all are done — blocked or failed \
                                  tasks need a decision"
                .to_string();
            for reason in &blocked_reasons {
                diagnostic.push_str("; ");
                diagnostic.push_str(reason);
            }
            return fail_with_tokens(ctx, node, diagnostic, false, tokens);
        };

        if iteration > max_iterations && !iterations_lifted {
            match super::budget::authorize_loop_overrun(ctx, &node.id, iteration, max_iterations)
                .await?
            {
                super::budget::BudgetDecision::Continue => iterations_lifted = true,
                super::budget::BudgetDecision::Pause { reason } => {
                    return fail_with_tokens(ctx, node, reason, false, tokens);
                }
            }
        }

        // Every batch member's worktree branches from the same starting
        // point (§5.5: "worktree por tarea desde el commit base actual"),
        // captured once so all N tasks work from an identical snapshot.
        let base_commit = head_commit(ctx.worktree).await?;

        let dispatches = futures::future::join_all(batch.iter().map(|task| {
            dispatch_task_in_isolation(
                ctx,
                node,
                task,
                &events,
                &base_commit,
                &instruction,
                adapter.as_ref(),
                scope_expansion,
            )
        }))
        .await;

        // Cumulative grants this run, kept live across the integration
        // loop below so each emitted `ScopeExpansionGranted` carries an
        // accurate `count_this_run` — `dispatch_task_in_isolation` above
        // already read its own (necessarily slightly stale, see
        // `granted_count`'s own doc comment) snapshot for the cap check;
        // this one only feeds the event payload, not any decision.
        let mut expansions_granted_this_run = granted_count(&events);

        // Integration is serial and follows the batch's own order, which
        // is ledger declaration order (§5.5: "en orden de declaración del
        // ledger — no en orden de finalización") — never the order
        // dispatch happened to finish in.
        for dispatch in dispatches {
            let (task, task_worktree, mut report) = dispatch?;
            let needs_human_decision = report.needs_human_decision;
            // The escalated request itself (paths, reason, criterion +
            // its pre-check exit), captured off the attempt that raised
            // it — what the §5.3 object below is built from.
            let mut escalated: Option<(u32, crate::scope_expansion::ScopeExpansionOutcome)> = None;

            let mut last_check_seq = ctx.emit(
                Some(&node.id),
                EventPayload::CriteriaChecked(CriteriaCheckedPayload {
                    task_id: task.id.clone(),
                    phase: Phase::Pre,
                    results: to_results(&report.pre_check),
                }),
            )?;

            for attempt in report.attempts.drain(..) {
                tokens = sum_tokens(tokens, attempt.tokens);
                last_check_seq = ctx.emit(
                    Some(&node.id),
                    EventPayload::CriteriaChecked(CriteriaCheckedPayload {
                        task_id: task.id.clone(),
                        phase: Phase::Post,
                        results: to_results(&attempt.post_check),
                    }),
                )?;
                ctx.emit(
                    Some(&node.id),
                    EventPayload::ScopeChecked(ScopeCheckedPayload {
                        task_id: Some(task.id.clone()),
                        diff: attempt.scope.diff,
                        violations: attempt.scope.violations,
                    }),
                )?;

                if let Some(outcome) = &attempt.scope_expansion {
                    emit_scope_expansion_events(
                        ctx,
                        node,
                        &task.id,
                        attempt.attempt,
                        outcome,
                        scope_expansion,
                        &mut expansions_granted_this_run,
                    )?;
                    if outcome.decision == crate::scope_expansion::Decision::Escalate {
                        escalated = Some((attempt.attempt, outcome.clone()));
                    }
                }
            }

            let blocked_reason = match report.outcome {
                TaskOutcome::Blocked { reason } => {
                    ctx.emit(
                        Some(&node.id),
                        EventPayload::TaskStatusChanged(TaskStatusChangedPayload {
                            task_id: task.id.clone(),
                            new_status: TaskStatus::Blocked,
                            caused_by: last_check_seq,
                        }),
                    )?;
                    Some(reason)
                }
                TaskOutcome::Done => {
                    let outcome = integrate_task(
                        ctx,
                        node,
                        task,
                        &task_worktree,
                        &ctx.memo,
                        &mut last_check_seq,
                    )
                    .await?;
                    // §5.5's own words: a rejected integration "vuelve a
                    // ready sobre el árbol nuevo" — back to `Pending`, so
                    // a future batch retries it automatically. This is
                    // deliberately NOT `Blocked`: green in isolation but
                    // broken by a sibling's integration is a timing
                    // artifact of concurrency, not evidence the task
                    // itself can't succeed — that verdict only comes from
                    // `run_task`'s own retry exhaustion, the branch above.
                    let new_status = match &outcome {
                        IntegrationOutcome::Integrated => TaskStatus::Done,
                        IntegrationOutcome::Rejected(reason) => {
                            tracing::warn!(
                                task_id = %task.id,
                                %reason,
                                "task integration rejected, returning to ready"
                            );
                            TaskStatus::Pending
                        }
                    };
                    ctx.emit(
                        Some(&node.id),
                        EventPayload::TaskStatusChanged(TaskStatusChangedPayload {
                            task_id: task.id.clone(),
                            new_status,
                            caused_by: last_check_seq,
                        }),
                    )?;
                    // Never counted toward the loop's own "no task ready"
                    // diagnostic — a `Pending` task is retriable, not
                    // stuck, so there's nothing to cite a human decision
                    // for yet.
                    None
                }
            };
            let was_blocked = blocked_reason.is_some();
            if let Some(reason) = blocked_reason {
                // An escalation-blocked task is the escalation flow's to
                // report (resolved below, or the pause diagnostic) — its
                // interim Blocked never feeds the generic tail.
                if !needs_human_decision {
                    blocked_reasons.push(format!("task `{}` blocked: {reason}", task.id));
                }
            }
            if needs_human_decision {
                if let Some((attempt_no, outcome)) = escalated {
                    pending_escalations.push(PendingEscalation {
                        task_id: task.id.clone(),
                        attempt_no,
                        outcome,
                        was_blocked,
                    });
                }
            }
        }

        if !pending_escalations.is_empty() {
            // The whole batch integrates before anything is asked (serial
            // integration order still holds — this only stops the *next*
            // batch from being dispatched while a decision is owed): a
            // pending `ask`/exhausted-cap request must reach a human
            // before the run spends any further budget, never be
            // bypassed just because the task that raised it happened to
            // succeed on its own declared scope (A6). DI-01: with a live
            // surface the human decides right here; only what stays
            // unresolved pauses the run, exactly as before.
            let mode = scope_expansion.map(|se| se.mode).unwrap_or_default();
            let max_per_run = scope_expansion.and_then(|se| se.max_per_run);
            let mut unresolved: Vec<yunta_core::TaskId> = Vec::new();
            for pending in pending_escalations.drain(..) {
                let escalation =
                    expansion_escalation(&pending, mode, max_per_run, expansions_granted_this_run);
                let Some(resolution) = ctx.human_interaction.resolve(&escalation).await else {
                    unresolved.push(pending.task_id);
                    continue;
                };
                // Same T7.2 convention as every other gate: waiting and
                // resolved land together, only once actually resolved —
                // an unresolved question re-asks on resume instead of
                // remembering a decision nobody made.
                ctx.emit(Some(&node.id), EventPayload::GateWaiting(escalation))?;
                let resolved_seq = ctx.emit(
                    Some(&node.id),
                    EventPayload::GateResolved(resolution.clone()),
                )?;
                let decided_by = Decider::Person {
                    id: resolution
                        .resolved_by
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string()),
                };
                if resolution.chosen_option.as_deref() == Some("grant") {
                    expansions_granted_this_run += 1;
                    ctx.emit(
                        Some(&node.id),
                        EventPayload::ScopeExpansionGranted(ScopeExpansionGrantedPayload {
                            task_id: pending.task_id.clone(),
                            decided_by,
                            mode,
                            count_this_run: expansions_granted_this_run,
                            paths: pending.outcome.request.paths.clone(),
                        }),
                    )?;
                } else {
                    // Anything that isn't an explicit grant denies — the
                    // conservative reading of an ambiguous resolution,
                    // and every denial converts to a finding (D80), same
                    // as the rule-mode path.
                    let reason = resolution
                        .free_text
                        .clone()
                        .unwrap_or_else(|| "denied by a human at the gate".to_string());
                    ctx.emit(
                        Some(&node.id),
                        EventPayload::ScopeExpansionDenied(ScopeExpansionDeniedPayload {
                            task_id: pending.task_id.clone(),
                            decided_by,
                            mode,
                            count_this_run: expansions_granted_this_run,
                            denial_reason: Some(reason.clone()),
                        }),
                    )?;
                    ctx.emit(
                        Some(&node.id),
                        EventPayload::FindingPosted(FindingPostedPayload {
                            finding: Finding {
                                id: format!(
                                    "scope-expansion-{}-{}",
                                    pending.task_id, pending.attempt_no
                                ),
                                severity: FindingSeverity::Minor,
                                title: format!(
                                    "scope expansion denied for task `{}`",
                                    pending.task_id
                                ),
                                location: pending.outcome.request.paths.join(", "),
                                detail: format!(
                                    "{reason} — agent's stated reason: {}",
                                    pending.outcome.request.reason
                                ),
                                proposed_criterion: pending
                                    .outcome
                                    .request
                                    .proposed_criterion
                                    .clone(),
                            },
                        }),
                    )?;
                }
                // Granted or denied, the task gets its retry: with the
                // widened scope (from the log's own granted paths), or
                // within the original one (§6.2 — a denial never kills
                // the task, it re-runs inside what was declared).
                if pending.was_blocked {
                    ctx.emit(
                        Some(&node.id),
                        EventPayload::TaskStatusChanged(TaskStatusChangedPayload {
                            task_id: pending.task_id.clone(),
                            new_status: TaskStatus::Pending,
                            caused_by: resolved_seq,
                        }),
                    )?;
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
                return fail_with_tokens(ctx, node, diagnostic, false, tokens);
            }
            // Everything resolved — the next iteration re-dispatches the
            // (now Pending again) tasks with the decisions on the log.
        }
    }
}

/// One `Escalate`d request waiting for the human's verdict (DI-01).
struct PendingEscalation {
    task_id: yunta_core::TaskId,
    attempt_no: u32,
    outcome: crate::scope_expansion::ScopeExpansionOutcome,
    /// Whether the task ended `Blocked` on this escalation (vs. `Done`
    /// with the question still owed) — decides whether resolution sets
    /// it back to `Pending` for a retry.
    was_blocked: bool,
}

/// Builds the §5.3 escalation object for a scope-expansion request — the
/// engine assembles summary + mechanical evidence from what it already
/// verified (the request, the proposed criterion's own pre-check exit,
/// the cap state); the agent's only contribution is the reason it wrote
/// into the request itself.
fn expansion_escalation(
    pending: &PendingEscalation,
    mode: yunta_core::events::ScopeExpansionMode,
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
        yunta_core::events::ScopeExpansionMode::Rules => "rules",
        yunta_core::events::ScopeExpansionMode::Ask => "ask",
        yunta_core::events::ScopeExpansionMode::Deny => "deny",
    };
    yunta_core::events::GateWaitingPayload {
        summary: format!(
            "task `{}` requests scope expansion: {}",
            pending.task_id, request.reason
        ),
        evidence: format!(
            "paths: {}; {precheck}; mode: {mode_name}; {cap}",
            request.paths.join(", ")
        ),
        options: vec![
            yunta_core::events::GateOption {
                id: "grant".to_string(),
                label: format!("Grant access to {}", request.paths.join(", ")),
                tradeoff: "The task's final diff is evaluated against its scope plus these \
                           paths; consumes 1 of max_per_run"
                    .to_string(),
            },
            yunta_core::events::GateOption {
                id: "deny".to_string(),
                label: "Deny the expansion".to_string(),
                tradeoff: "The denial becomes a finding (D80); the task retries within its \
                           original scope"
                    .to_string(),
            },
        ],
        external_ref: None,
    }
}

/// Up to `concurrency` tasks this iteration may work on, in ledger
/// declaration order: a task whose dependencies are all `Done` and is
/// itself still `Pending`, or an orphaned `Running` task with no
/// terminal event after it (a crash mid-batch, §5.5's own resume
/// guarantee — "las que quedaron `running` huérfanas se reejecutan").
/// Scope disjointness between independent tasks is **not** re-checked
/// here: `ledger::register` (T5.1) already refuses two tasks without a
/// `depends_on` edge declaring overlapping scope, so any two tasks that
/// can both be `ready` at once are disjoint by construction.
fn select_batch<'a>(ledger: &'a Ledger, state: &RunState, concurrency: u32) -> Vec<&'a Task> {
    ledger
        .tasks
        .iter()
        .filter(|task| match state.tasks.get(&task.id) {
            Some(TaskStatus::Pending) => task
                .depends_on
                .iter()
                .all(|dep| state.tasks.get(dep) == Some(&TaskStatus::Done)),
            Some(TaskStatus::Running) => true,
            _ => false,
        })
        .take(concurrency as usize)
        .collect()
}

/// How many times this task has already been dispatched `Running` in the
/// log — 1-indexed, so the first dispatch is attempt 1. Used only to keep
/// worktree/branch names unique across a resumed orphan's fresh attempt;
/// never fed into retry-limit logic (that's `run_task`'s own
/// `max_retries`, scoped to one dispatch).
fn attempt_number(events: &[Event], task_id: &yunta_core::TaskId) -> u32 {
    events
        .iter()
        .filter(|event| {
            matches!(
                &event.payload,
                EventPayload::TaskStatusChanged(p)
                    if p.task_id == *task_id && p.new_status == TaskStatus::Running
            )
        })
        .count() as u32
        + 1
}

/// How many `scope_expansion_granted` events the run's whole log already
/// has (§6.2: `max_per_run` is run-scoped, never per-task). Read once per
/// batch, same moment as `base_commit` — a known, documented tradeoff:
/// two tasks in the *same* concurrent batch that both get granted can
/// each see the pre-batch count, so `max_per_run` may be overshot by up
/// to `concurrency - 1` within one batch before the next batch's fresh
/// read catches it. Serializing expansion evaluation to close this
/// would undo T5.10's whole point (real concurrent dispatch) for a soft
/// cap whose purpose is catching "ten grants in a row", not enforcing a
/// hard security boundary — accepted, not fixed.
fn granted_count(events: &[Event]) -> u32 {
    events
        .iter()
        .filter(|event| matches!(&event.payload, EventPayload::ScopeExpansionGranted(_)))
        .count() as u32
}

/// Every path a prior `scope_expansion_granted` on the log authorized
/// for `task_id` (DI-01) — the retry after a human grant derives its
/// widened scope from here, never from in-memory state (I2).
fn granted_paths_for(events: &[Event], task_id: &yunta_core::TaskId) -> Vec<String> {
    events
        .iter()
        .filter_map(|event| match &event.payload {
            EventPayload::ScopeExpansionGranted(p) if &p.task_id == task_id => {
                Some(p.paths.iter().cloned())
            }
            _ => None,
        })
        .flatten()
        .collect()
}

/// Isolates one batch member in its own worktree (§5.5: "cada tarea del
/// lote recibe su propio worktree derivado del commit base actual") and
/// runs it through the ordinary task cycle there — pre-check, dispatch,
/// post-check, scope-check, retried up to `max_task_retries` exactly as
/// the sequential path always has. Never commits or marks the task
/// `done`/`blocked` in the log itself; that's the caller's job once every
/// batch member's dispatch has settled, so integration can stay strictly
/// serial and in declaration order.
#[allow(clippy::too_many_arguments)]
async fn dispatch_task_in_isolation<'a>(
    ctx: &RunCtx<'_>,
    node: &Node,
    task: &'a Task,
    events: &[Event],
    base_commit: &str,
    instruction: &str,
    adapter: &dyn yunta_adapters::Adapter,
    scope_expansion: Option<&yunta_core::ScopeExpansion>,
) -> Result<(&'a Task, PathBuf, TaskCycleReport), RunError> {
    let attempt = attempt_number(events, &task.id);
    let task_worktree = ctx
        .run_dir
        .join("task-worktrees")
        .join(format!("{}-{attempt}", task.id));
    let branch = format!("yunta/task/{}/{attempt}", task.id);
    prepare_worktree(
        ctx.worktree,
        &task_worktree,
        base_commit,
        &branch,
        Isolation::Worktree,
    )
    .await?;

    let registered_seq = events
        .iter()
        .find(|event| {
            matches!(&event.payload, EventPayload::TaskRegistered(p) if p.task_id == task.id)
        })
        .map(|event| event.seq)
        .unwrap_or(0);
    ctx.emit(
        Some(&node.id),
        EventPayload::TaskStatusChanged(TaskStatusChangedPayload {
            task_id: task.id.clone(),
            new_status: TaskStatus::Running,
            caused_by: registered_seq,
        }),
    )?;

    let report = run_task(
        task,
        instruction,
        adapter,
        &task_worktree,
        ctx.max_task_retries,
        ctx.session_budget()?,
        &ctx.memo,
        ctx.manifest.config.permissions.as_ref(),
        super::node_exec::session_profile(node),
        scope_expansion,
        granted_count(events),
        &granted_paths_for(events, &task.id),
    )
    .await?;

    Ok((task, task_worktree, report))
}

enum IntegrationOutcome {
    Integrated,
    Rejected(String),
}

/// Integrates one task verified `Done` in isolation (§5.5, D65): rebase
/// its branch onto the run's *current* integration HEAD (which may have
/// moved since this batch started, if an earlier-declared sibling
/// integrated first), re-run criteria and scope right there — green in
/// isolation is necessary, never sufficient, since a sibling's
/// integration can change the ground a task's work sits on — and only on
/// a clean re-verification does the shared worktree fast-forward onto the
/// rebased result. A rebase conflict or a post-integration criteria/scope
/// failure returns the task to `ready` on the new tree; nothing else in
/// the batch is touched.
async fn integrate_task(
    ctx: &RunCtx<'_>,
    node: &Node,
    task: &Task,
    task_worktree: &Path,
    memo: &Memo,
    last_check_seq: &mut u64,
) -> Result<IntegrationOutcome, RunError> {
    commit_task_work(task_worktree, task).await?;

    let integration_head = head_commit(ctx.worktree).await?;
    if !run_git_ok(task_worktree, &["rebase", &integration_head]).await? {
        let _ = run_git(task_worktree, &["rebase", "--abort"]).await;
        // No `post_check` ran — nothing to attach the reason to but the
        // rebase itself, so it's recorded the same way any other command
        // outcome is: a synthetic `CriterionRun` naming the git command
        // and its (failing) exit code, through the existing
        // `CriteriaChecked` vocabulary rather than a new event kind.
        *last_check_seq = ctx.emit(
            Some(&node.id),
            EventPayload::CriteriaChecked(CriteriaCheckedPayload {
                task_id: task.id.clone(),
                phase: Phase::Post,
                results: vec![CriterionResult {
                    cmd: format!("git rebase {integration_head}"),
                    exit_code: 1,
                    r#type: None,
                    reused: false,
                }],
            }),
        )?;
        return Ok(IntegrationOutcome::Rejected(format!(
            "rebase onto the integrated tree conflicted for task `{}`",
            task.id
        )));
    }

    let post_runs = post_check(task, task_worktree, memo).await?;
    *last_check_seq = ctx.emit(
        Some(&node.id),
        EventPayload::CriteriaChecked(CriteriaCheckedPayload {
            task_id: task.id.clone(),
            phase: Phase::Post,
            results: to_results(&post_runs),
        }),
    )?;
    let scope = scope_check(task_worktree, &task.scope).await?;
    ctx.emit(
        Some(&node.id),
        EventPayload::ScopeChecked(ScopeCheckedPayload {
            task_id: Some(task.id.clone()),
            diff: scope.diff.clone(),
            violations: scope.violations.clone(),
        }),
    )?;

    let criteria_green = post_runs.iter().all(|r| r.exit_code == 0);
    if !criteria_green || !scope.violations.is_empty() {
        return Ok(IntegrationOutcome::Rejected(format!(
            "criteria or scope failed after integration for task `{}`",
            task.id
        )));
    }

    let task_head = head_commit(task_worktree).await?;
    if !run_git_ok(ctx.worktree, &["merge", "--ff-only", &task_head]).await? {
        // §5.5's integration is strictly serial (the engine itself, not
        // an external actor, is the only writer to `ctx.worktree` between
        // reading `integration_head` above and this merge) — a non-fast-
        // forward here means that invariant broke, not a legitimate task
        // outcome, so it surfaces as an engine error rather than a
        // `ready` retry.
        return Err(RunError::Git {
            context: format!("fast-forward integration of task `{}`", task.id),
            detail: "expected a clean fast-forward after rebase but git refused it".to_string(),
        });
    }
    Ok(IntegrationOutcome::Integrated)
}

/// Commits a done task's work in `cwd` — the task's own isolated worktree
/// during integration (§5.5), or the run's shared worktree when
/// `concurrency` never applies. A task that changed nothing (its criteria
/// were satisfied by side effects that left no diff) simply produces no
/// commit — never an error.
async fn commit_task_work(cwd: &Path, task: &Task) -> Result<(), RunError> {
    let add = run_git(cwd, &["add", "-A"])
        .await
        .map_err(|source| RunError::Io {
            context: format!("stage task `{}` work", task.id),
            source,
        })?;
    if !add.status.success() {
        return Err(RunError::Git {
            context: format!("stage task `{}` work", task.id),
            detail: String::from_utf8_lossy(&add.stderr).trim().to_string(),
        });
    }

    let staged = run_git(cwd, &["diff", "--cached", "--quiet"])
        .await
        .map_err(|source| RunError::Io {
            context: format!("inspect staged work for task `{}`", task.id),
            source,
        })?;
    if staged.status.success() {
        return Ok(()); // nothing staged — nothing to commit
    }

    let commit = run_git(
        cwd,
        &[
            "commit",
            "-q",
            "-m",
            &format!("task {}: {}", task.id, task.title),
        ],
    )
    .await
    .map_err(|source| RunError::Io {
        context: format!("commit task `{}` work", task.id),
        source,
    })?;
    if !commit.status.success() {
        return Err(RunError::Git {
            context: format!("commit task `{}` work", task.id),
            detail: String::from_utf8_lossy(&commit.stderr).trim().to_string(),
        });
    }
    Ok(())
}

async fn run_git(cwd: &Path, args: &[&str]) -> std::io::Result<std::process::Output> {
    tokio::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .await
}

async fn run_git_ok(cwd: &Path, args: &[&str]) -> Result<bool, RunError> {
    let output = run_git(cwd, args).await.map_err(|source| RunError::Io {
        context: format!("run git {}", args.join(" ")),
        source,
    })?;
    Ok(output.status.success())
}

async fn head_commit(repo: &Path) -> Result<String, RunError> {
    let output = run_git(repo, &["rev-parse", "HEAD"])
        .await
        .map_err(|source| RunError::Io {
            context: "read the integration HEAD commit".to_string(),
            source,
        })?;
    if !output.status.success() {
        return Err(RunError::Git {
            context: "read the integration HEAD commit".to_string(),
            detail: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Emits the events one attempt's scope-expansion outcome requires (§6.2):
/// always a `ScopeExpansionRequested`, then a `Granted` or `Denied` — never
/// both, and neither for `Escalate`, which has no event kind of its own
/// (D73: nothing has been decided yet, so there's nothing to announce
/// beyond the request itself; the pause and its diagnostic already come
/// from `run_task`'s own `needs_human_decision`/`Blocked` outcome). Every
/// `Denied` also becomes a `FindingPosted` (D80), using the agent's own
/// `reason`/`proposed_criterion` as the finding's evidence rather than the
/// engine inventing new wording. `decided_by` is always `Decider::Rule`
/// here — this recorte has no `kind: gate` (T7.2) for a person to decide
/// through, so `ask` mode only ever reaches `Escalate`, never a rendered
/// verdict.
#[allow(clippy::too_many_arguments)]
fn emit_scope_expansion_events(
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
            proposed_criterion: outcome.request.proposed_criterion.clone(),
            proposed_criterion_precheck: outcome
                .precheck_exit
                .map(|exit_code| ProposedCriterionPrecheck { exit_code }),
        }),
    )?;

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
            )?;
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
            )?;
            ctx.emit(
                Some(&node.id),
                EventPayload::FindingPosted(FindingPostedPayload {
                    finding: Finding {
                        id: format!("scope-expansion-{task_id}-{attempt_number}"),
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
                        proposed_criterion: outcome.request.proposed_criterion.clone(),
                    },
                }),
            )?;
        }
        crate::scope_expansion::Decision::Escalate => {}
    }

    Ok(())
}

fn to_results(runs: &[CriterionRun]) -> Vec<CriterionResult> {
    runs.iter()
        .map(|run| CriterionResult {
            cmd: run.cmd.clone(),
            exit_code: run.exit_code,
            r#type: run.is_guard.then_some(CriterionType::Guard),
            reused: run.reused,
        })
        .collect()
}

fn sum_tokens(a: TokenUsage, b: TokenUsage) -> TokenUsage {
    TokenUsage {
        input: a.input + b.input,
        output: a.output + b.output,
        cached: match (a.cached, b.cached) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
        },
    }
}

/// Finds the task ledger the run registered: the `kind: task-ledger`
/// artifact of a node that produced it earlier, re-read from the run's
/// frozen `artifacts/` (I3: artifacts are immutable once written).
fn load_registered_ledger(ctx: &RunCtx<'_>) -> Result<Option<Ledger>, RunError> {
    for node in &ctx.manifest.workflow.nodes {
        let Some(artifacts) = &node.artifacts else {
            continue;
        };
        for spec in &artifacts.produces {
            let yunta_core::ArtifactSpec::Typed { name, kind } = spec else {
                continue;
            };
            if !matches!(kind, yunta_core::ArtifactKind::TaskLedger) {
                continue;
            }
            let path = ctx.run_dir.join("artifacts").join(name);
            if !path.exists() {
                continue;
            }
            let bytes = std::fs::read(&path).map_err(|source| RunError::Io {
                context: format!("read task ledger `{}`", path.display()),
                source,
            })?;
            let ledger: Ledger =
                serde_yaml::from_slice(&bytes).map_err(|e| RunError::CorruptLedger {
                    path: path.clone(),
                    detail: e.to_string(),
                })?;
            return Ok(Some(ledger));
        }
    }
    Ok(None)
}
