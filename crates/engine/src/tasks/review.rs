//! A plan as the run will judge it, for the person deciding on it: each
//! task's criteria and what holds the task to each, and the changes the
//! plan names on a test no session may write.

use yunta_core::events::AcceptedDeparture;
use yunta_core::shown::{
    DeniedChange, HeldTo, JudgedCriterion, PlanReview, SpecFileReview, TaskReview,
};
use yunta_core::{SpecFile, Task, TasksFile};

use super::judged::{specified, waived};

/// `plan` as the run judges it, given the `spec` it is shown with, the
/// `suite` the run measured green and the departures a person accepted.
pub fn review(
    plan: TasksFile,
    spec: Option<SpecFile>,
    suite: Option<&str>,
    departed: Vec<AcceptedDeparture>,
) -> PlanReview {
    let tasks = plan
        .tasks
        .iter()
        .map(|task| task_review(task, spec.as_ref(), suite, &departed))
        .collect();
    PlanReview {
        plan,
        departed,
        spec,
        suite: suite.map(str::to_string),
        tasks,
        handed_over: None,
    }
}

fn task_review(
    task: &Task,
    spec: Option<&SpecFile>,
    suite: Option<&str>,
    departed: &[AcceptedDeparture],
) -> TaskReview {
    let judged = waived(specified(task.clone(), spec), departed, suite, spec);
    let reviewed = |criterion: &yunta_core::Criterion| JudgedCriterion {
        cmd: criterion.cmd.clone(),
        proves: criterion.proves.clone(),
        from: held_to(task, &criterion.cmd, spec),
    };
    let (guards, criteria): (Vec<_>, Vec<_>) = judged
        .criteria
        .iter()
        .filter(|criterion| Some(criterion.cmd.trim()) != suite)
        .partition(|criterion| criterion.is_guard());
    TaskReview {
        task: task.id.clone(),
        criteria: criteria.into_iter().map(reviewed).collect(),
        denied: denied(task, spec),
        files: files(task, spec),
        guards: guards.into_iter().map(reviewed).collect(),
    }
}

/// Each file the spec wrote for `task`, with the commands of its tests
/// that run it.
fn files(task: &Task, spec: Option<&SpecFile>) -> Vec<SpecFileReview> {
    let Some(own) = spec.and_then(|spec| spec.of(&task.id)) else {
        return Vec::new();
    };
    own.files
        .iter()
        .map(|file| SpecFileReview {
            path: file.path.clone(),
            run_by: own
                .tests
                .iter()
                .filter(|test| yunta_core::names_file(&test.cmd, &file.in_repo()))
                .map(|test| test.cmd.clone())
                .collect(),
        })
        .collect()
}

/// What holds `task` to the criterion that runs `cmd`: a test of its own
/// spec, the test the spec gives another task, or its plan alone.
fn held_to(task: &Task, cmd: &str, spec: Option<&SpecFile>) -> HeldTo {
    let cmd = cmd.trim();
    let Some(spec) = spec else {
        return HeldTo::Plan;
    };
    let runs = |of: &yunta_core::Spec| of.tests.iter().any(|test| test.cmd.trim() == cmd);
    if spec.of(&task.id).is_some_and(runs) {
        return HeldTo::Spec;
    }
    match spec.specs.iter().find(|other| runs(other)) {
        Some(other) => HeldTo::AnotherTask {
            task: other.task.clone(),
        },
        None => HeldTo::Plan,
    }
}

