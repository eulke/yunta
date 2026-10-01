//! What the fold from the tasks domain says a criterion cost, and what
//! it refuses to count; and what a task owes a person about the plan.
//!
//! The order a pre-check runs its commands in is derived from this, so
//! a cost counted for an execution that never happened, or held apart
//! per task, would order real work by a number nobody measured.

use yunta_core::events::{
    AcceptedDeparture, CriteriaCheckedPayload, CriterionResult, DepartsFrom,
    DeviationDeclaredPayload, DeviationResolvedPayload, EventMeta, NodeEvent, Phase, TaskEvent,
    TaskLedger, TaskRegisteredPayload,
};
use yunta_core::NodeId;

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
    }
}

/// A pre-check of `task` against `results`.
fn checked(task: &str, results: Vec<CriterionResult>) -> NodeEvent {
    NodeEvent::CriteriaChecked(CriteriaCheckedPayload {
        task_id: task.into(),
        phase: Phase::Pre,
        results,
    })
}

#[test]
fn two_tasks_checked_against_one_command_share_its_history() {
    let mut ledger = TaskLedger::default();
    ledger.apply_criteria(&checked("T001", vec![result("cargo test", Some(900))]));
    ledger.apply_criteria(&checked("T002", vec![result("cargo test", Some(1_100))]));

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
    ledger.apply_criteria(&checked("T001", vec![result("cargo test", Some(900))]));
    ledger.apply_criteria(&checked("T001", vec![result("cargo test", None)]));

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
