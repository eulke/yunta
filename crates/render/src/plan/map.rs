//! A plan at a glance, before any task: each task in the step it runs
//! in, what proves it, how much of what it changes shows its code, and
//! a mark on each task that cannot be proven as it is written.

use yunta_core::shown::{HeldTo, PlanReview, TaskReview};
use yunta_core::text::counted;
use yunta_core::Task;

use crate::blocks::{Cell, Column, Holds, Row, Table};
use crate::ink::Tone;
use crate::Mark;

/// Every task of `review`, step by step.
pub(super) fn map(review: &PlanReview, steps: &[Vec<&Task>]) -> Table {
    let flawed: Vec<_> = review.flaws();
    let rows = steps
        .iter()
        .enumerate()
        .flat_map(|(at, step)| step.iter().map(move |task| (at + 1, *task)))
        .map(|(step, task)| {
            let judged = review.tasks.iter().find(|judged| judged.task == task.id);
            Row {
                mark: flawed
                    .iter()
                    .any(|flaw| *flaw.task() == task.id)
                    .then_some(Mark::Caution),
                cells: vec![
                    Cell::plain(step.to_string()),
                    Cell::plain(task.id.as_str()),
                    proven(judged),
                    coded(task, review),
                    Cell::plain(task.title.as_str()),
                ],
            }
        })
        .collect();
    Table {
        columns: vec![
            Column {
                title: "step",
                holds: Holds::Words,
            },
            Column {
                title: "task",
                holds: Holds::Id,
            },
            Column {
                title: "proven by",
                holds: Holds::Words,
            },
            Column {
                title: "code",
                holds: Holds::Words,
            },
            Column {
                title: "",
                holds: Holds::Rest,
            },
        ],
        rows,
    }
}

/// What proves a task: the spec's tests that run a file the spec wrote,
/// another task's test when it borrows one, or the plan's own checks
/// when the spec holds the task to nothing.
fn proven(judged: Option<&TaskReview>) -> Cell {
    let Some(judged) = judged else {
        return Cell::plain("");
    };
    let borrowed = judged
        .criteria
        .iter()
        .find_map(|criterion| match &criterion.from {
            HeldTo::AnotherTask { task } => Some(task),
            HeldTo::Plan | HeldTo::Spec => None,
        });
    if let Some(owner) = borrowed {
        return Cell::toned(Tone::Caution, format!("`{owner}`'s test"));
    }
    let specified = judged
        .criteria
        .iter()
        .any(|criterion| matches!(criterion.from, HeldTo::Spec))
        || !judged.files.is_empty();
    match (specified, judged.spec_tests_that_run()) {
        (true, 0) => Cell::toned(Tone::Caution, "no test runs its spec"),
        (true, tests) => Cell::plain(counted(tests, "spec test")),
        (false, _) => Cell::plain(counted(
            judged
                .criteria
                .iter()
                .filter(|criterion| matches!(criterion.from, HeldTo::Plan))
                .count(),
            "plan check",
        )),
    }
}

/// How many of a task's changes show the code the plan writes for them.
fn coded(task: &Task, review: &PlanReview) -> Cell {
    let total = task.changes.len();
    if total == 0 {
        return Cell::plain("");
    }
    let with = task
        .changes
        .iter()
        .filter(|change| {
            change.code.is_some()
                || review
                    .plan
                    .shapes
                    .iter()
                    .any(|shape| shape.owner == task.id && shape.file == change.file())
        })
        .count();
    let said = format!("code {with}/{total}");
    match with < total {
        true => Cell::toned(Tone::Caution, said),
        false => Cell::toned(Tone::Muted, said),
    }
}
