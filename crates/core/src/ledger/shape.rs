//! What a task ledger is, in the vocabulary its own readers use: which
//! keys it declares, which a writer plausibly reaches for instead, and
//! the values its closed sets accept.

use crate::diagnostic::{Named, Subject};
use crate::shape::{Hint, Walk};
use crate::yaml::{Mapping, Value};
use crate::TaskId;

use super::EXAMPLE;

/// The keys a task declares.
///
/// Restated from [`crate::Task`] because deriving them would put schema
/// generation in the shipped binary, and a binary that stays small is a
/// feature. What keeps the restatement true is a test that reads the
/// generated JSON Schema and fails the moment a field is added to the
/// type without being listed here — the alternative is a walk that
/// reports a valid key as unknown, which is a diagnostic that lies.
pub(crate) const TASK_REQUIRED: &[&str] = &["id", "title", "scope", "criteria"];
pub(crate) const TASK_OPTIONAL: &[&str] =
    &["depends_on", "notes", "manual_review", "justification"];

/// Keys a writer reaches for that this schema spells differently.
const TASK_HINTS: &[Hint] = &[
    ("description", "notes", "a task's prose goes in `notes:`"),
    (
        "acceptance_criteria",
        "criteria",
        "verification commands go in `criteria:`",
    ),
    (
        "files",
        "scope",
        "the paths a task may touch are its `scope:`",
    ),
    (
        "dependencies",
        "depends_on",
        "a task names its prerequisites with `depends_on:`",
    ),
    (
        "status",
        "",
        "task status lives in the event log, never in the ledger",
    ),
    (
        "done",
        "",
        "task status lives in the event log, never in the ledger",
    ),
];

pub(crate) const CRITERION_REQUIRED: &[&str] = &["cmd"];
pub(crate) const CRITERION_OPTIONAL: &[&str] = &["type"];

pub(super) fn diagnose(value: &Value, walk: &mut Walk) {
    walk.entries(value, "tasks", EXAMPLE, task);
}

/// One task: named by its own id first, so every problem below carries
/// it, and by position when the id is what could not be read.
fn task(walk: &mut Walk, index: usize, item: &Value) {
    walk.at(Subject::Task(Named::new(None, index)));
    let Some(map) = walk.mapping(item, "a mapping", "- id: add-dark-mode") else {
        return;
    };
    let named = Named::new(walk.id::<TaskId>(map), index);
    walk.at(Subject::Task(named.clone()))
        .keys(map, TASK_REQUIRED, TASK_OPTIONAL, TASK_HINTS)
        .string(map, "title", "title: \"Add the toggle\"")
        .string(map, "notes", "notes: \"One line of context.\"")
        .string_list(map, "scope", "a list of globs", "scope: [\"src/**\"]")
        .string_list(
            map,
            "depends_on",
            "a list of task ids",
            "depends_on: [add-dark-mode]",
        )
        .boolean(map, "manual_review", "manual_review: true")
        .string(
            map,
            "justification",
            "justification: \"No command can settle this.\"",
        );
    criteria(walk, map, &named);
}

fn criteria(walk: &mut Walk, task: &Mapping, named: &Named<TaskId>) {
    let Some(value) = task.get("criteria") else {
        return;
    };
    walk.at(Subject::Task(named.clone()));
    let Some(items) = walk.sequence(
        value,
        "a list of criteria",
        "criteria:\n      - cmd: \"cargo test\"",
    ) else {
        return;
    };
    for (index, item) in items.iter().enumerate() {
        walk.at(Subject::Criterion {
            task: named.clone(),
            index,
        });
        let Some(map) = walk.mapping(item, "a mapping", "- cmd: \"cargo test\"") else {
            continue;
        };
        walk.keys(map, CRITERION_REQUIRED, CRITERION_OPTIONAL, &[])
            .string(map, "cmd", "cmd: \"cargo test\"")
            .one_of(map, "type", &["guard"], "type: guard");
    }
}
