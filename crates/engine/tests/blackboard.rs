//! `coordination: blackboard` end to end: mock sessions acting as REAL MCP clients of the engine's per-run
//! listener — posting findings over the wire mid-session, the group
//! consolidating deterministically at its join, and every mount/
//! capability rule observable as run behavior.

use yunta_core::events::EventPayload;
use yunta_core::events::{FindingEvent, SessionEvent};
use yunta_engine::{NodeState, RunReport, RunTerminal};
use yunta_testkit::Bench;
use yunta_testkit_core::Log;

const BLACKBOARD_WORKFLOW: &str = r#"
name: board
nodes:
  - id: review
    kind: parallel
    coordination: blackboard
    nodes:
      - id: rev-a
        kind: prompt
        runner: executor
        prompt: "Review as A."
      - id: rev-b
        kind: prompt
        runner: executor
        prompt: "Review as B."
"#;

/// `a_first` staggers which reviewer's post lands on the log first —
/// the property under test is that the consolidation is identical
/// either way: it is ordered by content, never by arrival order.
fn blackboard_fixture(a_first: bool) -> String {
    let (a_delay, b_delay) = if a_first { (0, 60) } else { (60, 0) };
    format!(
        r#"
capabilities: {{ run_tools: true }}
sessions:
  - match_prompt_contains: "as A"
    steps:
      - {{ type: run_tool, tool: yunta_post_finding, after_ms: {a_delay}, arguments: {{ id: from-a, severity: minor, title: "a sees x", location: "src/x.rs", detail: "spotted by A" }} }}
    outcome: {{ type: completed, summary: "a done" }}
  - match_prompt_contains: "as B"
    steps:
      - {{ type: run_tool, tool: yunta_post_finding, after_ms: {b_delay}, arguments: {{ id: from-b, severity: major, title: "b sees y", location: "src/y.rs", detail: "spotted by B" }} }}
    outcome: {{ type: completed, summary: "b done" }}
"#
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn blackboard_posts_land_hot_and_the_join_consolidates_them() {
    let bench = Bench::new();
    let RunReport { terminal, state } = bench
        .run(BLACKBOARD_WORKFLOW, &blackboard_fixture(true))
        .await;

    // A failure here must self-diagnose — the terminal's own
    // Paused reason only says "child rev-b failed", while the child's
    // real error (e.g. a run_tool transport failure under load) lives
    // in the node state this message carries.
    assert_eq!(terminal, RunTerminal::Finished, "state: {state:?}");
    assert!(
        matches!(
            state.nodes.state("review"),
            Some(NodeState::Finished { .. })
        ),
        "state: {state:?}"
    );
    // Each post is a finding_posted authored by the session's own node,
    // mediated, logged, and attributed.
    let posted_by_a: Vec<String> = bench
        .findings_by("rev-a")
        .iter()
        .map(|finding| finding.id.to_string())
        .collect();
    let posted_by_b: Vec<String> = bench
        .findings_by("rev-b")
        .iter()
        .map(|finding| finding.id.to_string())
        .collect();
    assert_eq!(posted_by_a, vec!["from-a"]);
    assert_eq!(posted_by_b, vec!["from-b"]);

    // The group's own node-output carries the consolidation —
    // consumable by a node AFTER the parallel, never between siblings.
    let output = bench.group_output("review");
    assert!(
        !output.is_empty(),
        "the blackboard group must consolidate into its node-output"
    );
    // The group's node-output wraps the consolidated findings in the same
    // `stdout:`/`stderr:` envelope every node's captured output uses.
    let doc: serde_json::Value = serde_norway::from_str(&output).unwrap();
    let ids: Vec<&str> = doc["stdout"]
        .as_array()
        .unwrap()
        .iter()
        .map(|finding| finding["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["from-a", "from-b"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn consolidation_is_identical_whatever_order_the_posts_arrived_in() {
    let bench_1 = Bench::new();
    let RunReport {
        terminal: terminal_1,
        state: state_1,
    } = bench_1
        .run(BLACKBOARD_WORKFLOW, &blackboard_fixture(true))
        .await;
    let bench_2 = Bench::new();
    let RunReport {
        terminal: terminal_2,
        state: state_2,
    } = bench_2
        .run(BLACKBOARD_WORKFLOW, &blackboard_fixture(false))
        .await;

    // Same self-diagnosis rule as above — the state names which
    // child failed and why; the finding digests distinguish "a post
    // never landed" from "posts landed but consolidation differed".
    assert_eq!(
        terminal_1,
        RunTerminal::Finished,
        "run 1 (a first) state: {state_1:?}"
    );
    assert_eq!(
        terminal_2,
        RunTerminal::Finished,
        "run 2 (b first) state: {state_2:?}"
    );
    let output_1 = bench_1.group_output("review");
    assert!(
        !output_1.is_empty(),
        "run 1 must consolidate into its node-output"
    );
    let output_2 = bench_2.group_output("review");
    assert!(
        !output_2.is_empty(),
        "run 2 must consolidate into its node-output"
    );
    assert_eq!(
        output_1,
        output_2,
        "the consolidated blackboard must not depend on arrival order\n\
         run 1 findings a/b: {:?}/{:?}\nrun 2 findings a/b: {:?}/{:?}",
        bench_1.findings_by("rev-a"),
        bench_1.findings_by("rev-b"),
        bench_2.findings_by("rev-a"),
        bench_2.findings_by("rev-b"),
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_independent_group_never_serves_the_blackboard() {
    // `independent` (the default) mounts no blackboard tools at
    // all — a session that tries anyway fails, visibly, and the group
    // fails with it.
    let workflow = r#"
name: board
nodes:
  - id: review
    kind: parallel
    nodes:
      - id: rev-a
        kind: prompt
        runner: executor
        prompt: "Review as A."
"#;
    let fixture = r#"
capabilities: { run_tools: true }
sessions:
  - steps:
      - { type: run_tool, tool: yunta_get_blackboard }
    outcome: { type: completed, summary: "never reached" }
"#;
    let RunReport { terminal, state } = Bench::new().run(workflow, fixture).await;
    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    match state.nodes.state("rev-a") {
        Some(NodeState::Failed { failure, .. }) => {
            let outcome = failure.to_string();
            assert!(outcome.contains("blackboard"), "got: {outcome}");
        }
        other => panic!("expected rev-a failed, got {other:?}"),
    }
}

#[tokio::test]
async fn without_the_capability_no_endpoint_is_offered_and_nothing_degrades() {
    // Resting state: capability absent, endpoint None — the
    // session runs fine, and it is NOT a degradation event (nothing
    // declared needed the tools).
    let workflow = r#"
name: plain
nodes:
  - id: talk
    kind: prompt
    runner: executor
    prompt: "Do the thing."
"#;
    let fixture = r#"
sessions:
  - outcome: { type: completed, summary: "done" }
"#;
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench.run(workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(bench.mock().endpoints_seen(), vec![None]);
    assert!(!bench.events().iter().any(|e| matches!(
        e.payload(),
        Some(EventPayload::Session(SessionEvent::CapabilityDegraded(_)))
    )));
}

#[tokio::test]
async fn with_the_capability_every_session_gets_its_own_loopback_endpoint() {
    let workflow = r#"
name: plain
nodes:
  - id: talk
    kind: prompt
    runner: executor
    prompt: "Do the thing."
"#;
    let fixture = r#"
capabilities: { run_tools: true }
sessions:
  - outcome: { type: completed, summary: "done" }
"#;
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench.run(workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);
    let endpoints = bench.mock().endpoints_seen();
    assert_eq!(endpoints.len(), 1);
    let endpoint = endpoints[0]
        .as_ref()
        .expect("the session must get an endpoint");
    assert!(
        endpoint.url.starts_with("http://127.0.0.1:"),
        "got: {}",
        endpoint.url
    );
    assert!(
        endpoint.token.expose().len() >= 32,
        "token must be high-entropy"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_blackboard_child_on_a_capability_less_adapter_fails_actionably() {
    // Declared coordination the adapter can't carry is a node
    // failure with a diagnostic — never silent emulation, never a
    // silently-missing blackboard.
    let fixture = r#"
sessions:
  - outcome: { type: completed, summary: "never reached" }
  - outcome: { type: completed, summary: "never reached" }
"#;
    let RunReport { terminal, state } = Bench::new().run(BLACKBOARD_WORKFLOW, fixture).await;
    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    match state.nodes.state("rev-a") {
        Some(NodeState::Failed { failure, .. }) => {
            let outcome = failure.to_string();
            assert!(
                outcome.contains("run_tools") && outcome.contains("blackboard"),
                "the diagnostic must name the capability and the coordination: {outcome}"
            );
        }
        other => panic!("expected rev-a failed, got {other:?}"),
    }
}

// --- The pure half of the consolidation-order invariant -----------------------

#[test]
fn consolidate_blackboard_is_invariant_under_event_shuffling() {
    use yunta_core::events::{Finding, FindingPostedPayload, FindingSeverity};
    let posted = |id: &str| {
        EventPayload::Findings(FindingEvent::Posted(FindingPostedPayload {
            finding: Finding {
                id: id.into(),
                severity: FindingSeverity::Minor,
                title: format!("title {id}"),
                location: "src/x.rs".into(),
                detail: "detail".to_string(),
                proposed_criterion: None,
            },
        }))
    };
    let members = vec!["a".into(), "b".into()];
    let forward = Log::for_run("run-x")
        .node("a", posted("one"))
        .node("b", posted("two"))
        .node("a", posted("three"))
        .build();
    let mut reversed = forward.clone();
    reversed.reverse();

    let consolidated_forward = yunta_engine::consolidate_blackboard(&forward, &members);
    let consolidated_reversed = yunta_engine::consolidate_blackboard(&reversed, &members);
    assert_eq!(consolidated_forward, consolidated_reversed);
    let consolidated: Vec<serde_json::Value> =
        serde_norway::from_str(&consolidated_forward).unwrap();
    let ids: Vec<&str> = consolidated
        .iter()
        .map(|finding| finding["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["one", "three", "two"]);
}

/// The events of one group: `a` posts `f1` and `f2`, then takes `f1`
/// back and rewrites `f2`.
fn group_log() -> Vec<yunta_core::events::StoredEvent> {
    use yunta_core::events::{
        Finding, FindingPostedPayload, FindingSeverity, FindingUpdatedPayload,
        FindingWithdrawnPayload,
    };
    let finding = |id: &str, title: &str| Finding {
        id: id.into(),
        severity: FindingSeverity::Minor,
        title: title.to_string(),
        location: format!("src/{id}.rs").as_str().into(),
        detail: "detail".to_string(),
        proposed_criterion: None,
    };
    Log::for_run("run-x")
        .node(
            "a",
            EventPayload::Findings(FindingEvent::Posted(FindingPostedPayload {
                finding: finding("f1", "taken back"),
            })),
        )
        .node(
            "a",
            EventPayload::Findings(FindingEvent::Posted(FindingPostedPayload {
                finding: finding("f2", "first wording"),
            })),
        )
        .node(
            "a",
            EventPayload::Findings(FindingEvent::Withdrawn(FindingWithdrawnPayload {
                id: "f1".into(),
                reason: "it was the harness, not the code".to_string(),
            })),
        )
        .node(
            "a",
            EventPayload::Findings(FindingEvent::Updated(FindingUpdatedPayload {
                finding: finding("f2", "last wording"),
            })),
        )
        .build()
}

fn consolidated_titles(events: &[yunta_core::events::StoredEvent]) -> Vec<String> {
    let rendered = yunta_engine::consolidate_blackboard(events, &["a".into()]);
    let entries: Vec<serde_json::Value> = serde_norway::from_str(&rendered).unwrap();
    entries
        .iter()
        .map(|entry| entry["title"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_withdrawn_finding_leaves_the_blackboard() {
    let titles = consolidated_titles(&group_log());
    assert!(
        !titles.iter().any(|title| title == "taken back"),
        "a finding its author withdrew is not what the group leaves behind, got {titles:?}",
    );
}

#[test]
fn an_updated_finding_shows_its_last_content() {
    let titles = consolidated_titles(&group_log());
    assert_eq!(
        titles,
        ["last wording"],
        "the group leaves the content of the latest posting",
    );
}
