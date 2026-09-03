//! `coordination: blackboard` end to end: mock sessions acting as REAL MCP clients of the engine's per-run
//! listener — posting findings over the wire mid-session, the group
//! consolidating deterministically at its join, and every mount/
//! capability rule observable as run behavior.

use std::collections::HashMap;
use std::sync::Arc;

use yunta_adapters::{Adapter, MockAdapter};
use yunta_core::events::{EventBody, EventPayload};
use yunta_core::SeqIdSource;
use yunta_core::{AdapterId, ConfigLayer, RunId, Workflow};
use yunta_engine::{
    build_manifest, create_run, execute_run, CreateRunParams, NoInteraction, NodeState, RunEnv,
    RunTerminal, DEFAULT_MAX_RETRIES,
};
use yunta_storage::Storage;
use yunta_testkit::{init_repo, FixedClock};

/// Run ids for everything a test run gives birth to — unique across
/// the binary, so parallel tests never share a run directory.
static IDS: SeqIdSource = SeqIdSource::new("minted");

const CONFIG: &str = r#"
runners:
  executor:
    - { adapter: mock, model: mock-model }
"#;

struct Bench {
    _root: tempfile::TempDir,
    storage: Storage,
    run_id: RunId,
    run_dir: std::path::PathBuf,
    mock: Arc<MockAdapter>,
}

impl Bench {
    async fn run(
        workflow_yaml: &str,
        fixture_yaml: &str,
    ) -> (RunTerminal, yunta_engine::RunState, Self) {
        let root = tempfile::tempdir().unwrap();
        let worktree = root.path().join("worktree");
        std::fs::create_dir_all(&worktree).unwrap();
        init_repo(&worktree);
        let runs_root = root.path().join("runs");
        let storage = Storage::open(&root.path().join("yunta.db")).unwrap();
        let run_id = RunId::from("run-blackboard");

        let workflow: Workflow = serde_yaml::from_str(workflow_yaml).unwrap();
        let config: ConfigLayer = serde_yaml::from_str(CONFIG).unwrap();
        let manifest =
            build_manifest(&workflow, &config, &worktree, &worktree, &HashMap::new()).unwrap();
        let run_dir = create_run(
            CreateRunParams {
                run_id: &run_id,
                manifest: &manifest,
                runs_root: &runs_root,
                mode: &"default".into(),
                promoted_from: None,
                artifacts: &[],
            },
            &storage.async_handle(),
            &FixedClock,
        )
        .await
        .unwrap();

        let mock = Arc::new(MockAdapter::from_yaml(fixture_yaml).unwrap());
        let mut adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
        adapters.insert("mock".into(), mock.clone());

        let report = execute_run(RunEnv {
            run_id: &run_id,
            manifest: &manifest,
            run_dir: &run_dir,
            worktree: &worktree,
            adapters: &adapters,
            storage: &storage.async_handle(),
            clock: std::sync::Arc::new(FixedClock),
            ids: &IDS,
            max_task_retries: DEFAULT_MAX_RETRIES,
            human_interaction: &NoInteraction,
            forge: None,
            cancel: None,
            adapter_override: None,
        })
        .await
        .unwrap();
        (
            report.terminal,
            report.state,
            Bench {
                _root: root,
                storage,
                run_id,
                run_dir,
                mock,
            },
        )
    }

    fn findings_by(&self, node: &str) -> Vec<String> {
        self.storage
            .events_for_run(&self.run_id)
            .unwrap()
            .into_iter()
            .filter(|e| e.node_id.as_ref().map(|n| n.as_str()) == Some(node))
            .filter_map(|e| match e.payload() {
                Some(EventPayload::FindingPosted(p)) => Some(p.finding.id.to_string()),
                _ => None,
            })
            .collect()
    }

