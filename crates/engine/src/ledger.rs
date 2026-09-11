//! The ledger's registration rules.
//!
//! Shape is `yunta-core`'s frontier: by the time a `Ledger` exists, every
//! key is known and every value has its type. What is left are the rules
//! that only hold across a whole document — an id used twice, a
//! dependency on a task nobody declared, two independent tasks reaching
//! for the same files.
//!
//! Every rule reports a [`Diagnostic`] with the task as its subject and
//! a stable code, so the whole document's problems reach a reader as one
//! list and a receipt can count them without reading prose. What is
//! **not** validated here: whether a criterion's command exists or is
//! correct. The red pre-check answers that by running it, which is where
//! a trivial or broken criterion actually gives itself away.

use std::collections::{BTreeMap, HashMap, HashSet};

use yunta_core::diagnostic::{Diagnostic, Problem, Subject};
use yunta_core::{Ledger, Task, TaskId};

/// Names the task a rule blames, keeping its position for the rendering
/// that needs it.
fn task_subject(index: usize, id: &TaskId) -> Subject {
    Subject::Task {
        id: Some(id.clone()),
        index,
    }
}

fn broke(index: usize, id: &TaskId, code: &'static str, detail: impl Into<String>) -> Diagnostic {
    Diagnostic::new(task_subject(index, id), Problem::rule(code, detail))
}

/// Validates a parsed ledger against every rule, collecting every
/// violation rather than stopping at the first — whoever wrote this
/// corrects once, not once per round.
pub fn register(ledger: &Ledger) -> Vec<Diagnostic> {
    let known_ids: HashSet<TaskId> = ledger.tasks.iter().map(|task| task.id.clone()).collect();
    let mut errors = duplicate_ids(ledger);
    for (index, task) in ledger.tasks.iter().enumerate() {
        errors.extend(task_rules(index, task, &known_ids));
    }
    errors.extend(cycle(ledger));
    errors.extend(overlapping_scopes(&ledger.tasks));
    errors
}

/// An id used twice: a rule about the document, reported on the second
/// task to carry it — the one a reader has to change.
fn duplicate_ids(ledger: &Ledger) -> Vec<Diagnostic> {
    let mut seen: HashSet<&TaskId> = HashSet::new();
    ledger
        .tasks
        .iter()
        .enumerate()
        .filter(|(_, task)| !seen.insert(&task.id))
        .map(|(index, task)| {
            broke(
                index,
                &task.id,
                "duplicate-id",
                "a second task already carries this id; every id is declared once",
            )
        })
        .collect()
}

/// Everything one task has to satisfy on its own.
fn task_rules(index: usize, task: &Task, known_ids: &HashSet<TaskId>) -> Vec<Diagnostic> {
    let mut errors = Vec::new();
    for dep in &task.depends_on {
        if !known_ids.contains(dep) {
            errors.push(broke(
                index,
                &task.id,
                "unknown-dependency",
                format!("`depends_on` names `{dep}`, which no task in this file declares"),
            ));
        }
    }
    if task.title.trim().is_empty() {
        errors.push(broke(index, &task.id, "empty-title", "`title` is empty"));
    }
    if task.scope.is_empty() {
        errors.push(broke(
            index,
            &task.id,
            "empty-scope",
            "`scope` is empty; every task declares at least one glob, the only paths it \
             may touch",
        ));
    }
    errors.extend(criteria_rules(index, task));
    if task.manual_review
        && task
            .justification
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
    {
        errors.push(broke(
            index,
            &task.id,
            "manual-review-without-justification",
            "`manual_review: true` without `justification`; say why no command can verify \
             this task",
        ));
    }
    errors
}

