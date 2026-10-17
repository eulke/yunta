//! The rules a tasks document has to satisfy once it is readable.
//!
//! Shape is the frontier before this one: by the time a [`TasksFile`]
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

use super::TaskFault;
use crate::diagnostic::{Diagnostic, Named, Problem, Rule, RuleCode, Subject};
use crate::events::CriterionType;
use crate::{Task, TaskId, TasksFile};

/// Every rule this document is held to, in the order a writer meets them.
///
/// The list is what the shape publishes before a tasks document is written and what
/// the functions below enforce after. A rule that is not here is a rule no
/// writer was told about, and the tests hold the two ends together: every
/// `RuleCode` belongs to some document's list, and every entry here is
/// reachable by a document that breaks it.
pub(super) const RULES: &[Rule] = &[
    Rule {
        code: RuleCode::DuplicateId,
        demand: "each `id` is declared once in the file",
    },
    Rule {
        code: RuleCode::EmptyTitle,
        demand: "`title` says what the task does, in one non-empty line",
    },
    Rule {
        code: RuleCode::EmptyScope,
        demand: "`scope` lists at least one glob: the only paths the task may touch",
    },
    Rule {
        code: RuleCode::NoCriteria,
        demand: "every task declares at least one criterion",
    },
    Rule {
        code: RuleCode::AllCriteriaAreGuards,
        demand: "at least one criterion is not a `guard`, so something has to fail before the \
                 work and pass after it",
    },
    Rule {
        code: RuleCode::UnknownDependency,
        demand: "`depends_on` names only ids this file declares",
    },
    Rule {
        code: RuleCode::DependencyCycle,
        demand: "`depends_on` forms no cycle",
    },
    Rule {
        code: RuleCode::OverlappingScope,
        demand: "two tasks with no dependency between them declare no overlapping scope, so \
                 either give them disjoint scopes or declare the dependency",
    },
    Rule {
        code: RuleCode::DuplicateShape,
        demand: "each shape's `name` is declared once",
    },
    Rule {
        code: RuleCode::DuplicateDecision,
        demand: "each decision's `id` is declared once",
    },
    Rule {
        code: RuleCode::UnknownShapeOwner,
        demand: "a shape's `owner` names a task this file declares",
    },
    Rule {
        code: RuleCode::ShapeOutsideOwnerScope,
        demand: "a shape's `file` lies inside its owner's `scope`",
    },
    Rule {
        code: RuleCode::UnknownShape,
        demand: "`uses` names only shapes `shapes` declares",
    },
    Rule {
        code: RuleCode::ShapeUsedBeforeItsOwner,
        demand: "a task that `uses` a shape another task owns waits for that task, directly \
                 or through others",
    },
    Rule {
        code: RuleCode::ChangeOutsideScope,
        demand: "every place a task `changes` lies inside its own `scope`",
    },
];

/// What the engine demands of a task's criteria where it runs them,
/// checked by running each one when the document is submitted: in a
/// checkout of the run's tree, with the shell and `PATH` the run's
/// commands get. A task that depends on another is checked only for the
/// first: its criteria meet the tree only after that other task's work.
pub(super) const RUN_RULES: &[Rule] = &[
    Rule {
        code: RuleCode::CriterionCannotRun,
        demand: "every criterion runs where the engine runs criteria: each program it calls is \
                 on that `PATH`, and a file the task will create is only run after checking \
                 it exists (`test -f x && ./x`)",
    },
    Rule {
        code: RuleCode::CriterionAlreadyPasses,
        demand: "in a task with no `depends_on`, every criterion that is not a `guard` fails \
                 before the work",
    },
    Rule {
        code: RuleCode::GuardAlreadyRed,
        demand: "in a task with no `depends_on`, every `guard` passes before the work",
    },
];

/// What a person reviewing the plan needs it to say, demanded when the
/// workflow has a gate show it: the change in a line and in prose, and
/// for each task and each criterion what it is there for. The shapes, the
/// risks and what is left out are asked for by whoever writes the prompt
/// — a plan that only touches documentation creates no shape.
pub(super) const REVIEW_RULES: &[Rule] = &[
    Rule {
        code: RuleCode::NoSummary,
        demand: "`summary` says what the plan changes, in one non-empty line",
    },
    Rule {
        code: RuleCode::NoDescription,
        demand: "the plan and every task carry a `description`: what changes and why, in \
                 Markdown",
    },
    Rule {
        code: RuleCode::UnexplainedCriterion,
        demand: "every criterion says what passing it `proves`, in words",
    },
    Rule {
        code: RuleCode::NoOutcome,
        demand: "every task says what a person will observe once it is done, in `outcome`",
    },
    Rule {
        code: RuleCode::NoChanges,
        demand: "every task says what it `changes`, place by place",
    },
    Rule {
        code: RuleCode::UnexplainedDecision,
        demand: "every decision says `why` it chose what it chose",
    },
    Rule {
        code: RuleCode::ChangeWithoutCode,
        demand: "every change shows its `code` — the declarations it adds or changes, each \
                 type with its fields and each signature whole — unless its task declares a \
                 shape in that file",
    },
    Rule {
        code: RuleCode::ChangeCodeMissesAName,
        demand: "a change's `code` shows every symbol its `at` names — `pack.rs::add/update` \
                 shows `add` and `update` — or a shape its task declares in that file does",
    },
    Rule {
        code: RuleCode::ChangeCodeIsAComment,
        demand: "a change's `code` is code, not a comment about what the code will do, unless \
                 the change is to a document",
    },
];

