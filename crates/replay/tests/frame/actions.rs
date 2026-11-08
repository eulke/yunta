//! What a run offers a reader next: an action per step, chosen by where the run
//! stands and the decision it stopped on, each with what it does to the run.

use yunta_core::events::{FailureMenu, GateWaitingPayload, Stuck};
use yunta_core::reserved::ReservedOption;
use yunta_core::step::NextStep;
use yunta_core::RunId;
use yunta_replay::frame::{Action, Effect};

use super::*;

/// A run standing at `standing` in `phase`, the rest of it as the test kit
/// draws a run.
fn frame(standing: Standing, phase: RunPhase) -> RunFrame {
    RunFrame {
        standing,
        phase,
        ..RunFrame::sample(&run())
    }
}

fn run() -> RunId {
    RunId::from_static("run-1")
}

/// A run parked on a person.
fn parked() -> RunFrame {
    frame(
        Standing::NeedsYou,
        RunPhase::Waiting {
            on: WaitingOn::Run {
                reason: PauseReason::BudgetExhausted { spent: 5, cap: 4 },
            },
        },
    )
}

/// `step` offered for what `effect` says it does.
fn action(step: NextStep, effect: Effect) -> Action {
    Action { step, effect }
}

fn resume(effect: Effect) -> Action {
    action(NextStep::Resume { run: run() }, effect)
}

fn close() -> Action {
    action(NextStep::Close { run: run() }, Effect::Closes)
}

/// A decision on a failed node whose menu gives `stuck` as the reason no
/// remedy is offered, with nothing but abort on it.
fn decision_stuck_on(stuck: Stuck) -> GateWaitingPayload {
    let menu = FailureMenu {
        attempt: 1,
        failure: Failure::Message {
            outcome: "exit 1".into(),
        },
        remedies: Vec::new(),
        stuck: Some(stuck),
        reroute: None,
    };
    Escalation::new(
        Claim::NodeFailed {
            node: NodeId::from_static("build"),
        },
        Menu::Failure(Box::new(menu)),
        NonEmpty::from((GateOption::reserved(ReservedOption::Abort), Vec::new())),
    )
    .into_payload()
}

#[test]
fn a_failure_stuck_on_an_unchanged_tree_offers_the_resume_that_retries_once_it_changed() {
    let stuck = decision_stuck_on(Stuck::TreeUnchanged);
    assert_eq!(
        parked().actions(Some(&stuck)),
        vec![resume(Effect::RetriesOnceTreeChanged), close()]
    );
    let denied = decision_stuck_on(Stuck::ProjectDenies);
    assert_eq!(parked().actions(Some(&denied)), vec![close()]);
}

/// What a parent parked on its child waits on stands on the child's log, so
/// what a reader needs first is the way there.
#[test]
fn the_parent_of_a_stopped_child_offers_the_way_to_it_first() {
    let child = RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA");
    let parent = frame(
        Standing::NeedsYou,
        RunPhase::Waiting {
            on: WaitingOn::Run {
                reason: PauseReason::ChildPaused {
                    node: NodeId::from_static("compose"),
                    run: child.clone(),
                    child: Box::new(PauseReason::BudgetExhausted { spent: 5, cap: 4 }),
                },
            },
        },
    );
    let actions = parent.actions(None);
    assert_eq!(
        actions[0],
        action(NextStep::status(&child), Effect::ShowsWhatItWaitsOn)
    );
    assert_eq!(
        actions[1..],
        parked().actions(None),
        "and the parent is still handed back and closed as before"
    );
}

#[test]
fn a_run_that_needs_a_person_without_a_decision_is_handed_back_once_settled() {
    assert_eq!(
        parked().actions(None),
        vec![resume(Effect::HandsBackOnceSettled), close()]
    );
}

#[test]
fn a_run_waiting_on_a_reset_resumes_once_it_resets() {
    let reset = frame(
        Standing::Waiting,
        RunPhase::Waiting {
            on: WaitingOn::Run {
                reason: PauseReason::AwaitingReset {
                    node: NodeId::from_static("build"),
                    at: Utc.with_ymd_and_hms(2026, 10, 13, 12, 0, 0).unwrap(),
                },
            },
        },
    );
    assert_eq!(
        reset.actions(None),
        vec![resume(Effect::WaitsForReset), close()]
    );
}

#[test]
fn a_run_a_person_stopped_is_continued_while_open_and_left_once_closed() {
    let stopped = frame(
        Standing::Stopped,
        RunPhase::Waiting {
            on: WaitingOn::Run {
                reason: PauseReason::Cancelled,
            },
        },
    );
    assert_eq!(
        stopped.actions(None),
        vec![resume(Effect::ContinuesWhereStopped), close()]
    );
    assert!(frame(Standing::Stopped, RunPhase::Cancelled)
        .actions(None)
        .is_empty());
}

#[test]
fn a_finished_run_is_certified_a_broken_one_verified_and_a_moving_one_stopped() {
    assert_eq!(
        frame(Standing::Finished, RunPhase::Finished).actions(None),
        vec![action(NextStep::Receipt { run: run() }, Effect::Certifies)]
    );
    assert_eq!(
        frame(
            Standing::Broken,
            RunPhase::Broken {
                diagnostic: "a seq skipped".to_string()
            }
        )
        .actions(None),
        vec![action(
            NextStep::Verify { run: run() },
            Effect::FindsWhereLogStops
        )]
    );
    assert_eq!(
        frame(Standing::Running, RunPhase::Running).actions(None),
        vec![action(NextStep::Cancel { run: run() }, Effect::Stops)]
    );
}
