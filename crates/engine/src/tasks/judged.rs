//! The criteria a task is judged by: the ones its document declares, the
//! suite the run's lineage measured green before anything changed, the
//! tests the run's spec gives it — less the ones of its plan a person
//! accepted departing from.

use yunta_core::events::{
    AcceptedDeparture, BaselineCapturedPayload, CriterionType, DepartsFrom, TaskLedger,
};
use yunta_core::{Criterion, SpecFile, Task, TaskId, TasksFile};

/// What the lineage's suite shows when a task is held to it, in the words
/// the session reading its task and a person reading a blocked task see.
const SUITE_PROVES: &str =
    "what passed before this run changed anything still passes after this task's change";

/// `task` as the run judges it: its own criteria and, as a guard, the
/// suite that holds the run's tasks — `suite`, the one its lineage measured
/// green, or the one being measured while the run goes on.
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
pub fn judged_task(task: &Task, suite: Option<&str>) -> Task {
    let mut judged = task.clone();
    let Some(suite) = suite.map(str::trim) else {
        return judged;
    };
    if task
        .criteria
        .iter()
        .any(|criterion| criterion.cmd.trim() == suite)
    {
        return judged;
    }
    judged.criteria.push(Criterion {
        cmd: suite.to_string(),
        r#type: Some(CriterionType::Guard),
        proves: Some(SUITE_PROVES.to_string()),
    });
    judged
}

/// `task` held to its spec as well: each test the run's spec gives it,
/// among its criteria — none of them a guard, since its work is what
/// makes them pass. A test that runs what one of its own criteria runs
/// is that criterion already.
pub fn specified(mut task: Task, spec: Option<&SpecFile>) -> Task {
    let Some(spec) = spec.and_then(|spec| spec.of(&task.id)) else {
        return task;
    };
    for test in spec.criteria() {
        if !task
            .criteria
            .iter()
            .any(|criterion| criterion.cmd.trim() == test.cmd.trim())
        {
            task.criteria.push(test);
        }
    }
    task
}

/// What holds a task to one of its criteria — which is what a person
/// accepting a departure from it changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeldBy {
    /// The plan declares it and nothing else supplies it: accepting a
    /// departure from it waives it.
    Plan,
    /// The run's spec supplies it: its file is a test a person approved,
    /// denied to every session.
    Spec,
    /// The suite the run measured green before any work, which holds
    /// every task: no plan's to depart from.
    Suite,
}

/// The suite the run holds every task to: the command its lineage
/// measured, when that measurement passed.
pub fn suite_of(baseline: Option<&BaselineCapturedPayload>) -> Option<&str> {
    baseline
        .filter(|baseline| baseline.passed())
        .map(|baseline| baseline.command.trim())
}

/// What holds `task` to the criterion that runs `cmd`: the run's suite
/// first, then its spec, then the plan.
pub fn held_by(task: &TaskId, cmd: &str, suite: Option<&str>, spec: Option<&SpecFile>) -> HeldBy {
    let cmd = cmd.trim();
    if suite == Some(cmd) {
        return HeldBy::Suite;
    }
    let supplied = spec
        .and_then(|spec| spec.of(task))
        .is_some_and(|spec| spec.tests.iter().any(|test| test.cmd.trim() == cmd));
    match supplied {
        true => HeldBy::Spec,
        false => HeldBy::Plan,
    }
}

/// `task` without the criteria of its plan a person accepted departing
/// from: nothing else can rewrite such a criterion, so accepting the
/// departure is accepting that it no longer holds the task. A criterion
/// the suite or the spec supplies stays.
pub fn waived(
    mut task: Task,
    accepted: &[AcceptedDeparture],
    suite: Option<&str>,
    spec: Option<&SpecFile>,
) -> Task {
    let departed: Vec<&str> = accepted
        .iter()
        .filter_map(|accepted| match &accepted.declared.from {
            DepartsFrom::Criterion(cmd) => Some(cmd.trim()),
            _ => None,
        })
        .filter(|cmd| held_by(&task.id, cmd, suite, spec) == HeldBy::Plan)
        .collect();
    task.criteria
        .retain(|criterion| !departed.contains(&criterion.cmd.trim()));
    task
}

/// Every task of `plan` as the run judges it now: its own criteria, the
/// suite that holds the run's tasks, the tests its spec gives it, less
/// every criterion of the plan a person accepted departing from.
pub fn judged_plan(
    plan: &TasksFile,
    suite: Option<&str>,
    spec: Option<&SpecFile>,
    ledger: &TaskLedger,
) -> TasksFile {
    TasksFile {
        tasks: plan
            .tasks
            .iter()
            .map(|task| {
                let judged = specified(judged_task(task, suite), spec);
                let accepted = ledger
                    .get(&task.id)
                    .map_or(&[][..], |record| record.departures_accepted.as_slice());
                waived(judged, accepted, suite, spec)
            })
            .collect(),
        ..plan.clone()
    }
}
