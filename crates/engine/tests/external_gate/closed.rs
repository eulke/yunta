//! A gate whose pull request a person closed without merging: the run
//! pauses where the gate waits, and the next wake asks again, on a new pull
//! request or on the one a person reopened.

use yunta_adapters::MockForgeState;
use yunta_core::events::{EventPayload, NodeEvent, PauseReason, SessionEvent};
use yunta_core::NodeId;
use yunta_engine::{NodeState, NodeWait, RunReport, RunTerminal};
use yunta_testkit::Bench;

use super::{bench_on_a_forge, GATE_ONLY_WORKFLOW, NO_SESSIONS, PLANNER, PLAN_UNDER_REVIEW};

/// How many starts and failures of `node` the bench's log holds.
fn lifecycle_records(bench: &Bench, node: &str) -> usize {
    bench
        .events()
        .iter()
        .filter(|event| event.node_id.as_ref().map(|id| id.as_str()) == Some(node))
        .filter(|event| {
            matches!(
                event.payload(),
                Some(EventPayload::Node(
                    NodeEvent::Started(_) | NodeEvent::Failed(_)
                ))
            )
        })
        .count()
}

/// The wake that finds the pull request closed pauses the run where the gate
/// waits: the pause names the node and the pull request, the gate keeps no
/// state and no pull request reference, and the log holds no start or failure
/// of the node.
fn assert_the_close_pauses_the_run(bench: &Bench, report: &RunReport) {
    assert_eq!(
        report.terminal,
        RunTerminal::Paused {
            reason: PauseReason::PullRequestClosed {
                node: NodeId::from_static("approve"),
                url: "https://mock.forge/pr/1".to_string(),
            },
        }
    );
    assert!(
        !report.state.nodes.has_state("approve"),
        "the closed gate keeps no state"
    );
    assert_eq!(
        report
            .state
            .gates
            .last_external_ref(&NodeId::from_static("approve")),
        None
    );
    assert_eq!(
        lifecycle_records(bench, "approve"),
        0,
        "the close records neither a start nor a failure of the node"
    );
}

/// The wake after a close publishes the gate again: a new pull request that
/// waits for a review, the gate waiting on it, and the run paused on it.
fn assert_the_gate_is_published_again(forge_state: &MockForgeState, report: &RunReport) {
    let pull_requests = forge_state.pull_requests();
    assert_eq!(pull_requests.len(), 2);
    assert!(
        pull_requests[1].open,
        "the new pull request waits for a review"
    );
    assert!(
        matches!(
            report.state.nodes.state("approve"),
            Some(NodeState::Waiting {
                on: NodeWait::Gate {
                    external_ref: Some(_)
                }
            })
        ),
        "the gate waits on its new pull request, got {:?}",
        report.state.nodes.state("approve")
    );
    assert_eq!(
        report.terminal,
        RunTerminal::Paused {
            reason: PauseReason::ExternalGate {
                url: "https://mock.forge/pr/2".to_string(),
            },
        }
    );
}

/// A closed pull request is a decision left open rather than a failure: the run
/// pauses where its gate waits, keeps no state for the gate, and the next wake asks
/// again on a new pull request that an approval can finish.
#[tokio::test]
async fn a_closed_pull_request_pauses_the_run_and_a_resume_publishes_the_gate_again() {
    let (bench, forge_state) = bench_on_a_forge();
    bench.run(GATE_ONLY_WORKFLOW, NO_SESSIONS).await;

    forge_state.close(bench.run_id.as_str());
    let closed = bench.wake().await;
    assert_the_close_pauses_the_run(&bench, &closed);

    // The next wake publishes the gate again, on a new pull request.
    let republished = bench.wake().await;
    assert_the_gate_is_published_again(&forge_state, &republished);

    forge_state.approve(bench.run_id.as_str(), &"person-b".into());
    let RunReport { terminal, .. } = bench.wake().await;
    assert_eq!(terminal, RunTerminal::Finished);
}

/// A pull request a person reopens is the one the next wake asks about, so the run
/// waits on it again instead of opening a second pull request.
#[tokio::test]
async fn a_reopened_pull_request_is_asked_again() {
    let (bench, forge_state) = bench_on_a_forge();
    bench.run(GATE_ONLY_WORKFLOW, NO_SESSIONS).await;
    forge_state.close(bench.run_id.as_str());
    let closed = bench.wake().await;
    assert_the_close_pauses_the_run(&bench, &closed);

    forge_state.reopen(bench.run_id.as_str());
    let RunReport { terminal, state } = bench.wake().await;
    assert_eq!(
        terminal,
        RunTerminal::Paused {
            reason: PauseReason::ExternalGate {
                url: "https://mock.forge/pr/1".to_string(),
            },
        }
    );
    let pull_requests = forge_state.pull_requests();
    assert_eq!(
        pull_requests.len(),
        1,
        "the reopened pull request is the one asked again"
    );
    assert!(pull_requests[0].open);
    assert!(matches!(
        state.nodes.state("approve"),
        Some(NodeState::Waiting {
            on: NodeWait::Gate {
                external_ref: Some(_)
            }
        })
    ));
}

/// Closing a pull request is not a request for changes, so the plan is not sent
/// back to its planner, even where the gate declares a reroute for one.
#[tokio::test]
async fn a_closed_pull_request_does_not_reroute() {
    let (bench, forge_state) = bench_on_a_forge();
    bench.run(PLAN_UNDER_REVIEW, PLANNER).await;
    forge_state.close(bench.run_id.as_str());

    let RunReport { terminal, .. } = bench.wake().await;
    assert!(
        matches!(
            terminal,
            RunTerminal::Paused {
                reason: PauseReason::PullRequestClosed { .. }
            }
        ),
        "{terminal:?}"
    );
    let events = bench.events();
    assert!(
        !events.iter().any(|event| matches!(
            event.payload(),
            Some(EventPayload::Node(NodeEvent::Rerouted(_)))
        )),
        "a closed pull request pauses the run; it sends no plan back"
    );
    let planner_sessions = events
        .iter()
        .filter(|event| event.node_id.as_ref().map(|id| id.as_str()) == Some("plan"))
        .filter(|event| {
            matches!(
                event.payload(),
                Some(EventPayload::Session(SessionEvent::Opened(_)))
            )
        })
        .count();
    assert_eq!(planner_sessions, 1, "the planner ran once");
}
