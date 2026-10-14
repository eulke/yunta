//! The words of a loop and of the tasks it runs: the tasks that stopped it,
//! a plan no work can pass, a loop with no task ready, a run that holds no
//! tasks document for its loop, and a loop that reached its iteration cap.

use yunta_core::events::{TaskStop, UnpassableTask};
use yunta_core::text;
use yunta_core::NodeId;

use crate::pause::exceeded_loop_cap;

/// The loop's words: the line that says why no task of it is ready, then
/// each stopped task and what stopped it.
pub(super) fn tasks_stopped(stopped: &[TaskStop]) -> String {
    let mut said = loop_stalled();
    for stop in stopped {
        said.push_str("; ");
        said.push_str(&text::detailed(
            format!("task `{}` blocked", stop.task_id),
            &crate::blocks::stop::sentence(&stop.cause),
        ));
    }
    said
}

/// No task of the loop is ready, and not all are done: a decision is owed.
pub(super) fn loop_stalled() -> String {
    "no task is ready and not all are done — blocked or failed tasks need a decision".to_string()
}

/// The run holds no tasks document for the loop to read its tasks from.
pub(super) fn no_tasks_document() -> String {
    "this run holds no tasks document — an earlier node declares `produces: [tasks]`, or the \
     workflow declares a `type: document` input of `kind: tasks` and the run is given one"
        .to_string()
}

/// The plan's unpassable criteria: each task, its criterion and why it cannot pass.
pub(super) fn plan(unpassable: &[UnpassableTask]) -> String {
    format!(
        "the plan holds a criterion no work can pass — {}",
        unpassable
            .iter()
            .map(|task| format!("task `{}`: `{}` ({})", task.task_id, task.cmd, task.why))
            .collect::<Vec<_>>()
            .join("; ")
    )
}

/// A loop that reached the iteration cap its run declares.
pub(super) fn loop_capped(node: &NodeId, cap: u32) -> String {
    format!("loop `{node}` {}", exceeded_loop_cap(cap))
}
