//! The rules a task ledger has to satisfy once it is readable.
//!
//! Shape is the frontier before this one: by the time a [`Ledger`]
//! exists, every key is known and every value has its type. What is left
//! are the rules that only hold across a whole document — an id used
//! twice, a dependency on a task nobody declared, two independent tasks
//! reaching for the same files.
//!
//! Every rule reports a [`Diagnostic`] with the task as its subject and
//! a [`RuleCode`], so the whole document's problems reach a reader as
//! one list and a receipt counts them without reading prose. What is
//! **not** checked here: whether a criterion's command exists or is
//! correct. The red pre-check answers that by running it, which is where
//! a trivial or broken criterion actually gives itself away.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::diagnostic::{Diagnostic, Named, Problem, RuleCode, Subject};
use crate::events::CriterionType;
use crate::{Ledger, Task, TaskId};

fn broke(index: usize, id: &TaskId, code: RuleCode, detail: impl Into<String>) -> Diagnostic {
    Diagnostic::new(
        Subject::Task(Named::new(id.clone(), index)),
        Problem::rule(code, detail),
    )
}

/// Every violation the ledger carries, collected rather than stopped at
/// the first — whoever wrote this corrects once, not once per round.
pub(super) fn check(ledger: &Ledger) -> Vec<Diagnostic> {
    let known_ids: HashSet<TaskId> = ledger.tasks.iter().map(|task| task.id.clone()).collect();
    let mut broken = duplicate_ids(ledger);
    for (index, task) in ledger.tasks.iter().enumerate() {
        broken.extend(task_rules(index, task, &known_ids));
    }
    broken.extend(cycle(ledger));
    broken.extend(overlapping_scopes(&ledger.tasks));
    broken
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
                RuleCode::DuplicateId,
                "a second task already carries this id; every id is declared once",
            )
        })
        .collect()
}

/// Everything one task has to satisfy on its own.
fn task_rules(index: usize, task: &Task, known_ids: &HashSet<TaskId>) -> Vec<Diagnostic> {
    let mut broken = Vec::new();
    for dep in &task.depends_on {
        if !known_ids.contains(dep) {
            broken.push(broke(
                index,
                &task.id,
                RuleCode::UnknownDependency,
                format!("`depends_on` names `{dep}`, which no task in this file declares"),
            ));
        }
    }
    if task.title.trim().is_empty() {
        broken.push(broke(
            index,
            &task.id,
            RuleCode::EmptyTitle,
            "`title` is empty",
        ));
    }
    if task.scope.is_empty() {
        broken.push(broke(
            index,
            &task.id,
            RuleCode::EmptyScope,
            "`scope` is empty; every task declares at least one glob, the only paths it \
             may touch",
        ));
    }
    broken.extend(criteria_rules(index, task));
    if task.manual_review
        && task
            .justification
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
    {
        broken.push(broke(
            index,
            &task.id,
            RuleCode::ManualReviewWithoutJustification,
            "`manual_review: true` without `justification`; say why no command can verify \
             this task",
        ));
    }
    broken
}

/// What a task's `criteria` have to be for the red pre-check to mean
/// anything: at least one, and at least one that can fail before the
/// work starts.
fn criteria_rules(index: usize, task: &Task) -> Option<Diagnostic> {
    if task.criteria.is_empty() {
        return Some(broke(
            index,
            &task.id,
            RuleCode::NoCriteria,
            "no criteria declared; every task needs at least one command that verifies it",
        ));
    }
    let all_guards = task
        .criteria
        .iter()
        .all(|c| c.r#type == Some(CriterionType::Guard));
    all_guards.then(|| {
        broke(
            index,
            &task.id,
            RuleCode::AllCriteriaAreGuards,
            "every criterion is a `guard`; at least one must be able to fail before the work, \
             or there is nothing the work has to make pass",
        )
    })
}

/// A cycle is a property of the whole graph, so the document carries it
/// rather than any one task in the loop.
fn cycle(ledger: &Ledger) -> Option<Diagnostic> {
    let adjacency: BTreeMap<TaskId, Vec<TaskId>> = ledger
        .tasks
        .iter()
        .map(|t| (t.id.clone(), t.depends_on.clone()))
        .collect();
    let path = crate::graph::find_cycle(&adjacency)?
        .iter()
        .map(TaskId::as_str)
        .collect::<Vec<_>>()
        .join(" -> ");
    Some(Diagnostic::new(
        Subject::Document,
        Problem::rule(
            RuleCode::DependencyCycle,
            format!("`depends_on` forms a cycle: {path}"),
        ),
    ))
}

/// Transitive closure of `depends_on`, in either direction: `a` and `b`
/// are related if either can reach the other.
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

/// Two tasks with no dependency path between them, in either direction,
/// must not declare overlapping scopes: it makes parallel batching
/// ambiguous and the diff impossible to attribute.
fn overlapping_scopes(tasks: &[Task]) -> Vec<Diagnostic> {
    let related = transitively_related(tasks);
    let mut broken = Vec::new();

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
                    if crate::glob::might_overlap(glob_a, glob_b) {
                        broken.push(broke(
                            i,
                            &a.id,
                            RuleCode::OverlappingScope,
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
    broken
}
