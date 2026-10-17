//! What a tasks document is held to as part of the run it is handed over
//! in: rules about what proves its work, which only make sense against
//! the run — its spec, the questions a person answered — and so are never
//! asked of a document read back later. A plan an older run accepted
//! stays readable; a plan handed over now is held to them.

use super::TaskFault;
use crate::diagnostic::{Diagnostic, Named, Problem, Rule, RuleCode, Subject};
use crate::{QuestionId, SpecFile, Suite, Task, TaskId, TasksFile};

pub(super) const SPECIFIED_RULES: &[Rule] = &[
    Rule {
        code: RuleCode::CriterionChecksPresence,
        demand: "a criterion runs the behavior: none passes once a name is written in a file \
                 its task changes — a `grep` of that file, or a `test -f` of it nothing runs \
                 after — unless the file is a document whose words are the work",
    },
    Rule {
        code: RuleCode::SharedCriterion,
        demand: "no two tasks are judged by the same command: the second passes as soon as the \
                 first lands, before any work of its own",
    },
    Rule {
        code: RuleCode::OutsideTheSuite,
        demand: "a criterion that runs the project's tests extends the suite the run measures — \
                 `<suite> <selector>`, where `<selector>` picks the task's tests — so the build \
                 the suite leaves serves it",
    },
    Rule {
        code: RuleCode::UsesItsOwnShape,
        demand: "a task `uses` only shapes another task builds; its own are in its `changes`",
    },
    Rule {
        code: RuleCode::TaskWritesItsTest,
        demand: "when the workflow writes a spec, no task `changes` the test file one of its \
                 criteria runs: the spec writes each task's tests",
    },
    Rule {
        code: RuleCode::ChangesASpecTest,
        demand: "no task `changes` a file the run's spec wrote",
    },
    Rule {
        code: RuleCode::UnknownAnswer,
        demand: "a decision `answers` only a question this run asked and a person answered",
    },
];

impl TasksFile {
    /// Every rule the plan breaks that it can tell on its own: a criterion that
    /// passes by a name, two tasks judged by one command, a task that uses its
    /// own shape — and, when the run's workflow writes a spec, a task that
    /// changes the test its criterion runs; when the run measures a `suite`, a
    /// criterion that runs the project's tests without extending it.
    pub fn unspecifiable(&self, spec_planned: bool, suite: Option<&Suite>) -> Vec<Diagnostic> {
        let mut broken = Vec::new();
        for (index, task) in self.tasks.iter().enumerate() {
            broken.extend(by_a_name(index, task));
            if let Some(suite) = suite {
                broken.extend(outside_the_suite(index, task, suite));
            }
            broken.extend(own_shapes(index, task, self));
            if spec_planned {
                broken.extend(writes_its_test(index, task));
            }
        }
        broken.extend(shared(self));
        broken
    }

    /// Each change the plan names on a file `spec` wrote.
    pub fn against_spec(&self, spec: &SpecFile) -> Vec<Diagnostic> {
        let mut broken = Vec::new();
        for (index, task) in self.tasks.iter().enumerate() {
            for change in &task.changes {
                let at = crate::in_repo(change.file());
                let owner = spec
                    .specs
                    .iter()
                    .find(|of| of.files.iter().any(|file| file.in_repo() == at));
                if let Some(owner) = owner {
                    broken.push(of_task(
                        index,
                        &task.id,
                        TaskFault::ChangesASpecTest {
                            at: change.at.clone(),
                            owner: owner.task.clone(),
                        },
                    ));
                }
            }
        }
        broken
    }

    /// Each decision that `answers` a question none of `answered` is.
    pub fn unanswered(&self, answered: &[QuestionId]) -> Vec<Diagnostic> {
        self.decisions
            .iter()
            .filter_map(|decision| {
                let question = decision.answers.as_ref()?;
                (!answered.contains(question)).then(|| {
                    Diagnostic::new(
                        Subject::Document,
                        Problem::broke(TaskFault::UnknownAnswer {
                            decision: decision.id.clone(),
                            question: question.clone(),
                            answered: answered.to_vec(),
                        }),
                    )
                })
            })
            .collect()
    }
}

