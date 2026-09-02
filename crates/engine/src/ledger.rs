//! Ledger parsing and registration.
//!
//! `register` runs seven validation rules and returns every
//! violation found, never just the first — whoever writes this by hand
//! corrects once, not seven times. What it does **not** validate: whether
//! the criteria commands themselves exist or are correct — that's the
//! red pre-check's job, which is where a trivial or broken
//! criterion actually gets caught by being run.

use std::collections::{HashMap, HashSet};

use thiserror::Error;
use yunta_core::{Ledger, Task, TaskId};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LedgerError {
    #[error("{id}: duplicate task id")]
    DuplicateId { id: TaskId },

    #[error("{task}: `depends_on` references unknown task `{unknown}`")]
    UnknownDependency { task: TaskId, unknown: TaskId },

    #[error("cycle in depends_on: {path}")]
    DependencyCycle { path: String },

    #[error(
        "`{a}` and `{b}`: scopes overlap (`{glob_a}` / `{glob_b}`) without a dependency between them"
    )]
    OverlappingScope {
        a: TaskId,
        b: TaskId,
        glob_a: String,
        glob_b: String,
    },

    #[error("{task}: `scope` is empty — every task must declare at least one glob")]
    EmptyScope { task: TaskId },

    #[error("{task}: `title` is empty")]
    EmptyTitle { task: TaskId },

    #[error("{task}: no criteria declared — every task needs at least one")]
    NoCriteria { task: TaskId },

    #[error(
        "{task}: all criteria are `guard` — at least one must be able to fail before the work"
    )]
    AllCriteriaAreGuards { task: TaskId },

    #[error("{task}: `manual_review: true` without `justification`")]
    ManualReviewWithoutJustification { task: TaskId },
}

/// Validates a parsed ledger against every rule, collecting
/// every violation rather than stopping at the first.
pub fn register(ledger: &Ledger) -> Vec<LedgerError> {
    let mut errors = Vec::new();

    let mut known_ids: HashSet<TaskId> = HashSet::new();
    for task in &ledger.tasks {
        if !known_ids.insert(task.id.clone()) {
            errors.push(LedgerError::DuplicateId {
                id: task.id.clone(),
            });
        }
    }

    for task in &ledger.tasks {
        for dep in &task.depends_on {
            if !known_ids.contains(dep) {
                errors.push(LedgerError::UnknownDependency {
                    task: task.id.clone(),
                    unknown: dep.clone(),
                });
            }
        }

        if task.title.trim().is_empty() {
            errors.push(LedgerError::EmptyTitle {
                task: task.id.clone(),
            });
        }
        if task.scope.is_empty() {
            errors.push(LedgerError::EmptyScope {
                task: task.id.clone(),
            });
        }
        if task.criteria.is_empty() {
            errors.push(LedgerError::NoCriteria {
                task: task.id.clone(),
            });
        } else if task
            .criteria
            .iter()
            .all(|c| c.r#type == Some(yunta_core::events::CriterionType::Guard))
        {
            errors.push(LedgerError::AllCriteriaAreGuards {
                task: task.id.clone(),
            });
        }
        if task.manual_review
            && task
                .justification
                .as_deref()
                .unwrap_or("")
                .trim()
                .is_empty()
        {
            errors.push(LedgerError::ManualReviewWithoutJustification {
                task: task.id.clone(),
            });
        }
    }

    if let Some(cycle) = find_cycle(&ledger.tasks) {
        let path = cycle
            .iter()
            .map(TaskId::as_str)
            .collect::<Vec<_>>()
            .join(" -> ");
        errors.push(LedgerError::DependencyCycle { path });
    }

    errors.extend(overlapping_scopes(&ledger.tasks));

    errors
}

fn find_cycle(tasks: &[Task]) -> Option<Vec<TaskId>> {
    #[derive(Clone, Copy, PartialEq)]
    enum Color {
        White,
        /// Carries its own index in `stack`, so finding a gray node's
        /// position never needs a fallible search.
        Gray(usize),
        Black,
    }

    let adjacency: HashMap<TaskId, Vec<TaskId>> = tasks
        .iter()
        .map(|t| (t.id.clone(), t.depends_on.clone()))
        .collect();
    let mut color: HashMap<TaskId, Color> = adjacency
        .keys()
        .cloned()
        .map(|id| (id, Color::White))
        .collect();
    let mut stack: Vec<TaskId> = Vec::new();

    fn visit(
        id: &TaskId,
        adjacency: &HashMap<TaskId, Vec<TaskId>>,
        color: &mut HashMap<TaskId, Color>,
        stack: &mut Vec<TaskId>,
    ) -> Option<Vec<TaskId>> {
        color.insert(id.clone(), Color::Gray(stack.len()));
        stack.push(id.clone());

        if let Some(deps) = adjacency.get(id) {
            for dep in deps {
                if !adjacency.contains_key(dep) {
                    continue; // unknown dependency — reported separately
                }
                match color.get(dep).copied() {
                    Some(Color::Gray(pos)) => {
                        let mut cycle = stack[pos..].to_vec();
                        cycle.push(dep.clone());
                        return Some(cycle);
                    }
                    Some(Color::Black) => continue,
                    _ => {
                        if let Some(cycle) = visit(dep, adjacency, color, stack) {
                            return Some(cycle);
                        }
                    }
                }
            }
        }

        stack.pop();
        color.insert(id.clone(), Color::Black);
        None
    }

    for id in adjacency.keys() {
        if matches!(color.get(id), Some(Color::White)) {
            if let Some(cycle) = visit(id, &adjacency, &mut color, &mut stack) {
                return Some(cycle);
            }
        }
    }
    None
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
fn overlapping_scopes(tasks: &[Task]) -> Vec<LedgerError> {
    let related = transitively_related(tasks);
    let mut errors = Vec::new();

    for i in 0..tasks.len() {
        for j in (i + 1)..tasks.len() {
            let (a, b) = (&tasks[i], &tasks[j]);
            if related.contains(&(a.id.clone(), b.id.clone())) {
                continue;
            }
            for glob_a in &a.scope {
                for glob_b in &b.scope {
                    if globs_might_overlap(glob_a, glob_b) {
                        errors.push(LedgerError::OverlappingScope {
                            a: a.id.clone(),
                            b: b.id.clone(),
                            glob_a: glob_a.clone(),
                            glob_b: glob_b.clone(),
                        });
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
