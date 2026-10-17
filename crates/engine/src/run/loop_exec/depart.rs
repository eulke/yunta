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
    Claim, DepartsFrom, DepartureMenu, DeviationDeclaredPayload, DeviationResolvedPayload,
    Escalation, EventPayload, Failure, GateOption, HumanChoice, Menu, PauseReason, TaskCause,
    TaskEvent, TaskLedger, TaskStatus, TaskStatusChangedPayload, TaskStop, TokenUsage,
};
use yunta_core::reserved::ReservedOption;
use yunta_core::{Node, NonEmpty, Seq, TaskId};

use crate::run::escalation::decided;
use crate::run::node_close::fail_with;
use crate::run::node_exec::NodeEnd;
use crate::run::{RunCtx, RunError};
use crate::task_cycle::{TaskCycleReport, TaskOutcome};

/// What holds a task to its criteria besides its plan, which decides
/// what accepting a departure from one of them does — and the node that
/// writes the spec again, when one of the run does.
pub(super) struct Holders<'a> {
    pub(super) held: &'a [String],
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
                    crate::tasks::held_by(task, cmd, self.held, self.spec),
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
pub(super) fn owed_first(tasks: &TaskLedger, report: &TaskCycleReport) -> Option<TaskCause> {
    let asks = match &report.outcome {
        TaskOutcome::Done => true,
        TaskOutcome::Blocked { cause } => {
            !report.needs_human_decision
                && matches!(
                    cause,
                    TaskCause::Unmet { .. }
                        | TaskCause::SessionRefused(_)
                        | TaskCause::Unrunnable { .. }
                        | TaskCause::SessionDied(_)
                )
        }
        TaskOutcome::Interrupted => false,
    };
    let owed = tasks.get(&report.task_id)?.departures_owed.clone();
    NonEmpty::new(owed)
        .filter(|_| asks)
        .map(|deviations| TaskCause::DeviationOwed { deviations })
}

/// The answer a departure's question takes. Each answer is parsed once.
enum DepartureAnswer {
    Accept,
    SendBack,
    Abort,
}

/// The answer a person gave a departure's question, or `None` for an option
/// no departure offers.
fn answer_of(choice: &HumanChoice) -> Option<DepartureAnswer> {
    match ReservedOption::of(&choice.option)? {
        ReservedOption::Accept => Some(DepartureAnswer::Accept),
        ReservedOption::SendBack => Some(DepartureAnswer::SendBack),
        ReservedOption::Abort => Some(DepartureAnswer::Abort),
        _ => None,
    }
}

/// Where a person's answer to a departure leaves the node: the departure is
/// accepted or sent back, the run pauses on the answer, or no departure
/// takes the answer.
enum Settled {
    Accepted(bool),
    Paused(NodeEnd),
    Unanswered,
}

/// Settles a departure on the answer a person gave. The run pauses on an
/// abort, and the question comes back on resume.
fn settle(node: &Node, choice: &HumanChoice) -> Settled {
    match answer_of(choice) {
        Some(DepartureAnswer::Accept) => Settled::Accepted(true),
        Some(DepartureAnswer::SendBack) => Settled::Accepted(false),
        Some(DepartureAnswer::Abort) => Settled::Paused(NodeEnd::Paused {
            reason: PauseReason::GateAborted {
                node: node.id.clone(),
                free_text: choice.free_text.clone(),
            },
        }),
        None => Settled::Unanswered,
    }
}