fn by_a_name(index: usize, task: &Task) -> Vec<Diagnostic> {
    crate::passes_by_a_name(task)
        .into_iter()
        .map(|(cmd, file)| {
            criterion_of(
                index,
                task,
                &cmd,
                TaskFault::CriterionChecksPresence {
                    cmd: cmd.clone(),
                    file,
                },
            )
        })
        .collect()
}

/// Each criterion of `task` that runs the project's tests without extending
/// `suite`, which builds them again beside the suite's build.
fn outside_the_suite(index: usize, task: &Task, suite: &Suite) -> Vec<Diagnostic> {
    task.criteria
        .iter()
        .flat_map(|criterion| {
            suite.outside(&criterion.cmd).into_iter().map(|command| {
                criterion_of(
                    index,
                    task,
                    &criterion.cmd,
                    TaskFault::OutsideTheSuite {
                        command,
                        suite: suite.command(),
                    },
                )
            })
        })
        .collect()
}

fn own_shapes(index: usize, task: &Task, plan: &TasksFile) -> Vec<Diagnostic> {
    task.uses
        .iter()
        .filter(|name| {
            plan.shapes
                .iter()
                .any(|shape| shape.name == **name && shape.owner == task.id)
        })
        .map(|name| {
            of_task(
                index,
                &task.id,
                TaskFault::UsesItsOwnShape {
                    shape: name.clone(),
                },
            )
        })
        .collect()
}

fn writes_its_test(index: usize, task: &Task) -> Vec<Diagnostic> {
    task.changes
        .iter()
        .filter(|change| test_looking(change.file()))
        .filter_map(|change| {
            let cmd = task
                .criteria
                .iter()
                .filter(|criterion| !criterion.is_guard())
                .find(|criterion| crate::names_file(&criterion.cmd, change.file()))?;
            Some(of_task(
                index,
                &task.id,
                TaskFault::TaskWritesItsTest {
                    at: change.at.clone(),
                    cmd: cmd.cmd.clone(),
                },
            ))
        })
        .collect()
}

/// Whether `path` is where a test lives, by the conventions test runners
/// share: a `tests`/`test`/`__tests__` directory, or a name that says so.
fn test_looking(path: &str) -> bool {
    let path = crate::in_repo(path);
    let name = path.rsplit('/').next().unwrap_or(&path);
    path.split('/')
        .any(|part| matches!(part, "tests" | "test" | "__tests__" | "spec"))
        || name.starts_with("test_")
        || [".test.", ".spec.", "_test.", "_spec."]
            .iter()
            .any(|mark| name.contains(mark))
}

fn shared(plan: &TasksFile) -> Vec<Diagnostic> {
    let mut first: Vec<(&str, &TaskId)> = Vec::new();
    let mut broken = Vec::new();
    for (index, task) in plan.tasks.iter().enumerate() {
        for criterion in task.criteria.iter().filter(|c| !c.is_guard()) {
            let cmd = criterion.cmd.trim();
            match first
                .iter()
                .find(|(seen, owner)| *seen == cmd && **owner != task.id)
            {
                Some((_, owner)) => broken.push(criterion_of(
                    index,
                    task,
                    &criterion.cmd,
                    TaskFault::SharedCriterion {
                        cmd: cmd.to_string(),
                        owner: (**owner).clone(),
                    },
                )),
                None => first.push((cmd, &task.id)),
            }
        }
    }
    broken
}

/// The diagnostic about the first criterion of `task` whose command is `cmd`,
/// or the task's first criterion when none is.
fn criterion_of(index: usize, task: &Task, cmd: &str, fault: TaskFault) -> Diagnostic {
    let at = task
        .criteria
        .iter()
        .position(|criterion| criterion.cmd == cmd)
        .unwrap_or(0);
    Diagnostic::new(
        Subject::Criterion {
            task: Named::new(task.id.clone(), index),
            index: at,
        },
        Problem::broke(fault),
    )
}

fn of_task(index: usize, id: &TaskId, fault: TaskFault) -> Diagnostic {
    Diagnostic::new(
        Subject::Task(Named::new(id.clone(), index)),
        Problem::broke(fault),
    )
}
