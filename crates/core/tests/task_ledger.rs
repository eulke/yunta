//! What the fold from the tasks domain says a criterion cost, and what
//! it refuses to count; what a task owes a person about the plan; and
//! what a task is proved by and may write, with the checks that answered.
//!
//! The order a pre-check runs its commands in is derived from this, so
//! a cost counted for an execution that never happened, or held apart
//! per task, would order real work by a number nobody measured.

use yunta_core::events::tasks::{TaskCause, TaskStop};
use yunta_core::events::{
    AcceptedDeparture, CriteriaCheckedPayload, Criterion, CriterionResult, CriterionType, DeniedBy,
    DeniedCommand, DeniedRule, DepartsFrom, DeviationDeclaredPayload, DeviationResolvedPayload,
    EventMeta, NodeEvent, Phase, ScopeCheckedPayload, TaskCheckAnsweredPayload, TaskEvent,
    TaskLedger, TaskRegisteredPayload, TaskStatus, TaskStatusChangedPayload,
};
use yunta_core::{NodeId, ScopeGlob};

/// One criterion's outcome: what it cost, or nothing at all when the
/// check answered it out of the invocation's cache.
fn result(cmd: &str, duration_ms: Option<u64>) -> CriterionResult {
    CriterionResult {
        cmd: cmd.to_string(),
        exit_code: 0,
        r#type: None,
        reused: duration_ms.is_none(),
        duration_ms,
        output: None,
        tail: Vec::new(),
        tree: None,
        head: None,
        conclusion: None,
    }
}

/// A pre-check of `task` against `results`.
fn checked(task: &str, results: Vec<CriterionResult>) -> NodeEvent {
    NodeEvent::CriteriaChecked(CriteriaCheckedPayload {
        task_id: task.into(),
        phase: Phase::Pre,
        results,
        waiting: Vec::new(),
    })
}

#[test]
fn two_tasks_checked_against_one_command_share_its_history() {
    let mut ledger = TaskLedger::default();
    ledger.apply_checks(&checked("T001", vec![result("cargo test", Some(900))]));
    ledger.apply_checks(&checked("T002", vec![result("cargo test", Some(1_100))]));

    assert_eq!(
        ledger.criterion_durations("cargo test"),
        [900, 1_100],
        "a suite guarding two tasks is one command with one history"
    );
    assert!(
        ledger.criterion_durations("cargo fmt").is_empty(),
        "a command no check ran costs nothing the log can name"
    );
}

#[test]
fn a_reused_result_prices_nothing() {
    let mut ledger = TaskLedger::default();
    ledger.apply_checks(&checked("T001", vec![result("cargo test", Some(900))]));
    ledger.apply_checks(&checked("T001", vec![result("cargo test", None)]));

    assert_eq!(
        ledger.criterion_durations("cargo test"),
        [900],
        "nothing ran, so the command's cost is still what its executions measured"
    );
}

/// Folds `events` in order, under one loop node.
fn fold(events: &[TaskEvent]) -> TaskLedger {
    let node = NodeId::from_static("implement");
    let mut ledger = TaskLedger::default();
    for (seq, event) in events.iter().enumerate() {
        let meta = EventMeta {
            seq: (seq as u64 + 1).into(),
            at: chrono::DateTime::UNIX_EPOCH,
            node: Some(&node),
        };
        ledger.apply(event, &meta).unwrap();
    }
    ledger
}

fn registered(task: &str) -> TaskEvent {
    TaskEvent::Registered(TaskRegisteredPayload {
        task_id: task.into(),
        criteria: Vec::new(),
        scope: Vec::new(),
        depends_on: Vec::new(),
    })
}

fn declared(task: &str, shape: &str) -> DeviationDeclaredPayload {
    DeviationDeclaredPayload {
        task_id: task.into(),
        from: DepartsFrom::Shape(shape.to_string()),
        planned: "what the plan says".to_string(),
        instead: "what was built".to_string(),
        why: "why".to_string(),
    }
}

fn resolved(task: &str, accepted: bool, said: Option<&str>) -> TaskEvent {
    TaskEvent::DeviationResolved(DeviationResolvedPayload {
        task_id: task.into(),
        accepted,
        said: said.map(str::to_string),
        respecified_by: None,
    })
}

