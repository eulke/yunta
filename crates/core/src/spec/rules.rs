//! The rules a spec document has to satisfy once it is readable: one
//! spec per task, every file named once and inside the repository, and
//! every spec with a test that says what it proves.
//!
//! Whether its tasks are the plan's, whether its files are new, and
//! whether each test fails before the work, is the engine's to check when
//! the document is submitted: only the run holds the plan and the tree.

use std::collections::HashSet;
use std::path::{Component, Path};

use super::SpecFault;
use crate::diagnostic::{Diagnostic, Named, Problem, Rule, RuleCode, Subject};
use crate::protected::is_register;
use crate::{RelativePath, Spec, SpecFile};

/// Every rule this document is held to — see `crate::tasks::rules` for
/// what this list is for and what holds it true.
pub(super) const RULES: &[Rule] = &[
    Rule {
        code: RuleCode::DuplicateSpec,
        demand: "each task is given one spec",
    },
    Rule {
        code: RuleCode::NoSpecTest,
        demand: "every spec lists at least one test",
    },
    Rule {
        code: RuleCode::UnexplainedTest,
        demand: "every test says what passing it `proves`, in words",
    },
    Rule {
        code: RuleCode::TestFileEscapes,
        demand: "a file's `path` is relative, inside the repository and outside `.git`",
    },
    Rule {
        code: RuleCode::DuplicateTestFile,
        demand: "each file is named once in the document",
    },
];

/// What the engine demands where it runs the tests: the run's plan
/// declares every task a spec names, every file is new to the run's tree,
/// and in a checkout of that tree with every file of the document written
/// into it, each test runs and fails.
pub(super) const RUN_RULES: &[Rule] = &[
    Rule {
        code: RuleCode::UnknownSpecTask,
        demand: "`task` names a task of the run's plan",
    },
    Rule {
        code: RuleCode::TestFileExists,
        demand: "every file is new: the run's tree holds nothing at its `path`",
    },
    Rule {
        code: RuleCode::OtherSpecChanged,
        demand: "written again for a departure, every other task's spec is as the run holds it",
    },
    Rule {
        code: RuleCode::DepartedSpecUnchanged,
        demand: "written again for a departure, the spec of the task departed from changes",
    },
    Rule {
        code: RuleCode::CriterionCannotRun,
        demand: "every test runs where the engine runs criteria: each program it calls is on \
                 that `PATH`",
    },
    Rule {
        code: RuleCode::CriterionAlreadyPasses,
        demand: "every test fails before the work",
    },
];

/// Every violation the document carries, collected rather than stopped
/// at the first.
pub(super) fn check(file: &SpecFile) -> Vec<Diagnostic> {
    let mut broken = Vec::new();
    let mut tasks = HashSet::new();
    let mut paths: HashSet<String> = HashSet::new();
    for (index, spec) in file.specs.iter().enumerate() {
        let mut problems = Vec::new();
        if !tasks.insert(&spec.task) {
            problems.push(SpecFault::DuplicateSpec);
        }
        if spec.tests.is_empty() {
            problems.push(SpecFault::NoSpecTest);
        }
        for test in spec
            .tests
            .iter()
            .filter(|test| test.proves.trim().is_empty())
        {
            problems.push(SpecFault::UnexplainedTest {
                cmd: test.cmd.clone(),
            });
        }
        problems.extend(files(spec, &mut paths));
        broken.extend(problems.into_iter().map(|fault| {
            Diagnostic::new(
                Subject::Spec(Named::new(spec.task.clone(), index)),
                Problem::broke(fault),
            )
        }));
    }
    broken
}

/// What `spec`'s files break: a path outside the repository, or one an
/// earlier file of the document already has, however it is spelled —
/// `seen` holds those as git names them.
fn files(spec: &Spec, seen: &mut HashSet<String>) -> Vec<SpecFault> {
    let mut problems = Vec::new();
    for test_file in &spec.files {
        if !inside(&test_file.path) {
            problems.push(SpecFault::TestFileEscapes {
                path: test_file.path.clone(),
            });
        }
        if !seen.insert(test_file.in_repo()) {
            problems.push(SpecFault::DuplicateTestFile {
                path: test_file.path.clone(),
            });
        }
    }
    problems
}

/// Whether `path` names a file of the repository the engine may write.
fn inside(path: &str) -> bool {
    let mut names = Path::new(path)
        .components()
        .filter(|component| *component != Component::CurDir);
    path.parse::<RelativePath>().is_ok()
        && matches!(names.next(), Some(Component::Normal(_)))
        && !is_register(Path::new(path))
}
