//! End-to-end runs with the mock adapter (T4.1 recorte): the bootstrap
//! shape — a prompt plan node that produces the ledger, a loop that
//! implements it task by task, a bash gate — plus re-routes, pauses and
//! resume, all derived from the event log alone.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use yunta_adapters::{Adapter, MockAdapter};
use yunta_core::{Clock, ConfigLayer, RunId, Workflow};
use yunta_engine::{
    build_manifest, create_run, execute_run, NodeState, RunTerminal, DEFAULT_MAX_RETRIES,
};
use yunta_storage::Storage;

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .expect("valid timestamp")
            .with_timezone(&Utc)
    }
}

fn git(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

fn init_repo(dir: &Path) {
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.email", "test@example.com"]);
    git(dir, &["config", "user.name", "Test"]);
    std::fs::write(dir.join(".gitkeep"), "").unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "initial"]);
}

const CONFIG: &str = r#"
runners:
  planner:
    - { adapter: mock, model: mock-model }
  executor:
    - { adapter: mock, model: mock-model }
"#;

struct Bench {
    _root: tempfile::TempDir,
    worktree: std::path::PathBuf,
    runs_root: std::path::PathBuf,
    storage: Storage,
    run_id: RunId,
}

impl Bench {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let worktree = root.path().join("worktree");
        std::fs::create_dir_all(&worktree).unwrap();
        init_repo(&worktree);
        let runs_root = root.path().join("runs");
        let storage = Storage::open(&root.path().join("yunta.db")).unwrap();
        Bench {
            _root: root,
            worktree,
            runs_root,
            storage,
            run_id: RunId::from("run-test-1"),
        }
    }

    /// The absolute run dir this bench's run will use — known before the
    /// run exists, so fixtures can embed absolute artifact paths the way
    /// a real agent would after reading `{{run.dir}}` from its prompt.
    fn run_dir(&self) -> std::path::PathBuf {
        self.runs_root.join(self.run_id.as_str())
    }

    async fn run(
        &self,
        workflow_yaml: &str,
        fixture_yaml: &str,
    ) -> (RunTerminal, yunta_engine::RunState) {
        let workflow: Workflow = serde_yaml::from_str(workflow_yaml).unwrap();
        let config: ConfigLayer = serde_yaml::from_str(CONFIG).unwrap();
        let manifest = build_manifest(&workflow, &config, &self.worktree, &self.worktree).unwrap();

        let run_dir = create_run(
            &self.run_id,
            &manifest,
            &self.runs_root,
            &self.storage,
            &FixedClock,
        )
        .unwrap();

        let adapter = MockAdapter::from_yaml(fixture_yaml).unwrap();
        let mut adapters: HashMap<String, Arc<dyn Adapter>> = HashMap::new();
        adapters.insert("mock".to_string(), Arc::new(adapter));

        let report = execute_run(
            &self.run_id,
            &manifest,
            &run_dir,
            &self.worktree,
            &adapters,
            &self.storage,
            &FixedClock,
            DEFAULT_MAX_RETRIES,
        )
        .await
        .unwrap();
        (report.terminal, report.state)
    }
}

#[tokio::test]
async fn the_bootstrap_shape_runs_end_to_end_plan_loop_and_gate() {
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");

    let workflow = r#"
name: bootstrap
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the ledger to {{run.dir}}/artifacts/plan.yaml."
    artifacts:
      produces:
        - { name: plan.yaml, kind: task-ledger }
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Read your task from the ledger and implement it."
  - id: verify
    kind: bash
    depends_on: [implement]
    run: "test -f hello.txt && test -f world.txt"
"#;

    // Session 1 is the planner: it "writes" the ledger artifact the way a
    // real agent would, at the absolute path its prompt named. Sessions 2
    // and 3 are one executor session per task.
    let fixture = format!(
        r#"
sessions:
  - effects:
      - {{ path: "{artifacts}/plan.yaml", content: "tasks:\n  - id: T001\n    title: \"Create hello\"\n    scope: [\"hello.txt\"]\n    criteria:\n      - cmd: \"test -f hello.txt\"\n  - id: T002\n    title: \"Create world\"\n    scope: [\"world.txt\"]\n    criteria:\n      - cmd: \"test -f world.txt\"\n    depends_on: [T001]\n" }}
    outcome: {{ type: completed, summary: "planned" }}
  - effects:
      - {{ path: hello.txt, content: "hello" }}
    steps:
      - {{ type: usage, input_tokens: 100, output_tokens: 20 }}
    outcome: {{ type: completed, summary: "did T001" }}
  - effects:
      - {{ path: world.txt, content: "world" }}
    steps:
      - {{ type: usage, input_tokens: 80, output_tokens: 10 }}
    outcome: {{ type: completed, summary: "did T002" }}
"#,
        artifacts = artifacts_dir.display()
    );

    let (terminal, state) = bench.run(workflow, &fixture).await;

    assert_eq!(terminal, RunTerminal::Finished);
    for node in ["plan", "implement", "verify"] {
        assert!(
            matches!(
                state.nodes.get(&node.into()),
                Some(NodeState::Finished { .. })
            ),
            "node `{node}` should be finished, got {:?}",
            state.nodes.get(&node.into())
        );
    }
    assert_eq!(
        state.tasks.get(&"T001".into()),
        Some(&yunta_core::events::TaskStatus::Done)
    );
    assert_eq!(
        state.tasks.get(&"T002".into()),
        Some(&yunta_core::events::TaskStatus::Done)
    );
    // Tokens from both executor sessions were attributed to the run.
    assert_eq!(state.total_tokens.input, 180);
    assert_eq!(state.total_tokens.output, 30);
}

