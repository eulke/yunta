//! The departures from the plan a loop's task sessions declared, put to
//! a person once the batch is on the log.
//!
//! A task whose session departed from the plan does not close on its own,
//! whatever its criteria say: the plan is what a person approved, and a
//! different one is theirs to accept. Accepted, the task continues from
//! the work it left — closing at once when that work already passes;
//! sent back, the session that departed picks the work back up with what
//! the person said.

use yunta_core::events::{
    DepartsFrom, DeviationDeclaredPayload, DeviationResolvedPayload, Escalation, EventPayload,
    Fact, GateEvent, GateOption, GateResolvedPayload, TaskEvent, TaskLedger, TaskStatus,
    TaskStatusChangedPayload, TokenUsage,
};
use yunta_core::{Node, NonEmpty, OptionId, TaskId};

use crate::reserved::offers;
use crate::run::node_close::fail_with_tokens;
use crate::run::node_exec::NodeEnd;
use crate::run::{RunCtx, RunError};
use crate::task_cycle::{BlockedCause, TaskCycleReport, TaskOutcome};

/// The option that accepts a departure.
const ACCEPT: &str = "accept";
/// The option that sends a departure back to the session that made it.
const SEND_BACK: &str = "send-back";

/// What holds a task to its criteria besides its plan, which decides
/// what accepting a departure from one of them does — and the node that
/// writes the spec again, when one of the run does.
pub(super) struct Holders<'a> {
    pub(super) suite: Option<&'a str>,
    pub(super) spec: Option<&'a yunta_core::SpecFile>,
    pub(super) writer: Option<&'a yunta_core::NodeId>,
}

impl Holders<'_> {
    /// What holds `departure`'s task to each criterion it departs from.
    fn of<'d>(&self, departure: &'d PendingDeparture) -> Vec<(&'d str, crate::tasks::HeldBy)> {
        let task = &departure.task_id;
        departure
            .deviations
            .as_slice()
            .iter()
            .filter_map(|deviation| match &deviation.from {
                DepartsFrom::Criterion(cmd) => Some((
                    cmd.as_str(),
                    crate::tasks::held_by(task, cmd, self.suite, self.spec),
                )),
                _ => None,
            })
            .collect()
    }

    /// Whether `departure` departs from a test the run's spec gave its
    /// task, which accepting writes again.
    fn respecifies(&self, departure: &PendingDeparture) -> bool {
        self.of(departure)
            .iter()
            .any(|(_, held)| *held == crate::tasks::HeldBy::Spec)
    }
}

/// A task blocked on the departures its session declared.
pub(super) struct PendingDeparture {
    pub(super) task_id: TaskId,
    pub(super) deviations: NonEmpty<DeviationDeclaredPayload>,
}

/// What blocks `report`'s task when it owes a person an answer about a
/// departure from the plan: whatever its sessions' work came to, done or
/// not, the task closes on nothing until the answer exists. Never a cycle
/// that did not judge that work, nor a task a scope answer it is owed
/// reopens — that task's next close asks.
pub(super) fn owed_first(tasks: &TaskLedger, report: &TaskCycleReport) -> Option<BlockedCause> {
    let asks = match &report.outcome {
        TaskOutcome::Done => true,
        TaskOutcome::Blocked { cause } => {
            !report.needs_human_decision
                && matches!(
                    cause,
                    BlockedCause::Unmet { .. }
                        | BlockedCause::NonRetryable
                        | BlockedCause::Unrunnable { .. }
                        | BlockedCause::SessionDied(_)
                )
        }
        TaskOutcome::Interrupted => false,
    };
    let owed = tasks.get(&report.task_id)?.departures_owed.clone();
    NonEmpty::new(owed)
        .filter(|_| asks)
        .map(|deviations| BlockedCause::DeviationOwed { deviations })
}

/// Puts each task's departures to a person and reopens the task on the
/// answer. Returns the node's end when any is left unanswered — no one
/// to ask, or a person who chose to stop — and the run pauses owing it.
pub(super) async fn resolve_departures(
    ctx: &RunCtx<'_>,
    node: &Node,
    pending: Vec<PendingDeparture>,
    holders: &Holders<'_>,
    tokens: TokenUsage,
) -> Result<Option<NodeEnd>, RunError> {
    let mut unanswered: Vec<TaskId> = Vec::new();
    for departure in pending {
        let escalation = escalation(&departure, holders).map_err(|source| RunError::Broken {
            diagnostic: format!("task `{}`'s departure: {source}", departure.task_id),
        })?;
        let Some(choice) = ctx.ask_human(&escalation).await? else {
            unanswered.push(departure.task_id);
            continue;
        };
        let resolved = asked_and_answered(ctx, node, escalation, &choice).await?;
        let accepted = match choice.option.as_str() {
            ACCEPT => true,
            SEND_BACK => false,
            _ => {
                unanswered.push(departure.task_id);
                continue;
            }
        };
        let respecified_by = holders
            .writer
            .filter(|_| accepted && holders.respecifies(&departure))
            .cloned();
        ctx.emit(
            Some(&node.id),
            EventPayload::Tasks(TaskEvent::DeviationResolved(DeviationResolvedPayload {
                task_id: departure.task_id.clone(),
                accepted,
                said: choice.free_text.filter(|said| !said.trim().is_empty()),
                respecified_by,
            })),
        )
        .await?;
        let reopened = reopened(ctx, &departure.task_id, accepted, resolved).await?;
        ctx.emit(
            Some(&node.id),
            EventPayload::Tasks(TaskEvent::StatusChanged(reopened)),
        )
        .await?;
    }
    if unanswered.is_empty() {
        return Ok(None);
    }
    let owed: Vec<&str> = unanswered.iter().map(TaskId::as_str).collect();
    let diagnostic = format!(
        "a departure from the plan needs a person's answer before this run can continue: {}",
        yunta_core::text::listed(owed)
    );
    Ok(Some(
        fail_with_tokens(ctx, node, diagnostic, false, tokens).await?,
    ))
}