#[test]
fn a_task_owes_every_departure_declared_until_one_answer_settles_them_all() {
    let first = declared("T001", "Greeting");
    let second = declared("T001", "Farewell");
    let asked = [
        registered("T001"),
        TaskEvent::DeviationDeclared(first.clone()),
        TaskEvent::DeviationDeclared(second.clone()),
    ];
    let owing = fold(&asked);
    let record = owing.get("T001").unwrap();
    assert_eq!(record.departures_owed, [first.clone(), second.clone()]);
    assert!(record.departures_accepted.is_empty());

    let accepted = fold(&[asked.as_slice(), &[resolved("T001", true, Some("fine"))]].concat());
    let record = accepted.get("T001").unwrap();
    assert!(
        record.departures_owed.is_empty(),
        "answered, nothing is owed"
    );
    assert_eq!(
        record.departures_accepted,
        [
            AcceptedDeparture {
                declared: first,
                said: Some("fine".to_string())
            },
            AcceptedDeparture {
                declared: second,
                said: Some("fine".to_string())
            },
        ]
    );

    let sent_back = fold(&[asked.as_slice(), &[resolved("T001", false, None)]].concat());
    let record = sent_back.get("T001").unwrap();
    assert!(
        record.departures_owed.is_empty(),
        "answered, nothing is owed"
    );
    assert!(
        record.departures_accepted.is_empty(),
        "a departure sent back is not the plan's"
    );
}

#[test]
fn a_task_whose_tests_a_person_accepted_are_wrong_names_who_writes_them_again() {
    let departed = declared("T001", "Greeting");
    let mut accepting = resolved("T001", true, Some("say Hi"));
    if let TaskEvent::DeviationResolved(answer) = &mut accepting {
        answer.respecified_by = Some(NodeId::from_static("spec"));
    }
    let events = [
        registered("T001"),
        TaskEvent::DeviationDeclared(departed.clone()),
        accepting,
    ];

    let ledger = fold(&events);
    let respecified = ledger.get("T001").unwrap().respecified.clone().unwrap();
    assert_eq!(respecified.by.as_str(), "spec");
    assert_eq!(respecified.at, 3u64.into(), "where the person answered");
    assert_eq!(respecified.departures, [departed]);
    assert_eq!(respecified.said.as_deref(), Some("say Hi"));

    let sent_back = fold(&[events[..2].to_vec(), vec![resolved("T001", false, None)]].concat());
    assert!(
        sent_back.get("T001").unwrap().respecified.is_none(),
        "nothing is written again for a departure sent back"
    );
}

/// A task moved to `status`, with no stop named.
fn moved_to(task: &str, status: TaskStatus) -> TaskEvent {
    TaskEvent::StatusChanged(TaskStatusChangedPayload::to(task.into(), status, 1.into()))
}

/// A task blocked by `cause`.
fn blocked(task: &str, cause: TaskCause) -> TaskEvent {
    TaskEvent::StatusChanged(TaskStatusChangedPayload::blocked(
        task.into(),
        1.into(),
        None,
        cause,
    ))
}

#[test]
fn stops_name_each_blocked_task_in_task_order_until_its_status_moves_on() {
    let first = TaskCause::ScopeDecisionOwed;
    let second = TaskCause::CommandDenied {
        rule: DeniedRule::Command(DeniedCommand {
            command: "cargo test".to_string(),
            by: DeniedBy::Deny {
                pattern: "cargo *".to_string(),
            },
        }),
    };
    let ledger = fold(&[
        registered("T002"),
        registered("T001"),
        registered("T003"),
        blocked("T002", second.clone()),
        blocked("T001", first.clone()),
        moved_to("T003", TaskStatus::Blocked),
    ]);
    assert_eq!(
        ledger.stops(),
        vec![
            TaskStop {
                task_id: "T001".into(),
                cause: first,
            },
            TaskStop {
                task_id: "T002".into(),
                cause: second.clone(),
            },
        ],
        "a blocked task that names no stop is not listed"
    );

    let moved_on = fold(&[
        registered("T001"),
        blocked("T001", second),
        moved_to("T001", TaskStatus::Pending),
    ]);
    assert!(
        moved_on.stops().is_empty(),
        "a status change that names no stop clears the one before it"
    );
}

#[test]
fn a_task_keeps_the_criteria_and_the_scope_it_was_registered_with() {
    let criteria = vec![
        Criterion {
            cmd: "cargo test -p yunta-core".to_string(),
            r#type: None,
        },
        Criterion {
            cmd: "cargo fmt --check".to_string(),
            r#type: Some(CriterionType::Guard),
        },
    ];
    let scope = vec![ScopeGlob::from("crates/core/**")];
    let ledger = fold(&[TaskEvent::Registered(TaskRegisteredPayload {
        task_id: "T001".into(),
        criteria: criteria.clone(),
        scope: scope.clone(),
        depends_on: Vec::new(),
    })]);

    let record = ledger.get("T001").unwrap();
    assert_eq!(record.criteria, criteria, "what proves it, in its order");
    assert_eq!(record.scope, scope, "what it may write");
}

/// A check of `task` in `phase` whose one criterion exited `exit_code`,
/// printing `tail` when it did not pass.
fn check(task: &str, phase: Phase, exit_code: i32, tail: &[&str]) -> CriteriaCheckedPayload {
    CriteriaCheckedPayload {
        task_id: task.into(),
        phase,
        results: vec![CriterionResult {
            exit_code,
            tail: tail.iter().map(|line| line.to_string()).collect(),
            ..result("cargo test", Some(900))
        }],
        waiting: Vec::new(),
    }
}

