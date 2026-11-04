//! The steps a plan takes: each task in the step it runs in, by its id and
//! what it does, and the order its tasks wait on each other in, drawn.

use surco::components::{Cell, Column, Holds, Row, Table};
use surco::diagram::Diagram;
use surco::doc::{Block, Doc};
use yunta_core::Task;

use super::{blank, titled};

/// `doc` with the plan's `steps`: an index of every task under the step it
/// runs in, each title whole, and a drawing of the order its tasks wait on
/// each other in when any does.
pub(super) fn steps(doc: Doc, steps: &[Vec<&Task>]) -> Doc {
    let rows = steps
        .iter()
        .enumerate()
        .flat_map(|(at, step)| {
            step.iter().enumerate().map(move |(n, task)| Row {
                mark: None,
                cells: vec![
                    Cell::plain(match n {
                        0 => (at + 1).to_string(),
                        _ => String::new(),
                    }),
                    Cell::plain(task.id.as_str()),
                    Cell::plain(task.title.as_str()),
                ],
            })
        })
        .collect();
    let index = Table {
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
                title: "what it does",
                holds: Holds::Prose,
            },
        ],
        rows,
    };
    let mut blocks: Vec<Block> = vec![index.into()];
    let ids: Vec<&str> = steps
        .iter()
        .flatten()
        .map(|task| task.id.as_str())
        .collect();
    if let Some(order) = order(steps, &ids) {
        blocks.push(blank());
        blocks.push(Block::Diagram(order));
    }
    doc.with(titled("steps", blocks))
}

/// The order a plan's tasks wait on each other in, drawn: a box for each
/// task, labelled by `labels` in the order the steps hold the tasks, and an
/// arrow from each to every task that waits on it. `None` when no task waits
/// on another: every task is in one step, which the index says.
pub(super) fn order(steps: &[Vec<&Task>], labels: &[&str]) -> Option<Diagram> {
    let tasks: Vec<&Task> = steps.iter().flatten().copied().collect();
    let place = |id: &yunta_core::TaskId| tasks.iter().position(|task| task.id == *id);
    let links: Vec<(usize, usize)> = tasks
        .iter()
        .enumerate()
        .flat_map(|(at, task)| {
            task.depends_on
                .iter()
                .filter_map(move |dependency| Some((place(dependency)?, at)))
        })
        .collect();
    if links.is_empty() {
        return None;
    }
    Some(Diagram::linked(labels, &links))
}

/// What tells `ids` apart: each without the words they all open with, where they share
/// any and each has something left. `global-pack-storage` and `global-pack-cli` are
/// `storage` and `cli`; `T001` and `T002` share no word and stay as they are.
pub(super) fn short_names(ids: &[&str]) -> Vec<String> {
    let first = ids.first().copied().unwrap_or_default();
    let shared = first
        .match_indices('-')
        .map(|(at, _)| at + 1)
        .rfind(|at| {
            ids.iter()
                .all(|id| id.len() > *at && id.starts_with(&first[..*at]))
        })
        .unwrap_or(0);
    let cut = if ids.len() > 1 { shared } else { 0 };
    ids.iter().map(|id| id[cut..].to_string()).collect()
}