/// Every way the plan leaves a person reviewing it without an
/// explanation, collected rather than stopped at the first.
pub(super) fn reviewed(tasks: &TasksFile) -> Vec<Diagnostic> {
    let document = |fault: TaskFault| Diagnostic::new(Subject::Document, Problem::broke(fault));
    let mut broken = Vec::new();
    if !said(&tasks.summary) {
        broken.push(document(TaskFault::NoSummary));
    }
    if !said(&tasks.description) {
        broken.push(document(TaskFault::NoPlanDescription));
    }
    for decision in &tasks.decisions {
        if !said(&decision.why) {
            broken.push(document(TaskFault::UnexplainedDecision {
                decision: decision.id.clone(),
                choice: decision.choice.clone(),
            }));
        }
    }
    for (index, task) in tasks.tasks.iter().enumerate() {
        broken.extend(task_reviewed(index, task));
        broken.extend(codeless(index, task, &tasks.shapes));
        broken.extend(super::code::unshown(index, task, &tasks.shapes));
    }
    broken
}

/// Each change of `task` a person reviewing the plan could not see the
/// code of: it shows none, and the task declares no shape in its file.
fn codeless(index: usize, task: &Task, shapes: &[crate::Shape]) -> Vec<Diagnostic> {
    task.changes
        .iter()
        .filter(|change| !said(&change.code))
        .filter(|change| {
            !shapes
                .iter()
                .any(|shape| shape.owner == task.id && shape.file == change.file())
        })
        .map(|change| {
            broke(
                index,
                &task.id,
                TaskFault::ChangeWithoutCode {
                    at: change.at.clone(),
                },
            )
        })
        .collect()
}

/// Every way one task leaves a person reviewing the plan without an
/// explanation.
fn task_reviewed(index: usize, task: &Task) -> Vec<Diagnostic> {
    let mut broken = Vec::new();
    let missing = [
        (!said(&task.description), TaskFault::NoTaskDescription),
        (!said(&task.outcome), TaskFault::NoOutcome),
        (task.changes.is_empty(), TaskFault::NoChanges),
    ];
    for (_, fault) in missing.into_iter().filter(|(lacks, _)| *lacks) {
        broken.push(broke(index, &task.id, fault));
    }
    for (at, criterion) in task.criteria.iter().enumerate() {
        if !said(&criterion.proves) {
            broken.push(Diagnostic::new(
                Subject::Criterion {
                    task: Named::new(task.id.clone(), index),
                    index: at,
                },
                Problem::broke(TaskFault::UnexplainedCriterion {
                    cmd: criterion.cmd.clone(),
                }),
            ));
        }
    }
    broken
}

/// Whether `text` says something.
fn said(text: &Option<String>) -> bool {
    text.as_deref().is_some_and(|t| !t.trim().is_empty())
}

pub(super) fn broke(index: usize, id: &TaskId, fault: TaskFault) -> Diagnostic {
    Diagnostic::new(
        Subject::Task(Named::new(id.clone(), index)),
        Problem::broke(fault),
    )
}

/// Every violation the tasks document carries, collected rather than stopped at
/// the first — whoever wrote this corrects once, not once per round.
pub(super) fn check(tasks: &TasksFile) -> Vec<Diagnostic> {
    let known_ids: HashSet<TaskId> = tasks.tasks.iter().map(|task| task.id.clone()).collect();
    let mut broken = duplicate_ids(tasks);
    for (index, task) in tasks.tasks.iter().enumerate() {
        broken.extend(task_rules(index, task, &known_ids));
    }
    broken.extend(cycle(tasks));
    broken.extend(overlapping_scopes(&tasks.tasks));
    broken.extend(super::owned::check(tasks));
    broken
}

/// An id used twice: a rule about the document, reported on the second
/// task to carry it — the one a reader has to change.
fn duplicate_ids(tasks: &TasksFile) -> Vec<Diagnostic> {
    let mut seen: HashSet<&TaskId> = HashSet::new();
    tasks
        .tasks
        .iter()
        .enumerate()
        .filter(|(_, task)| !seen.insert(&task.id))
        .map(|(index, task)| broke(index, &task.id, TaskFault::DuplicateTask))
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
                TaskFault::UnknownDependency {
                    dependency: dep.clone(),
                },
            ));
        }
    }
    if task.title.trim().is_empty() {
        broken.push(broke(index, &task.id, TaskFault::EmptyTitle));
    }
    if task.scope.is_empty() {
        broken.push(broke(index, &task.id, TaskFault::EmptyScope));
    }
    broken.extend(criteria_rules(index, task));
    broken
}

/// What a task's `criteria` have to be for the red pre-check to mean
/// anything: at least one, and at least one that can fail before the
/// work starts.
fn criteria_rules(index: usize, task: &Task) -> Option<Diagnostic> {
    if task.criteria.is_empty() {
        return Some(broke(index, &task.id, TaskFault::NoCriteria));
    }
    let all_guards = task
        .criteria
        .iter()
        .all(|c| c.r#type == Some(CriterionType::Guard));
    all_guards.then(|| broke(index, &task.id, TaskFault::AllCriteriaAreGuards))
}

/// A cycle is a property of the whole graph, so the document carries it
/// rather than any one task in the loop.
fn cycle(tasks: &TasksFile) -> Option<Diagnostic> {
    let adjacency: BTreeMap<TaskId, Vec<TaskId>> = tasks
        .tasks
        .iter()
        .map(|t| (t.id.clone(), t.depends_on.clone()))
        .collect();
    let cycle = crate::graph::find_cycle(&adjacency)?;
    Some(Diagnostic::new(
        Subject::Document,
        Problem::broke(TaskFault::DependencyCycle { cycle }),
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
                            TaskFault::OverlappingScope {
                                glob: glob_a.clone(),
                                other: b.id.clone(),
                                other_glob: glob_b.clone(),
                            },
                        ));
                    }
                }
            }
        }
    }
    broken
}
