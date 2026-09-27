//! What a person's choice to run a failed loop again does to the tasks
//! it left blocked: they start a fresh cycle, once per decision — from
//! the run's tree, or from the work their last attempt left.

use yunta_core::events::{
    EventPayload, GateResolvedPayload, TaskEvent, TaskStatus, TaskStatusChangedPayload,
};
use yunta_core::{Node, NodeId, Seq, TasksFile};

use crate::replay::RunState;
use crate::reserved::ReservedOption;
use crate::run::{RunCtx, RunError};

/// A person chose to run this loop again after it failed: every task of
/// its document the failure left blocked starts a fresh cycle, citing
/// that decision. Under `continue-work`, a task whose last attempt left
/// work continues from it; under `retry`, and for a task that left
/// nothing, the cycle starts from the run's tree alone.
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
    let view = ctx.run_view().await?;
    let Some((decision, chosen)) = run_again_after_failure(&view.state, &node.id) else {
        return Ok(());
    };
    let acted_on = view.events.iter().any(|event| {
        matches!(
            event.payload(),
            Some(EventPayload::Tasks(TaskEvent::StatusChanged(p))) if p.caused_by == decision
        )
    });
    if acted_on {
        return Ok(());
    }
    for task in &tasks.tasks {
        let Some(record) = view.state.tasks.get(&task.id) else {
            continue;
        };
        if record.status != TaskStatus::Blocked {
            continue;
        }
        let left_work = record
            .left_work
            .as_ref()
            .filter(|(by, _)| by == &node.id && chosen == ReservedOption::ContinueWork)
            .map(|(_, work)| work.clone());
        let reopened = match left_work {
            Some(work) => TaskStatusChangedPayload::continuing(task.id.clone(), decision, work),
            None => TaskStatusChangedPayload::to(task.id.clone(), TaskStatus::Pending, decision),
        };
        ctx.emit(
            Some(&node.id),
            EventPayload::Tasks(TaskEvent::StatusChanged(reopened)),
        )
        .await?;
    }
    Ok(())
}

/// Where the log holds the choice a person made to run `node` again
/// after its latest failure, and which one it was — if its latest
/// decision is one.
fn run_again_after_failure(state: &RunState, node: &NodeId) -> Option<(Seq, ReservedOption)> {
    let failed = state.nodes.get(node)?.last_failed?;
    let (resolution, at) = state.gates.get(node)?.resolved.last()?;
    let GateResolvedPayload::Chosen(choice) = resolution else {
        return None;
    };
    let chosen = ReservedOption::of(&choice.option).filter(|option| option.runs_again())?;
    (*at > failed).then_some((*at, chosen))
}
