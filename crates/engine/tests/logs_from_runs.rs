//! Logs an earlier binary wrote, checked in as they were recorded: each one
//! replays with no broken state, and the pause one of them recorded reads
//! back as the sentence it stored.

use yunta_core::events::{Failure, PauseReason, StoredEvent};
use yunta_core::NodeId;
use yunta_engine::{derive, NodeState};

/// The events of a log checked in under `tests/fixtures/logs`, as recorded.
fn load(name: &str) -> Vec<StoredEvent> {
    let path = format!(
        "{}/tests/fixtures/logs/{name}.jsonl",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn checked_in_logs_replay_to_what_they_recorded() {
    for name in [
        "pause-sentence",
        "policy-sentence",
        "reroute-cause",
        "outcome-sentence",
        "scope-request",
        "codex-write-refused",
        "claude-run",
        "closed-pull-request",
    ] {
        assert_eq!(derive(&load(name)).broken, None, "{name}");
    }
    let state = derive(&load("pause-sentence"));
    assert_eq!(
        state.run.paused().map(|(pause, _)| pause),
        Some(&PauseReason::Recorded {
            sentence: "node `grill` failed: Not logged in · Please run /login".into(),
        })
    );
}

/// A gate whose pull request closed in an earlier binary's log replays as the
/// failure its node recorded, and its run pauses on that failure.
#[test]
fn a_closed_pull_request_in_an_older_log_replays_as_the_failure_it_recorded() {
    let state = derive(&load("closed-pull-request"));
    let failure = match state.nodes.state("approve") {
        Some(NodeState::Failed { failure, .. }) => failure,
        other => panic!("the node failed on its closed pull request: {other:?}"),
    };
    assert_eq!(
        failure,
        &Failure::PullRequestClosed {
            closed_pull_request: Some("https://mock.forge/pr/1".to_string()),
        }
    );
    assert_eq!(
        state
            .gates
            .last_external_ref(&NodeId::from_static("approve")),
        None
    );
    let (pause, _) = state.run.paused().expect("the run is paused");
    assert_eq!(
        yunta_render::pause::sentence(pause),
        "node `approve` failed: the pull request was closed without approval"
    );
}