/// The question and the person's answer, together on the log once the
/// answer exists — where the answer sits is what the task's reopening
/// cites.
async fn asked_and_answered(
    ctx: &RunCtx<'_>,
    node: &Node,
    escalation: Escalation,
    choice: &yunta_core::events::HumanChoice,
) -> Result<yunta_core::Seq, RunError> {
    ctx.emit(
        Some(&node.id),
        EventPayload::Gates(GateEvent::Waiting(escalation.into_payload())),
    )
    .await?;
    ctx.emit(
        Some(&node.id),
        EventPayload::Gates(GateEvent::Resolved(GateResolvedPayload::Chosen(
            choice.clone(),
        ))),
    )
    .await
}

/// How the task goes on after the answer: accepted, from the work its
/// last attempt left, which a cycle judges before any session; sent
/// back, with the session that departed picking that work back up.
async fn reopened(
    ctx: &RunCtx<'_>,
    task: &TaskId,
    accepted: bool,
    resolved: yunta_core::Seq,
) -> Result<TaskStatusChangedPayload, RunError> {
    let state = ctx.run_view().await?.state;
    let record = state.tasks.get(task);
    let work = record.and_then(|record| record.left_work.as_ref().map(|(_, work)| work.clone()));
    let session = record.and_then(|record| record.last_session.clone());
    Ok(match (work, session, accepted) {
        (Some(work), Some(session), false) => {
            TaskStatusChangedPayload::resuming(task.clone(), resolved, work, session)
        }
        (Some(work), _, _) => TaskStatusChangedPayload::continuing(task.clone(), resolved, work),
        (None, _, _) => TaskStatusChangedPayload::to(task.clone(), TaskStatus::Pending, resolved),
    })
}

/// The question: what the session departed from, in its own words, and
/// the answers that can change something. A departure from a test the
/// run's spec gave the task is accepted only where a node of the run
/// writes the spec again: accepted with nobody to rewrite the test, the
/// task would stay held to what everyone agreed is wrong.
fn escalation(
    departure: &PendingDeparture,
    holders: &Holders<'_>,
) -> Result<Escalation, yunta_core::events::EscalationError> {
    let mut facts = Vec::new();
    for deviation in &departure.deviations {
        facts.push(Fact::labelled("departs from", deviation.from.to_string()));
        facts.push(Fact::labelled("the plan says", deviation.planned.clone()));
        facts.push(Fact::labelled(
            "its work instead",
            deviation.instead.clone(),
        ));
        facts.push(Fact::labelled("because", deviation.why.clone()));
    }
    let send_back = GateOption {
        id: OptionId::from_static(SEND_BACK),
        label: "Send it back".to_string(),
        tradeoff: "Its session picks the work back up with what you say".to_string(),
        asks: Some("what should it do instead?".to_string()),
    };
    let options = match (holders.respecifies(departure), holders.writer) {
        (true, None) => {
            facts.push(Fact::labelled(
                "not offered: accepting",
                "the tests came with the run, and no node of it writes them again",
            ));
            NonEmpty::from((send_back, vec![offers::abort()]))
        }
        _ => NonEmpty::from((
            GateOption {
                id: OptionId::from_static(ACCEPT),
                label: "Accept the departure".to_string(),
                tradeoff: accepting(departure, holders),
                asks: None,
            },
            vec![send_back, offers::abort()],
        )),
    };
    Escalation::new(
        format!(
            "task `{}`'s session departs from the plan",
            departure.task_id
        ),
        facts.into(),
        options,
    )
}

/// What accepting `departure` does, in the words its option offers: the
/// task closes on the work it left; a criterion of its plan it departs
/// from stops holding it, since nothing else can rewrite one; and a test
/// the run's spec gave it is written again by the node that wrote it.
fn accepting(departure: &PendingDeparture, holders: &Holders<'_>) -> String {
    let waived: Vec<&str> = holders
        .of(departure)
        .into_iter()
        .filter(|(_, held)| *held == crate::tasks::HeldBy::Plan)
        .map(|(cmd, _)| cmd)
        .collect();
    let rewritten = holders
        .writer
        .filter(|_| holders.respecifies(departure))
        .map(|writer| {
            format!(
                "its tests are written again by `{writer}` from this departure and what you \
                 say, and it goes on held to the new ones"
            )
        });
    let changes: Vec<String> = (!waived.is_empty())
        .then(|| format!("stops being held to {}", yunta_core::text::listed(waived)))
        .into_iter()
        .chain(rewritten)
        .collect();
    match changes.is_empty() {
        true => "The task closes on the work as it stands, if its criteria pass; the rest of \
                 the plan builds on what it did instead"
            .to_string(),
        false => format!(
            "The task {} and closes on the work as it stands, if its other criteria pass; the \
             rest of the plan builds on what it did instead",
            changes.join(", and ")
        ),
    }
}
