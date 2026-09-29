//! The criteria a task is judged by: the ones its document declares, and
//! the suite the run's lineage measured green before anything changed.

use yunta_core::events::{BaselineCapturedPayload, CriterionType};
use yunta_core::{Criterion, Task};

/// What the lineage's suite shows when a task is held to it, in the words
/// the session reading its task and a person reading a blocked task see.
const SUITE_PROVES: &str =
    "what passed before this run changed anything still passes after this task's change";

/// `task` as the run judges it: its own criteria and, when the run's
/// lineage measured its suite green, that suite as a guard.
///
/// A change that breaks what passed is then judged against the task that
/// made it, while the session that made it can still answer for it —
/// not after every task closed, when nobody who knows why the change was
/// made is left. A measurement that was already red holds nothing, since
/// no task could keep green what never was; and a task that declares the
/// suite itself is judged by its own declaration.
///
/// The document the planner wrote stays as it is: this is how the run
/// judges the task, not what the task says.
pub fn judged_task(task: &Task, baseline: Option<&BaselineCapturedPayload>) -> Task {
    let mut judged = task.clone();
    let Some(baseline) = baseline.filter(|baseline| baseline.passed()) else {
        return judged;
    };
    let suite = baseline.command.trim();
    if task
        .criteria
        .iter()
        .any(|criterion| criterion.cmd.trim() == suite)
    {
        return judged;
    }
    judged.criteria.push(Criterion {
        cmd: baseline.command.clone(),
        r#type: Some(CriterionType::Guard),
        proves: Some(SUITE_PROVES.to_string()),
    });
    judged
}
