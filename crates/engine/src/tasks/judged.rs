//! The criteria a task is judged by: the ones its document declares, the
//! commands the run's lineage measured green before anything changed —
//! its suite, and each command `baseline.held` names — the tests the
//! run's spec gives it, less the ones of its plan a person accepted
//! departing from.

use yunta_core::events::{AcceptedDeparture, CriterionType, DepartsFrom, RunLedger, TaskLedger};
use yunta_core::{Criterion, SpecFile, Task, TaskId, TasksFile};

/// What a command the lineage measured green shows when a task is held to
/// it, in the words the session reading its task and a person reading a
/// blocked task see.
const HELD_PROVES: &str =
    "what passed before this run changed anything still passes after this task's change";

/// `task` as the run judges it: its own criteria and, as guards, the
/// commands that hold the run's tasks — `held`, the ones its lineage
/// measured green, and the ones being measured while the run goes on.
///
/// A change that breaks what passed is then judged against the task that
/// made it, while the session that made it can still answer for it —
/// not after every task closed, when nobody who knows why the change was
/// made is left. A measurement that was already red holds nothing, since
/// no task could keep green what never was; and a task that declares one
/// of them itself is judged by its own declaration.
///
/// The document the planner wrote stays as it is: this is how the run
/// judges the task, not what the task says.
pub fn judged_task(task: &Task, held: &[String]) -> Task {
    let mut judged = task.clone();
    for command in held.iter().map(|command| command.trim()) {
        if judged
            .criteria
            .iter()
            .any(|criterion| criterion.cmd.trim() == command)
        {
            continue;
        }
        judged.criteria.push(Criterion {
            cmd: command.to_string(),
            r#type: Some(CriterionType::Guard),
            proves: Some(HELD_PROVES.to_string()),
        });
    }
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
    /// A command the run measured green before any work — its suite, or
    /// one `baseline.held` names — which holds every task: no plan's to
    /// depart from.
    Baseline,
}

/// What the lineage measured of the commands that hold its tasks: each
/// command `baseline.held` names, then the suite, with whether it passed.
/// The cheaper held commands come first, so a red one answers a check
/// before the suite runs.
pub fn measured(run: &RunLedger) -> Vec<(String, bool)> {
    run.held()
        .values()
        .map(|held| (held.command.trim().to_string(), held.passed()))
        .chain(
            run.baseline()
                .map(|suite| (suite.command.trim().to_string(), suite.passed())),
        )
        .collect()
}

/// The commands the run holds every task to: each its lineage measured
/// green.
pub fn held_of(run: &RunLedger) -> Vec<String> {
    measured(run)
        .into_iter()
        .filter(|(_, passed)| *passed)
        .map(|(command, _)| command)
        .collect()
}

/// What holds `task` to the criterion that runs `cmd`: the commands the
/// run measured first, then its spec, then the plan.
pub fn held_by(task: &TaskId, cmd: &str, held: &[String], spec: Option<&SpecFile>) -> HeldBy {
    let cmd = cmd.trim();
    if held.iter().any(|command| command.trim() == cmd) {
        return HeldBy::Baseline;
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
/// the measured commands or the spec supply stays.
pub fn waived(
    mut task: Task,
    accepted: &[AcceptedDeparture],
    held: &[String],
    spec: Option<&SpecFile>,
) -> Task {
    let departed: Vec<&str> = accepted
        .iter()
        .filter_map(|accepted| match &accepted.declared.from {
            DepartsFrom::Criterion(cmd) => Some(cmd.trim()),
            _ => None,
        })
        .filter(|cmd| held_by(&task.id, cmd, held, spec) == HeldBy::Plan)
        .collect();
    task.criteria
        .retain(|criterion| !departed.contains(&criterion.cmd.trim()));
    task
}

/// Every task of `plan` as the run judges it now: its own criteria, the
/// commands that hold the run's tasks, the tests its spec gives it, less
/// every criterion of the plan a person accepted departing from.
pub fn judged_plan(
    plan: &TasksFile,
    held: &[String],
    spec: Option<&SpecFile>,
    ledger: &TaskLedger,
) -> TasksFile {
    TasksFile {
        tasks: plan
            .tasks
            .iter()
            .map(|task| {
                let judged = specified(judged_task(task, held), spec);
                let accepted = ledger
                    .get(&task.id)
                    .map_or(&[][..], |record| record.departures_accepted.as_slice());
                waived(judged, accepted, held, spec)
            })
            .collect(),
        ..plan.clone()
    }
}
