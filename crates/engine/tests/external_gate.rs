//! `kind: gate` with `external: {kind: pull_request}`,
//! exercised end-to-end against `MockForge` — never a real network call
//! (the no-real-LLM-or-network-call-in-tests rule extended to forges). The
//! core scenario: person B approves the PR without Yunta installed at all (simulated by
//! driving `MockForgeState` directly, never through the `Forge` trait —
//! exactly what "no Yunta on B's machine" means), and person A's
//! machine picks up the approval on a completely separate `execute_run`
//! call, simulating `yunta resume`.

use std::sync::Arc;

use yunta_adapters::{MockForge, MockForgeState};
use yunta_core::events::{FindingEvent, GateEvent, NodeEvent};
use yunta_engine::{NodeState, RunReport, RunTerminal};
use yunta_testkit::Bench;

const GATE_ONLY_WORKFLOW: &str = r#"
name: gate-scenario
nodes:
  - id: approve
    kind: gate
    assignee: reviewer
    external:
      kind: pull_request
      artifacts: []
      branch: "{{run.branch}}"
"#;

/// A gate followed by a node that always fails with no `on_failure` —
/// once the gate resolves the run still has unresolved work (`after`
/// pauses rather than finishing), which is what keeps the run's own log
/// free of `run_finished` long enough to exercise a *later* SHA-drift
/// recheck: `execute_run` treats a genuinely finished run as an
/// immutable no-op (reopening anything after `run_finished`
/// would mean mutating a run the log already closed), so the drift
/// recheck only ever matters, and only ever runs, while the run is
/// still open.
const GATE_THEN_UNRESOLVED_WORKFLOW: &str = r#"
name: gate-scenario
nodes:
  - id: approve
    kind: gate
    assignee: reviewer
    external:
      kind: pull_request
      artifacts: []
      branch: "{{run.branch}}"
  - id: after
    kind: bash
    depends_on: [approve]
    run: "false"
"#;

/// A bench whose every wake reads one forge, alongside that forge's
/// state — the handle person B acts through, with no Yunta on their end.
fn bench_on_a_forge() -> (Bench, MockForgeState) {
    let forge_state = MockForgeState::new();
    let bench =
        Bench::with_run_id("run-gate-1").with_forge(Arc::new(MockForge::new(forge_state.clone())));
    (bench, forge_state)
}

#[tokio::test]
async fn an_external_gate_publishes_pauses_and_resolves_on_a_separate_wake() {
    let (bench, forge_state) = bench_on_a_forge();

    // Person A's machine: first wake reaches the gate, publishes, pauses.
    let RunReport { terminal, state } = bench.run(GATE_ONLY_WORKFLOW, "sessions: []\n").await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "got {terminal:?}"
    );
    // A published, unresolved gate derives `waiting` —
    // never "absent" and never running/failed.
    assert!(
        matches!(
            state.nodes.state("approve"),
            Some(NodeState::Waiting {
                external_ref: Some(_)
            })
        ),
        "a published unresolved gate must derive Waiting with its PR ref, got {:?}",
        state.nodes.state("approve")
    );
    assert!(
        forge_state.pr_number(bench.run_id.as_str()).is_some(),
        "publish must have opened a PR"
    );

    // Person B: reviews and approves directly on the forge — no Yunta
    // involved on their end at all.
    forge_state.approve(bench.run_id.as_str(), &"person-b".into());

    // Person A's machine, a second, wholly separate `execute_run` call
    // (simulating `yunta resume`): picks the approval up on its own.
    let RunReport { terminal, state } = bench.wake().await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.state("approve"),
        Some(NodeState::Finished { .. })
    ));

    let events = bench.events();
    let resolved = events.iter().find_map(|e| match e.payload() {
        Some(yunta_core::events::EventPayload::Gates(GateEvent::Resolved(p))) => Some(p),
        _ => None,
    });
    let Some(yunta_core::events::GateResolvedPayload::Approved { by, .. }) = resolved else {
        panic!("gate_resolved records the approval with the SHA it covers, got {resolved:?}");
    };
    assert_eq!(by.as_str(), "person-b");
}

#[tokio::test]
async fn a_commit_after_approval_returns_the_gate_to_waiting() {
    // Needs a still-open run (see this workflow's own doc comment) —
    // `after` fails with no `on_failure`, so the run pauses rather than
    // reaching `run_finished` once the gate resolves.
    let (bench, forge_state) = bench_on_a_forge();

    let RunReport { terminal, .. } = bench
        .run(GATE_THEN_UNRESOLVED_WORKFLOW, "sessions: []\n")
        .await;
    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    forge_state.approve(bench.run_id.as_str(), &"person-b".into());
    let RunReport { terminal, state } = bench.wake().await;
    // The gate itself resolved (Finished), but `after` failed on its own
    // with nowhere to reroute — the run as a whole is still Paused, not
    // Finished, which is exactly what keeps it open to recheck.
    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    assert!(matches!(
        state.nodes.state("approve"),
        Some(NodeState::Finished { .. })
    ));

    // A new commit lands on the PR *after* the approval — real forges
    // don't dismiss the old review just because the branch moved.
    forge_state.push_commit(bench.run_id.as_str());

    // A third wake must notice the approval no longer covers the
    // current head and go back to waiting.
    let RunReport { terminal, state } = bench.wake().await;
    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    assert!(
        !matches!(
            state.nodes.state("approve"),
            Some(NodeState::Finished { .. })
        ),
        "a stale approval must return the gate to waiting, not stay silently Finished"
    );

    let events = bench.events();
    let started_count = events
        .iter()
        .filter(|e| {
            e.node_id.as_ref().map(|id| id.as_str()) == Some("approve")
                && matches!(
                    e.payload(),
                    Some(yunta_core::events::EventPayload::Node(NodeEvent::Started(
                        _
                    )))
                )
        })
        .count();
    assert_eq!(
        started_count, 2,
        "the drift recheck must re-open the node (a second node_started), not fabricate a new one"
    );

    // A fresh approval at the new head resolves it again, same as the
    // first time.
    forge_state.approve(bench.run_id.as_str(), &"person-b".into());
    let RunReport { state, .. } = bench.wake().await;
    assert!(matches!(
        state.nodes.state("approve"),
        Some(NodeState::Finished { .. })
    ));
}

