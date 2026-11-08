use chrono::TimeZone;
use yunta_core::events::{Claim, Escalation, Failure, FailureMenu, GateOption, Menu, PauseReason};
use yunta_core::reserved::ReservedOption;
use yunta_core::{NodeId, NonEmpty};

use super::*;

/// A run standing at `standing` in `phase`, the rest of it as the test
/// kit draws a run.
fn frame(standing: Standing, phase: RunPhase) -> RunFrame {
    RunFrame {
        standing,
        phase,
        ..yunta_replay::frame::RunFrame::sample(&RunId::from_static("run-1"))
    }
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

#[test]
fn a_run_paused_on_a_reset_names_its_node_and_the_moment_it_resumes() {
    let at = chrono::Utc
        .with_ymd_and_hms(2026, 10, 13, 12, 0, 0)
        .unwrap();
    let phase = RunPhase::Waiting {
        on: WaitingOn::Run {
            reason: PauseReason::AwaitingReset {
                node: NodeId::from_static("build"),
                at,
            },
        },
    };
    let (node, resumes) = reset_pause(&phase).expect("a run paused on a reset");
    assert_eq!((node.as_str(), resumes), ("build", at));
    assert!(reset_pause(&RunPhase::Running).is_none());
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
fn a_failure_stuck_on_an_unchanged_tree_lists_the_resume_that_retries_once_it_changed() {
    let stuck = decision_stuck_on(Stuck::TreeUnchanged);
    assert_eq!(
        after(&parked(), Some(&stuck)),
        vec![
            (
                "yunta resume run-1".to_string(),
                "offers a retry once the run's tree changed",
            ),
            ("yunta close run-1".to_string(), "closes it for good"),
        ]
    );
    let denied = decision_stuck_on(Stuck::ProjectDenies);
    assert_eq!(
        after(&parked(), Some(&denied)),
        vec![("yunta close run-1".to_string(), "closes it for good")]
    );
}

/// What a parent parked on its child waits on stands on the child's log, so what a reader
/// needs first is the way there, named as that reader copies it.
#[test]
fn the_parent_of_a_stopped_child_offers_the_command_that_reaches_it() {
    let whole = "01AAAAAAAAAAAAAAAAAAAAAAAA";
    let parent = frame(
        Standing::NeedsYou,
        RunPhase::Waiting {
            on: WaitingOn::Run {
                reason: PauseReason::ChildPaused {
                    node: NodeId::from_static("compose"),
                    run: RunId::from_static(whole),
                    child: Box::new(PauseReason::BudgetExhausted { spent: 5, cap: 4 }),
                },
            },
        },
    );
    let why = "shows what it is waiting on";
    assert_eq!(
        after(&parent, None)[0],
        ("yunta status AAAAAA".to_string(), why)
    );
    assert_eq!(
        after_for(&parent, None, Reader::Agent)[0],
        (format!("yunta status {whole}"), why)
    );
    assert_eq!(
        after(&parent, None)[1..],
        after(&parked(), None),
        "and the parent is still handed back and closed as before"
    );
}

#[test]
fn a_run_that_needs_a_person_without_a_decision_is_handed_back_once_settled() {
    assert_eq!(
        after(&parked(), None),
        vec![
            (
                "yunta resume run-1".to_string(),
                "hands it back once that is settled",
            ),
            ("yunta close run-1".to_string(), "closes it for good"),
        ]
    );
}

#[test]
fn a_run_waiting_on_a_reset_is_offered_the_reset_words() {
    let reset = frame(
        Standing::Waiting,
        RunPhase::Waiting {
            on: WaitingOn::Run {
                reason: PauseReason::AwaitingReset {
                    node: NodeId::from_static("build"),
                    at: chrono::Utc
                        .with_ymd_and_hms(2026, 10, 13, 12, 0, 0)
                        .unwrap(),
                },
            },
        },
    );
    assert_eq!(
        after(&reset, None),
        goes_on(
            &RunId::from_static("run-1"),
            Reader::Person,
            "waits for the reset, then drives on"
        )
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
        after(&stopped, None),
        vec![
            (
                "yunta resume run-1".to_string(),
                "continues it where it stopped"
            ),
            ("yunta close run-1".to_string(), "closes it for good"),
        ]
    );
    assert!(after(&frame(Standing::Stopped, RunPhase::Cancelled), None).is_empty());
}