#[tokio::test]
async fn a_failing_bash_node_reroutes_to_its_corrective_node_and_returns() {
    let bench = Bench::new();

    // `lint` is red until fixed.txt exists; the corrective prompt node
    // writes it; lint re-runs and goes green (§11.2's lint → fix-lint →
    // lint example, with mock).
    let workflow = r#"
name: lint-recovery
nodes:
  - id: lint
    kind: bash
    run: "test -f fixed.txt"
    on_failure: { goto: fix-lint, max_reroutes: 2 }
  - id: fix-lint
    kind: prompt
    runner: executor
    prompt: "Fix the lint errors."
"#;

    let fixture = r#"
sessions:
  - effects:
      - { path: fixed.txt, content: "fixed" }
    outcome: { type: completed, summary: "fixed it" }
"#;

    let (terminal, state) = bench.run(workflow, fixture).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.get(&"lint".into()),
        Some(NodeState::Finished { .. })
    ));
    assert!(matches!(
        state.nodes.get(&"fix-lint".into()),
        Some(NodeState::Finished { .. })
    ));
}

#[tokio::test]
async fn exhausted_reroutes_pause_the_run_instead_of_looping_forever() {
    let bench = Bench::new();

    // The corrective node never actually fixes anything, so lint fails
    // again after every reroute until the cap pauses the run.
    let workflow = r#"
name: hopeless
nodes:
  - id: lint
    kind: bash
    run: "test -f never-created.txt"
    on_failure: { goto: fix-lint, max_reroutes: 1 }
  - id: fix-lint
    kind: prompt
    runner: executor
    prompt: "Try to fix it."
"#;

    let fixture = r#"
sessions:
  - outcome: { type: completed, summary: "did nothing useful" }
"#;

    let (terminal, _) = bench.run(workflow, fixture).await;

    match terminal {
        RunTerminal::Paused { reason } => {
            assert!(reason.contains("re-route"), "got: {reason}");
        }
        other => panic!("expected Paused, got {other:?}"),
    }
}

#[tokio::test]
async fn a_node_that_fails_without_a_reroute_pauses_the_run_with_its_diagnostic() {
    let bench = Bench::new();

    let workflow = r#"
name: plain-failure
nodes:
  - id: build
    kind: bash
    run: "exit 3"
"#;

    let (terminal, state) = bench.run(workflow, "sessions: []").await;

    match terminal {
        RunTerminal::Paused { reason } => assert!(reason.contains("build"), "got: {reason}"),
        other => panic!("expected Paused, got {other:?}"),
    }
    assert!(matches!(
        state.nodes.get(&"build".into()),
        Some(NodeState::Failed { .. })
    ));
}