#[tokio::test]
async fn changes_requested_posts_findings_and_fails_the_node_retryably() {
    let (bench, forge_state) = bench_on_a_forge();

    bench.run(GATE_ONLY_WORKFLOW, "sessions: []\n").await;
    forge_state.request_changes(
        bench.run_id.as_str(),
        &"person-b".into(),
        vec![yunta_core::port::ReviewComment {
            author: "person-b".to_string(),
            body: "please add a test".to_string(),
            path: Some("src/lib.rs".to_string()),
        }],
    );

    let RunReport { terminal, state } = bench.wake().await;
    // No `on_failure` declared on this workflow's gate, so a retryable
    // failure with nowhere to reroute to just pauses — the same rule
    // any other failed node without `on_failure` already follows.
    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    assert!(matches!(
        state.nodes.state("approve"),
        Some(NodeState::Failed {
            retryable: true,
            ..
        })
    ));

    let events = bench.events();
    let finding = events.iter().find_map(|e| match e.payload() {
        Some(yunta_core::events::EventPayload::Findings(FindingEvent::Posted(p))) => {
            Some(&p.finding)
        }
        _ => None,
    });
    assert_eq!(
        finding.map(|f| f.detail.as_str()),
        Some("please add a test")
    );
}

#[tokio::test]
async fn a_merged_pr_resolves_the_gate_as_approved_by_the_merger() {
    let (bench, forge_state) = bench_on_a_forge();

    bench.run(GATE_ONLY_WORKFLOW, "sessions: []\n").await;
    // Person B merges the PR outright: an approval that also landed.
    let merge_sha = forge_state.merge(bench.run_id.as_str(), &"person-b".into());

    let RunReport { terminal, state } = bench.wake().await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.state("approve"),
        Some(NodeState::Finished { .. })
    ));
    let events = bench.events();
    let resolved = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::Gates(GateEvent::Resolved(p))) => Some(p),
            _ => None,
        })
        .expect("the gate resolves");
    assert_eq!(
        resolved,
        &yunta_core::events::GateResolvedPayload::Approved {
            by: "person-b".into(),
            sha: merge_sha,
        },
        "the evidence is the merge commit"
    );
}

#[tokio::test]
async fn a_merged_gate_stays_resolved_on_later_wakes() {
    // Needs a still-open run (see this workflow's own doc comment): the
    // drift recheck only runs while the run is open.
    let (bench, forge_state) = bench_on_a_forge();

    bench
        .run(GATE_THEN_UNRESOLVED_WORKFLOW, "sessions: []\n")
        .await;
    forge_state.merge(bench.run_id.as_str(), &"person-b".into());
    let RunReport { terminal, state } = bench.wake().await;
    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    assert!(matches!(
        state.nodes.state("approve"),
        Some(NodeState::Finished { .. })
    ));

    // A merged pull request cannot move, and its evidence is the merge
    // commit, not the branch head. A later wake leaves the gate as it
    // resolved instead of reading the difference as drift.
    let RunReport { state, .. } = bench.wake().await;
    assert!(matches!(
        state.nodes.state("approve"),
        Some(NodeState::Finished { .. })
    ));
    let events = bench.events();
    let resolutions = events
        .iter()
        .filter(|e| {
            e.node_id.as_ref().map(|id| id.as_str()) == Some("approve")
                && matches!(
                    e.payload(),
                    Some(yunta_core::events::EventPayload::Gates(
                        GateEvent::Resolved(_)
                    ))
                )
        })
        .count();
    assert_eq!(
        resolutions, 1,
        "a merged gate resolves once; a later wake records neither drift nor a second resolution"
    );
}

#[tokio::test]
async fn a_closed_pr_fails_the_node_non_retryably() {
    let (bench, forge_state) = bench_on_a_forge();

    bench.run(GATE_ONLY_WORKFLOW, "sessions: []\n").await;
    forge_state.close(bench.run_id.as_str());

    let RunReport { state, .. } = bench.wake().await;
    assert!(matches!(
        state.nodes.state("approve"),
        Some(NodeState::Failed {
            retryable: false,
            ..
        })
    ));
}

#[tokio::test]
async fn with_no_forge_the_gate_degrades_to_console_and_never_publishes() {
    // A bench with no forge and no human answering: the gate takes the
    // same degrade-to-pause path a headless console already takes.
    let bench = Bench::with_run_id("run-gate-1");
    let RunReport { terminal, state } = bench.run(GATE_ONLY_WORKFLOW, "sessions: []\n").await;
    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    assert!(
        !state.nodes.has_state("approve"),
        "no forge and no answer from the console must not fabricate a resolution"
    );

    let events = bench.events();
    assert!(
        !events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::Gates(GateEvent::Waiting(
                _
            )))
        )),
        "an unresolved degraded gate must not be recorded as published"
    );
}
