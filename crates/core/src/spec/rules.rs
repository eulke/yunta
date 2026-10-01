//! The rules a spec document has to satisfy once it is readable: one
//! spec per task, every file named once and inside the repository, and
//! every spec with a test that says what it proves.
//!
//! Whether its tasks are the plan's, whether its files are new, and
//! whether each test fails before the work, is the engine's to check when
//! the document is submitted: only the run holds the plan and the tree.

use std::collections::HashSet;
use std::path::{Component, Path};

use crate::diagnostic::{Diagnostic, Named, Problem, Rule, RuleCode, Subject};
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
            problems.push((
                RuleCode::DuplicateSpec,
                "a second spec already names this task; give each task one".to_string(),
            ));
        }
        if spec.tests.is_empty() {
            problems.push((
                RuleCode::NoSpecTest,
                "lists no test; a spec is the tests its task is held to".to_string(),
            ));
        }
        for test in spec
            .tests
            .iter()
            .filter(|test| test.proves.trim().is_empty())
        {
            problems.push((
                RuleCode::UnexplainedTest,
                format!("`{}` does not say what passing it proves", test.cmd),
            ));
        }
        problems.extend(files(spec, &mut paths));
        broken.extend(problems.into_iter().map(|(code, detail)| {
            Diagnostic::new(
                Subject::Spec(Named::new(spec.task.clone(), index)),
                Problem::rule(code, detail),
            )
        }));
    }
    broken
}

/// What `spec`'s files break: a path outside the repository, or one an
/// earlier file of the document already has, however it is spelled —
/// `seen` holds those as git names them.
fn files(spec: &Spec, seen: &mut HashSet<String>) -> Vec<(RuleCode, String)> {
    let mut problems = Vec::new();
    for test_file in &spec.files {
        if !inside(&test_file.path) {
            problems.push((
                RuleCode::TestFileEscapes,
                format!(
                    "`{}` is not a file of the repository: a path is relative, never climbs \
                     out, and never reaches into `.git`",
                    test_file.path
                ),
            ));
        }
        if !seen.insert(test_file.in_repo()) {
            problems.push((
                RuleCode::DuplicateTestFile,
                format!(
                    "`{}` is named by a second file; each file is named once",
                    test_file.path
                ),
            ));
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
        && matches!(names.next(), Some(Component::Normal(first)) if first != ".git")
}
