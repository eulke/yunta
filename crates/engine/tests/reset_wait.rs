//! A limit with a reset is a scheduled resume: the run pauses awaiting the
//! reset its forge named, and a wake resumes the node once the clock has
//! reached that reset. The engine does not wait for the reset itself, so a
//! wake before it pauses again at the same instant.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use yunta_adapters::{MockForge, MockForgeState};
use yunta_core::events::{EventPayload, NodeEvent, PauseReason};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{bare_origin, Bench, MOCK_CONFIG};
use yunta_testkit_core::{fixed_now, HostClock};

/// The forge the run's pull requests go through: a GitHub repository the mock forge stands in for.
const FORGE: &str = "forge:\n  github: { repo: acme/web, token_env: ACME_TOKEN }\n";

/// A node that writes, then the pull request of what it wrote.
const WRITES_THEN_OPENS: &str = r#"
name: opens
nodes:
  - { id: make, kind: bash, run: "echo made > made.txt" }
  - { id: pr, kind: pull_request, depends_on: [make], title: "add dark mode", body: "What the run did." }
"#;

/// The wait the forge names on its refusal: a minute past the failure. The
/// bench's clock starts at the fixed origin, so the reset falls at a known
/// instant.
const RETRY_AFTER: Duration = Duration::from_secs(60);

/// A bench that opens its pull requests on the mock forge, on a clock the
/// test moves.
fn forged_on(host: &Arc<HostClock>, state: &MockForgeState) -> Bench {
    let bench = Bench::new()
        .with_clock(host.clone())
        .with_forge(Arc::new(MockForge::new(state.clone())));
    bare_origin(&bench.worktree);
    bench
}

/// The node a paused run awaits the reset of, and the instant of that reset.
fn awaiting_reset(terminal: &RunTerminal) -> (String, DateTime<Utc>) {
    let RunTerminal::Paused {
        reason: PauseReason::AwaitingReset { node, at },
    } = terminal
    else {
        panic!("the run pauses awaiting a reset: {terminal:?}");
    };
    (node.as_str().to_string(), *at)
}

/// How many times `node` started, across every wake the run took.
fn started(bench: &Bench, node: &str) -> usize {
    bench
        .events()
        .iter()
        .filter(|event| {
            event.node_id.as_ref().is_some_and(|id| id.as_str() == node)
                && matches!(
                    event.payload(),
                    Some(EventPayload::Node(NodeEvent::Started(_)))
                )
        })
        .count()
}

/// Runs the workflow with the forge refusing its pull request for
/// `RETRY_AFTER`, and returns the reset the run pauses awaiting.
async fn parked_on_the_reset(bench: &Bench, state: &MockForgeState) -> (String, DateTime<Utc>) {
    // Any retry-after serves: the run only has to pause on the reset it names.
    state.rate_limit_next(RETRY_AFTER);
    let RunReport { terminal, .. } = bench
        .run_with_config(
            WRITES_THEN_OPENS,
            "dialect: claude-code\nsessions: []\n",
            &format!("{MOCK_CONFIG}{FORGE}"),
        )
        .await;
    awaiting_reset(&terminal)
}

/// The run pauses awaiting the reset the forge named: the wait past the
/// failure, which the bench's clock reads at the fixed origin.
#[tokio::test]
async fn a_forge_limit_with_a_reset_pauses_awaiting_that_reset() {
    let host = Arc::new(HostClock::default());
    let state = MockForgeState::new();
    let bench = forged_on(&host, &state);

    let (node, at) = parked_on_the_reset(&bench, &state).await;

    assert_eq!(node, "pr");
    let wait = chrono::Duration::from_std(RETRY_AFTER).unwrap();
    assert_eq!(at, fixed_now() + wait);
    assert_eq!(started(&bench, "pr"), 1, "only the refused attempt so far");
}

/// A wake while the clock has not reached the reset pauses again awaiting the
/// same reset, and starts no node.
#[tokio::test]
async fn a_wake_before_the_reset_pauses_again_at_the_same_instant() {
    let host = Arc::new(HostClock::default());
    let state = MockForgeState::new();
    let bench = forged_on(&host, &state);
    let (node, at) = parked_on_the_reset(&bench, &state).await;

    let RunReport { terminal, .. } = bench.wake().await;

    assert_eq!(awaiting_reset(&terminal), (node, at));
    assert_eq!(started(&bench, "pr"), 1, "no node starts before the reset");
    assert!(
        state.pull_requests().is_empty(),
        "no pull request is opened before the reset"
    );
}

/// A wake with the clock at the reset runs the node again, and the pull
/// request the forge refused the first time opens.
#[tokio::test]
async fn a_wake_at_the_reset_runs_the_node_again() {
    let host = Arc::new(HostClock::default());
    let state = MockForgeState::new();
    let bench = forged_on(&host, &state);
    parked_on_the_reset(&bench, &state).await;

    // The clock moves to exactly the reset: the wait the forge named has passed.
    host.advance(RETRY_AFTER);
    let RunReport { terminal, .. } = bench.wake().await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(started(&bench, "pr"), 2, "the node ran again at the reset");
    assert_eq!(state.pull_requests().len(), 1);
}