/// Each change of `task` the plan names on a file the spec wrote: a test
/// a person approves, which no session of the run may write.
fn denied(task: &Task, spec: Option<&SpecFile>) -> Vec<DeniedChange> {
    let Some(spec) = spec else {
        return Vec::new();
    };
    task.changes
        .iter()
        .filter_map(|change| {
            let at = yunta_core::in_repo(change.file());
            spec.specs
                .iter()
                .find(|of| of.files.iter().any(|file| file.in_repo() == at))
                .map(|of| DeniedChange {
                    at: change.at.clone(),
                    owner: of.task.clone(),
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAN: &str = r#"
tasks:
  - id: store
    title: Store
    scope: [src/store.rs, tests/store.rs]
    criteria:
      - { cmd: cargo test --test store, proves: it stores }
    changes:
      - { at: src/store.rs::Store, what: the store }
      - { at: tests/store.rs, what: its test }
  - id: list
    title: List
    scope: [src/list.rs]
    depends_on: [store]
    criteria:
      - { cmd: cargo test --test store, proves: it lists what it stored }
    changes:
      - { at: tests/store.rs, what: a listing test }
"#;

    const SPEC: &str = r##"
specs:
  - task: store
    files:
      - { path: tests/store.rs, content: "#[test] fn stores() {}\n" }
    tests:
      - { cmd: cargo test --test store, proves: it stores }
  - task: list
    files:
      - { path: tests/list.rs, content: "#[test] fn lists() {}\n" }
    tests:
      - { cmd: cargo test --test list, proves: it lists }
"##;

    fn reviewed() -> PlanReview {
        let plan: TasksFile = yunta_core::yaml::parse(PLAN).unwrap();
        let spec: SpecFile = yunta_core::yaml::parse(SPEC).unwrap();
        review(plan, Some(spec), Some("cargo test"), Vec::new())
    }

    #[test]
    fn a_change_to_a_test_the_spec_wrote_is_marked_with_the_task_that_owns_it() {
        let review = reviewed();
        let denied: Vec<(&str, &str)> = review
            .tasks
            .iter()
            .flat_map(|task| {
                task.denied
                    .iter()
                    .map(move |change| (task.task.as_str(), change.owner.as_str()))
            })
            .collect();
        assert_eq!(denied, vec![("store", "store"), ("list", "store")]);
    }

    #[test]
    fn a_spec_test_joins_its_task_s_criteria_once() {
        let review = reviewed();
        let store: Vec<&HeldTo> = review.tasks[0].criteria.iter().map(|c| &c.from).collect();
        assert_eq!(store, vec![&HeldTo::Spec]);
        assert_eq!(
            review.tasks[0].files,
            vec![SpecFileReview {
                path: "tests/store.rs".to_string(),
                run_by: vec!["cargo test --test store".to_string()],
            }],
            "the file is tied to the test that runs it"
        );
    }

    #[test]
    fn a_plan_criterion_running_another_task_s_spec_test_says_so() {
        let review = reviewed();
        let list: Vec<(&str, &HeldTo)> = review.tasks[1]
            .criteria
            .iter()
            .map(|c| (c.cmd.as_str(), &c.from))
            .collect();
        assert_eq!(
            list,
            vec![
                (
                    "cargo test --test store",
                    &HeldTo::AnotherTask {
                        task: "store".into()
                    }
                ),
                ("cargo test --test list", &HeldTo::Spec),
            ]
        );
    }

    #[test]
    fn the_suite_guards_every_task_once() {
        let review = reviewed();
        assert_eq!(review.suite.as_deref(), Some("cargo test"));
        assert!(review
            .tasks
            .iter()
            .all(|task| task.criteria.iter().all(|c| c.cmd != "cargo test")));
    }

    /// A plan whose criteria pass once a name is written, and a spec whose
    /// files nothing runs and whose tests run none of its files.
    const BY_A_NAME: &str = r#"
tasks:
  - id: cli
    title: CLI
    scope: [src/pack.rs, tests/pack_cmd.rs]
    criteria:
      - { cmd: "cargo test --test pack_cmd && grep -q 'scope_case' tests/pack_cmd.rs", proves: the case exists }
      - { cmd: cargo test --test pack_lock, type: guard, proves: the lock stays as it was }
    changes:
      - { at: src/pack.rs::PackStore, what: the store }
      - { at: ./tests/pack_cmd.rs, what: lifecycle tests }
"#;

    const HOLLOW: &str = r##"
specs:
  - task: cli
    files:
      - { path: tests/scope_spec.rs, content: "#[test] fn holds() { assert!(true); }\n" }
    tests:
      - { cmd: "cargo test --test pack_cmd && grep -q 'scope_case' tests/pack_cmd.rs", proves: the case exists }
"##;

    fn flaws_of(plan: &str, spec: &str) -> Vec<yunta_core::shown::Flaw> {
        let plan: TasksFile = yunta_core::yaml::parse(plan).unwrap();
        let spec: SpecFile = yunta_core::yaml::parse(spec).unwrap();
        review(plan, Some(spec), Some("cargo test"), Vec::new()).flaws()
    }

    #[test]
    fn a_plan_proven_by_a_name_and_a_spec_that_runs_none_of_its_files_are_flawed() {
        use yunta_core::shown::Flaw;
        let flaws = flaws_of(BY_A_NAME, HOLLOW);
        let cmd = "cargo test --test pack_cmd && grep -q 'scope_case' tests/pack_cmd.rs";
        assert_eq!(
            flaws,
            vec![
                Flaw::PassesByAName {
                    task: "cli".into(),
                    cmd: cmd.to_string(),
                    file: "tests/pack_cmd.rs".to_string(),
                },
                Flaw::HollowSpecTest {
                    task: "cli".into(),
                    cmd: cmd.to_string(),
                },
                Flaw::UnrunSpecFile {
                    task: "cli".into(),
                    path: "tests/scope_spec.rs".to_string(),
                },
            ]
        );
    }

    #[test]
    fn a_change_to_a_spec_test_is_found_however_its_path_is_written() {
        let plan = PLAN.replace(
            "{ at: tests/store.rs, what: its test }",
            "{ at: ./tests/store.rs, what: its test }",
        );
        let plan: TasksFile = yunta_core::yaml::parse(&plan).unwrap();
        let spec: SpecFile = yunta_core::yaml::parse(SPEC).unwrap();
        let review = review(plan, Some(spec), None, Vec::new());
        assert_eq!(review.tasks[0].denied.len(), 1, "{:?}", review.tasks[0]);
    }

    #[test]
    fn a_task_s_own_guards_are_kept_apart_from_its_criteria() {
        let plan: TasksFile = yunta_core::yaml::parse(BY_A_NAME).unwrap();
        let review = review(plan, None, Some("cargo test"), Vec::new());
        let guards: Vec<&str> = review.tasks[0]
            .guards
            .iter()
            .map(|g| g.cmd.as_str())
            .collect();
        assert_eq!(guards, ["cargo test --test pack_lock"]);
        assert!(review.tasks[0]
            .criteria
            .iter()
            .all(|criterion| criterion.cmd != "cargo test --test pack_lock"));
    }
}
