//! What a person's choice to run a failed loop again does to the tasks
//! it left blocked: they start a fresh cycle, once per decision — from
//! the run's tree, or from the work their last attempt left — and a task
//! whose scope request the person answered goes back to the session that
//! asked. A task a limit stopped goes back to its session once the
//! limit's reset has come, with no person.

use chrono::{DateTime, Utc};
use yunta_core::events::{
    EventPayload, Failure, TaskCause, TaskEvent, TaskRecord, TaskStatus, TaskStatusChangedPayload,
};
use yunta_core::{Node, NodeId, Seq, TaskId, TasksFile};

use crate::replay::RunState;
use crate::run::{RunCtx, RunError};
use yunta_core::reserved::ReservedOption;

/// A person chose to run this loop again after it failed: every task of
/// its document the failure left blocked starts a fresh cycle, citing
/// that decision. A task whose scope request the loop ended owing goes
/// back to the session that asked, granted or refused ([`after_answer`]).
/// Under `continue-work`, and under a `grant` that widened the node's
/// scope, a task whose last attempt left work continues from it; under
/// `retry`, and for a task that left nothing, the cycle starts from the
/// run's tree alone. An answer to the departures the loop owed reopens
/// nothing here: the loop asks each departure again, the answer the log
/// holds settles it, and that reopens its task.
///
/// Once per decision. A restart of this same attempt finds the decision
/// already cited and leaves alone what this attempt blocked in turn, and
/// a plain resume — no decision — never reopens anything: spending again
/// on a task is a person's call, never the engine's.
pub(super) async fn after_retry(
    ctx: &RunCtx<'_>,
    node: &Node,
    tasks: &TasksFile,
) -> Result<(), RunError> {
    let state = ctx.run_view().await?.state;
    let Some((decision, chosen)) = run_again_after_failure(&state, &node.id) else {
        return Ok(());
    };
    if state.tasks.caused_by(decision)
        || matches!(chosen, ReservedOption::Accept | ReservedOption::SendBack)
    {
        return Ok(());
    }
    let owed = owed_tasks(&state, &node.id);
    for task in &tasks.tasks {
        let Some(record) = state.tasks.get(&task.id) else {
            continue;
        };
        if record.status != TaskStatus::Blocked {
            continue;
        }
        let left_work = record
            .left_work
            .as_ref()
            .filter(|(by, _)| by == &node.id && chosen.continues_work())
            .map(|(_, work)| work.clone());
        let reopened = match (owed.contains(&task.id), left_work) {
            (true, _) => after_answer(Some(record), task.id.clone(), decision),
            (false, Some(work)) => {
                TaskStatusChangedPayload::continuing(task.id.clone(), decision, work)
            }
            (false, None) => {
                TaskStatusChangedPayload::to(task.id.clone(), TaskStatus::Pending, decision)
            }
        };
        ctx.emit(
            Some(&node.id),
            EventPayload::Tasks(TaskEvent::StatusChanged(reopened)),
        )
        .await?;
    }
    Ok(())
}

/// The tasks a limit stopped go back to work once the limit's reset has
/// come, with no person: a limit that named its reset resumes on its own,
/// and the reset is the one thing that changed for them. Each picks its
/// work back up in the session the limit cut ([`after_answer`]). Once per
/// attempt: a restart of this same attempt finds them reopened already.
pub(super) async fn after_reset(
    ctx: &RunCtx<'_>,
    node: &Node,
    tasks: &TasksFile,
) -> Result<(), RunError> {
    let state = ctx.run_view().await?.state;
    let Some((started, at)) = state
        .nodes
        .get(&node.id)
        .and_then(|record| record.open_since)
    else {
        return Ok(());
    };
    if state.tasks.caused_by(started) {
        return Ok(());
    }
    for task in &tasks.tasks {
        let Some(record) = state
            .tasks
            .get(&task.id)
            .filter(|record| reset_came(record, at))
        else {
            continue;
        };
        ctx.emit(
            Some(&node.id),
            EventPayload::Tasks(TaskEvent::StatusChanged(after_answer(
                Some(record),
                task.id.clone(),
                started,
            ))),
        )
        .await?;
    }
    Ok(())
}

/// Whether `record` is a task a limit stopped whose reset had come by `at`.
fn reset_came(record: &TaskRecord, at: DateTime<Utc>) -> bool {
    record.status == TaskStatus::Blocked && limit_reset_by(record.stop.as_ref(), at)
}

/// Whether `stop` is a limit that named its reset, and that reset had come
/// by `at`. A limit that named none waits for a person.
fn limit_reset_by(stop: Option<&TaskCause>, at: DateTime<Utc>) -> bool {
    matches!(
        stop,
        Some(TaskCause::SessionLimited(limit)) if limit.resets_at.is_some_and(|reset| reset <= at)
    )
}

/// How a task blocked on a scope request a person answered goes back to
/// work: the session that asked picks its work back up where it left it,
/// since the answer is the one thing that changed for it; with no work or
/// no session on the log, the task starts over. The answer goes back to
/// work this way whether the person gave it while the loop ran or from the
/// failure the loop ended owing it on.
pub(super) fn after_answer(
    record: Option<&TaskRecord>,
    task: TaskId,
    decided: Seq,
) -> TaskStatusChangedPayload {
    match (
        record.and_then(|record| record.left_work.as_ref()),
        record.and_then(|record| record.last_session.clone()),
    ) {
        (Some((_, work)), Some(session)) => {
            TaskStatusChangedPayload::resuming(task, decided, work.clone(), session)
        }
        _ => TaskStatusChangedPayload::to(task, TaskStatus::Pending, decided),
    }
}

/// The tasks whose scope requests `node`'s loop ended owing a person, when
/// that is how it failed last. Read from the failures the node recorded:
/// by the time the loop reopens its tasks, the attempt the answer starts is
/// already open.
fn owed_tasks(state: &RunState, node: &NodeId) -> Vec<TaskId> {
    match state
        .nodes
        .get(node)
        .and_then(|record| record.failures.last())
    {
        Some(Failure::ScopeOwed { owed }) => {
            owed.iter().map(|request| request.task_id.clone()).collect()
        }
        _ => Vec::new(),
    }
}

/// Where the log holds the choice a person made to run `node` again
/// after its latest failure, and which one it was — if its latest
/// decision is one.
fn run_again_after_failure(state: &RunState, node: &NodeId) -> Option<(Seq, ReservedOption)> {
    let (at, choice) = state.choice_after_failure(node)?;
    let chosen = ReservedOption::of(&choice.option).filter(|option| option.runs_again())?;
    Some((at, chosen))
}

#[cfg(test)]
mod tests {
    use super::*;
    use yunta_core::events::{AccountLimit, LimitSource};

    fn limited(resets_at: Option<DateTime<Utc>>) -> TaskCause {
        TaskCause::SessionLimited(AccountLimit {
            source: LimitSource::Session {
                adapter: "claude-code".into(),
            },
            resets_at,
            message: "You've hit your session limit".to_string(),
        })
    }

    #[test]
    fn a_task_a_limit_stopped_goes_back_once_its_reset_has_come() {
        let reset = yunta_testkit_core::fixed_now();
        let later = reset + chrono::Duration::seconds(1);
        assert!(limit_reset_by(Some(&limited(Some(reset))), reset));
        assert!(limit_reset_by(Some(&limited(Some(reset))), later));
        assert!(!limit_reset_by(Some(&limited(Some(later))), reset));
        assert!(!limit_reset_by(Some(&limited(None)), later));
        assert!(!limit_reset_by(Some(&TaskCause::ScopeDecisionOwed), later));
    }
}
