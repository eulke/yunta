//! What a person's `retry` of a failed loop does to the tasks it left
//! blocked: they start a fresh cycle, once per decision.

use yunta_core::events::{
    EventPayload, GateResolvedPayload, TaskEvent, TaskStatus, TaskStatusChangedPayload,
};
use yunta_core::{Node, NodeId, Seq, TasksFile};

use crate::replay::RunState;
use crate::reserved::ReservedOption;
use crate::run::{RunCtx, RunError};

/// A person chose `retry` for this loop after it failed: every task of
/// its document the failure left blocked starts a fresh cycle, citing
/// that decision.
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
    let Some(decision) = retry_chosen_after_failure(&view.state, &node.id) else {
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
        if view.state.tasks.status(&task.id) == Some(TaskStatus::Blocked) {
            ctx.emit(
                Some(&node.id),
                EventPayload::Tasks(TaskEvent::StatusChanged(TaskStatusChangedPayload::to(
                    task.id.clone(),
                    TaskStatus::Pending,
                    decision,
                ))),
            )
            .await?;
        }
    }
    Ok(())
}

/// Where the log holds the `retry` a person chose for `node` after its
/// latest failure, if its latest decision is one.
fn retry_chosen_after_failure(state: &RunState, node: &NodeId) -> Option<Seq> {
    let failed = state.nodes.get(node)?.last_failed?;
    let (resolution, at) = state.gates.get(node)?.resolved.last()?;
    let GateResolvedPayload::Chosen(choice) = resolution else {
        return None;
    };
    (ReservedOption::of(&choice.option) == Some(ReservedOption::Retry) && *at > failed)
        .then_some(*at)
}
