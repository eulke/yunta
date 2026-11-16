//! The small runs that wait on something other than a person here.

use yunta_core::events::PauseReason;
use yunta_testkit_ui::small_runs::waiting;

use crate::{built_twice, stand};

#[tokio::test]
async fn a_run_refused_by_a_spent_rate_limit_waits_for_its_reset() {
    let run = built_twice(waiting::wakes_alone).await;
    let stand = stand(&run).await;
    assert!(
        matches!(&stand.pause, Some(PauseReason::AwaitingReset { node, .. }) if node.as_str() == "pr"),
        "{:?}",
        stand.pause
    );
}

#[tokio::test]
async fn a_gate_published_waits_on_the_forges_reviewers() {
    let run = built_twice(waiting::pull_request).await;
    let stand = stand(&run).await;
    assert!(
        matches!(&stand.pause, Some(PauseReason::ExternalGate { url }) if url.ends_with("/pull/7")),
        "{:?}",
        stand.pause
    );
}

#[tokio::test]
async fn a_parent_parks_on_its_child_parked_on_its_gate() {
    let run = built_twice(waiting::child).await;
    let stand = stand(&run).await;
    let Some(PauseReason::ChildPaused {
        node, run: child, ..
    }) = &stand.pause
    else {
        panic!("the parent parks on its child: {:?}", stand.pause);
    };
    assert_eq!(node.as_str(), "feat");
    assert!(run.runs().contains(child), "the child stands in the store");
}
