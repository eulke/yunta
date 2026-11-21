//! The small runs that stand with nothing a person picks to answer them.

use yunta_core::events::{NodeState, PauseReason};
use yunta_core::NodeId;
use yunta_replay::frame::RunPhase;
use yunta_testkit_ui::small_runs::standstills;

use crate::{at, built_twice, stand};

#[tokio::test]
async fn a_pause_whose_escalation_carries_no_menu_is_answered_by_no_option() {
    let run = built_twice(standstills::no_menu).await;
    let stand = stand(&run).await;
    let Some(PauseReason::Escalation(escalation)) = &stand.pause else {
        panic!("the run parks on an escalation: {:?}", stand.pause);
    };
    assert_eq!(escalation.menu(), None);
    assert_eq!(stand.menu, None, "the engine rebuilds no menu for it");
}

#[tokio::test]
async fn a_run_with_no_room_on_the_disk_parks_before_its_node() {
    let run = built_twice(standstills::no_room).await;
    let stand = stand(&run).await;
    assert!(
        matches!(stand.pause, Some(PauseReason::NoRoom { .. })),
        "{:?}",
        stand.pause
    );
}

#[tokio::test]
async fn a_node_left_half_done_parks_the_run_on_it() {
    let run = built_twice(standstills::orphans).await;
    let stand = stand(&run).await;
    assert_eq!(
        stand.pause,
        Some(PauseReason::UncertainOrphans {
            nodes: vec![NodeId::from("only")]
        })
    );
}

#[tokio::test]
async fn a_run_with_nothing_runnable_parks_blocked() {
    let run = built_twice(standstills::blocked).await;
    let stand = stand(&run).await;
    assert_eq!(
        stand.pause,
        Some(PauseReason::Blocked {
            node: NodeId::from("ship"),
            on: vec![NodeId::from("build")],
        })
    );
}

#[tokio::test]
async fn a_run_stopped_while_its_session_works_parks_cancelled() {
    let run = built_twice(standstills::stopped).await;
    assert_eq!(stand(&run).await.pause, Some(PauseReason::Cancelled));
}

#[tokio::test]
async fn a_decision_answered_abort_parks_where_it_is_with_why() {
    let run = built_twice(standstills::aborted).await;
    assert_eq!(
        stand(&run).await.pause,
        Some(PauseReason::GateAborted {
            node: NodeId::from("lint"),
            free_text: Some("not worth chasing today".to_string()),
        })
    );
}

#[tokio::test]
async fn a_run_its_engine_left_mid_node_says_the_node_runs() {
    let run = built_twice(standstills::stalled).await;
    let stand = stand(&run).await;
    assert!(
        matches!(stand.frame.phase, RunPhase::Running),
        "{:?}",
        stand.frame.phase
    );
    let build = stand
        .frame
        .nodes
        .iter()
        .find(|node| node.id.as_str() == "build")
        .expect("the run's first node");
    assert!(
        matches!(
            build.state,
            yunta_replay::frame::NodeStanding::Reached(NodeState::Running { attempt: 1 })
        ),
        "{:?}",
        build.state
    );
}

#[tokio::test]
async fn a_gate_asked_at_the_engines_terminal_stands_open_before_its_answer() {
    let run = built_twice(standstills::asked_elsewhere).await;
    let events = run.events(run.run());
    let asked = at(&events, "node_started", Some("first"), 0);
    let answered = at(&events, "gate_waiting", Some("ship"), asked);
    assert!(
        !crate::kinds(&events[..answered])
            .iter()
            .any(|(kind, _)| kind == "run_paused"),
        "the run never parked before its answer"
    );
    at(&events, "gate_resolved", Some("ship"), answered);
    assert!(matches!(stand(&run).await.frame.phase, RunPhase::Finished));
}
