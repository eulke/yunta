//! The two older runs every run's screen stands beside in its field.

use chrono::TimeDelta;
use yunta_core::events::{Failure, Menu, NodeState, PauseReason, RefusalCause};
use yunta_replay::frame::{NodeStanding, RunPhase};
use yunta_testkit_ui::small_runs::{beside, SmallRun};

use crate::{built_twice, stand, waiting};

/// How long before the instant the design's run starts `run` wrote its last event.
fn waited_before_the_design(run: &SmallRun) -> TimeDelta {
    let last = run
        .events(run.run())
        .last()
        .expect("a small run writes events")
        .timestamp;
    yunta_testkit_core::fixed_now() - last
}

#[tokio::test]
async fn a_run_whose_cli_nobody_logged_in_to_parks_on_its_grills_failure_forty_days_before() {
    let run = built_twice(beside::not_logged_in).await;
    let stand = stand(&run).await;
    assert!(waiting(&stand.frame.phase), "{:?}", stand.frame.phase);
    assert_eq!(stand.frame.mode.to_string(), "quick");
    assert!(
        matches!(&stand.pause, Some(PauseReason::NodeFailed { node, failure: Failure::SessionRefused { session_refused } })
            if node.as_str() == "grill"
                && session_refused.cause == RefusalCause::Authentication
                && session_refused.message.starts_with("Not logged in")),
        "{:?}",
        stand.pause
    );
    assert!(
        matches!(stand.menu, Some(Menu::Failure(_))),
        "{:?}",
        stand.menu
    );
    let waited = waited_before_the_design(&run);
    assert!(
        (TimeDelta::days(40)..TimeDelta::days(41)).contains(&waited),
        "it has waited {waited} when the design's run starts"
    );
}

#[tokio::test]
async fn a_run_its_engine_left_at_grill_has_stood_eleven_days_before() {
    let run = built_twice(beside::long_stalled).await;
    let stand = stand(&run).await;
    assert!(
        matches!(stand.frame.phase, RunPhase::Running),
        "{:?}",
        stand.frame.phase
    );
    assert_eq!(stand.frame.mode.to_string(), "standard");
    let grill = stand
        .frame
        .nodes
        .iter()
        .find(|node| node.id.as_str() == "grill")
        .expect("the run's first node");
    assert!(
        matches!(
            grill.state,
            NodeStanding::Reached(NodeState::Running { attempt: 1 })
        ),
        "{:?}",
        grill.state
    );
    let stood = waited_before_the_design(&run);
    assert!(
        (TimeDelta::days(11)..TimeDelta::days(12)).contains(&stood),
        "it has stood {stood} when the design's run starts"
    );
}
