//! The human decisions a loop raises: the scope requests its tasks made that
//! the engine could not decide, put to the gate, and recorded as the grant or
//! refusal the person's choice makes.

use yunta_core::events::{
    Decider, Escalation, EventPayload, Fact, Failure, GateEvent, GateResolvedPayload, HumanChoice,
    OwedScope, ProposedCheck, ScopeWait, TaskEvent,
};
use yunta_core::expansion::{Decision, Precheck, Waiting};
use yunta_core::{Node, NonEmpty, ScopeGlob, Seq};

use crate::reserved::{decision_of_choice, offers};
use crate::run::scope_record::{self, Recording, Subject};
use crate::run::{RunCtx, RunError};
use crate::scope_expansion::{GrantWindow, ScopeExpansionOutcome};

/// Resolves each escalated scope request through the run's human interaction
/// surface, once the whole batch is on the log. Answers the requests nobody
/// was there to decide: their tasks stay blocked, the loop goes on with what
/// does not depend on them, and the requests are owed once nothing else can
/// run.
pub(super) async fn resolve_escalations(
    ctx: &RunCtx<'_>,
    node: &Node,
    pending_escalations: Vec<PendingEscalation>,
    grants: &GrantWindow,
) -> Result<Vec<PendingEscalation>, RunError> {
    // The whole batch integrates before anything is asked: a pending
    // `ask`/exhausted-cap request reaches a person as soon as one is there,
    // never bypassed just because the task that raised it happened to
    // succeed on its own declared scope. Nobody there, the task waits and
    // its request is owed.
    let mut unresolved: Vec<PendingEscalation> = Vec::new();
    for pending in pending_escalations {
        if let Some(pending) = resolve_one(ctx, node, pending, grants).await? {
            unresolved.push(pending);
        }
    }
    Ok(unresolved)
}

/// Puts one escalated request to the person. The request comes back when
/// nobody answers, and is recorded as the answer's grant or refusal when one
/// does.
async fn resolve_one(
    ctx: &RunCtx<'_>,
    node: &Node,
    pending: PendingEscalation,
    grants: &GrantWindow,
) -> Result<Option<PendingEscalation>, RunError> {
    let (paths, because) = pending.escalated()?;
    let granted = yunta_core::listed_globs(paths);
    let escalation = Escalation::new(
        format!(
            "task `{}` requests scope expansion: {}",
            pending.task_id,
            reason_of(&pending.outcome)
        ),
        vec![Fact::labelled("paths", granted.clone())].into(),
        NonEmpty::from((offers::grant(&granted), vec![offers::deny()])),
    )
    .map_err(|source| RunError::Escalation {
        context: format!("task `{}`'s expansion request", pending.task_id),
        source,
    })?
    .with_scope(ScopeWait {
        because,
        proposed: proposed_of(&pending.outcome),
    });
    let Some(choice) = ctx
        .ask_human(Some(&node.id), Some(&pending.task_id), &escalation)
        .await?
    else {
        return Ok(Some(pending));
    };
    // Same convention as every other gate: waiting and resolved land
    // together, only once actually resolved — an unresolved question
    // re-asks on resume instead of remembering a decision nobody made.
    ctx.emit(
        Some(&node.id),
        EventPayload::Gates(GateEvent::Waiting(escalation.into_payload())),
    )
    .await?;
    let resolved_seq = ctx
        .emit(
            Some(&node.id),
            EventPayload::Gates(GateEvent::Resolved(GateResolvedPayload::Chosen(
                choice.clone(),
            ))),
        )
        .await?;
    record_choice(ctx, node, &pending, paths, &choice, grants).await?;
    if pending.was_blocked {
        reopen(ctx, node, &pending, resolved_seq).await?;
    }
    Ok(None)
}

