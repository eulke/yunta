//! A plan as the run will judge it, for the person deciding on it: each
//! task's criteria and what holds the task to each, and the changes the
//! plan names on a test no session may write.

use yunta_core::events::AcceptedDeparture;
use yunta_core::shown::{DeniedChange, HeldTo, JudgedCriterion, PlanReview, TaskReview};
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
    }
}

fn task_review(
    task: &Task,
    spec: Option<&SpecFile>,
    suite: Option<&str>,
    departed: &[AcceptedDeparture],
) -> TaskReview {
    let judged = waived(specified(task.clone(), spec), departed, suite, spec);
    let criteria = judged
        .criteria
        .iter()
        .filter(|criterion| !criterion.is_guard() && Some(criterion.cmd.trim()) != suite)
        .map(|criterion| JudgedCriterion {
            cmd: criterion.cmd.clone(),
            proves: criterion.proves.clone(),
            from: held_to(task, &criterion.cmd, spec),
        })
        .collect();
    TaskReview {
        task: task.id.clone(),
        criteria,
        denied: denied(task, spec),
    }
}

/// What holds `task` to the criterion that runs `cmd`: a test of its own
/// spec, the test the spec gives another task, or its plan alone.
fn held_to(task: &Task, cmd: &str, spec: Option<&SpecFile>) -> HeldTo {
    let cmd = cmd.trim();
    let Some(spec) = spec else {
        return HeldTo::Plan;
    };
    let runs = |of: &yunta_core::Spec| of.tests.iter().any(|test| test.cmd.trim() == cmd);
    if let Some(own) = spec.of(&task.id).filter(|own| runs(own)) {
        return HeldTo::Spec {
            file: own.files.first().map(|file| file.path.clone()),
        };
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
            spec.specs
                .iter()
                .find(|of| of.files.iter().any(|file| file.path == change.file()))
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
        assert_eq!(
            store,
            vec![&HeldTo::Spec {
                file: Some("tests/store.rs".to_string())
            }]
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
                (
                    "cargo test --test list",
                    &HeldTo::Spec {
                        file: Some("tests/list.rs".to_string())
                    }
                ),
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
}
