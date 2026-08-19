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
        self.run_with_config(workflow_yaml, fixture_yaml, CONFIG)
            .await
    }

    /// Same as [`Bench::run`] but with a caller-chosen config layer — for
    /// tests that need `baseline:`/`coverage:` alongside the usual
    /// `runners:`.
    async fn run_with_config(
        &self,
        workflow_yaml: &str,
        fixture_yaml: &str,
        config_yaml: &str,
    ) -> (RunTerminal, yunta_engine::RunState) {
        let workflow: Workflow = serde_yaml::from_str(workflow_yaml).unwrap();
        let config: ConfigLayer = serde_yaml::from_str(config_yaml).unwrap();
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
async fn a_failing_before_hook_aborts_the_node_without_opening_a_session() {
    let bench = Bench::new();

    let workflow = r#"
name: before-hook-guard
nodes:
  - id: implement
    kind: prompt
    runner: executor
    hooks:
      before:
        - run: "exit 1"
    prompt: "should never run"
"#;

    // No sessions declared: if the engine opened one despite the failing
    // before-hook, the mock would fail for a different, distinguishable
    // reason than "before hook".
    let (terminal, _) = bench.run(workflow, "sessions: []").await;

    match terminal {
        RunTerminal::Paused { reason } => assert!(reason.contains("before hook"), "got: {reason}"),
        other => panic!("expected Paused, got {other:?}"),
    }
}

#[tokio::test]
async fn an_after_hook_defaults_to_failing_the_node() {
    let bench = Bench::new();

    let workflow = r#"
name: after-hook-fails-by-default
nodes:
  - id: only
    kind: bash
    run: "true"
    hooks:
      after:
        - run: "exit 1"
"#;

    let (terminal, _) = bench.run(workflow, "sessions: []").await;

    match terminal {
        RunTerminal::Paused { reason } => assert!(reason.contains("after hook"), "got: {reason}"),
        other => panic!("expected Paused, got {other:?}"),
    }
}

#[tokio::test]
async fn an_after_hook_with_on_failure_warn_lets_the_node_finish() {
    let bench = Bench::new();

    let workflow = r#"
name: after-hook-warns
nodes:
  - id: only
    kind: bash
    run: "true"
    hooks:
      after:
        - run: "exit 1"
          on_failure: warn
"#;

    let (terminal, _) = bench.run(workflow, "sessions: []").await;
    assert_eq!(terminal, RunTerminal::Finished);
}

#[tokio::test]
async fn a_hook_that_exceeds_its_timeout_fails_the_node_without_waiting_it_out() {
    let bench = Bench::new();

    let workflow = r#"
name: hook-timeout
nodes:
  - id: only
    kind: bash
    run: "true"
    hooks:
      before:
        - run: "sleep 5"
          timeout_seconds: 1
"#;

    let started = std::time::Instant::now();
    let (terminal, _) = bench.run(workflow, "sessions: []").await;
    let elapsed = started.elapsed();

    match terminal {
        RunTerminal::Paused { reason } => assert!(reason.contains("before hook"), "got: {reason}"),
        other => panic!("expected Paused, got {other:?}"),
    }
    assert!(
        elapsed < std::time::Duration::from_secs(3),
        "expected the 1s timeout to cut the 5s sleep short, took {elapsed:?}"
    );
}

#[tokio::test]
async fn node_defaults_hooks_apply_when_a_node_declares_none_of_its_own() {
    let bench = Bench::new();

    let workflow = r#"
name: uses-node-defaults
node_defaults:
  hooks:
    after:
      - run: "touch defaults-ran.txt"
nodes:
  - id: only
    kind: bash
    run: "true"
"#;

    let (terminal, _) = bench.run(workflow, "sessions: []").await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert!(bench.worktree.join("defaults-ran.txt").exists());
}

#[tokio::test]
async fn a_node_s_own_hooks_replace_node_defaults_for_that_phase_instead_of_merging() {
    let bench = Bench::new();

    let workflow = r#"
name: overrides-node-defaults
node_defaults:
  hooks:
    after:
      - run: "touch should-not-run.txt"
nodes:
  - id: only
    kind: bash
    run: "true"
    hooks:
      after:
        - run: "touch overridden.txt"
"#;

    let (terminal, _) = bench.run(workflow, "sessions: []").await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert!(bench.worktree.join("overridden.txt").exists());
    assert!(!bench.worktree.join("should-not-run.txt").exists());
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
async fn a_findings_artifact_emits_finding_posted_events_consumable_by_replay() {
    let bench = Bench::new();

    let workflow = r#"
name: review
nodes:
  - id: review
    kind: prompt
    runner: executor
    prompt: "Review the changes."
    artifacts:
      produces:
        - { name: findings.yaml, kind: findings }
"#;

    let artifacts_dir = bench.run_dir().join("artifacts");
    let fixture = format!(
        r#"
sessions:
  - effects:
      - {{ path: "{artifacts}/findings.yaml", content: "findings:\n  - id: f1\n    severity: major\n    title: \"Unchecked error\"\n    location: \"src/lib.rs:10\"\n    detail: \"The Result is discarded.\"\n" }}
    outcome: {{ type: completed, summary: "reviewed" }}
"#,
        artifacts = artifacts_dir.display()
    );

    let (terminal, state) = bench.run(workflow, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(state.findings.len(), 1);
    assert_eq!(state.findings[0].id, "f1");
}

// --- T5.14: kind: questions (§4.1, D86) ------------------------------------

const QUESTIONS_WORKFLOW: &str = r#"
name: ask
nodes:
  - id: ask
    kind: prompt
    runner: executor
    prompt: "Ask what you need to know before continuing."
    artifacts:
      produces:
        - { name: questions.yaml, kind: questions }
"#;

fn questions_fixture(artifacts_dir: &std::path::Path) -> String {
    format!(
        r#"
sessions:
  - effects:
      - {{ path: "{artifacts}/questions.yaml", content: "questions:\n  - id: q1\n    text: \"Which environment?\"\n    answer_type: choice\n    values: [staging, production]\n    required: true\n  - id: q2\n    text: \"Any notes?\"\n    answer_type: text\n    required: false\n" }}
    outcome: {{ type: completed, summary: "asked" }}
"#,
        artifacts = artifacts_dir.display()
    )
}

#[tokio::test]
async fn a_questions_artifact_pauses_the_run_after_its_own_session_already_closed() {
    // ✓ del Plan: "el nodo que pregunta cierra su sesión antes de que se
    // renderice nada" (la sesión mock corre y cierra normalmente, y solo
    // *después* de eso el engine actúa sobre las preguntas) y "sin TTY el
    // run queda `waiting`, nunca cuelga ni falla" — en este recorte no
    // existe ninguna superficie TTY/MCP/PR (T7.1/T7.2/T8.x), así que ese
    // es el único camino: el run pausa citando las preguntas, no panickea
    // ni queda colgado.
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");
    let fixture = questions_fixture(&artifacts_dir);

    let (terminal, _state) = bench.run(QUESTIONS_WORKFLOW, &fixture).await;

    match &terminal {
        RunTerminal::Paused { reason } => {
            assert!(reason.contains("q1"), "reason must name q1: {reason}");
            assert!(reason.contains("q2"), "reason must name q2: {reason}");
        }
        other => panic!("expected the run to pause on unanswered questions, got {other:?}"),
    }

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(
        events.iter().any(|e| matches!(
            &e.payload,
            yunta_core::events::EventPayload::ArtifactWritten(p)
                if p.path.to_string_lossy().contains("questions.yaml")
        )),
        "the questions artifact must still be recorded as written"
    );
    assert!(
        !events.iter().any(|e| matches!(
            &e.payload,
            yunta_core::events::EventPayload::NodeFinished(_)
        )),
        "a node with unanswered questions must never reach node_finished"
    );
}

#[tokio::test]
async fn resuming_a_run_paused_on_unanswered_questions_replays_the_same_pause_without_a_new_session(
) {
    // ✓ del Plan: "matar el engine durante la espera y reanudar rehace
    // las preguntas sin estado conversacional" — el segundo `execute_run`
    // usa un fixture sin sesiones disponibles; si el resume intentara
    // volver a despachar el nodo, fallaría por "fixture exhausted" en vez
    // de devolver la misma pausa.
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");
    let workflow: yunta_core::Workflow = serde_yaml::from_str(QUESTIONS_WORKFLOW).unwrap();
    let config: yunta_core::ConfigLayer = serde_yaml::from_str(CONFIG).unwrap();
    let manifest = build_manifest(&workflow, &config, &bench.worktree, &bench.worktree).unwrap();
    let run_dir = create_run(
        &bench.run_id,
        &manifest,
        &bench.runs_root,
        &bench.storage,
        &FixedClock,
    )
    .unwrap();

    let first_adapter = MockAdapter::from_yaml(&questions_fixture(&artifacts_dir)).unwrap();
    let mut first_adapters: HashMap<String, Arc<dyn Adapter>> = HashMap::new();
    first_adapters.insert("mock".to_string(), Arc::new(first_adapter));
    let first_report = execute_run(
        &bench.run_id,
        &manifest,
        &run_dir,
        &bench.worktree,
        &first_adapters,
        &bench.storage,
        &FixedClock,
        DEFAULT_MAX_RETRIES,
    )
    .await
    .unwrap();
    match &first_report.terminal {
        RunTerminal::Paused { .. } => {}
        other => panic!("expected the first run to pause, got {other:?}"),
    }

    // No `sessions:` at all — any attempt to dispatch a new session errors.
    let empty_adapter = MockAdapter::from_yaml("sessions: []").unwrap();
    let mut resume_adapters: HashMap<String, Arc<dyn Adapter>> = HashMap::new();
    resume_adapters.insert("mock".to_string(), Arc::new(empty_adapter));
    let resumed_report = execute_run(
        &bench.run_id,
        &manifest,
        &run_dir,
        &bench.worktree,
        &resume_adapters,
        &bench.storage,
        &FixedClock,
        DEFAULT_MAX_RETRIES,
    )
    .await
    .unwrap();

    assert_eq!(
        resumed_report.terminal, first_report.terminal,
        "resume must replay the exact same pause, no new session needed"
    );
}

fn interval(worktree: &std::path::Path, id: &str) -> (i128, i128) {
    let start = std::fs::read_to_string(worktree.join(format!("{id}-start.txt"))).unwrap();
    let end = std::fs::read_to_string(worktree.join(format!("{id}-end.txt"))).unwrap();
    (start.trim().parse().unwrap(), end.trim().parse().unwrap())
}

/// Sweep-line max overlap: ties break end-before-start, so ambiguous
/// simultaneity under-counts rather than over-counts — the right side to
/// err on for a cap-respected assertion.
fn max_concurrent_intervals(intervals: &[(i128, i128)]) -> usize {
    let mut events: Vec<(i128, i32)> = Vec::new();
    for &(start, end) in intervals {
        events.push((start, 1));
        events.push((end, -1));
    }
    events.sort();
    let mut current = 0i32;
    let mut max = 0i32;
    for (_, delta) in events {
        current += delta;
        max = max.max(current);
    }
    max as usize
}

#[tokio::test]
async fn independent_nodes_run_concurrently_up_to_max_parallel_nodes() {
    let bench = Bench::new();

    let workflow_yaml = r#"
name: fan-out
nodes:
  - id: a
    kind: bash
    run: "date +%s%N > a-start.txt; sleep 0.3; date +%s%N > a-end.txt"
  - id: b
    kind: bash
    run: "date +%s%N > b-start.txt; sleep 0.3; date +%s%N > b-end.txt"
  - id: c
    kind: bash
    run: "date +%s%N > c-start.txt; sleep 0.3; date +%s%N > c-end.txt"
"#;
    let workflow: Workflow = serde_yaml::from_str(workflow_yaml).unwrap();
    let config: ConfigLayer = serde_yaml::from_str("defaults:\n  max_parallel_nodes: 2\n").unwrap();
    let manifest = build_manifest(&workflow, &config, &bench.worktree, &bench.worktree).unwrap();
    let run_dir = create_run(
        &bench.run_id,
        &manifest,
        &bench.runs_root,
        &bench.storage,
        &FixedClock,
    )
    .unwrap();

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

    let intervals = ["a", "b", "c"].map(|id| interval(&bench.worktree, id));
    assert_eq!(
        max_concurrent_intervals(&intervals),
        2,
        "expected exactly max_parallel_nodes (2) nodes to overlap at once, batch then batch"
    );
}

#[tokio::test]
async fn max_parallel_nodes_defaults_to_1_and_stays_fully_sequential() {
    let bench = Bench::new();

    let workflow_yaml = r#"
name: fan-out
nodes:
  - id: a
    kind: bash
    run: "date +%s%N > a-start.txt; sleep 0.1; date +%s%N > a-end.txt"
  - id: b
    kind: bash
    run: "date +%s%N > b-start.txt; sleep 0.1; date +%s%N > b-end.txt"
"#;
    let workflow: Workflow = serde_yaml::from_str(workflow_yaml).unwrap();
    let config = ConfigLayer::default();
    let manifest = build_manifest(&workflow, &config, &bench.worktree, &bench.worktree).unwrap();
    let run_dir = create_run(
        &bench.run_id,
        &manifest,
        &bench.runs_root,
        &bench.storage,
        &FixedClock,
    )
    .unwrap();

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

    let intervals = ["a", "b"].map(|id| interval(&bench.worktree, id));
    assert_eq!(
        max_concurrent_intervals(&intervals),
        1,
        "unset max_parallel_nodes must stay fully sequential (default 1)"
    );
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
async fn a_node_with_on_interrupt_fail_if_uncertain_pauses_instead_of_restarting() {
    let bench = Bench::new();

    let workflow_yaml = r#"
name: uncertain-on-crash
nodes:
  - id: only
    kind: bash
    run: "test -f present.txt"
    on_interrupt: fail_if_uncertain
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

    // Same simulated crash as the restart_node test: node_started with no
    // terminal event.
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

    match report.terminal {
        RunTerminal::Paused { reason } => assert!(reason.contains("only"), "got: {reason}"),
        other => panic!("expected Paused, got {other:?}"),
    }
    // Never restarted: no second node_started attempt was ever emitted.
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let starts = events
        .iter()
        .filter(|e| matches!(e.payload, yunta_core::events::EventPayload::NodeStarted(_)))
        .count();
    assert_eq!(starts, 1, "fail_if_uncertain must never blindly restart");
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

#[tokio::test]
async fn a_parallel_group_with_join_all_finishes_when_every_child_finishes() {
    let bench = Bench::new();

    let workflow = r#"
name: pre-launch
nodes:
  - id: pre-launch
    kind: parallel
    join: all
    nodes:
      - id: write-docs
        kind: bash
        run: "touch docs.txt"
      - id: load-test
        kind: bash
        run: "touch load.txt"
"#;

    let (terminal, state) = bench.run(workflow, "sessions: []").await;
    assert_eq!(terminal, RunTerminal::Finished);
    for id in ["pre-launch", "write-docs", "load-test"] {
        assert!(
            matches!(
                state.nodes.get(&id.into()),
                Some(NodeState::Finished { .. })
            ),
            "expected `{id}` finished, got {:?}",
            state.nodes.get(&id.into())
        );
    }
    assert!(bench.worktree.join("docs.txt").exists());
    assert!(bench.worktree.join("load.txt").exists());
}

#[tokio::test]
async fn a_parallel_group_with_join_all_fails_if_any_child_fails() {
    let bench = Bench::new();

    let workflow = r#"
name: pre-launch
nodes:
  - id: pre-launch
    kind: parallel
    join: all
    nodes:
      - id: write-docs
        kind: bash
        run: "touch docs.txt"
      - id: load-test
        kind: bash
        run: "exit 1"
"#;

    let (terminal, state) = bench.run(workflow, "sessions: []").await;
    match terminal {
        RunTerminal::Paused { reason } => assert!(reason.contains("load-test"), "got: {reason}"),
        other => panic!("expected Paused, got {other:?}"),
    }
    assert!(matches!(
        state.nodes.get(&"load-test".into()),
        Some(NodeState::Failed { .. })
    ));
}

#[tokio::test]
async fn a_parallel_group_with_join_any_completes_with_the_first_success_and_interrupts_the_rest() {
    let bench = Bench::new();

    let workflow = r#"
name: race
nodes:
  - id: race
    kind: parallel
    join: any
    nodes:
      - id: fast
        kind: bash
        run: "true"
      - id: slow
        kind: bash
        run: "sleep 5 && touch slow-finished-fully.txt"
"#;

    let started = std::time::Instant::now();
    let (terminal, state) = bench.run(workflow, "sessions: []").await;
    let elapsed = started.elapsed();

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(
        elapsed < std::time::Duration::from_secs(3),
        "expected join: any to return as soon as `fast` won, took {elapsed:?}"
    );
    assert!(matches!(
        state.nodes.get(&"fast".into()),
        Some(NodeState::Finished { .. })
    ));
    // The slow sibling was interrupted before its own `touch` ran — proof
    // the process was actually cut short, not just outraced by chance.
    assert!(!bench.worktree.join("slow-finished-fully.txt").exists());
}

#[tokio::test]
async fn resuming_a_crashed_parallel_group_never_re_runs_a_child_that_already_finished() {
    let bench = Bench::new();

    let workflow_yaml = r#"
name: pre-launch
nodes:
  - id: pre-launch
    kind: parallel
    join: all
    nodes:
      - id: write-docs
        kind: bash
        run: "touch docs.txt"
      - id: load-test
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

    // Simulate a crash mid-group: the parallel node and one child
    // (write-docs) finished; the other child (load-test) never started.
    for event in [
        yunta_core::events::Event {
            run_id: bench.run_id.clone(),
            seq: 0,
            timestamp: FixedClock.now(),
            node_id: Some("pre-launch".into()),
            payload: yunta_core::events::EventPayload::NodeStarted(
                yunta_core::events::NodeStartedPayload { attempt: 1 },
            ),
        },
        yunta_core::events::Event {
            run_id: bench.run_id.clone(),
            seq: 0,
            timestamp: FixedClock.now(),
            node_id: Some("write-docs".into()),
            payload: yunta_core::events::EventPayload::NodeStarted(
                yunta_core::events::NodeStartedPayload { attempt: 1 },
            ),
        },
        yunta_core::events::Event {
            run_id: bench.run_id.clone(),
            seq: 0,
            timestamp: FixedClock.now(),
            node_id: Some("write-docs".into()),
            payload: yunta_core::events::EventPayload::NodeFinished(
                yunta_core::events::NodeFinishedPayload {
                    outcome: "exit 0".to_string(),
                    tokens_used: Default::default(),
                },
            ),
        },
    ] {
        bench.storage.append_event(&event).unwrap();
    }
    // If write-docs re-ran, it would overwrite this — instead assert it
    // survives untouched, since a second `touch` would only prove nothing.
    std::fs::write(bench.worktree.join("docs.txt"), "original").unwrap();
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
    let write_docs_starts = events
        .iter()
        .filter(|e| {
            e.node_id.as_ref().map(|id| id.as_str()) == Some("write-docs")
                && matches!(e.payload, yunta_core::events::EventPayload::NodeStarted(_))
        })
        .count();
    assert_eq!(
        write_docs_starts, 1,
        "an already-finished child must not restart on resume"
    );
}

const CONFIG_WITH_BASELINE: &str = r#"
runners:
  planner:
    - { adapter: mock, model: mock-model }
  executor:
    - { adapter: mock, model: mock-model }
baseline:
  suite: "cat marker.txt"
"#;

const CONFIG_WITH_COVERAGE: &str = r#"
runners:
  planner:
    - { adapter: mock, model: mock-model }
  executor:
    - { adapter: mock, model: mock-model }
coverage:
  cmd: "cat coverage.txt"
  threshold: 80.0
"#;

#[tokio::test]
async fn baseline_compare_passes_on_its_first_run_with_nothing_to_compare_against() {
    let bench = Bench::new();
    std::fs::write(bench.worktree.join("marker.txt"), "ok").unwrap();

    let workflow = r#"
name: baseline-first-run
nodes:
  - id: no-regressions
    kind: check
    builtin: baseline_compare
"#;

    let (terminal, _) = bench
        .run_with_config(workflow, "sessions: []", CONFIG_WITH_BASELINE)
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
}

#[tokio::test]
async fn baseline_compare_fails_when_a_previously_green_suite_turns_red() {
    let bench = Bench::new();
    // `cat marker.txt` exits 0 while the file exists — the first
    // `baseline_compare` node below captures that as the baseline.
    std::fs::write(bench.worktree.join("marker.txt"), "ok").unwrap();

    let workflow = r#"
name: baseline-regression
nodes:
  - id: capture
    kind: check
    builtin: baseline_compare
  - id: regress
    kind: bash
    run: "rm marker.txt"
    depends_on: [capture]
  - id: compare
    kind: check
    builtin: baseline_compare
    depends_on: [regress]
"#;

    let (terminal, _) = bench
        .run_with_config(workflow, "sessions: []", CONFIG_WITH_BASELINE)
        .await;
    match terminal {
        RunTerminal::Paused { reason } => assert!(
            reason.contains("regression"),
            "expected a regression diagnostic, got: {reason}"
        ),
        other => panic!("expected the second baseline_compare to pause the run, got {other:?}"),
    }
}

#[tokio::test]
async fn coverage_gate_passes_when_measured_coverage_meets_the_threshold() {
    let bench = Bench::new();
    std::fs::write(bench.worktree.join("coverage.txt"), "lines: 92.5%\n").unwrap();

    let workflow = r#"
name: coverage-ok
nodes:
  - id: coverage
    kind: check
    builtin: coverage_gate
"#;

    let (terminal, _) = bench
        .run_with_config(workflow, "sessions: []", CONFIG_WITH_COVERAGE)
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
}

#[tokio::test]
async fn coverage_gate_fails_when_measured_coverage_is_below_the_threshold() {
    let bench = Bench::new();
    std::fs::write(bench.worktree.join("coverage.txt"), "lines: 40.0%\n").unwrap();

    let workflow = r#"
name: coverage-low
nodes:
  - id: coverage
    kind: check
    builtin: coverage_gate
"#;

    let (terminal, _) = bench
        .run_with_config(workflow, "sessions: []", CONFIG_WITH_COVERAGE)
        .await;
    match terminal {
        RunTerminal::Paused { reason } => {
            assert!(reason.contains("below"), "unexpected reason: {reason}");
            assert!(reason.contains("40"), "unexpected reason: {reason}");
        }
        other => panic!("expected the coverage gate to pause the run, got {other:?}"),
    }
}

#[tokio::test]
async fn findings_gate_fails_when_a_posted_finding_meets_max_severity() {
    let bench = Bench::new();

    let workflow = r#"
name: findings-gate
nodes:
  - id: review
    kind: prompt
    runner: executor
    prompt: "Review the changes."
    artifacts:
      produces:
        - { name: findings.yaml, kind: findings }
  - id: gate
    kind: check
    builtin: findings_gate
    max_severity: major
    depends_on: [review]
"#;

    let artifacts_dir = bench.run_dir().join("artifacts");
    let fixture = format!(
        r#"
sessions:
  - effects:
      - {{ path: "{artifacts}/findings.yaml", content: "findings:\n  - id: f1\n    severity: blocking\n    title: \"Unchecked error\"\n    location: \"src/lib.rs:10\"\n    detail: \"The Result is discarded.\"\n" }}
    outcome: {{ type: completed, summary: "reviewed" }}
"#,
        artifacts = artifacts_dir.display()
    );

    let (terminal, _) = bench.run(workflow, &fixture).await;
    match terminal {
        RunTerminal::Paused { reason } => assert!(reason.contains("f1")),
        other => panic!("expected the gate to pause the run, got {other:?}"),
    }
}

#[tokio::test]
async fn findings_gate_passes_when_no_finding_meets_max_severity() {
    let bench = Bench::new();

    let workflow = r#"
name: findings-gate-clean
nodes:
  - id: review
    kind: prompt
    runner: executor
    prompt: "Review the changes."
    artifacts:
      produces:
        - { name: findings.yaml, kind: findings }
  - id: gate
    kind: check
    builtin: findings_gate
    max_severity: blocking
    depends_on: [review]
"#;

    let artifacts_dir = bench.run_dir().join("artifacts");
    let fixture = format!(
        r#"
sessions:
  - effects:
      - {{ path: "{artifacts}/findings.yaml", content: "findings:\n  - id: f1\n    severity: minor\n    title: \"Style nit\"\n    location: \"src/lib.rs:10\"\n    detail: \"Naming.\"\n" }}
    outcome: {{ type: completed, summary: "reviewed" }}
"#,
        artifacts = artifacts_dir.display()
    );

    let (terminal, _) = bench.run(workflow, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);
}

#[tokio::test]
async fn progress_md_is_regenerated_at_run_dir_after_each_node_finished() {
    let bench = Bench::new();

    let workflow = r#"
name: two-nodes
nodes:
  - id: write
    kind: bash
    run: "touch out.txt"
    description: "Writes the output file"
  - id: verify
    kind: bash
    run: "test -f out.txt"
    depends_on: [write]
"#;

    let (terminal, _) = bench.run(workflow, "sessions: []").await;
    assert_eq!(terminal, RunTerminal::Finished);

    let progress = std::fs::read_to_string(bench.run_dir().join("progress.md")).unwrap();
    assert!(progress.contains("- **write** — Writes the output file"));
    assert!(progress.contains("- **verify** — verify"));
    assert!(progress.contains("_none_"), "nothing should have failed");
}

#[tokio::test]
async fn progress_md_lists_a_node_s_artifacts_after_it_finishes() {
    let bench = Bench::new();

    let workflow = r#"
name: findings-progress
nodes:
  - id: review
    kind: prompt
    runner: executor
    prompt: "Review the changes."
    description: "Reviews the diff for issues"
    artifacts:
      produces:
        - { name: findings.yaml, kind: findings }
"#;

    let artifacts_dir = bench.run_dir().join("artifacts");
    let fixture = format!(
        r#"
sessions:
  - effects:
      - {{ path: "{artifacts}/findings.yaml", content: "findings: []\n" }}
    outcome: {{ type: completed, summary: "reviewed" }}
"#,
        artifacts = artifacts_dir.display()
    );

    let (terminal, _) = bench.run(workflow, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let progress = std::fs::read_to_string(bench.run_dir().join("progress.md")).unwrap();
    assert!(progress.contains("- **review** — Reviews the diff for issues"));
    assert!(progress.contains("artifact:"));
    assert!(progress.contains("findings.yaml"));
}

const CONFIG_WITH_EXECUTOR: &str = r#"
runners:
  planner:
    - { adapter: mock, model: mock-model }
  executor:
    - { adapter: mock, model: mock-model }
skills:
  executors:
    - { name: probe, kind: binary, path: probe.py }
"#;

fn write_executable_script(path: &std::path::Path, contents: &str) {
    std::fs::write(path, contents).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).unwrap();
    }
}

#[tokio::test]
async fn an_executor_node_completes_the_full_cycle_with_a_dependency_free_python_script() {
    let bench = Bench::new();
    write_executable_script(
        &bench.worktree.join("probe.py"),
        r#"#!/usr/bin/env python3
import json, sys
data = json.load(sys.stdin)
threshold = data["with"]["threshold"]
run_dir = data["run"]["dir"]
assert run_dir, "run.dir must be present in stdin"
print(json.dumps({"summary": f"threshold was {threshold}"}))
sys.exit(0)
"#,
    );

    let workflow = r#"
name: executor-happy-path
nodes:
  - id: probe
    kind: executor
    executor: probe
    with:
      threshold: 80
"#;

    let (terminal, state) = bench
        .run_with_config(workflow, "sessions: []", CONFIG_WITH_EXECUTOR)
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
    match state.nodes.get(&"probe".into()) {
        Some(NodeState::Finished { outcome, .. }) => {
            assert_eq!(outcome, "threshold was 80");
        }
        other => panic!("expected probe to finish, got {other:?}"),
    }
}

#[tokio::test]
async fn an_executor_node_that_exits_non_zero_fails_the_node_with_its_stderr() {
    let bench = Bench::new();
    write_executable_script(
        &bench.worktree.join("probe.py"),
        r#"#!/usr/bin/env python3
import sys
sys.stderr.write("threshold not met\n")
sys.exit(1)
"#,
    );

    let workflow = r#"
name: executor-failure
nodes:
  - id: probe
    kind: executor
    executor: probe
"#;

    let (terminal, _) = bench
        .run_with_config(workflow, "sessions: []", CONFIG_WITH_EXECUTOR)
        .await;
    match terminal {
        RunTerminal::Paused { reason } => {
            assert!(reason.contains("exited 1"), "unexpected reason: {reason}");
            assert!(
                reason.contains("threshold not met"),
                "unexpected reason: {reason}"
            );
        }
        other => panic!("expected the run to pause, got {other:?}"),
    }
}

#[tokio::test]
async fn an_executor_node_that_exceeds_its_timeout_fails_with_a_diagnostic() {
    let bench = Bench::new();
    write_executable_script(
        &bench.worktree.join("probe.py"),
        r#"#!/usr/bin/env python3
import time
time.sleep(5)
"#,
    );

    let workflow = r#"
name: executor-timeout
nodes:
  - id: probe
    kind: executor
    executor: probe
    timeout_seconds: 1
"#;

    let (terminal, _) = bench
        .run_with_config(workflow, "sessions: []", CONFIG_WITH_EXECUTOR)
        .await;
    match terminal {
        RunTerminal::Paused { reason } => {
            assert!(reason.contains("timeout"), "unexpected reason: {reason}");
        }
        other => panic!("expected the run to pause, got {other:?}"),
    }
}

#[tokio::test]
async fn an_executor_node_referencing_an_unregistered_name_fails_with_a_diagnostic() {
    let bench = Bench::new();

    let workflow = r#"
name: executor-unregistered
nodes:
  - id: probe
    kind: executor
    executor: does-not-exist
"#;

    let (terminal, _) = bench.run(workflow, "sessions: []").await;
    match terminal {
        RunTerminal::Paused { reason } => {
            assert!(
                reason.contains("skills.executors"),
                "unexpected reason: {reason}"
            );
        }
        other => panic!("expected the run to pause, got {other:?}"),
    }
}

const CONFIG_WITH_DENY: &str = r#"
runners:
  planner:
    - { adapter: mock, model: mock-model }
  executor:
    - { adapter: mock, model: mock-model }
permissions:
  commands:
    deny: ["*forbidden-marker*"]
"#;

#[tokio::test]
async fn a_template_built_command_that_violates_at_runtime_fails_the_node_citing_the_rule() {
    // T5.7 ✓2: the YAML text alone never matches the denied pattern — the
    // violation only exists after {{run.worktree}} renders. The static
    // scan can't see it; the runtime moment must.
    let bench = Bench::new();
    let marked = bench.worktree.join("forbidden-marker");
    std::fs::create_dir_all(&marked).unwrap();

    let workflow = r#"
name: runtime-violation
nodes:
  - id: sneaky
    kind: bash
    run: "ls {{run.worktree}}/forbidden-marker"
"#;

    let (terminal, _) = bench
        .run_with_config(workflow, "sessions: []", CONFIG_WITH_DENY)
        .await;
    match terminal {
        RunTerminal::Paused { reason } => {
            assert!(
                reason.contains("forbidden-marker") && reason.contains("denied"),
                "must cite the rule: {reason}"
            );
        }
        other => panic!("expected the run to pause on the violation, got {other:?}"),
    }
}

#[tokio::test]
async fn a_denied_hook_command_fails_the_node_even_with_on_failure_warn() {
    // Governance is not a hook outcome: `on_failure: warn` downgrades a
    // hook's own failure, never a permission violation — otherwise any
    // hook could opt out of the model (§6.1).
    let bench = Bench::new();

    let workflow = r#"
name: hook-violation
nodes:
  - id: build
    kind: bash
    run: "true"
    hooks:
      before:
        - run: "echo forbidden-marker"
          on_failure: warn
"#;

    let (terminal, _) = bench
        .run_with_config(workflow, "sessions: []", CONFIG_WITH_DENY)
        .await;
    match terminal {
        RunTerminal::Paused { reason } => {
            assert!(reason.contains("denied"), "must cite the rule: {reason}");
        }
        other => panic!("expected the run to pause, got {other:?}"),
    }
}

#[tokio::test]
async fn a_denied_task_criterion_blocks_the_task_citing_the_rule() {
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");

    let workflow = r#"
name: criterion-violation
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the ledger."
    artifacts:
      produces:
        - { name: plan.yaml, kind: task-ledger }
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Do the task."
"#;

    let fixture = format!(
        r#"
sessions:
  - effects:
      - {{ path: "{artifacts}/plan.yaml", content: "tasks:\n  - id: T001\n    title: \"Task\"\n    scope: [\"out.txt\"]\n    criteria:\n      - cmd: \"test -f forbidden-marker\"\n" }}
    outcome: {{ type: completed, summary: "planned" }}
"#,
        artifacts = artifacts_dir.display()
    );

    let (terminal, _) = bench
        .run_with_config(workflow, &fixture, CONFIG_WITH_DENY)
        .await;
    match terminal {
        RunTerminal::Paused { reason } => {
            assert!(
                reason.contains("denied"),
                "the blocked task must cite the rule: {reason}"
            );
        }
        other => panic!("expected the run to pause, got {other:?}"),
    }
}

#[tokio::test]
async fn a_node_with_network_false_is_never_blocked_by_the_engine() {
    // T5.7 ✓3 — a test that documents the limit, not a bug (D105):
    // `network: false` is declarative; the engine runs the command anyway.
    let bench = Bench::new();

    let workflow = r#"
name: network-declarative
nodes:
  - id: declared-offline
    kind: bash
    run: "echo simulating-a-network-call"
    network: false
"#;

    let (terminal, _) = bench.run(workflow, "sessions: []").await;
    assert_eq!(
        terminal,
        RunTerminal::Finished,
        "network: false activates no sandbox — policy, not capability"
    );
}

#[tokio::test]
async fn a_denied_executor_path_fails_the_node_citing_the_rule() {
    let bench = Bench::new();
    write_executable_script(
        &bench.worktree.join("probe.py"),
        "#!/usr/bin/env python3\nprint('{}')\n",
    );

    let workflow = r#"
name: executor-denied
nodes:
  - id: probe
    kind: executor
    executor: probe
"#;

    let config = r#"
runners:
  executor:
    - { adapter: mock, model: mock-model }
skills:
  executors:
    - { name: probe, kind: binary, path: probe.py }
permissions:
  commands:
    deny: ["*probe.py"]
"#;

    let (terminal, _) = bench
        .run_with_config(workflow, "sessions: []", config)
        .await;
    match terminal {
        RunTerminal::Paused { reason } => {
            assert!(reason.contains("denied"), "must cite the rule: {reason}");
        }
        other => panic!("expected the run to pause, got {other:?}"),
    }
}

#[tokio::test]
async fn events_jsonl_is_written_at_run_dir_when_the_run_finishes() {
    let bench = Bench::new();

    let workflow = r#"
name: single-node
nodes:
  - id: only
    kind: bash
    run: "true"
"#;

    let (terminal, state) = bench.run(workflow, "sessions: []").await;
    assert_eq!(terminal, RunTerminal::Finished);

    let jsonl = std::fs::read_to_string(bench.run_dir().join("events.jsonl")).unwrap();
    let round_tripped: Vec<yunta_core::events::Event> = jsonl
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(yunta_engine::derive(&round_tripped), state);
    assert!(
        jsonl.contains("\"kind\":\"run_finished\""),
        "the closing event itself must be included in the export"
    );
}

#[tokio::test]
async fn events_jsonl_is_also_written_when_the_run_pauses() {
    let bench = Bench::new();

    let workflow = r#"
name: no-runner
nodes:
  - id: plan
    kind: prompt
    prompt: "plan it"
"#;

    let (terminal, _) = bench.run(workflow, "sessions: []").await;
    match terminal {
        RunTerminal::Paused { .. } => {}
        other => panic!("expected the run to pause, got {other:?}"),
    }

    let jsonl = std::fs::read_to_string(bench.run_dir().join("events.jsonl")).unwrap();
    assert!(
        jsonl.contains("\"kind\":\"run_paused\""),
        "a paused run's export must include the pause itself"
    );
}

// --- T5.10: concurrency: N in loop nodes (§5.5, D65) -----------------------

const CONCURRENCY_CONFIG: &str = r#"
runners:
  planner:
    - { adapter: mock, model: mock-model }
  executor:
    - { adapter: mock, model: mock-model }
"#;

/// An 8-independent-task ledger: no `depends_on` between any of them, each
/// with its own disjoint scope (`out-N.txt`) so `ledger::register` (T5.1)
/// accepts it as a legal batch of fully parallelizable work.
fn task_yaml(id: &str, title: &str, scope: &str, criterion: &str) -> String {
    format!(
        "  - id: {id}\n    title: \"{title}\"\n    scope: [\"{scope}\"]\n    criteria:\n      - cmd: \"{criterion}\"\n"
    )
}

fn eight_independent_tasks_ledger() -> String {
    let mut yaml = String::from("tasks:\n");
    for n in 1..=8 {
        yaml.push_str(&format!(
            "  - id: task-{n}\n    title: \"Write out-{n}\"\n    scope: [\"out-{n}.txt\"]\n    criteria:\n      - cmd: \"test -f out-{n}.txt\"\n"
        ));
    }
    yaml
}

fn concurrency_workflow(concurrency: u32) -> String {
    format!(
        r#"
name: eight-tasks
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the ledger to {{{{run.dir}}}}/artifacts/plan.yaml."
    artifacts:
      produces:
        - {{ name: plan.yaml, kind: task-ledger }}
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    concurrency: {concurrency}
    prompt: "Read your task from the ledger and implement it."
"#
    )
}

/// One mock session per task, matched by its own id (never by call order —
/// concurrent dispatch races several `spawn()` calls at once) plus the
/// planner's own session first.
fn eight_tasks_fixture(artifacts_dir: &std::path::Path) -> String {
    let mut yaml = format!(
        "sessions:\n  - effects:\n      - {{ path: \"{}/plan.yaml\", content: {:?} }}\n    outcome: {{ type: completed, summary: planned }}\n",
        artifacts_dir.display(),
        eight_independent_tasks_ledger(),
    );
    for n in 1..=8 {
        yaml.push_str(&format!(
            "  - match_prompt_contains: \"task-{n}\"\n    effects:\n      - {{ path: out-{n}.txt, content: \"{n}\" }}\n    outcome: {{ type: completed, summary: \"did task-{n}\" }}\n"
        ));
    }
    yaml
}

/// Commit subjects on `worktree`'s current branch, oldest first, excluding
/// the `init_repo` seed commit.
fn commit_subjects(worktree: &std::path::Path) -> Vec<String> {
    let output = std::process::Command::new("git")
        .args(["log", "--format=%s", "--reverse"])
        .current_dir(worktree)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| *line != "initial")
        .map(str::to_string)
        .collect()
}

#[tokio::test]
async fn eight_independent_tasks_at_concurrency_4_match_concurrency_1_state_and_commits() {
    // T5.10 ✓: same final state, same commit sequence, regardless of
    // concurrency — the batch mechanism integrates strictly in ledger
    // declaration order no matter how many tasks dispatch at once.
    let sequential = Bench::new();
    let workflow_seq = concurrency_workflow(1);
    let fixture_seq = eight_tasks_fixture(&sequential.run_dir().join("artifacts"));
    let (terminal_seq, state_seq) = sequential
        .run_with_config(&workflow_seq, &fixture_seq, CONCURRENCY_CONFIG)
        .await;
    assert_eq!(terminal_seq, RunTerminal::Finished);

    let parallel = Bench::new();
    let workflow_par = concurrency_workflow(4);
    let fixture_par = eight_tasks_fixture(&parallel.run_dir().join("artifacts"));
    let (terminal_par, state_par) = parallel
        .run_with_config(&workflow_par, &fixture_par, CONCURRENCY_CONFIG)
        .await;
    assert_eq!(terminal_par, RunTerminal::Finished);

    for n in 1..=8 {
        let id: yunta_core::TaskId = format!("task-{n}").into();
        assert_eq!(
            state_seq.tasks.get(&id),
            Some(&yunta_core::events::TaskStatus::Done)
        );
        assert_eq!(
            state_par.tasks.get(&id),
            state_seq.tasks.get(&id),
            "task-{n} status must match between concurrency levels"
        );
    }

    let commits_seq = commit_subjects(&sequential.worktree);
    let commits_par = commit_subjects(&parallel.worktree);
    assert_eq!(
        commits_seq.len(),
        8,
        "expected one commit per task, got {commits_seq:?}"
    );
    assert_eq!(
        commits_seq, commits_par,
        "the same ledger must produce the same commit sequence at any concurrency"
    );
    // Declaration order, not finishing order.
    let expected: Vec<String> = (1..=8)
        .map(|n| format!("task task-{n}: Write out-{n}"))
        .collect();
    assert_eq!(commits_seq, expected);
}

#[tokio::test]
async fn a_task_green_in_isolation_but_broken_by_a_sibling_s_integration_returns_to_ready() {
    // Task A always integrates cleanly. Task B's own criterion is
    // satisfied in isolation (its own worktree predates A's integration)
    // but is re-checked false once A's file exists on the tree B rebases
    // onto — exactly "pasa en su worktree pero rompe tras la integración
    // de otra". B must go back to `ready` without touching A.
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");

    let workflow = r#"
name: integration-conflict
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
    concurrency: 2
    prompt: "Read your task from the ledger and implement it."
"#;

    let ledger = format!(
        "tasks:\n{}{}",
        task_yaml("task-a", "Create a", "a.txt", "test -f a.txt"),
        task_yaml(
            "task-b",
            "Create b, require no a",
            "b.txt",
            "test -f b.txt && test ! -f a.txt"
        ),
    );

    let mut fixture = format!(
        "sessions:\n  - effects:\n      - {{ path: \"{}/plan.yaml\", content: {:?} }}\n    outcome: {{ type: completed, summary: planned }}\n",
        artifacts_dir.display(),
        ledger,
    );
    fixture.push_str(
        "  - match_prompt_contains: \"task-a\"\n    effects:\n      - { path: a.txt, content: \"a\" }\n    outcome: { type: completed, summary: did-a }\n",
    );
    // Several task-b sessions: the first attempt succeeds in isolation and
    // is rejected at integration (back to ready); the retried attempt(s)
    // are now genuinely red (a.txt is already on the integrated tree) and
    // exhaust run_task's own retries into a real Blocked.
    for _ in 0..(DEFAULT_MAX_RETRIES + 2) {
        fixture.push_str(
            "  - match_prompt_contains: \"task-b\"\n    effects:\n      - { path: b.txt, content: \"b\" }\n    outcome: { type: completed, summary: did-b }\n",
        );
    }

    let (terminal, state) = bench
        .run_with_config(workflow, &fixture, CONCURRENCY_CONFIG)
        .await;

    // task-a must have succeeded and stayed succeeded, unaffected by
    // task-b's fate.
    assert_eq!(
        state.nodes.get(&"implement".into()),
        state.nodes.get(&"implement".into()),
    );
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let a_statuses: Vec<_> = events
        .iter()
        .filter_map(|e| match &e.payload {
            yunta_core::events::EventPayload::TaskStatusChanged(p)
                if p.task_id.as_str() == "task-a" =>
            {
                Some(p.new_status)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        a_statuses,
        vec![
            yunta_core::events::TaskStatus::Running,
            yunta_core::events::TaskStatus::Done,
        ],
        "task-a must reach Done exactly once and never regress"
    );

    let b_statuses: Vec<_> = events
        .iter()
        .filter_map(|e| match &e.payload {
            yunta_core::events::EventPayload::TaskStatusChanged(p)
                if p.task_id.as_str() == "task-b" =>
            {
                Some(p.new_status)
            }
            _ => None,
        })
        .collect();
    assert!(
        b_statuses.contains(&yunta_core::events::TaskStatus::Pending),
        "task-b's rejected integration must return it to Pending (ready), not Blocked: {b_statuses:?}"
    );
    // Confirms the ordering claimed above: Pending shows up strictly after
    // task-b's first Running, i.e. it really was reverted mid-flight.
    let first_running = b_statuses
        .iter()
        .position(|s| *s == yunta_core::events::TaskStatus::Running)
        .unwrap();
    let reverted = b_statuses
        .iter()
        .position(|s| *s == yunta_core::events::TaskStatus::Pending)
        .unwrap();
    assert!(reverted > first_running);

    // The run eventually gives up on task-b (it can never satisfy "no
    // a.txt" once a.txt is permanently integrated) — that's expected, not
    // a test bug: the point here is task-a's own success was untouched.
    match terminal {
        RunTerminal::Paused { reason } => assert!(reason.contains("task-b")),
        other => panic!("expected the run to eventually pause on task-b, got {other:?}"),
    }
    let _ = state;
}

#[tokio::test]
async fn a_task_s_scope_is_checked_against_its_own_diff_never_a_sibling_s() {
    // Two independent tasks dispatched in the same batch; task-x's own
    // declared scope never mentions task-y's file. If scope were checked
    // against anything but task-x's own isolated diff, task-y's write
    // would spuriously violate it.
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");

    let workflow = concurrency_workflow(2);
    let ledger = format!(
        "tasks:\n{}{}",
        task_yaml("task-x", "x", "x.txt", "test -f x.txt"),
        task_yaml("task-y", "y", "y.txt", "test -f y.txt"),
    );
    let fixture = format!(
        "sessions:\n  - effects:\n      - {{ path: \"{}/plan.yaml\", content: {:?} }}\n    outcome: {{ type: completed, summary: planned }}\n  - match_prompt_contains: \"task-x\"\n    effects:\n      - {{ path: x.txt, content: \"x\" }}\n    outcome: {{ type: completed, summary: did-x }}\n  - match_prompt_contains: \"task-y\"\n    effects:\n      - {{ path: y.txt, content: \"y\" }}\n    outcome: {{ type: completed, summary: did-y }}\n",
        artifacts_dir.display(),
        ledger,
    );

    let (terminal, state) = bench
        .run_with_config(&workflow, &fixture, CONCURRENCY_CONFIG)
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        state.tasks.get(&"task-x".into()),
        Some(&yunta_core::events::TaskStatus::Done)
    );
    assert_eq!(
        state.tasks.get(&"task-y".into()),
        Some(&yunta_core::events::TaskStatus::Done)
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    for (task, forbidden) in [("task-x", "y.txt"), ("task-y", "x.txt")] {
        for event in &events {
            if let yunta_core::events::EventPayload::ScopeChecked(p) = &event.payload {
                if p.task_id.as_ref().map(|id| id.as_str()) == Some(task) {
                    assert!(
                        !p.diff
                            .iter()
                            .any(|path| path.to_string_lossy().contains(forbidden)),
                        "task `{task}`'s own scope check must never see `{forbidden}`: {:?}",
                        p.diff
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn killing_the_engine_mid_batch_and_resuming_only_reruns_the_orphan() {
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");

    let workflow: yunta_core::Workflow = serde_yaml::from_str(&concurrency_workflow(2)).unwrap();
    let config: yunta_core::ConfigLayer = serde_yaml::from_str(CONCURRENCY_CONFIG).unwrap();
    let manifest = build_manifest(&workflow, &config, &bench.worktree, &bench.worktree).unwrap();
    let run_dir = create_run(
        &bench.run_id,
        &manifest,
        &bench.runs_root,
        &bench.storage,
        &FixedClock,
    )
    .unwrap();

    let ledger = format!(
        "tasks:\n{}{}",
        task_yaml("task-p", "p", "p.txt", "test -f p.txt"),
        task_yaml("task-q", "q", "q.txt", "test -f q.txt"),
    );
    std::fs::create_dir_all(&artifacts_dir).unwrap();
    std::fs::write(artifacts_dir.join("plan.yaml"), &ledger).unwrap();

    // Simulate the crash by hand-writing the log up through: plan already
    // registered, the loop started, task-p already Done and committed,
    // and task-q left `Running` with no terminal event — an orphan.
    git(&bench.worktree, &["checkout", "-b", "yunta/task/task-p/1"]);
    std::fs::write(bench.worktree.join("p.txt"), "p").unwrap();
    git(&bench.worktree, &["add", "-A"]);
    git(&bench.worktree, &["commit", "-q", "-m", "task task-p: p"]);
    git(&bench.worktree, &["checkout", "-"]);
    git(
        &bench.worktree,
        &["merge", "--ff-only", "yunta/task/task-p/1"],
    );

    for event in [
        yunta_core::events::Event {
            run_id: bench.run_id.clone(),
            seq: 0,
            timestamp: FixedClock.now(),
            node_id: Some("plan".into()),
            payload: yunta_core::events::EventPayload::NodeStarted(
                yunta_core::events::NodeStartedPayload { attempt: 1 },
            ),
        },
        yunta_core::events::Event {
            run_id: bench.run_id.clone(),
            seq: 0,
            timestamp: FixedClock.now(),
            node_id: Some("plan".into()),
            payload: yunta_core::events::EventPayload::ArtifactWritten(
                yunta_core::events::ArtifactWrittenPayload {
                    path: "artifacts/plan.yaml".into(),
                    content_hash: "irrelevant".to_string(),
                },
            ),
        },
        yunta_core::events::Event {
            run_id: bench.run_id.clone(),
            seq: 0,
            timestamp: FixedClock.now(),
            node_id: Some("plan".into()),
            payload: yunta_core::events::EventPayload::TaskRegistered(
                yunta_core::events::TaskRegisteredPayload {
                    task_id: "task-p".into(),
                    criteria: vec![],
                    scope: vec!["p.txt".to_string()],
                    depends_on: vec![],
                },
            ),
        },
        yunta_core::events::Event {
            run_id: bench.run_id.clone(),
            seq: 0,
            timestamp: FixedClock.now(),
            node_id: Some("plan".into()),
            payload: yunta_core::events::EventPayload::TaskRegistered(
                yunta_core::events::TaskRegisteredPayload {
                    task_id: "task-q".into(),
                    criteria: vec![],
                    scope: vec!["q.txt".to_string()],
                    depends_on: vec![],
                },
            ),
        },
        yunta_core::events::Event {
            run_id: bench.run_id.clone(),
            seq: 0,
            timestamp: FixedClock.now(),
            node_id: Some("plan".into()),
            payload: yunta_core::events::EventPayload::NodeFinished(
                yunta_core::events::NodeFinishedPayload {
                    outcome: "planned".to_string(),
                    tokens_used: Default::default(),
                },
            ),
        },
        yunta_core::events::Event {
            run_id: bench.run_id.clone(),
            seq: 0,
            timestamp: FixedClock.now(),
            node_id: Some("implement".into()),
            payload: yunta_core::events::EventPayload::NodeStarted(
                yunta_core::events::NodeStartedPayload { attempt: 1 },
            ),
        },
        yunta_core::events::Event {
            run_id: bench.run_id.clone(),
            seq: 0,
            timestamp: FixedClock.now(),
            node_id: Some("implement".into()),
            payload: yunta_core::events::EventPayload::TaskStatusChanged(
                yunta_core::events::TaskStatusChangedPayload {
                    task_id: "task-p".into(),
                    new_status: yunta_core::events::TaskStatus::Running,
                    caused_by: 0,
                },
            ),
        },
        yunta_core::events::Event {
            run_id: bench.run_id.clone(),
            seq: 0,
            timestamp: FixedClock.now(),
            node_id: Some("implement".into()),
            payload: yunta_core::events::EventPayload::TaskStatusChanged(
                yunta_core::events::TaskStatusChangedPayload {
                    task_id: "task-q".into(),
                    new_status: yunta_core::events::TaskStatus::Running,
                    caused_by: 0,
                },
            ),
        },
        yunta_core::events::Event {
            run_id: bench.run_id.clone(),
            seq: 0,
            timestamp: FixedClock.now(),
            node_id: Some("implement".into()),
            payload: yunta_core::events::EventPayload::TaskStatusChanged(
                yunta_core::events::TaskStatusChangedPayload {
                    task_id: "task-p".into(),
                    new_status: yunta_core::events::TaskStatus::Done,
                    caused_by: 0,
                },
            ),
        },
        // task-q never got a follow-up — orphaned Running, no p.txt-style
        // commit ever landed for it.
    ] {
        bench.storage.append_event(&event).unwrap();
    }

    let fixture = "sessions:\n  - match_prompt_contains: \"task-q\"\n    effects:\n      - { path: q.txt, content: \"q\" }\n    outcome: { type: completed, summary: did-q }\n";
    let adapter = yunta_adapters::MockAdapter::from_yaml(fixture).unwrap();
    let mut adapters: HashMap<String, Arc<dyn Adapter>> = HashMap::new();
    adapters.insert("mock".to_string(), Arc::new(adapter));

    let report = execute_run(
        &bench.run_id,
        &manifest,
        &run_dir,
        &bench.worktree,
        &adapters,
        &bench.storage,
        &FixedClock,
        DEFAULT_MAX_RETRIES,
    )
    .await
    .unwrap();

    assert_eq!(report.terminal, RunTerminal::Finished);
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let p_running_count = events
        .iter()
        .filter(|e| {
            matches!(
                &e.payload,
                yunta_core::events::EventPayload::TaskStatusChanged(p)
                    if p.task_id.as_str() == "task-p" && p.new_status == yunta_core::events::TaskStatus::Running
            )
        })
        .count();
    assert_eq!(
        p_running_count, 1,
        "an already-Done task must never be re-dispatched on resume"
    );
    assert_eq!(
        report.state.tasks.get(&"task-q".into()),
        Some(&yunta_core::events::TaskStatus::Done),
        "the orphaned task must be re-run to completion"
    );
}

// --- T5.11: scope_expansion (§6.2, D73) ------------------------------------

/// A loop node declaring `scope_expansion:` — `within` is only rendered
/// when the caller passes something, so `rules`-mode tests can still omit
/// it when a test wants an empty ceiling.
fn scope_expansion_workflow(mode: &str, within: &[&str], max_per_run: Option<u32>) -> String {
    let within_line = if within.is_empty() {
        String::new()
    } else {
        let items = within
            .iter()
            .map(|w| format!("\"{w}\""))
            .collect::<Vec<_>>()
            .join(", ");
        format!("      within: [{items}]\n")
    };
    let cap_line = max_per_run
        .map(|n| format!("      max_per_run: {n}\n"))
        .unwrap_or_default();
    format!(
        r#"
name: scope-expansion
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the ledger to {{{{run.dir}}}}/artifacts/plan.yaml."
    artifacts:
      produces:
        - {{ name: plan.yaml, kind: task-ledger }}
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Read your task from the ledger and implement it."
    scope_expansion:
      mode: {mode}
{within_line}{cap_line}"#
    )
}

/// The same loop shape with no `scope_expansion:` key at all — §6.2's own
/// default (an absent block behaves exactly like `mode: deny` with no
/// `within`/`max_per_run`) — proving that default is really live, not
/// just documented.
fn no_scope_expansion_workflow() -> String {
    r#"
name: scope-expansion-default
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
"#
    .to_string()
}

fn plan_session(artifacts_dir: &std::path::Path, ledger: &str) -> String {
    format!(
        "sessions:\n  - effects:\n      - {{ path: \"{}/plan.yaml\", content: {:?} }}\n    outcome: {{ type: completed, summary: planned }}\n",
        artifacts_dir.display(),
        ledger,
    )
}

fn findings_posted(events: &[yunta_core::events::Event]) -> Vec<&yunta_core::events::Finding> {
    events
        .iter()
        .filter_map(|e| match &e.payload {
            yunta_core::events::EventPayload::FindingPosted(p) => Some(&p.finding),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn writing_outside_scope_without_a_request_is_a_plain_violation_never_an_implicit_expansion()
{
    // ✓ del Plan: nada le da a un agente una vía para ampliar su propio
    // scope salvo el protocolo de request — ni un `within` que
    // técnicamente cubriría el path lo salva si nunca se escribió un
    // request. `scope_expansion: { mode: rules, within: [b.txt] }` está
    // declarado, pero el agente jamás escribe el archivo de request.
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");

    let workflow = scope_expansion_workflow("rules", &["b.txt"], None);
    let ledger = format!(
        "tasks:\n{}",
        task_yaml("task-s", "s", "a.txt", "test -f a.txt")
    );

    let mut fixture = plan_session(&artifacts_dir, &ledger);
    for _ in 0..=DEFAULT_MAX_RETRIES {
        fixture.push_str(
            "  - match_prompt_contains: \"task-s\"\n    effects:\n      - { path: a.txt, content: \"a\" }\n      - { path: b.txt, content: \"b\" }\n    outcome: { type: completed, summary: did-s }\n",
        );
    }

    let (terminal, state) = bench.run(&workflow, &fixture).await;

    assert_eq!(
        state.tasks.get(&"task-s".into()),
        Some(&yunta_core::events::TaskStatus::Blocked),
        "an out-of-scope write with no request must block the task, never silently pass"
    );
    match terminal {
        RunTerminal::Paused { .. } => {}
        other => panic!("expected the run to pause on task-s, got {other:?}"),
    }

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(
        !events.iter().any(|e| matches!(
            &e.payload,
            yunta_core::events::EventPayload::ScopeExpansionRequested(p)
                if p.task_id.as_str() == "task-s"
        )),
        "no scope_expansion_* event may fire when the agent never wrote a request"
    );
}

#[tokio::test]
async fn an_already_passing_proposed_criterion_is_denied_without_consulting_even_in_ask_mode() {
    // ✓ del Plan: un criterio propuesto que ya pasa se rechaza sin
    // consultar en NINGÚN modo — ni siquiera `ask`, que de otro modo
    // escalaría y pausaría el run.
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");

    let workflow = scope_expansion_workflow("ask", &[], None);
    let ledger = format!(
        "tasks:\n{}",
        task_yaml("task-p", "p", "a.txt", "test -f a.txt")
    );

    let request_yaml = "paths:\n  - c.txt\nreason: \"already fine, no work needed\"\nproposed_criterion:\n  cmd: \"true\"\n";
    let mut fixture = plan_session(&artifacts_dir, &ledger);
    fixture.push_str(&format!(
        "  - match_prompt_contains: \"task-p\"\n    effects:\n      - {{ path: a.txt, content: \"a\" }}\n      - {{ path: {:?}, content: {:?} }}\n    outcome: {{ type: completed, summary: did-p }}\n",
        yunta_engine::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE,
        request_yaml,
    ));

    let (terminal, state) = bench.run(&workflow, &fixture).await;

    assert_eq!(
        terminal,
        RunTerminal::Finished,
        "an auto-rejected request must never pause the run, even under ask mode"
    );
    assert_eq!(
        state.tasks.get(&"task-p".into()),
        Some(&yunta_core::events::TaskStatus::Done)
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let denied = events
        .iter()
        .find_map(|e| match &e.payload {
            yunta_core::events::EventPayload::ScopeExpansionDenied(p)
                if p.task_id.as_str() == "task-p" =>
            {
                Some(p)
            }
            _ => None,
        })
        .expect("a Denied event must be recorded");
    assert!(denied
        .denial_reason
        .as_deref()
        .unwrap_or_default()
        .contains("already passes"));

    let findings = findings_posted(&events);
    assert!(
        findings
            .iter()
            .any(|f| f.detail.contains("already fine, no work needed")),
        "the finding must carry the agent's own reason: {findings:?}"
    );
}

#[tokio::test]
async fn every_denial_becomes_a_finding_carrying_the_agent_s_reason_and_criterion() {
    // ✓ del Plan: toda denegación —acá, el default `deny` sin ningún
    // bloque `scope_expansion:` en el workflow— se convierte en un
    // finding (D80) que lleva el reason y el proposed_criterion del
    // propio agente, no una explicación inventada por el engine.
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");

    let workflow = no_scope_expansion_workflow();
    let ledger = format!(
        "tasks:\n{}",
        task_yaml("task-d", "d", "a.txt", "test -f a.txt")
    );

    let request_yaml = "paths:\n  - b.txt\nreason: \"need an adjacent fix in b.txt\"\nproposed_criterion:\n  cmd: \"test -f b.txt\"\n";
    let mut fixture = plan_session(&artifacts_dir, &ledger);
    fixture.push_str(&format!(
        "  - match_prompt_contains: \"task-d\"\n    effects:\n      - {{ path: a.txt, content: \"a\" }}\n      - {{ path: {:?}, content: {:?} }}\n    outcome: {{ type: completed, summary: did-d }}\n",
        yunta_engine::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE,
        request_yaml,
    ));

    let (terminal, state) = bench.run(&workflow, &fixture).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        state.tasks.get(&"task-d".into()),
        Some(&yunta_core::events::TaskStatus::Done)
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let denied = events
        .iter()
        .find_map(|e| match &e.payload {
            yunta_core::events::EventPayload::ScopeExpansionDenied(p)
                if p.task_id.as_str() == "task-d" =>
            {
                Some(p)
            }
            _ => None,
        })
        .expect("a Denied event must be recorded under the default deny mode");
    assert_eq!(
        denied.denial_reason.as_deref(),
        Some("scope_expansion mode is deny (the default)")
    );

    let findings = findings_posted(&events);
    let finding = findings
        .iter()
        .find(|f| f.detail.contains("need an adjacent fix in b.txt"))
        .expect("a finding carrying the agent's own reason must exist");
    assert_eq!(
        finding.proposed_criterion,
        Some(yunta_core::events::ProposedCriterion {
            cmd: "test -f b.txt".to_string()
        })
    );
    assert!(finding.location.contains("b.txt"));
}

#[tokio::test]
async fn a_granted_expansion_widens_what_the_final_scope_check_accepts() {
    // ✓ del Plan: "el diff final se evalúa contra scope declarado más
    // ampliaciones autorizadas" — mismo diff, mismo agente; sólo el modo
    // cambia entre las dos corridas.
    let ledger = format!(
        "tasks:\n{}",
        task_yaml("task-w", "w", "a.txt", "test -f a.txt")
    );
    let request_yaml = "paths:\n  - b.txt\nreason: \"small adjacent fix\"\nproposed_criterion:\n  cmd: \"test -f nonexistent-marker\"\n";
    let session = format!(
        "  - match_prompt_contains: \"task-w\"\n    effects:\n      - {{ path: a.txt, content: \"a\" }}\n      - {{ path: b.txt, content: \"b\" }}\n      - {{ path: {:?}, content: {:?} }}\n    outcome: {{ type: completed, summary: did-w }}\n",
        yunta_engine::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE,
        request_yaml,
    );

    // Granted: `rules` mode, `within` covers b.txt.
    let granted_bench = Bench::new();
    let granted_artifacts = granted_bench.run_dir().join("artifacts");
    let granted_workflow = scope_expansion_workflow("rules", &["b.txt"], None);
    let mut granted_fixture = plan_session(&granted_artifacts, &ledger);
    granted_fixture.push_str(&session);
    let (granted_terminal, granted_state) =
        granted_bench.run(&granted_workflow, &granted_fixture).await;
    assert_eq!(granted_terminal, RunTerminal::Finished);
    assert_eq!(
        granted_state.tasks.get(&"task-w".into()),
        Some(&yunta_core::events::TaskStatus::Done),
        "a granted expansion must let b.txt through the final scope check"
    );

    // Denied: same diff, `deny` mode — b.txt is never granted, so the
    // same write is now a real violation and the task never satisfies
    // its own scope check.
    let denied_bench = Bench::new();
    let denied_artifacts = denied_bench.run_dir().join("artifacts");
    let denied_workflow = scope_expansion_workflow("deny", &[], None);
    let mut denied_fixture = plan_session(&denied_artifacts, &ledger);
    for _ in 0..=DEFAULT_MAX_RETRIES {
        denied_fixture.push_str(&session);
    }
    let (_denied_terminal, denied_state) =
        denied_bench.run(&denied_workflow, &denied_fixture).await;
    assert_eq!(
        denied_state.tasks.get(&"task-w".into()),
        Some(&yunta_core::events::TaskStatus::Blocked),
        "without a grant, b.txt stays a scope violation on the same diff"
    );
}

#[tokio::test]
async fn the_request_object_is_recorded_identically_across_all_three_modes() {
    // ✓ del Plan: el request object es "idéntico en los tres modos" —
    // mismo agente, mismos paths/reason/proposed_criterion; sólo el modo
    // de la config cambia entre corridas. El evento `ScopeExpansionRequested`
    // debe grabar exactamente lo mismo en los tres casos, incluso cuando
    // el veredicto que sigue difiere.
    let ledger = format!(
        "tasks:\n{}",
        task_yaml("task-g", "g", "a.txt", "test -f a.txt")
    );
    let request_yaml = "paths:\n  - c.txt\nreason: \"golden request\"\nproposed_criterion:\n  cmd: \"test -f nonexistent-marker\"\n";
    let session = format!(
        "  - match_prompt_contains: \"task-g\"\n    effects:\n      - {{ path: a.txt, content: \"a\" }}\n      - {{ path: {:?}, content: {:?} }}\n    outcome: {{ type: completed, summary: did-g }}\n",
        yunta_engine::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE,
        request_yaml,
    );

    let mut requested_payloads = Vec::new();
    for mode in ["rules", "ask", "deny"] {
        let bench = Bench::new();
        let artifacts_dir = bench.run_dir().join("artifacts");
        let workflow = scope_expansion_workflow(mode, &[], None);
        let mut fixture = plan_session(&artifacts_dir, &ledger);
        fixture.push_str(&session);
        let _ = bench.run(&workflow, &fixture).await;

        let events = bench.storage.events_for_run(&bench.run_id).unwrap();
        let requested = events
            .iter()
            .find_map(|e| match &e.payload {
                yunta_core::events::EventPayload::ScopeExpansionRequested(p)
                    if p.task_id.as_str() == "task-g" =>
                {
                    Some(p.clone())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("mode `{mode}` must record a ScopeExpansionRequested event"));
        requested_payloads.push((mode, requested));
    }

    let (first_mode, first) = &requested_payloads[0];
    for (mode, payload) in &requested_payloads[1..] {
        assert_eq!(
            payload.paths, first.paths,
            "paths must be identical between `{first_mode}` and `{mode}`"
        );
        assert_eq!(payload.reason, first.reason);
        assert_eq!(payload.proposed_criterion, first.proposed_criterion);
        assert_eq!(
            payload.proposed_criterion_precheck,
            first.proposed_criterion_precheck
        );
    }
}