/// Records a person's choice on one escalated request as the grant or the
/// refusal it makes. A refusal's finding keeps the session's own reason, so
/// the request it answers is passed for it; a grant has no finding to word.
async fn record_choice(
    ctx: &RunCtx<'_>,
    node: &Node,
    pending: &PendingEscalation,
    paths: &[ScopeGlob],
    choice: &HumanChoice,
    grants: &GrantWindow,
) -> Result<(), RunError> {
    let by = Decider::Person {
        id: choice.by.clone(),
    };
    let protected = ctx.protected();
    let decision = decision_of_choice(choice, paths, &protected, &by);
    let asked = match &decision {
        Decision::Deny(_) => pending.outcome.asked.as_ref(),
        Decision::Grant { .. } | Decision::Escalate { .. } => None,
    };
    scope_record::record(
        &ctx.log(),
        &node.id,
        Recording {
            subject: Subject::Task(&pending.task_id),
            mode: Some(super::mode_in_force(node)),
            by,
            asked,
            decision: &decision,
            count: grants.granted().await,
            located: false,
            precheck: None,
            attempt: pending.attempt_no,
            new_request: true,
        },
    )
    .await
}

/// Sends a task that was blocked on an answered request back to work.
/// Granted or refused, the task goes on: with the widened scope (from the
/// log's own granted paths), or within the original one — a denial never
/// kills the task, it re-runs inside what was declared. The answer is the one
/// thing that changed for the session that asked, so that session picks its
/// work back up where it left it; with no work or no session on the log, the
/// task starts over.
async fn reopen(
    ctx: &RunCtx<'_>,
    node: &Node,
    pending: &PendingEscalation,
    resolved_seq: Seq,
) -> Result<(), RunError> {
    let state = ctx.run_view().await?.state;
    let reopened = super::reopen::after_answer(
        state.tasks.get(&pending.task_id),
        pending.task_id.clone(),
        resolved_seq,
    );
    ctx.emit(
        Some(&node.id),
        EventPayload::Tasks(TaskEvent::StatusChanged(reopened)),
    )
    .await?;
    Ok(())
}

/// The failure a loop ends on while scope requests are owed a person's
/// decision and nothing else of it can run: each task's request, which the
/// person grants or refuses from that failure.
pub(super) fn owed(owed: &[PendingEscalation]) -> Result<Option<Failure>, RunError> {
    if owed.is_empty() {
        return Ok(None);
    }
    let owed = owed
        .iter()
        .map(|pending| -> Result<OwedScope, RunError> {
            let (paths, _) = pending.escalated()?;
            Ok(OwedScope {
                task_id: pending.task_id.clone(),
                paths: paths.to_vec(),
                reason: reason_of(&pending.outcome),
            })
        })
        .collect::<Result<Vec<_>, RunError>>()?;
    Ok(Some(Failure::ScopeOwed { owed }))
}

/// One `Escalate`d request waiting for the human's verdict.
pub(super) struct PendingEscalation {
    pub(super) task_id: yunta_core::TaskId,
    pub(super) attempt_no: u32,
    pub(super) outcome: ScopeExpansionOutcome,
    /// Whether the task ended `Blocked` on this escalation (vs. `Done`
    /// with the question still owed) — decides whether resolution sets
    /// it back to `Pending` for a retry.
    pub(super) was_blocked: bool,
}

impl PendingEscalation {
    /// The paths the request asks to be allowed, and what holds it. Only a
    /// request the screen escalated is ever pending.
    fn escalated(&self) -> Result<(&[ScopeGlob], Waiting), RunError> {
        match &self.outcome.decision {
            Decision::Escalate { paths, because } => Ok((paths.as_slice(), *because)),
            Decision::Grant { .. } | Decision::Deny(_) => Err(RunError::Broken {
                diagnostic: format!(
                    "task `{}`'s pending request was decided, not escalated",
                    self.task_id
                ),
            }),
        }
    }
}

/// The reason a person reads for a request: the session's own words, or, for
/// a refusal the session asked nothing about, the criterion that located it.
fn reason_of(outcome: &ScopeExpansionOutcome) -> String {
    match &outcome.asked {
        Some(asked) => asked.reason.clone(),
        None => "a red criterion of the task locates the refused writes".to_string(),
    }
}

/// The criterion the session proposed for its request, with what the engine's
/// own run of it found. A request that proposed none has none.
fn proposed_of(outcome: &ScopeExpansionOutcome) -> Option<ProposedCheck> {
    let criterion = outcome.asked.as_ref()?.proposed_criterion.as_ref()?;
    Some(ProposedCheck {
        cmd: criterion.cmd.clone(),
        precheck: outcome.precheck.unwrap_or(Precheck::NotRun),
    })
}
