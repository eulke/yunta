//! What the fold from the tasks domain says a criterion cost, and what
//! it refuses to count.
//!
//! The order a pre-check runs its commands in is derived from this, so
//! a cost counted for an execution that never happened, or held apart
//! per task, would order real work by a number nobody measured.

use yunta_core::events::{CriteriaCheckedPayload, CriterionResult, NodeEvent, Phase, TaskLedger};

/// One criterion's outcome: what it cost, or nothing at all when the
/// check answered it out of the invocation's cache.
fn result(cmd: &str, duration_ms: Option<u64>) -> CriterionResult {
    CriterionResult {
        cmd: cmd.to_string(),
        exit_code: 0,
        r#type: None,
        reused: duration_ms.is_none(),
        duration_ms,
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