    fn group_output(&self, group: &str) -> Option<String> {
        std::fs::read_to_string(
            self.run_dir
                .join("node-output")
                .join(format!("{group}.txt")),
        )
        .ok()
    }
}

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
    let (terminal, state, bench) = Bench::run(BLACKBOARD_WORKFLOW, &blackboard_fixture(true)).await;

    // A failure here must self-diagnose — the terminal's own
    // Paused reason only says "child rev-b failed", while the child's
    // real error (e.g. a run_tool transport failure under load) lives
    // in the node state this message carries.
    assert_eq!(terminal, RunTerminal::Finished, "state: {state:?}");
    assert!(
        matches!(state.nodes.get("review"), Some(NodeState::Finished { .. })),
        "state: {state:?}"
    );
    // Each post is a finding_posted authored by the session's own node,
    // mediated, logged, and attributed.
    assert_eq!(bench.findings_by("rev-a"), vec!["from-a"]);
    assert_eq!(bench.findings_by("rev-b"), vec!["from-b"]);

    // The group's own node-output carries the consolidation —
    // consumable by a node AFTER the parallel, never between siblings.
    let output = bench
        .group_output("review")
        .expect("the blackboard group must consolidate into its node-output");
    assert!(output.contains("from-a"), "got: {output}");
    assert!(output.contains("from-b"), "got: {output}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn consolidation_is_identical_whatever_order_the_posts_arrived_in() {
    let (terminal_1, state_1, bench_1) =
        Bench::run(BLACKBOARD_WORKFLOW, &blackboard_fixture(true)).await;
    let (terminal_2, state_2, bench_2) =
        Bench::run(BLACKBOARD_WORKFLOW, &blackboard_fixture(false)).await;

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
    let output_1 = bench_1
        .group_output("review")
        .expect("run 1 must consolidate into its node-output");
    let output_2 = bench_2
        .group_output("review")
        .expect("run 2 must consolidate into its node-output");
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
    let (terminal, state, _bench) = Bench::run(workflow, fixture).await;
    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    match state.nodes.get("rev-a") {
        Some(NodeState::Failed { outcome, .. }) => {
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
    let (terminal, _, bench) = Bench::run(workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(bench.mock.endpoints_seen(), vec![None]);
    assert!(!bench
        .storage
        .events_for_run(&bench.run_id)
        .unwrap()
        .iter()
        .any(|e| matches!(e.payload(), Some(EventPayload::CapabilityDegraded(_)))));
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
    let (terminal, _, bench) = Bench::run(workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);
    let endpoints = bench.mock.endpoints_seen();
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
    let (terminal, state, _bench) = Bench::run(BLACKBOARD_WORKFLOW, fixture).await;
    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    match state.nodes.get("rev-a") {
        Some(NodeState::Failed { outcome, .. }) => {
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
    use yunta_core::events::{Finding, FindingPostedPayload, FindingSeverity, StoredEvent};
    let finding = |id: &str| Finding {
        id: id.into(),
        severity: FindingSeverity::Minor,
        title: format!("title {id}"),
        location: "src/x.rs".to_string(),
        detail: "detail".to_string(),
        proposed_criterion: None,
    };
    let event = |node: &str, id: &str, seq: u64| StoredEvent {
        run_id: RunId::from("run-x"),
        seq: seq.into(),
        timestamp: chrono::Utc::now(),
        node_id: Some(node.into()),
        body: EventBody::Known(EventPayload::FindingPosted(FindingPostedPayload {
            finding: finding(id),
        })),
    };
    let members = vec!["a".into(), "b".into()];
    let forward = vec![
        event("a", "one", 1),
        event("b", "two", 2),
        event("a", "three", 3),
    ];
    let mut reversed = forward.clone();
    reversed.reverse();

    let consolidated_forward = yunta_engine::consolidate_blackboard(&forward, &members);
    let consolidated_reversed = yunta_engine::consolidate_blackboard(&reversed, &members);
    assert_eq!(consolidated_forward, consolidated_reversed);
    assert!(consolidated_forward.contains("one"));
    assert!(consolidated_forward.contains("three"));
}
