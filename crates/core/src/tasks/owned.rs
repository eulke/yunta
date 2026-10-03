//! The rules that make a plan's shapes, decisions and changes mean what
//! they say: a shape has one owner, whose scope covers the file it lives
//! in, and every task that builds on it waits for that owner; a change a
//! task declares lies inside that task's scope.
//!
//! Each of them is a way a plan can hand one task something another
//! task was meant to build — the shape declared whole in the design,
//! then built by a task whose scope leaves out half of what it needs.

use std::collections::{HashMap, HashSet};

use crate::diagnostic::{Diagnostic, Named, Problem, RuleCode, Subject};
use crate::glob::scope_globset;
use crate::{Task, TaskId, TasksFile};

/// Every violation of these rules the document carries.
pub(super) fn check(tasks: &TasksFile) -> Vec<Diagnostic> {
    let mut broken = duplicates(tasks);
    broken.extend(owners(tasks));
    broken.extend(uses(tasks));
    for (index, task) in tasks.tasks.iter().enumerate() {
        broken.extend(changes(index, task));
    }
    broken
}

/// A shape named twice, or a decision given an id twice.
fn duplicates(tasks: &TasksFile) -> Vec<Diagnostic> {
    let mut broken = Vec::new();
    let mut shapes = HashSet::new();
    for shape in &tasks.shapes {
        if !shapes.insert(shape.name.as_str()) {
            broken.push(document(
                RuleCode::DuplicateShape,
                format!(
                    "shape `{}` is declared twice; every shape is declared once",
                    shape.name
                ),
            ));
        }
    }
    let mut decisions = HashSet::new();
    for decision in &tasks.decisions {
        if !decisions.insert(decision.id.as_str()) {
            broken.push(document(
                RuleCode::DuplicateDecision,
                format!(
                    "decision `{}` is declared twice; every decision has an id of its own",
                    decision.id
                ),
            ));
        }
    }
    broken
}

/// A shape whose owner nobody declared, or whose file its owner may not
/// touch.
fn owners(tasks: &TasksFile) -> Vec<Diagnostic> {
    let mut broken = Vec::new();
    for shape in &tasks.shapes {
        let Some(owner) = tasks.tasks.iter().find(|task| task.id == shape.owner) else {
            broken.push(document(
                RuleCode::UnknownShapeOwner,
                format!(
                    "shape `{}` is owned by `{}`, which no task in this file declares",
                    shape.name, shape.owner
                ),
            ));
            continue;
        };
        if !covers(owner, &shape.file) {
            broken.push(document(
                RuleCode::ShapeOutsideOwnerScope,
                format!(
                    "shape `{}` lives in `{}`, which its owner `{}`'s scope does not cover; \
                     the task that builds a shape may write its file",
                    shape.name, shape.file, shape.owner
                ),
            ));
        }
    }
    broken
}

/// A shape a task uses that nobody declared, or whose owner the task
/// does not wait for.
fn uses(tasks: &TasksFile) -> Vec<Diagnostic> {
    let owners: HashMap<&str, &TaskId> = tasks
        .shapes
        .iter()
        .map(|shape| (shape.name.as_str(), &shape.owner))
        .collect();
    let waits_for = waits_for(&tasks.tasks);
    let mut broken = Vec::new();
    for (index, task) in tasks.tasks.iter().enumerate() {
        for name in &task.uses {
            match owners.get(name.as_str()) {
                None => broken.push(of_task(
                    index,
                    task,
                    RuleCode::UnknownShape,
                    format!("`uses` names shape `{name}`, which `shapes` does not declare"),
                )),
                Some(owner) if **owner != task.id && !waits_for.contains(&(&task.id, *owner)) => {
                    broken.push(of_task(
                        index,
                        task,
                        RuleCode::ShapeUsedBeforeItsOwner,
                        format!(
                            "uses shape `{name}`, which `{owner}` builds, without waiting for \
                             it; add `{owner}` to `depends_on`"
                        ),
                    ))
                }
                Some(_) => {}
            }
        }
    }
    broken
}

/// A change a task declares in a file its scope does not cover.
fn changes(index: usize, task: &Task) -> Vec<Diagnostic> {
    task.changes
        .iter()
        .filter(|change| !covers(task, change.file()))
        .map(|change| {
            of_task(
                index,
                task,
                RuleCode::ChangeOutsideScope,
                format!(
                    "changes `{}`, which its scope does not cover; widen the scope, or leave \
                     the change to the task that owns the file",
                    change.file()
                ),
            )
        })
        .collect()
}

/// Whether `task`'s scope lets it write `file`.
fn covers(task: &Task, file: &str) -> bool {
    scope_globset(&task.scope).is_ok_and(|set| set.is_match(file))
}

/// Every `(task, earlier)` pair where `task` waits, directly or through
/// others, for `earlier`.
fn waits_for(tasks: &[Task]) -> HashSet<(&TaskId, &TaskId)> {
    let depends: HashMap<&TaskId, &[TaskId]> = tasks
        .iter()
        .map(|task| (&task.id, task.depends_on.as_slice()))
        .collect();
    let mut pairs = HashSet::new();
    for task in tasks {
        let mut stack: Vec<&TaskId> = task.depends_on.iter().collect();
        while let Some(earlier) = stack.pop() {
            if pairs.insert((&task.id, earlier)) {
                stack.extend(depends.get(earlier).copied().unwrap_or_default());
            }
        }
    }
    pairs
}

fn document(code: RuleCode, detail: String) -> Diagnostic {
    Diagnostic::new(Subject::Document, Problem::rule(code, detail))
}

fn of_task(index: usize, task: &Task, code: RuleCode, detail: String) -> Diagnostic {
    Diagnostic::new(
        Subject::Task(Named::new(task.id.clone(), index)),
        Problem::rule(code, detail),
    )
}