#[test]
fn a_task_is_checked_by_its_latest_check_after_the_work_alone() {
    let mut ledger = fold(&[registered("T001")]);
    let checked = |ledger: &TaskLedger| ledger.get("T001").unwrap().checked.clone();

    ledger.apply_checks(&NodeEvent::CriteriaChecked(check(
        "T001",
        Phase::Pre,
        1,
        &["no test named greeting"],
    )));
    assert_eq!(
        checked(&ledger),
        None,
        "a check before the work proves none"
    );

    let red = check("T001", Phase::Post, 1, &["assertion failed: hello"]);
    ledger.apply_checks(&NodeEvent::CriteriaChecked(red.clone()));
    assert_eq!(checked(&ledger), Some(red.clone()));

    let meta = EventMeta {
        seq: 9u64.into(),
        at: chrono::DateTime::UNIX_EPOCH,
        node: None,
    };
    let session_s_own = TaskEvent::CheckAnswered(TaskCheckAnsweredPayload {
        task_id: "T001".into(),
        closes: true,
        results: vec![result("cargo test", Some(900))],
        waiting: Vec::new(),
        outside_scope: Vec::new(),
        denied: Vec::new(),
        duration_ms: 900,
    });
    ledger.apply(&session_s_own, &meta).unwrap();
    assert_eq!(
        checked(&ledger),
        Some(red),
        "a session's own check judges work in progress and proves nothing"
    );

    let green = check("T001", Phase::Post, 0, &[]);
    ledger.apply_checks(&NodeEvent::CriteriaChecked(green.clone()));
    assert_eq!(
        checked(&ledger),
        Some(green),
        "a retried task is proved by its last attempt"
    );
}

#[test]
fn a_tasks_check_before_the_work_and_its_check_after_are_kept_apart() {
    let mut ledger = fold(&[registered("T001")]);
    let kept = |ledger: &TaskLedger| {
        let record = ledger.get("T001").unwrap();
        (record.checked_before.clone(), record.checked.clone())
    };

    let red_before = check("T001", Phase::Pre, 1, &["no test named greeting"]);
    ledger.apply_checks(&NodeEvent::CriteriaChecked(red_before.clone()));
    assert_eq!(kept(&ledger), (Some(red_before.clone()), None));

    let green_after = check("T001", Phase::Post, 0, &[]);
    ledger.apply_checks(&NodeEvent::CriteriaChecked(green_after.clone()));
    assert_eq!(
        kept(&ledger),
        (Some(red_before), Some(green_after.clone())),
        "a check after the work leaves the one before it as it was"
    );

    let again_before = check("T001", Phase::Pre, 1, &["assertion failed: hello"]);
    ledger.apply_checks(&NodeEvent::CriteriaChecked(again_before.clone()));
    assert_eq!(
        kept(&ledger),
        (Some(again_before), Some(green_after)),
        "a retried task's check before the work is its latest"
    );
}

/// A scope check of `task`, or of its node when `task` is `None`, that
/// found `diff` changed and `violations` outside what it may write.
fn scope_check(task: Option<&str>, diff: &[&str], violations: &[&str]) -> ScopeCheckedPayload {
    ScopeCheckedPayload {
        task_id: task.map(Into::into),
        diff: diff.iter().map(Into::into).collect(),
        violations: violations.iter().map(Into::into).collect(),
        denied: Vec::new(),
    }
}

#[test]
fn a_task_keeps_its_latest_scope_check_and_a_node_s_lands_on_no_task() {
    let mut ledger = fold(&[registered("T001"), registered("T002")]);
    let scope_checked =
        |ledger: &TaskLedger, task: &str| ledger.get(task).unwrap().scope_checked.clone();

    let first = scope_check(Some("T001"), &["src/a.rs", "build.rs"], &["build.rs"]);
    ledger.apply_checks(&NodeEvent::ScopeChecked(first.clone()));
    assert_eq!(scope_checked(&ledger, "T001"), Some(first));

    let latest = scope_check(Some("T001"), &["src/a.rs"], &[]);
    ledger.apply_checks(&NodeEvent::ScopeChecked(latest.clone()));
    ledger.apply_checks(&NodeEvent::ScopeChecked(scope_check(
        None,
        &["src/b.rs"],
        &["src/b.rs"],
    )));
    assert_eq!(
        scope_checked(&ledger, "T001"),
        Some(latest),
        "a retried task is held to its last attempt's paths, and a node's check is not one of its"
    );
    assert_eq!(
        scope_checked(&ledger, "T002"),
        None,
        "a check that names no task is kept on none"
    );
}