/// What a task's `criteria` have to be for the red pre-check to mean
/// anything: at least one, and at least one that can fail before the
/// work starts.
fn criteria_rules(index: usize, task: &Task) -> Option<Diagnostic> {
    if task.criteria.is_empty() {
        return Some(broke(
            index,
            &task.id,
            "no-criteria",
            "no criteria declared; every task needs at least one command that verifies it",
        ));
    }
    let all_guards = task
        .criteria
        .iter()
        .all(|c| c.r#type == Some(yunta_core::events::CriterionType::Guard));
    all_guards.then(|| {
        broke(
            index,
            &task.id,
            "all-criteria-are-guards",
            "every criterion is a `guard`; at least one must be able to fail before the work, \
             or there is nothing the work has to make pass",
        )
    })
}

/// A cycle is a property of the whole graph, so the document carries it
/// rather than any one task in the loop.
fn cycle(ledger: &Ledger) -> Option<Diagnostic> {
    let cycle = find_cycle(&ledger.tasks)?;
    let path = cycle
        .iter()
        .map(TaskId::as_str)
        .collect::<Vec<_>>()
        .join(" -> ");
    Some(Diagnostic::new(
        Subject::Document,
        Problem::rule(
            "dependency-cycle",
            format!("`depends_on` forms a cycle: {path}"),
        ),
    ))
}

fn find_cycle(tasks: &[Task]) -> Option<Vec<TaskId>> {
    let adjacency: BTreeMap<TaskId, Vec<TaskId>> = tasks
        .iter()
        .map(|t| (t.id.clone(), t.depends_on.clone()))
        .collect();
    crate::graph::find_cycle(&adjacency)
}

/// Transitive closure of `depends_on`, in either direction: `a` and `b`
/// are considered related if either can reach the other.
fn transitively_related(tasks: &[Task]) -> HashSet<(TaskId, TaskId)> {
    let adjacency: HashMap<&TaskId, &Vec<TaskId>> =
        tasks.iter().map(|t| (&t.id, &t.depends_on)).collect();
    let mut related = HashSet::new();

    for task in tasks {
        let mut visited = HashSet::new();
        let mut stack = vec![&task.id];
        while let Some(current) = stack.pop() {
            if let Some(deps) = adjacency.get(current) {
                for dep in *deps {
                    if visited.insert(dep) {
                        related.insert((task.id.clone(), dep.clone()));
                        related.insert((dep.clone(), task.id.clone()));
                        stack.push(dep);
                    }
                }
            }
        }
    }
    related
}

/// Two tasks with no dependency path between them, in
/// either direction, must not declare overlapping scopes — it would
/// make parallel batching ambiguous and the diff impossible to
/// attribute.
///
/// Overlap is a deliberately conservative approximation, not full glob
/// algebra (Yunta doesn't have a glob-intersection library and building
/// one is out of scope here): two globs are flagged when one's literal
/// prefix (everything before its first `*`/`?`/`[`) starts with the
/// other's. This can flag a pair that wouldn't actually collide (e.g.
/// `src/*.rs` vs `src/sub/mod.rs`, since `src/*.rs` doesn't recurse into
/// subdirectories) but never misses a real collision — for a safety
/// check, a false alarm the author can adjust is the right side to err
/// on, not a silent miss.
fn overlapping_scopes(tasks: &[Task]) -> Vec<Diagnostic> {
    let related = transitively_related(tasks);
    let mut errors = Vec::new();

    for i in 0..tasks.len() {
        for j in (i + 1)..tasks.len() {
            let (Some(a), Some(b)) = (tasks.get(i), tasks.get(j)) else {
                continue;
            };
            if related.contains(&(a.id.clone(), b.id.clone())) {
                continue;
            }
            for glob_a in &a.scope {
                for glob_b in &b.scope {
                    if globs_might_overlap(glob_a, glob_b) {
                        errors.push(broke(
                            i,
                            &a.id,
                            "overlapping-scope",
                            format!(
                                "`{glob_a}` overlaps task `{}`'s `{glob_b}`, and neither \
                                 depends on the other; give them disjoint scopes or declare \
                                 the dependency",
                                b.id
                            ),
                        ));
                    }
                }
            }
        }
    }
    errors
}

fn glob_literal_prefix(glob: &str) -> &str {
    let end = glob.find(['*', '?', '[']).unwrap_or(glob.len());
    &glob[..end]
}

/// Same conservative approximation as this module's own rule above,
/// reused by `check`'s `parallel` scope-collision rule —
/// one heuristic, not two independently-drifting copies.
pub(crate) fn globs_might_overlap(a: &str, b: &str) -> bool {
    let (pa, pb) = (glob_literal_prefix(a), glob_literal_prefix(b));
    pa.starts_with(pb) || pb.starts_with(pa)
}
