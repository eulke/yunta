//! Which suite a task is held to: the one its run's lineage measured
//! green, read off the log before any run works the task.

use yunta_core::events::{BaselineCapturedPayload, BaselineOrigin, BaselineResults, RunEvent};
use yunta_core::{Criterion, RunId, Task};
use yunta_engine::judged_task;

use super::*;

fn measured(exit_code: i32, origin: BaselineOrigin) -> BaselineCapturedPayload {
    BaselineCapturedPayload {
        command: SUITE.to_string(),
        results: BaselineResults {
            exit_code,
            summary: String::new(),
        },
        hash: yunta_core::sha256_hex(b""),
        origin,
        tree: None,
        duration_ms: None,
    }
}

/// What a run whose log holds `measurements` holds its tasks to.
fn held(measurements: Vec<RunEvent>) -> Vec<String> {
    let log = measurements
        .into_iter()
        .fold(
            yunta_testkit_core::Log::for_run("run-held"),
            |log, event| log.event(EventPayload::Run(event)),
        )
        .build();
    yunta_engine::held_of(&RunState::replay(&log).run)
}

/// The suite, measured with `exit_code`.
fn suite(exit_code: i32, origin: BaselineOrigin) -> RunEvent {
    RunEvent::BaselineCaptured(measured(exit_code, origin))
}

fn task_with(criteria: Vec<Criterion>) -> Task {
    Task {
        id: "T001".into(),
        title: "Make it".to_string(),
        scope: vec!["made.txt".into()],
        criteria,
        depends_on: Vec::new(),
        notes: None,
        description: None,
        changes: Vec::new(),
        outcome: None,
        uses: Vec::new(),
        invariants: Vec::new(),
    }
}

fn criterion(cmd: &str, r#type: Option<CriterionType>) -> Criterion {
    Criterion {
        cmd: cmd.to_string(),
        r#type,
        proves: None,
    }
}

/// The suite each criterion list is judged by, as `(cmd, is_guard)`.
fn commands(task: &Task) -> Vec<(&str, bool)> {
    task.criteria
        .iter()
        .map(|criterion| (criterion.cmd.as_str(), criterion.is_guard()))
        .collect()
}

#[test]
fn a_task_is_judged_by_the_suite_its_lineage_measured_green() {
    let task = task_with(vec![criterion("test -f made.txt", None)]);
    let judged = judged_task(&task, &held(vec![suite(0, BaselineOrigin::Measured)]));
    assert_eq!(
        commands(&judged),
        vec![("test -f made.txt", false), (SUITE, true)]
    );
    assert!(
        judged.criteria[1].proves.is_some(),
        "the guard says what it is there to show: {judged:#?}"
    );
    assert_eq!(judged.id, task.id, "only the criteria change");
}

#[test]
fn a_red_measurement_adds_no_guard_to_any_task() {
    let task = task_with(vec![criterion("test -f made.txt", None)]);
    assert_eq!(
        judged_task(&task, &held(vec![suite(1, BaselineOrigin::Measured)])),
        task
    );
    assert_eq!(judged_task(&task, &[]), task, "nor does no measurement");
}

#[test]
fn a_task_that_declares_the_suite_is_judged_by_its_own_declaration() {
    for declared in [Some(CriterionType::Guard), None] {
        let task = task_with(vec![
            criterion("test -f made.txt", None),
            criterion(&format!("  {SUITE} "), declared),
        ]);
        assert_eq!(
            judged_task(&task, &held(vec![suite(0, BaselineOrigin::Measured)])),
            task
        );
    }
}

#[test]
fn an_inherited_measurement_holds_tasks_to_the_same_suite() {
    let task = task_with(vec![criterion("test -f made.txt", None)]);
    let inherited = suite(
        0,
        BaselineOrigin::Inherited {
            run: RunId::from("01JROOT0000000000000000000"),
        },
    );
    assert_eq!(
        commands(&judged_task(&task, &held(vec![inherited]))),
        vec![("test -f made.txt", false), (SUITE, true)]
    );
}