/// Puts each task's departures to a person and reopens the task on the
/// answer. Returns the node's end when any is left unanswered — no one
/// to ask, or an answer no departure takes — and the node fails owing it,
/// or when a person chose to stop, and the run pauses on that answer.
pub(super) async fn resolve_departures(
    ctx: &RunCtx<'_>,
    node: &Node,
    pending: Vec<PendingDeparture>,
    holders: &Holders<'_>,
    tokens: TokenUsage,
) -> Result<Option<NodeEnd>, RunError> {
    let mut unanswered: Vec<TaskStop> = Vec::new();
    for departure in pending {
        let escalation = escalation(&departure, holders);
        let state = ctx.run_view().await?.state;
        let Some((choice, resolved)) =
            decided(ctx, &state, &node.id, Some(&departure.task_id), &escalation).await?
        else {
            unanswered.push(owed_stop(&departure));
            continue;
        };
        let accepted = match settle(node, &choice) {
            Settled::Accepted(accepted) => accepted,
            Settled::Paused(end) => return Ok(Some(end)),
            Settled::Unanswered => {
                unanswered.push(owed_stop(&departure));
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
    owing(ctx, node, unanswered, tokens).await
}

/// The departures the ledger holds owed a person's answer: each blocked task
/// whose stop is a departure owed. A loop that finds nothing to run asks them
/// again, so a resume after an abort puts the same question.
pub(super) fn owed_by(stopped: &[TaskStop]) -> Vec<PendingDeparture> {
    stopped
        .iter()
        .filter_map(|stop| match &stop.cause {
            TaskCause::DeviationOwed { deviations } => Some(PendingDeparture {
                task_id: stop.task_id.clone(),
                deviations: deviations.clone(),
            }),
            _ => None,
        })
        .collect()
}

/// The stop a departure that still owes a person's answer leaves its task with.
fn owed_stop(departure: &PendingDeparture) -> TaskStop {
    TaskStop {
        task_id: departure.task_id.clone(),
        cause: TaskCause::DeviationOwed {
            deviations: departure.deviations.clone(),
        },
    }
}

/// The node's end while `unanswered` departures still owe a person's answer:
/// the node fails with each owed departure as its stop. `None` when none does.
async fn owing(
    ctx: &RunCtx<'_>,
    node: &Node,
    unanswered: Vec<TaskStop>,
    tokens: TokenUsage,
) -> Result<Option<NodeEnd>, RunError> {
    if unanswered.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        fail_with(
            ctx,
            node,
            Failure::TasksStopped {
                stopped: unanswered,
            },
            tokens,
        )
        .await?,
    ))
}

/// How the task goes on after the answer: accepted, from the work its
/// last attempt left, which a cycle judges before any session; sent
/// back, with the session that departed picking that work back up.
async fn reopened(
    ctx: &RunCtx<'_>,
    task: &TaskId,
    accepted: bool,
    resolved: Seq,
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

/// The question a departure puts to a person: the departure itself, as the
/// menu's data, and the answers that can change something. A departure from
/// a test the run's spec gave the task is accepted only where a node of the
/// run writes the spec again: accepted with nobody to rewrite the test, the
/// task would stay held to what everyone agreed is wrong.
fn escalation(departure: &PendingDeparture, holders: &Holders<'_>) -> Escalation {
    let accept_offered = !(holders.respecifies(departure) && holders.writer.is_none());
    let options = match accept_offered {
        true => NonEmpty::from((
            GateOption::reserved(ReservedOption::Accept),
            vec![
                GateOption::reserved(ReservedOption::SendBack),
                GateOption::reserved(ReservedOption::Abort),
            ],
        )),
        false => NonEmpty::from((
            GateOption::reserved(ReservedOption::SendBack),
            vec![GateOption::reserved(ReservedOption::Abort)],
        )),
    };
    let menu = DepartureMenu {
        deviations: departure.deviations.as_slice().to_vec(),
        waived: holders
            .of(departure)
            .into_iter()
            .filter(|(_, held)| *held == crate::tasks::HeldBy::Plan)
            .map(|(cmd, _)| cmd.to_string())
            .collect(),
        rewritten_by: holders
            .writer
            .filter(|_| holders.respecifies(departure))
            .cloned(),
        accept_offered,
    };
    Escalation::new(
        Claim::Departs {
            task: departure.task_id.clone(),
        },
        Menu::Departure(Box::new(menu)),
        options,
    )
}