#[tokio::test]
async fn an_agent_that_never_writes_its_declared_artifact_fails_the_node() {
    let bench = Bench::new();

    let workflow = r#"
name: broken-plan
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the plan."
    artifacts:
      produces:
        - { name: plan.yaml, kind: task-ledger }
"#;

    // The session claims success but writes nothing — I5: the engine
    // verifies, and the missing artifact fails the node.
    let fixture = r#"
sessions:
  - outcome: { type: completed, summary: "trust me, it is written" }
"#;

    let (terminal, state) = bench.run(workflow, fixture).await;

    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    match state.nodes.get(&"plan".into()) {
        Some(NodeState::Failed { outcome, .. }) => {
            assert!(outcome.contains("plan.yaml"), "got: {outcome}");
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[tokio::test]
async fn executing_a_finished_run_again_is_a_no_op() {
    let bench = Bench::new();

    let workflow = r#"
name: tiny
nodes:
  - id: only
    kind: bash
    run: "true"
"#;

    let (terminal, _) = bench.run(workflow, "sessions: []").await;
    assert_eq!(terminal, RunTerminal::Finished);

    // Second execution: same log, no new adapter, still Finished — and
    // no duplicate node execution (the log would show a second start).
    let workflow: Workflow = serde_yaml::from_str(workflow).unwrap();
    let config: ConfigLayer = serde_yaml::from_str(CONFIG).unwrap();
    let manifest = build_manifest(&workflow, &config, &bench.worktree, &bench.worktree).unwrap();
    let report = execute_run(
        &bench.run_id,
        &manifest,
        &bench.run_dir(),
        &bench.worktree,
        &HashMap::new(),
        &bench.storage,
        &FixedClock,
        DEFAULT_MAX_RETRIES,
    )
    .await
    .unwrap();
    assert_eq!(report.terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let starts = events
        .iter()
        .filter(|e| matches!(e.payload, yunta_core::events::EventPayload::NodeStarted(_)))
        .count();
    assert_eq!(starts, 1, "the finished node must not have re-run");
}

#[tokio::test]
async fn a_bash_node_can_reference_the_run_s_worktree_by_template() {
    let bench = Bench::new();

    let workflow = format!(
        r#"
name: worktree-template
nodes:
  - id: only
    kind: bash
    run: "test -d {{{{run.worktree}}}} && test $(pwd) = '{worktree}'"
"#,
        worktree = bench.worktree.display()
    );

    let (terminal, _) = bench.run(&workflow, "sessions: []").await;
    assert_eq!(terminal, RunTerminal::Finished);
}

#[tokio::test]
async fn a_run_interrupted_mid_node_resumes_by_restarting_the_orphan() {
    let bench = Bench::new();

    let workflow_yaml = r#"
name: resumable
nodes:
  - id: only
    kind: bash
    run: "test -f present.txt"
"#;
    let workflow: Workflow = serde_yaml::from_str(workflow_yaml).unwrap();
    let config: ConfigLayer = serde_yaml::from_str(CONFIG).unwrap();
    let manifest = build_manifest(&workflow, &config, &bench.worktree, &bench.worktree).unwrap();
    let run_dir = create_run(
        &bench.run_id,
        &manifest,
        &bench.runs_root,
        &bench.storage,
        &FixedClock,
    )
    .unwrap();

    // Simulate a crash mid-node: the log has node_started with no
    // terminal event — exactly what a killed engine leaves behind.
    bench
        .storage
        .append_event(&yunta_core::events::Event {
            run_id: bench.run_id.clone(),
            seq: 0,
            timestamp: FixedClock.now(),
            node_id: Some("only".into()),
            payload: yunta_core::events::EventPayload::NodeStarted(
                yunta_core::events::NodeStartedPayload { attempt: 1 },
            ),
        })
        .unwrap();

    std::fs::write(bench.worktree.join("present.txt"), "here").unwrap();

    let report = execute_run(
        &bench.run_id,
        &manifest,
        &run_dir,
        &bench.worktree,
        &HashMap::new(),
        &bench.storage,
        &FixedClock,
        DEFAULT_MAX_RETRIES,
    )
    .await
    .unwrap();

    assert_eq!(report.terminal, RunTerminal::Finished);
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e.payload, yunta_core::events::EventPayload::RunResumed(_))),
        "resume must be recorded in the log"
    );
    // The orphan restarted as attempt 2.
    let last_start = events
        .iter()
        .filter_map(|e| match &e.payload {
            yunta_core::events::EventPayload::NodeStarted(p) => Some(p.attempt),
            _ => None,
        })
        .next_back();
    assert_eq!(last_start, Some(2));
}

#[tokio::test]
async fn a_blocked_task_fails_the_loop_and_pauses_the_run() {
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");

    let workflow = r#"
name: blocked-task
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the ledger to {{run.dir}}/artifacts/plan.yaml."
    artifacts:
      produces:
        - { name: plan.yaml, kind: task-ledger }
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Implement your task."
"#;

    // One task whose criterion the executor never satisfies; with
    // DEFAULT_MAX_RETRIES=2 that's three executor sessions, then blocked.
    let fixture = format!(
        r#"
sessions:
  - effects:
      - {{ path: "{artifacts}/plan.yaml", content: "tasks:\n  - id: T001\n    title: \"Impossible\"\n    scope: [\"missing.txt\"]\n    criteria:\n      - cmd: \"test -f missing.txt\"\n" }}
    outcome: {{ type: completed, summary: "planned" }}
  - outcome: {{ type: completed, summary: "attempt 1" }}
  - outcome: {{ type: completed, summary: "attempt 2" }}
  - outcome: {{ type: completed, summary: "attempt 3" }}
"#,
        artifacts = artifacts_dir.display()
    );

    let (terminal, state) = bench.run(workflow, &fixture).await;

    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    assert_eq!(
        state.tasks.get(&"T001".into()),
        Some(&yunta_core::events::TaskStatus::Blocked)
    );
}
