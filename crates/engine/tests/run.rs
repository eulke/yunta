//! End-to-end runs with the mock adapter: the bootstrap
//! shape — a prompt plan node that produces the ledger, a loop that
//! implements it task by task, a bash gate — plus re-routes, pauses and
//! resume, all derived from the event log alone.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use yunta_adapters::{Adapter, MockAdapter};
use yunta_core::SeqIdSource;
use yunta_core::{AdapterId, Clock, ConfigLayer, RunId, Workflow};
use yunta_engine::{
    build_manifest, create_run, execute_run, CreateRunParams, NoInteraction, NodeState, RunEnv,
    RunTerminal, DEFAULT_MAX_RETRIES,
};
use yunta_storage::Storage;

/// Run ids for everything a test run gives birth to — unique across
/// the binary, so parallel tests never share a run directory.
static IDS: SeqIdSource = SeqIdSource::new("minted");

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
    /// tests that need `baseline:`/`coverage:`/`limits:` alongside the
    /// usual `runners:`.
    async fn run_with_config(
        &self,
        workflow_yaml: &str,
        fixture_yaml: &str,
        config_yaml: &str,
    ) -> (RunTerminal, yunta_engine::RunState) {
        self.run_full(workflow_yaml, fixture_yaml, config_yaml, &NoInteraction)
            .await
    }

    /// Same as [`Bench::run`] but with a caller-chosen
    /// [`yunta_engine::HumanInteraction`] — for tests that need to
    /// script a gate's resolution instead of always degrading to pause.
    async fn run_with_interaction(
        &self,
        workflow_yaml: &str,
        fixture_yaml: &str,
        human_interaction: &dyn yunta_engine::HumanInteraction,
    ) -> (RunTerminal, yunta_engine::RunState) {
        self.run_full(workflow_yaml, fixture_yaml, CONFIG, human_interaction)
            .await
    }

    /// The fully-parameterized shape every other `run*` helper delegates
    /// to: caller-chosen config layer *and* interaction surface.
    async fn run_full(
        &self,
        workflow_yaml: &str,
        fixture_yaml: &str,
        config_yaml: &str,
        human_interaction: &dyn yunta_engine::HumanInteraction,
    ) -> (RunTerminal, yunta_engine::RunState) {
        let workflow: Workflow = serde_yaml::from_str(workflow_yaml).unwrap();
        let config: ConfigLayer = serde_yaml::from_str(config_yaml).unwrap();
        let manifest = build_manifest(
            &workflow,
            &config,
            &self.worktree,
            &self.worktree,
            &HashMap::new(),
        )
        .unwrap();

        let run_dir = create_run(
            CreateRunParams {
                run_id: &self.run_id,
                manifest: &manifest,
                runs_root: &self.runs_root,
                mode: &"default".into(),
                promoted_from: None,
                artifacts: &[],
            },
            &self.storage.async_handle(),
            &FixedClock,
        )
        .await
        .unwrap();

        let adapter = MockAdapter::from_yaml(fixture_yaml).unwrap();
        let mut adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
        adapters.insert("mock".into(), Arc::new(adapter));

        let report = execute_run(RunEnv {
            run_id: &self.run_id,
            manifest: &manifest,
            run_dir: &run_dir,
            worktree: &self.worktree,
            adapters: &adapters,
            storage: &self.storage.async_handle(),
            clock: &FixedClock,
            ids: &IDS,
            max_task_retries: DEFAULT_MAX_RETRIES,
            human_interaction,
            forge: None,
            cancel: None,
            adapter_override: None,
        })
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
            matches!(state.nodes.get(node), Some(NodeState::Finished { .. })),
            "node `{node}` should be finished, got {:?}",
            state.nodes.get(node)
        );
    }
    assert_eq!(
        state.tasks.get("T001"),
        Some(&yunta_core::events::TaskStatus::Done)
    );
    assert_eq!(
        state.tasks.get("T002"),
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
    // writes it; lint re-runs and goes green (the canonical lint → fix-lint →
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
        state.nodes.get("lint"),
        Some(NodeState::Finished { .. })
    ));
    assert!(matches!(
        state.nodes.get("fix-lint"),
        Some(NodeState::Finished { .. })
    ));
}

#[tokio::test]
async fn a_goto_target_with_no_depends_on_never_runs_when_its_source_never_fails() {
    let bench = Bench::new();

    // `lint` always passes — `on_failure.goto` never fires. `fix-lint`
    // names no `depends_on` (it exists solely as a re-route target),
    // so nothing but an actual re-route may ever start it.
    // Before the fix, the generic "fresh nodes" batch scheduled
    // it anyway, purely because an empty `depends_on` reads as
    // trivially satisfied — wasting a session on every green run.
    let workflow = r#"
name: lint-clean
nodes:
  - id: lint
    kind: bash
    run: "true"
    on_failure: { goto: fix-lint, max_reroutes: 2 }
  - id: fix-lint
    kind: prompt
    runner: executor
    prompt: "should never be asked to fix anything"
"#;

    // No session scripted at all — if `fix-lint` is ever dispatched,
    // the mock adapter has nothing to hand it and the run errors out
    // instead of quietly leaking a false pass.
    let fixture = r#"
sessions: []
"#;

    let (terminal, state) = bench.run(workflow, fixture).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.get("lint"),
        Some(NodeState::Finished { .. })
    ));
    assert_eq!(
        state.nodes.get("fix-lint"),
        None,
        "fix-lint has no depends_on and lint never failed — it must never have started"
    );
}

#[tokio::test]
async fn a_gate_on_target_with_no_depends_on_never_runs_before_the_gate_maps_to_it() {
    let bench = Bench::new();

    // `redo` is only reachable via `approve.on.redo` — it declares no
    // `depends_on` of its own, same shape as a `goto` target. Choosing
    // `ship` (unmapped) must never have started `redo`.
    let workflow = r#"
name: gate-on-target
nodes:
  - id: approve
    kind: gate
    assignee: lead
    options: [redo, ship]
    on: { redo: redo-node }
  - id: redo-node
    kind: bash
    run: "true"
"#;

    let (terminal, state) = bench
        .run_with_interaction(
            workflow,
            "sessions: []",
            &SequencedInteraction::choosing(&["ship"]),
        )
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        state.nodes.get("redo-node"),
        None,
        "redo-node has no depends_on and the gate never mapped to it — it must never have started"
    );
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
        state.nodes.get("build"),
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

    // The session claims success but writes nothing — the engine
    // verifies, and the missing artifact fails the node.
    let fixture = r#"
sessions:
  - outcome: { type: completed, summary: "trust me, it is written" }
"#;

    let (terminal, state) = bench.run(workflow, fixture).await;

    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    match state.nodes.get("plan") {
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
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let report = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &bench.run_dir(),
        worktree: &bench.worktree,
        adapters: &HashMap::new(),
        storage: &bench.storage.async_handle(),
        clock: &FixedClock,
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
    })
    .await
    .unwrap();
    assert_eq!(report.terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let starts = events
        .iter()
        .filter(|e| {
            matches!(
                e.payload(),
                Some(yunta_core::events::EventPayload::NodeStarted(_))
            )
        })
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

// --- kind: questions ------------------------------------

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
    // "el nodo que pregunta cierra su sesión antes de que se
    // renderice nada" (la sesión mock corre y cierra normalmente, y solo
    // *después* de eso el engine actúa sobre las preguntas) y "sin TTY el
    // run queda `waiting`, nunca cuelga ni falla" — en este recorte no
    // existe ninguna superficie TTY/MCP/PR todavía, así que ese
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
            e.payload(),
            Some(yunta_core::events::EventPayload::ArtifactWritten(p)) if p.path.to_string_lossy().contains("questions.yaml")
        )),
        "the questions artifact must still be recorded as written"
    );
    assert!(
        !events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::NodeFinished(_))
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
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();

    let first_adapter = MockAdapter::from_yaml(&questions_fixture(&artifacts_dir)).unwrap();
    let mut first_adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
    first_adapters.insert("mock".into(), Arc::new(first_adapter));
    let first_report = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &first_adapters,
        storage: &bench.storage.async_handle(),
        clock: &FixedClock,
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
    })
    .await
    .unwrap();
    match &first_report.terminal {
        RunTerminal::Paused { .. } => {}
        other => panic!("expected the first run to pause, got {other:?}"),
    }

    // No `sessions:` at all — any attempt to dispatch a new session errors.
    let empty_adapter = MockAdapter::from_yaml("sessions: []").unwrap();
    let mut resume_adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
    resume_adapters.insert("mock".into(), Arc::new(empty_adapter));
    let resumed_report = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &resume_adapters,
        storage: &bench.storage.async_handle(),
        clock: &FixedClock,
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
    })
    .await
    .unwrap();

    assert_eq!(
        resumed_report.terminal, first_report.terminal,
        "resume must replay the exact same pause, no new session needed"
    );
}

// --- kind: questions → superficie interactiva ------------

/// A test surface that answers questions from a script — `resolve`
/// deliberately returns `None` so these tests prove `ask` alone drives
/// the flow.
struct ScriptedAnswers {
    answers: Vec<yunta_core::Answer>,
}

#[async_trait::async_trait]
impl yunta_engine::HumanInteraction for ScriptedAnswers {
    async fn resolve(
        &self,
        _escalation: &yunta_core::events::GateWaitingPayload,
    ) -> Option<yunta_core::events::GateResolvedPayload> {
        None
    }
    async fn ask(
        &self,
        _questions: &yunta_core::QuestionsFile,
        _interactive: bool,
    ) -> Option<yunta_engine::QuestionsReply> {
        Some(yunta_engine::QuestionsReply {
            answers: self.answers.clone(),
            channel: yunta_core::events::Channel::Tty,
            responder: Some("eulke".to_string()),
        })
    }
}

fn answer(id: &str, value: &str) -> yunta_core::Answer {
    yunta_core::Answer {
        id: id.into(),
        value: value.to_string(),
    }
}

#[tokio::test]
async fn answered_questions_finish_the_node_and_materialize_the_answers_artifact() {
    // With a live surface, the questions are answered in the same
    // invocation — the node finishes, the answers land as an artifact a
    // following node can mount, and `questions_answered` records hash,
    // channel and responder.
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");
    let fixture = questions_fixture(&artifacts_dir);

    let interaction = ScriptedAnswers {
        answers: vec![answer("q1", "staging")], // q2 is not required
    };
    let (terminal, state) = bench
        .run_with_interaction(QUESTIONS_WORKFLOW, &fixture, &interaction)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.get("ask"),
        Some(yunta_engine::NodeState::Finished { .. })
    ));

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let answered = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::QuestionsAnswered(p)) => Some(p),
            _ => None,
        })
        .expect("questions_answered must be on the log");
    assert_eq!(answered.channel, yunta_core::events::Channel::Tty);
    assert_eq!(answered.responder.as_deref(), Some("eulke"));
    assert!(!answered.answers_hash.is_empty());

    // The answers are a real artifact next to the questions, with
    // the given values, consumable by a later node via `artifact:`.
    let answers_path = artifacts_dir.join("questions.yaml.answers.yaml");
    let raw = std::fs::read_to_string(&answers_path).expect("answers artifact must exist");
    let parsed: yunta_core::AnswersFile = serde_yaml::from_str(&raw).unwrap();
    assert_eq!(parsed.answers, vec![answer("q1", "staging")]);
}

#[tokio::test]
async fn a_reply_missing_a_required_answer_pauses_citing_the_question() {
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");
    let fixture = questions_fixture(&artifacts_dir);

    let interaction = ScriptedAnswers {
        answers: vec![answer("q2", "just a note")], // q1 (required) missing
    };
    let (terminal, _state) = bench
        .run_with_interaction(QUESTIONS_WORKFLOW, &fixture, &interaction)
        .await;

    match &terminal {
        RunTerminal::Paused { reason } => {
            assert!(reason.contains("q1"), "must cite the missing q1: {reason}");
        }
        other => panic!("an incomplete reply must pause, got {other:?}"),
    }
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(
        !events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::QuestionsAnswered(_))
        )),
        "an invalid reply must never be recorded as answered"
    );
}

#[tokio::test]
async fn resuming_a_questions_pause_with_a_live_surface_answers_and_continues() {
    // The waiting state is derived from the log, so a *separate*
    // invocation (yunta resume with a TTY) re-asks and continues — no
    // conversational state, no new agent session.
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");
    let workflow: yunta_core::Workflow = serde_yaml::from_str(QUESTIONS_WORKFLOW).unwrap();
    let config: yunta_core::ConfigLayer = serde_yaml::from_str(CONFIG).unwrap();
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();

    // First invocation: headless — asks, pauses.
    let first_adapter = MockAdapter::from_yaml(&questions_fixture(&artifacts_dir)).unwrap();
    let mut first_adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
    first_adapters.insert("mock".into(), Arc::new(first_adapter));
    let first = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &first_adapters,
        storage: &bench.storage.async_handle(),
        clock: &FixedClock,
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
    })
    .await
    .unwrap();
    assert!(matches!(first.terminal, RunTerminal::Paused { .. }));
    // The paused node derives `waiting`, never "absent" or failed.
    assert!(
        matches!(
            first.state.nodes.get("ask"),
            Some(yunta_engine::NodeState::Waiting { .. })
        ),
        "got {:?}",
        first.state.nodes.get("ask")
    );

    // Second invocation: a live surface, an empty fixture — answering
    // needs no new session, only the log and the artifact on disk.
    let empty_adapter = MockAdapter::from_yaml("sessions: []").unwrap();
    let mut resume_adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
    resume_adapters.insert("mock".into(), Arc::new(empty_adapter));
    let interaction = ScriptedAnswers {
        answers: vec![answer("q1", "production")],
    };
    let resumed = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &resume_adapters,
        storage: &bench.storage.async_handle(),
        clock: &FixedClock,
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &interaction,
        forge: None,
        cancel: None,
        adapter_override: None,
    })
    .await
    .unwrap();

    assert_eq!(resumed.terminal, RunTerminal::Finished);
    assert!(matches!(
        resumed.state.nodes.get("ask"),
        Some(yunta_engine::NodeState::Finished { .. })
    ));
    let raw = std::fs::read_to_string(artifacts_dir.join("questions.yaml.answers.yaml")).unwrap();
    let parsed: yunta_core::AnswersFile = serde_yaml::from_str(&raw).unwrap();
    assert_eq!(parsed.answers, vec![answer("q1", "production")]);
}

#[tokio::test]
async fn a_choice_answer_outside_its_declared_values_pauses_citing_the_value() {
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");
    let fixture = questions_fixture(&artifacts_dir);

    let interaction = ScriptedAnswers {
        answers: vec![answer("q1", "qa")], // not in [staging, production]
    };
    let (terminal, _state) = bench
        .run_with_interaction(QUESTIONS_WORKFLOW, &fixture, &interaction)
        .await;

    match &terminal {
        RunTerminal::Paused { reason } => {
            assert!(
                reason.contains("qa") || reason.contains("q1"),
                "must cite the invalid value or its question: {reason}"
            );
        }
        other => panic!("an out-of-values choice must pause, got {other:?}"),
    }
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
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();

    let report = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &HashMap::new(),
        storage: &bench.storage.async_handle(),
        clock: &FixedClock,
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
    })
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
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();

    let report = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &HashMap::new(),
        storage: &bench.storage.async_handle(),
        clock: &FixedClock,
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
    })
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
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();

    // Simulate a crash mid-node: the log has node_started with no
    // terminal event — exactly what a killed engine leaves behind.
    bench
        .storage
        .append(
            &yunta_core::events::EventDraft {
                run_id: bench.run_id.clone(),
                node_id: Some("only".into()),
                payload: yunta_core::events::EventPayload::NodeStarted(
                    yunta_core::events::NodeStartedPayload { attempt: 1 },
                ),
            },
            &yunta_core::SystemClock,
        )
        .unwrap();

    std::fs::write(bench.worktree.join("present.txt"), "here").unwrap();

    let report = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &HashMap::new(),
        storage: &bench.storage.async_handle(),
        clock: &FixedClock,
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
    })
    .await
    .unwrap();

    assert_eq!(report.terminal, RunTerminal::Finished);
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(
        events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::RunResumed(_))
        )),
        "resume must be recorded in the log"
    );
    // The orphan restarted as attempt 2.
    let last_start = events
        .iter()
        .filter_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::NodeStarted(p)) => Some(p.attempt),
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
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();

    // Same simulated crash as the restart_node test: node_started with no
    // terminal event.
    bench
        .storage
        .append(
            &yunta_core::events::EventDraft {
                run_id: bench.run_id.clone(),
                node_id: Some("only".into()),
                payload: yunta_core::events::EventPayload::NodeStarted(
                    yunta_core::events::NodeStartedPayload { attempt: 1 },
                ),
            },
            &yunta_core::SystemClock,
        )
        .unwrap();

    let report = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &HashMap::new(),
        storage: &bench.storage.async_handle(),
        clock: &FixedClock,
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
    })
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
        .filter(|e| {
            matches!(
                e.payload(),
                Some(yunta_core::events::EventPayload::NodeStarted(_))
            )
        })
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
        state.tasks.get("T001"),
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
            matches!(state.nodes.get(id), Some(NodeState::Finished { .. })),
            "expected `{id}` finished, got {:?}",
            state.nodes.get(id)
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
        state.nodes.get("load-test"),
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
        state.nodes.get("fast"),
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
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();

    // Simulate a crash mid-group: the parallel node and one child
    // (write-docs) finished; the other child (load-test) never started.
    for event in [
        yunta_core::events::EventDraft {
            run_id: bench.run_id.clone(),
            node_id: Some("pre-launch".into()),
            payload: yunta_core::events::EventPayload::NodeStarted(
                yunta_core::events::NodeStartedPayload { attempt: 1 },
            ),
        },
        yunta_core::events::EventDraft {
            run_id: bench.run_id.clone(),
            node_id: Some("write-docs".into()),
            payload: yunta_core::events::EventPayload::NodeStarted(
                yunta_core::events::NodeStartedPayload { attempt: 1 },
            ),
        },
        yunta_core::events::EventDraft {
            run_id: bench.run_id.clone(),
            node_id: Some("write-docs".into()),
            payload: yunta_core::events::EventPayload::NodeFinished(
                yunta_core::events::NodeFinishedPayload {
                    outcome: "exit 0".to_string(),
                    tokens_used: Default::default(),
                },
            ),
        },
    ] {
        bench
            .storage
            .append(&event, &yunta_core::SystemClock)
            .unwrap();
    }
    // If write-docs re-ran, it would overwrite this — instead assert it
    // survives untouched, since a second `touch` would only prove nothing.
    std::fs::write(bench.worktree.join("docs.txt"), "original").unwrap();
    std::fs::write(bench.worktree.join("present.txt"), "here").unwrap();

    let report = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &HashMap::new(),
        storage: &bench.storage.async_handle(),
        clock: &FixedClock,
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
    })
    .await
    .unwrap();

    assert_eq!(report.terminal, RunTerminal::Finished);
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let write_docs_starts = events
        .iter()
        .filter(|e| {
            e.node_id.as_ref().map(|id| id.as_str()) == Some("write-docs")
                && matches!(
                    e.payload(),
                    Some(yunta_core::events::EventPayload::NodeStarted(_))
                )
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
    match state.nodes.get("probe") {
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
    // The YAML text alone never matches the denied pattern — the
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
    // hook could opt out of the permission model.
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
    // A test that documents the limit, not a bug:
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
    let round_tripped: Vec<yunta_core::events::StoredEvent> = jsonl
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

// --- concurrency: N in loop nodes -----------------------

const CONCURRENCY_CONFIG: &str = r#"
runners:
  planner:
    - { adapter: mock, model: mock-model }
  executor:
    - { adapter: mock, model: mock-model }
"#;

/// An 8-independent-task ledger: no `depends_on` between any of them, each
/// with its own disjoint scope (`out-N.txt`) so `ledger::register`
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
    // Same final state, same commit sequence, regardless of
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
        let id: yunta_core::TaskId = format!("task-{n}").parse().unwrap();
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
    assert_eq!(state.nodes.get("implement"), state.nodes.get("implement"),);
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let a_statuses: Vec<_> = events
        .iter()
        .filter_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::TaskStatusChanged(p))
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
        .filter_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::TaskStatusChanged(p))
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
        state.tasks.get("task-x"),
        Some(&yunta_core::events::TaskStatus::Done)
    );
    assert_eq!(
        state.tasks.get("task-y"),
        Some(&yunta_core::events::TaskStatus::Done)
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    for (task, forbidden) in [("task-x", "y.txt"), ("task-y", "x.txt")] {
        for event in &events {
            if let Some(yunta_core::events::EventPayload::ScopeChecked(p)) = event.payload() {
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
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
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
        yunta_core::events::EventDraft {
            run_id: bench.run_id.clone(),
            node_id: Some("plan".into()),
            payload: yunta_core::events::EventPayload::NodeStarted(
                yunta_core::events::NodeStartedPayload { attempt: 1 },
            ),
        },
        yunta_core::events::EventDraft {
            run_id: bench.run_id.clone(),
            node_id: Some("plan".into()),
            payload: yunta_core::events::EventPayload::ArtifactWritten(
                yunta_core::events::ArtifactWrittenPayload {
                    path: "artifacts/plan.yaml".into(),
                    content_hash: "irrelevant".to_string(),
                    artifact_kind: None,
                },
            ),
        },
        yunta_core::events::EventDraft {
            run_id: bench.run_id.clone(),
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
        yunta_core::events::EventDraft {
            run_id: bench.run_id.clone(),
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
        yunta_core::events::EventDraft {
            run_id: bench.run_id.clone(),
            node_id: Some("plan".into()),
            payload: yunta_core::events::EventPayload::NodeFinished(
                yunta_core::events::NodeFinishedPayload {
                    outcome: "planned".to_string(),
                    tokens_used: Default::default(),
                },
            ),
        },
        yunta_core::events::EventDraft {
            run_id: bench.run_id.clone(),
            node_id: Some("implement".into()),
            payload: yunta_core::events::EventPayload::NodeStarted(
                yunta_core::events::NodeStartedPayload { attempt: 1 },
            ),
        },
        yunta_core::events::EventDraft {
            run_id: bench.run_id.clone(),
            node_id: Some("implement".into()),
            payload: yunta_core::events::EventPayload::TaskStatusChanged(
                yunta_core::events::TaskStatusChangedPayload {
                    task_id: "task-p".into(),
                    new_status: yunta_core::events::TaskStatus::Running,
                    caused_by: 1.into(),
                },
            ),
        },
        yunta_core::events::EventDraft {
            run_id: bench.run_id.clone(),
            node_id: Some("implement".into()),
            payload: yunta_core::events::EventPayload::TaskStatusChanged(
                yunta_core::events::TaskStatusChangedPayload {
                    task_id: "task-q".into(),
                    new_status: yunta_core::events::TaskStatus::Running,
                    caused_by: 1.into(),
                },
            ),
        },
        yunta_core::events::EventDraft {
            run_id: bench.run_id.clone(),
            node_id: Some("implement".into()),
            payload: yunta_core::events::EventPayload::TaskStatusChanged(
                yunta_core::events::TaskStatusChangedPayload {
                    task_id: "task-p".into(),
                    new_status: yunta_core::events::TaskStatus::Done,
                    caused_by: 1.into(),
                },
            ),
        },
        // task-q never got a follow-up — orphaned Running, no p.txt-style
        // commit ever landed for it.
    ] {
        bench
            .storage
            .append(&event, &yunta_core::SystemClock)
            .unwrap();
    }

    let fixture = "sessions:\n  - match_prompt_contains: \"task-q\"\n    effects:\n      - { path: q.txt, content: \"q\" }\n    outcome: { type: completed, summary: did-q }\n";
    let adapter = yunta_adapters::MockAdapter::from_yaml(fixture).unwrap();
    let mut adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
    adapters.insert("mock".into(), Arc::new(adapter));

    let report = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &adapters,
        storage: &bench.storage.async_handle(),
        clock: &FixedClock,
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
    })
    .await
    .unwrap();

    assert_eq!(report.terminal, RunTerminal::Finished);
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let p_running_count = events
        .iter()
        .filter(|e| {
            matches!(e.payload(), Some(yunta_core::events::EventPayload::TaskStatusChanged(p)) if p.task_id.as_str() == "task-p" && p.new_status == yunta_core::events::TaskStatus::Running)
        })
        .count();
    assert_eq!(
        p_running_count, 1,
        "an already-Done task must never be re-dispatched on resume"
    );
    assert_eq!(
        report.state.tasks.get("task-q"),
        Some(&yunta_core::events::TaskStatus::Done),
        "the orphaned task must be re-run to completion"
    );
}

// --- scope_expansion ------------------------------------

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

/// The same loop shape with no `scope_expansion:` key at all — the
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

fn findings_posted(
    events: &[yunta_core::events::StoredEvent],
) -> Vec<&yunta_core::events::Finding> {
    events
        .iter()
        .filter_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::FindingPosted(p)) => Some(&p.finding),
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
        state.tasks.get("task-s"),
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
            e.payload(),
            Some(yunta_core::events::EventPayload::ScopeExpansionRequested(p)) if p.task_id.as_str() == "task-s"
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
        state.tasks.get("task-p"),
        Some(&yunta_core::events::TaskStatus::Done)
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let denied = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::ScopeExpansionDenied(p))
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
    // Toda denegación —acá, el default `deny` sin ningún
    // bloque `scope_expansion:` en el workflow— se convierte en un
    // finding que lleva el reason y el proposed_criterion del
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
        state.tasks.get("task-d"),
        Some(&yunta_core::events::TaskStatus::Done)
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let denied = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::ScopeExpansionDenied(p))
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
        granted_state.tasks.get("task-w"),
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
        denied_state.tasks.get("task-w"),
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
            .find_map(|e| match e.payload() {
                Some(yunta_core::events::EventPayload::ScopeExpansionRequested(p))
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

// --- escalación de scope expansion → gate real ---------

/// One `ask`-mode attempt that writes a.txt (in scope), b.txt (outside)
/// and the request file asking for b.txt.
fn requesting_session(task_id: &str) -> String {
    let request_yaml = "paths:\n  - b.txt\nreason: \"adjacent fix in b.txt\"\nproposed_criterion:\n  cmd: \"test -f nonexistent-marker\"\n";
    format!(
        "  - match_prompt_contains: {task_id:?}\n    effects:\n      - {{ path: a.txt, content: \"a\" }}\n      - {{ path: b.txt, content: \"b\" }}\n      - {{ path: {:?}, content: {:?} }}\n    outcome: {{ type: completed, summary: asked }}\n",
        yunta_engine::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE,
        request_yaml,
    )
}

#[tokio::test]
async fn an_ask_mode_request_granted_by_a_human_lets_the_retry_use_the_expanded_scope() {
    // `mode: ask` with a live HumanInteraction consults instead of
    // pausing. Grant → the task returns to ready and its next attempt's
    // diff is evaluated against scope + the granted paths, which the
    // engine derives from the log's own `scope_expansion_granted.paths`.
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");

    let workflow = scope_expansion_workflow("ask", &[], None);
    let ledger = format!(
        "tasks:\n{}",
        task_yaml("task-h", "h", "a.txt", "test -f a.txt")
    );
    let mut fixture = plan_session(&artifacts_dir, &ledger);
    // Attempt 1: asks. Attempt 2 (after the human grants): same diff,
    // no new request — b.txt must now be covered by the grant on the log.
    fixture.push_str(&requesting_session("task-h"));
    fixture.push_str(
        "  - match_prompt_contains: \"task-h\"\n    effects:\n      - { path: a.txt, content: \"a\" }\n      - { path: b.txt, content: \"b\" }\n    outcome: { type: completed, summary: did-h }\n",
    );

    let interaction = ScriptedInteraction {
        resolution: yunta_core::events::GateResolvedPayload {
            chosen_option: Some("grant".to_string()),
            resolved_by: Some("eulke".to_string()),
            free_text: None,
            approved_sha: None,
        },
    };
    let (terminal, state) = bench
        .run_with_interaction(&workflow, &fixture, &interaction)
        .await;

    assert_eq!(
        terminal,
        RunTerminal::Finished,
        "grant must unblock the run"
    );
    assert_eq!(
        state.tasks.get("task-h"),
        Some(&yunta_core::events::TaskStatus::Done),
        "the retry's b.txt write must pass the widened scope check"
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let granted = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::ScopeExpansionGranted(p))
                if p.task_id.as_str() == "task-h" =>
            {
                Some(p)
            }
            _ => None,
        })
        .expect("a human grant must be recorded as scope_expansion_granted");
    assert_eq!(
        granted.decided_by,
        yunta_core::events::Decider::Person {
            id: "eulke".to_string()
        }
    );
    assert_eq!(
        granted.paths,
        vec!["b.txt".to_string()],
        "the grant must name exactly what it authorized — self-contained audit"
    );
    // The interaction itself is on the log, same vocabulary as every
    // other gate: waiting + resolved, together.
    assert!(events.iter().any(|e| matches!(
        e.payload(),
        Some(yunta_core::events::EventPayload::GateWaiting(p)) if p.summary.contains("task-h")
    )));
    assert!(events.iter().any(|e| matches!(
        e.payload(),
        Some(yunta_core::events::EventPayload::GateResolved(p)) if p.chosen_option.as_deref() == Some("grant")
    )));
}

#[tokio::test]
async fn an_ask_mode_request_denied_by_a_human_becomes_a_finding_and_the_task_retries_in_scope() {
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");

    let workflow = scope_expansion_workflow("ask", &[], None);
    let ledger = format!(
        "tasks:\n{}",
        task_yaml("task-n", "n", "a.txt", "test -f a.txt")
    );
    let mut fixture = plan_session(&artifacts_dir, &ledger);
    // Attempt 1 asks; the human denies; attempt 2 complies with the
    // original scope (a.txt only) and succeeds.
    fixture.push_str(&requesting_session("task-n"));
    fixture.push_str(
        "  - match_prompt_contains: \"task-n\"\n    effects:\n      - { path: a.txt, content: \"a\" }\n    outcome: { type: completed, summary: did-n }\n",
    );

    let interaction = ScriptedInteraction {
        resolution: yunta_core::events::GateResolvedPayload {
            chosen_option: Some("deny".to_string()),
            resolved_by: Some("eulke".to_string()),
            free_text: Some("out of this sprint".to_string()),
            approved_sha: None,
        },
    };
    let (terminal, state) = bench
        .run_with_interaction(&workflow, &fixture, &interaction)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        state.tasks.get("task-n"),
        Some(&yunta_core::events::TaskStatus::Done)
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let denied = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::ScopeExpansionDenied(p))
                if p.task_id.as_str() == "task-n" =>
            {
                Some(p)
            }
            _ => None,
        })
        .expect("the human denial must be recorded");
    assert_eq!(
        denied.decided_by,
        yunta_core::events::Decider::Person {
            id: "eulke".to_string()
        }
    );
    assert!(denied
        .denial_reason
        .as_deref()
        .unwrap_or_default()
        .contains("out of this sprint"));

    // Every denial — human ones included — becomes a finding
    // carrying the agent's own reason and proposed criterion.
    let findings = findings_posted(&events);
    let finding = findings
        .iter()
        .find(|f| f.detail.contains("adjacent fix in b.txt"))
        .expect("the denial must convert into a finding");
    assert_eq!(
        finding.proposed_criterion,
        Some(yunta_core::events::ProposedCriterion {
            cmd: "test -f nonexistent-marker".to_string()
        })
    );
}

#[tokio::test]
async fn an_ask_mode_request_with_no_surface_still_pauses_exactly_as_before() {
    // A live surface must not change the headless behavior: NoInteraction (yunta
    // test, CI) keeps degrading to a pause, with no gate recorded (an
    // unresolved question re-asks on resume, same convention as any gate).
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");

    let workflow = scope_expansion_workflow("ask", &[], None);
    let ledger = format!(
        "tasks:\n{}",
        task_yaml("task-p", "p", "a.txt", "test -f a.txt")
    );
    let mut fixture = plan_session(&artifacts_dir, &ledger);
    fixture.push_str(&requesting_session("task-p"));

    let (terminal, _state) = bench.run(&workflow, &fixture).await;
    match terminal {
        RunTerminal::Paused { .. } => {}
        other => panic!("headless ask must pause, got {other:?}"),
    }
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(
        !events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::GateWaiting(_))
        )),
        "an unresolved escalation must not be recorded as a published gate"
    );
}

// --- re-plan ---------------------------------------------

#[tokio::test]
async fn a_replan_preserves_an_identical_task_and_resets_one_whose_criteria_changed() {
    // ✓ del Plan (los tres): task-a se declara idéntica en ambos ledgers
    // y debe conservar `done` sin volver a correr; task-c cambia de
    // criterio (mismo id) y debe volver a `pending`; el commit de task-a
    // sigue en el worktree después del re-plan, y task-c corre sobre ese
    // mismo estado, no sobre uno revertido.
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");

    let workflow = r#"
name: replan
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
    on_failure: { goto: plan, max_reroutes: 1 }
"#;

    let ledger_v1 = format!(
        "tasks:\n{}{}",
        task_yaml("task-a", "Write a", "a.txt", "test -f a.txt"),
        // Never satisfiable by any effect a session can produce — task-c
        // exhausts its retries and blocks, which is what fails the loop
        // and triggers the reroute back to `plan`.
        task_yaml(
            "task-c",
            "Write c (bad criterion)",
            "c.txt",
            "test -f nonexistent-marker-c"
        ),
    );
    // Same id, same scope for both tasks; task-a's criterion is byte-
    // identical, task-c's is fixed to something satisfiable — the one
    // real identity change in this re-plan.
    let ledger_v2 = format!(
        "tasks:\n{}{}",
        task_yaml("task-a", "Write a", "a.txt", "test -f a.txt"),
        task_yaml("task-c", "Write c (fixed)", "c.txt", "test -f c.txt"),
    );

    let mut fixture = format!(
        "sessions:\n  - effects:\n      - {{ path: \"{}/plan.yaml\", content: {:?} }}\n    outcome: {{ type: completed, summary: planned }}\n",
        artifacts_dir.display(),
        ledger_v1,
    );
    fixture.push_str(
        "  - match_prompt_contains: \"task-a\"\n    effects:\n      - { path: a.txt, content: \"a\" }\n    outcome: { type: completed, summary: did-a }\n",
    );
    for _ in 0..=DEFAULT_MAX_RETRIES {
        fixture.push_str(
            "  - match_prompt_contains: \"task-c\"\n    outcome: { type: completed, summary: \"tried and failed\" }\n",
        );
    }
    fixture.push_str(&format!(
        "  - effects:\n      - {{ path: \"{}/plan.yaml\", content: {:?} }}\n    outcome: {{ type: completed, summary: replanned }}\n",
        artifacts_dir.display(),
        ledger_v2,
    ));
    fixture.push_str(
        "  - match_prompt_contains: \"task-c\"\n    effects:\n      - { path: c.txt, content: \"c\" }\n    outcome: { type: completed, summary: did-c }\n",
    );

    let (terminal, state) = bench.run(workflow, &fixture).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        state.tasks.get("task-a"),
        Some(&yunta_core::events::TaskStatus::Done)
    );
    assert_eq!(
        state.tasks.get("task-c"),
        Some(&yunta_core::events::TaskStatus::Done)
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let statuses_of = |task: &str| -> Vec<yunta_core::events::TaskStatus> {
        events
            .iter()
            .filter_map(|e| match e.payload() {
                Some(yunta_core::events::EventPayload::TaskStatusChanged(p))
                    if p.task_id.as_str() == task =>
                {
                    Some(p.new_status)
                }
                _ => None,
            })
            .collect()
    };

    assert_eq!(
        statuses_of("task-a"),
        vec![
            yunta_core::events::TaskStatus::Running,
            yunta_core::events::TaskStatus::Done,
        ],
        "an identical re-registration must never dispatch task-a again"
    );

    assert_eq!(
        statuses_of("task-c"),
        vec![
            yunta_core::events::TaskStatus::Running,
            yunta_core::events::TaskStatus::Blocked,
            yunta_core::events::TaskStatus::Pending,
            yunta_core::events::TaskStatus::Running,
            yunta_core::events::TaskStatus::Done,
        ],
        "a changed criterion must reset task-c to pending and let it run again"
    );

    let registered_count = events
        .iter()
        .filter(|e| {
            matches!(e.payload(), Some(yunta_core::events::EventPayload::TaskRegistered(p)) if p.task_id.as_str() == "task-c")
        })
        .count();
    assert_eq!(
        registered_count, 2,
        "both the original and the re-planned registration must stay in the log"
    );

    let commits = commit_subjects(&bench.worktree);
    assert!(
        commits.contains(&"task task-a: Write a".to_string()),
        "task-a's committed work must survive the re-plan: {commits:?}"
    );
}

// --- context: -----------------------------------------------------

/// A single `prompt` node named `ask` declaring `context_yaml` verbatim
/// under `context:`. `runner: executor` matches `CONFIG`'s own mock
/// candidate.
fn context_workflow(context_yaml: &str) -> String {
    format!(
        "name: ctx\nnodes:\n  - id: ask\n    kind: prompt\n    runner: executor\n    prompt: \"Do the thing.\"\n    context:\n{context_yaml}"
    )
}

/// The one `context_assembled` event's `sources`, for the given node.
fn context_sources(
    events: &[yunta_core::events::StoredEvent],
    node: &str,
) -> Vec<yunta_core::events::ContextSourceRef> {
    events
        .iter()
        .find_map(|e| match (&e.node_id, e.payload()) {
            (Some(n), Some(yunta_core::events::EventPayload::ContextAssembled(p)))
                if n.as_str() == node =>
            {
                Some(p.sources.clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("no context_assembled event found for node `{node}`"))
}

/// Confirms a resolved source is genuinely replayable: the file
/// materialized under `context/<content_hash>/content` exists and its
/// own hash matches what the event recorded — reconstructing it never
/// needs to re-run the command, re-read the original path outside the
/// snapshot, or touch the network.
fn assert_materialized(run_dir: &std::path::Path, source: &yunta_core::events::ContextSourceRef) {
    let path = run_dir
        .join("context")
        .join(&source.content_hash)
        .join("content");
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("materialized file missing at {path:?}: {e}"));
    assert_eq!(
        yunta_core::sha256_hex(&bytes),
        source.content_hash,
        "materialized content must hash to exactly what the event recorded"
    );
}

#[tokio::test]
async fn a_files_source_resolves_a_literal_path_and_is_replayable() {
    let bench = Bench::new();
    std::fs::write(bench.worktree.join("a.txt"), "MARKER-FILES-CONTENT\n").unwrap();

    let workflow = context_workflow("      - files: [\"a.txt\"]\n");
    let fixture = "sessions:\n  - match_prompt_contains: \"MARKER-FILES-CONTENT\"\n    outcome: { type: completed, summary: ok }\n";

    let (terminal, _state) = bench.run(&workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let sources = context_sources(&events, "ask");
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].kind, "files");
    assert_materialized(&bench.run_dir(), &sources[0]);
}

#[tokio::test]
async fn a_command_source_resolves_stdout_and_is_replayable() {
    let bench = Bench::new();
    let workflow = context_workflow("      - command: \"echo MARKER-COMMAND-OUTPUT\"\n");
    let fixture = "sessions:\n  - match_prompt_contains: \"MARKER-COMMAND-OUTPUT\"\n    outcome: { type: completed, summary: ok }\n";

    let (terminal, _state) = bench.run(&workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let sources = context_sources(&events, "ask");
    assert_eq!(sources[0].kind, "command");
    assert_materialized(&bench.run_dir(), &sources[0]);
}

#[tokio::test]
async fn an_artifact_source_creates_an_implicit_dependency_and_resolves_the_content() {
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");

    // No explicit `depends_on` on `plan` — the ordering must come purely
    // from `context: [{ artifact: { node: grill } }]`.
    let workflow = r#"
name: ctx-artifact
nodes:
  - id: grill
    kind: prompt
    runner: executor
    prompt: "Write the brief."
    artifacts:
      produces: [brief.md]
  - id: plan
    kind: prompt
    runner: executor
    prompt: "Plan from the brief."
    context:
      - artifact: { node: grill, name: brief.md }
"#;
    let fixture = format!(
        "sessions:\n  - effects:\n      - {{ path: \"{}/brief.md\", content: \"MARKER-ARTIFACT-CONTENT\" }}\n    outcome: {{ type: completed, summary: grilled }}\n  - match_prompt_contains: \"MARKER-ARTIFACT-CONTENT\"\n    outcome: {{ type: completed, summary: planned }}\n",
        artifacts_dir.display()
    );

    let (terminal, _state) = bench.run(workflow, &fixture).await;
    assert_eq!(
        terminal,
        RunTerminal::Finished,
        "the implicit dependency must order grill before plan without any explicit depends_on"
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let sources = context_sources(&events, "plan");
    assert_eq!(sources[0].kind, "artifact");
    assert_materialized(&bench.run_dir(), &sources[0]);
}

#[tokio::test]
async fn a_missing_artifact_reference_fails_the_node_never_silently_empty() {
    // ✓ del Plan: "fuente caída = nodo failed" — referencing a real,
    // already-run node whose artifact was simply never produced.
    let bench = Bench::new();
    let workflow = r#"
name: ctx-missing-artifact
nodes:
  - id: grill
    kind: bash
    run: "true"
  - id: plan
    kind: prompt
    runner: executor
    prompt: "Plan from the brief."
    depends_on: [grill]
    context:
      - artifact: { node: grill, name: brief.md }
"#;
    let fixture = "sessions: []";

    let (terminal, state) = bench.run(workflow, fixture).await;
    match &state.nodes.get("plan") {
        Some(yunta_engine::NodeState::Failed { outcome, .. }) => {
            assert!(outcome.contains("brief.md"), "got: {outcome}");
        }
        other => panic!("expected plan to fail citing the missing artifact, got {other:?}"),
    }
    match terminal {
        RunTerminal::Paused { .. } => {}
        other => panic!("expected the run to pause, got {other:?}"),
    }
}

#[tokio::test]
async fn a_run_events_source_resolves_filtered_failures_and_is_replayable() {
    let bench = Bench::new();
    let workflow = r#"
name: ctx-run-events
nodes:
  - id: lint
    kind: bash
    run: "false"
    on_failure: { goto: fix-lint, max_reroutes: 1 }
  - id: fix-lint
    kind: prompt
    runner: executor
    prompt: "Fix the lint errors."
    context:
      - run-events: { filter: failed }
"#;
    let fixture =
        "sessions:\n  - match_prompt_contains: \"NodeFailed\"\n    outcome: { type: completed, summary: tried }\n";

    let _ = bench.run(workflow, fixture).await;

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let sources = context_sources(&events, "fix-lint");
    assert_eq!(sources[0].kind, "run-events");
    assert_materialized(&bench.run_dir(), &sources[0]);
}

#[tokio::test]
async fn a_ledger_source_resolves_aggregate_task_state_and_is_replayable() {
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");
    let workflow = r#"
name: ctx-ledger
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the ledger to {{run.dir}}/artifacts/plan.yaml."
    artifacts:
      produces:
        - { name: plan.yaml, kind: task-ledger }
  - id: audit
    kind: prompt
    runner: executor
    depends_on: [plan]
    prompt: "Summarize the ledger."
    context:
      - ledger: {}
"#;
    let ledger = format!(
        "tasks:\n{}",
        task_yaml("task-x", "x", "x.txt", "test -f x.txt")
    );
    let fixture = format!(
        "sessions:\n  - effects:\n      - {{ path: \"{}/plan.yaml\", content: {:?} }}\n    outcome: {{ type: completed, summary: planned }}\n  - match_prompt_contains: \"task-x\"\n    outcome: {{ type: completed, summary: audited }}\n",
        artifacts_dir.display(),
        ledger,
    );

    let (terminal, _state) = bench.run(workflow, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let sources = context_sources(&events, "audit");
    assert_eq!(sources[0].kind, "ledger");
    assert_materialized(&bench.run_dir(), &sources[0]);
}

#[tokio::test]
async fn a_knowledge_source_resolves_the_repo_layer_and_is_replayable() {
    let bench = Bench::new();
    std::fs::create_dir_all(bench.worktree.join(".yunta/knowledge")).unwrap();
    std::fs::write(
        bench.worktree.join(".yunta/knowledge/note.md"),
        "MARKER-KNOWLEDGE-CONTENT\n",
    )
    .unwrap();

    let workflow = context_workflow("      - knowledge: {}\n");
    let fixture = "sessions:\n  - match_prompt_contains: \"MARKER-KNOWLEDGE-CONTENT\"\n    outcome: { type: completed, summary: ok }\n";

    let (terminal, _state) = bench.run(&workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let sources = context_sources(&events, "ask");
    assert_eq!(sources[0].kind, "knowledge");
    assert_materialized(&bench.run_dir(), &sources[0]);
}

#[tokio::test]
async fn a_node_output_source_resolves_a_bash_node_s_captured_stdout() {
    let bench = Bench::new();
    let workflow = r#"
name: ctx-node-output
nodes:
  - id: build
    kind: bash
    run: "echo MARKER-BUILD-OUTPUT"
  - id: report
    kind: prompt
    runner: executor
    depends_on: [build]
    prompt: "Report on the build."
    context:
      - node-output: { node: build }
"#;
    let fixture = "sessions:\n  - match_prompt_contains: \"MARKER-BUILD-OUTPUT\"\n    outcome: { type: completed, summary: reported }\n";

    let (terminal, _state) = bench.run(workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let sources = context_sources(&events, "report");
    assert_eq!(sources[0].kind, "node-output");
    assert_materialized(&bench.run_dir(), &sources[0]);
}

// --- templates — {{runner.role}}, {{project.*}} ----------------------

#[tokio::test]
async fn a_node_can_reference_its_own_runner_role_by_template() {
    // render golden — {{runner.role}} es el nombre de rol
    // declarado en `runner:`, conocido estáticamente, nunca el
    // adapter/model que una resolución posterior elige.
    let bench = Bench::new();
    let workflow = r#"
name: role-template
nodes:
  - id: only
    kind: bash
    runner: executor
    run: "test 'executor' = '{{runner.role}}'"
"#;
    let (terminal, _) = bench.run(workflow, "sessions: []").await;
    assert_eq!(terminal, RunTerminal::Finished);
}

#[tokio::test]
async fn a_node_can_reference_project_config_by_template() {
    let bench = Bench::new();
    let config = format!(
        "{CONFIG}\nproject:\n  name: mi-repo\n  base_branch: main\n  branch_prefix: yunta/\n"
    );
    let workflow = r#"
name: project-template
nodes:
  - id: only
    kind: bash
    run: "test '{{project.name}}' = 'mi-repo' && test '{{project.base_branch}}' = 'main' && test '{{project.branch_prefix}}' = 'yunta/'"
"#;
    let (terminal, _) = bench
        .run_with_config(workflow, "sessions: []", &config)
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
}

#[tokio::test]
async fn an_undefined_inputs_variable_still_fails_the_node_clearly() {
    // Declaring/validating/supplying `{{inputs.*}}` happens elsewhere — absent
    // that, referencing it is exactly the same "undefined variable"
    // failure any other unknown name would get, never silent text.
    let bench = Bench::new();
    let workflow = r#"
name: undefined-input
nodes:
  - id: only
    kind: bash
    run: "echo {{inputs.idea}}"
"#;
    let (terminal, _) = bench.run(workflow, "sessions: []").await;
    match terminal {
        RunTerminal::Paused { reason } => {
            assert!(reason.contains("inputs.idea"), "got: {reason}");
        }
        other => panic!("expected the run to pause citing the undefined variable, got {other:?}"),
    }
}

#[tokio::test]
async fn a_declared_input_s_default_resolves_in_a_node_s_own_template() {
    // `Bench::run` never supplies `--input` values (`&HashMap::new()`
    // throughout its own harness) — an input with a `default` is exactly
    // the case that still has a value to resolve without one.
    let bench = Bench::new();
    let workflow = r#"
name: default-input
inputs:
  greeting:
    type: string
    default: hola
nodes:
  - id: only
    kind: bash
    run: "test '{{inputs.greeting}}' = 'hola'"
"#;
    let (terminal, _) = bench.run(workflow, "sessions: []").await;
    assert_eq!(terminal, RunTerminal::Finished);
}

// --- ensamblado estable-primero --------------------------------

fn stable_first_workflow(volatile_command_output: &str) -> String {
    format!(
        "name: stable-first\nnodes:\n  - id: grill\n    kind: prompt\n    runner: executor\n    prompt: \"Write the brief.\"\n    artifacts:\n      produces: [brief.md]\n  - id: plan\n    kind: prompt\n    runner: executor\n    depends_on: [grill]\n    prompt: \"Plan from context.\"\n    context:\n      - command: \"echo {volatile_command_output}\"\n      - artifact: {{ node: grill, name: brief.md }}\n      - files: [\"stable.txt\"]\n"
    )
}

async fn run_stable_first(
    bench: &Bench,
    volatile_command_output: &str,
) -> (
    yunta_core::events::ContextSourceRef,
    yunta_core::events::ContextSourceRef,
    BTreeMap<String, String>,
) {
    std::fs::write(bench.worktree.join("stable.txt"), "STABLE-CONTENT\n").unwrap();
    let artifacts_dir = bench.run_dir().join("artifacts");
    let fixture = format!(
        "sessions:\n  - effects:\n      - {{ path: \"{}/brief.md\", content: \"FIXED-BRIEF-CONTENT\" }}\n    outcome: {{ type: completed, summary: grilled }}\n  - match_prompt_contains: \"{volatile_command_output}\"\n    outcome: {{ type: completed, summary: planned }}\n",
        artifacts_dir.display(),
    );
    let workflow = stable_first_workflow(volatile_command_output);

    let (terminal, _state) = bench.run(&workflow, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let payload = events
        .iter()
        .find_map(|e| match (&e.node_id, e.payload()) {
            (Some(n), Some(yunta_core::events::EventPayload::ContextAssembled(p)))
                if n.as_str() == "plan" =>
            {
                Some(p.clone())
            }
            _ => None,
        })
        .expect("context_assembled event for `plan`");

    let stable = payload
        .sources
        .iter()
        .find(|s| s.kind == "files")
        .unwrap()
        .clone();
    let run_stable = payload
        .sources
        .iter()
        .find(|s| s.kind == "artifact")
        .unwrap()
        .clone();
    (stable, run_stable, payload.segment_hashes)
}

#[tokio::test]
async fn the_stable_and_run_stable_segments_hash_identically_across_runs_with_different_volatile_content(
) {
    // "comparar hashes entre sesiones es la
    // verificación mecánica de que el prefijo se mantuvo estable" —
    // `command:` (volatile) cambia entre las dos corridas; `files:`
    // (stable) y `artifact:` (run-stable) no.
    let bench_a = Bench::new();
    let (stable_a, run_stable_a, segments_a) = run_stable_first(&bench_a, "VOLATILE-A").await;

    let bench_b = Bench::new();
    let (stable_b, run_stable_b, segments_b) = run_stable_first(&bench_b, "VOLATILE-B").await;

    assert_eq!(
        stable_a.content_hash, stable_b.content_hash,
        "the `files:` source itself must hash identically — its own content never changed"
    );
    assert_eq!(run_stable_a.content_hash, run_stable_b.content_hash);

    assert_eq!(
        segments_a["stable"], segments_b["stable"],
        "the stable segment's own canonical text must be byte-identical across runs"
    );
    assert_eq!(segments_a["run-stable"], segments_b["run-stable"]);
    assert_ne!(
        segments_a["volatile"], segments_b["volatile"],
        "the volatile segment must differ when the command's own output differs"
    );
    assert_eq!(
        segments_a.keys().collect::<std::collections::HashSet<_>>(),
        std::collections::HashSet::from([
            &"stable".to_string(),
            &"run-stable".to_string(),
            &"volatile".to_string()
        ]),
        "all three classes are in play for this workflow, so all three must be recorded"
    );
}

// --- knowledge layering, repo > user > org ---------------------

/// `YUNTA_HOME` is process-global state (`yunta_core::user_state_root`
/// reads it live, same as the CLI's own `project::resolve`), so any test
/// that points it at a scratch directory must hold this for its entire
/// critical section — never across an `.await`, so clippy's
/// `await_holding_lock` stays clean and no tokio runtime blocks another
/// task waiting on it.
static KNOWLEDGE_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Runs `body` with `YUNTA_HOME` pointed at `user_home` for its duration,
/// restoring whatever `YUNTA_HOME` held before (or clearing it) even if
/// `body` panics — so a failing assertion never leaks a bad env var into
/// whichever test runs next in this process.
fn with_user_home<T>(user_home: &std::path::Path, body: impl FnOnce() -> T) -> T {
    let _guard = KNOWLEDGE_ENV_LOCK.lock().unwrap();
    let previous = std::env::var("YUNTA_HOME").ok();
    std::env::set_var("YUNTA_HOME", user_home);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));

    match previous {
        Some(value) => std::env::set_var("YUNTA_HOME", value),
        None => std::env::remove_var("YUNTA_HOME"),
    }
    match result {
        Ok(value) => value,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

#[test]
fn a_knowledge_source_with_only_the_user_layer_resolves_the_user_root_and_is_replayable() {
    let user_home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(user_home.path().join("knowledge")).unwrap();
    std::fs::write(
        user_home.path().join("knowledge/note.md"),
        "MARKER-USER-ONLY-CONTENT\n",
    )
    .unwrap();

    with_user_home(user_home.path(), || {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let bench = Bench::new();
            let workflow = context_workflow("      - knowledge: { layers: [user] }\n");
            let fixture = "sessions:\n  - match_prompt_contains: \"MARKER-USER-ONLY-CONTENT\"\n    outcome: { type: completed, summary: ok }\n";

            let (terminal, _state) = bench.run(&workflow, fixture).await;
            assert_eq!(terminal, RunTerminal::Finished);

            let events = bench.storage.events_for_run(&bench.run_id).unwrap();
            let sources = context_sources(&events, "ask");
            assert_eq!(sources[0].kind, "knowledge");
            assert_materialized(&bench.run_dir(), &sources[0]);
        })
    });
}

#[test]
fn a_knowledge_source_merges_repo_and_user_with_repo_winning_a_name_collision() {
    let user_home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(user_home.path().join("knowledge")).unwrap();
    // Same filename in both layers: repo must win.
    std::fs::write(
        user_home.path().join("knowledge/shared.md"),
        "MARKER-FROM-USER-LOSES\n",
    )
    .unwrap();
    std::fs::write(
        user_home.path().join("knowledge/user-only.md"),
        "MARKER-USER-ONLY\n",
    )
    .unwrap();

    with_user_home(user_home.path(), || {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let bench = Bench::new();
            std::fs::create_dir_all(bench.worktree.join(".yunta/knowledge")).unwrap();
            std::fs::write(
                bench.worktree.join(".yunta/knowledge/shared.md"),
                "MARKER-FROM-REPO-WINS\n",
            )
            .unwrap();

            let workflow = context_workflow("      - knowledge: {}\n");
            let fixture = "sessions:\n  - match_prompt_contains: \"MARKER-FROM-REPO-WINS\"\n    outcome: { type: completed, summary: ok }\n";

            let (terminal, _state) = bench.run(&workflow, fixture).await;
            assert_eq!(terminal, RunTerminal::Finished);

            let events = bench.storage.events_for_run(&bench.run_id).unwrap();
            let sources = context_sources(&events, "ask");
            assert_materialized(&bench.run_dir(), &sources[0]);
            let path = bench
                .run_dir()
                .join("context")
                .join(&sources[0].content_hash)
                .join("content");
            let content = std::fs::read_to_string(path).unwrap();
            assert!(
                content.contains("MARKER-FROM-REPO-WINS"),
                "repo's `shared.md` must win over user's: {content}"
            );
            assert!(
                !content.contains("MARKER-FROM-USER-LOSES"),
                "user's overridden `shared.md` must not survive the merge: {content}"
            );
            assert!(
                content.contains("MARKER-USER-ONLY"),
                "user's own untouched file must still be present: {content}"
            );
        })
    });
}

// --- the org layer resolves from installed knowledge packs ------

/// One installed org knowledge pack under the worktree's own
/// `.yunta/packs/` — the vendored shape `pack add` produces, built
/// directly on disk (same convention as `catalog.rs`'s fixtures).
fn write_org_pack(worktree: &Path, publisher: &str, name: &str, files: &[(&str, &str)]) {
    let pack_dir = worktree.join(".yunta/packs").join(publisher).join(name);
    std::fs::create_dir_all(pack_dir.join("knowledge")).unwrap();
    std::fs::write(
        pack_dir.join("pack.yaml"),
        format!(
            "name: {name}\npublisher: {publisher}\nversion: 1.0.0\ndeclares:\n  \
             permissions: read-only\ncontents:\n  knowledge: [knowledge/]\n"
        ),
    )
    .unwrap();
    for (file, content) in files {
        std::fs::write(pack_dir.join("knowledge").join(file), content).unwrap();
    }
}

#[tokio::test]
async fn a_knowledge_source_resolves_an_installed_org_knowledge_pack() {
    // The org layer is the union of installed knowledge packs —
    // "knowledge pack instalado se resuelve
    // como capa org sin config extra".
    let bench = Bench::new();
    write_org_pack(
        &bench.worktree,
        "acme",
        "org-knowledge",
        &[("conventions.md", "MARKER-ORG-CONVENTIONS\n")],
    );

    let workflow = context_workflow("      - knowledge: { layers: [org] }\n");
    let fixture = "sessions:\n  - match_prompt_contains: \"MARKER-ORG-CONVENTIONS\"\n    outcome: { type: completed, summary: ok }\n";

    let (terminal, state) = bench.run(&workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished, "state: {state:?}");

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let sources = context_sources(&events, "ask");
    assert_eq!(sources[0].kind, "knowledge");
    assert_materialized(&bench.run_dir(), &sources[0]);
}

#[tokio::test]
async fn repo_knowledge_wins_a_name_collision_with_an_org_pack() {
    // The unchanged inter-layer precedence (org < user < repo) now
    // exercised against a real pack — and, since `knowledge: {}` is the
    // default that includes org, this also proves the default source no
    // longer errors the moment a knowledge pack is installed.
    let bench = Bench::new();
    write_org_pack(
        &bench.worktree,
        "acme",
        "org-knowledge",
        &[
            ("shared.md", "MARKER-FROM-ORG-LOSES\n"),
            ("org-only.md", "MARKER-ORG-ONLY\n"),
        ],
    );
    std::fs::create_dir_all(bench.worktree.join(".yunta/knowledge")).unwrap();
    std::fs::write(
        bench.worktree.join(".yunta/knowledge/shared.md"),
        "MARKER-FROM-REPO-WINS\n",
    )
    .unwrap();

    let workflow = context_workflow("      - knowledge: {}\n");
    let fixture = "sessions:\n  - match_prompt_contains: \"MARKER-FROM-REPO-WINS\"\n    outcome: { type: completed, summary: ok }\n";

    let (terminal, state) = bench.run(&workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished, "state: {state:?}");

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let sources = context_sources(&events, "ask");
    let path = bench
        .run_dir()
        .join("context")
        .join(&sources[0].content_hash)
        .join("content");
    let content = std::fs::read_to_string(path).unwrap();
    assert!(
        content.contains("MARKER-FROM-REPO-WINS"),
        "repo's `shared.md` must win over the org pack's: {content}"
    );
    assert!(
        !content.contains("MARKER-FROM-ORG-LOSES"),
        "the org pack's overridden `shared.md` must not survive the merge: {content}"
    );
    assert!(
        content.contains("MARKER-ORG-ONLY"),
        "the org pack's own untouched file must still be present: {content}"
    );
}

#[tokio::test]
async fn two_org_packs_shipping_the_same_filename_fail_the_node_naming_both() {
    // Between org packs there is no order — same filename from
    // two installed packs is a typed error naming both and the file,
    // never resolved alphabetically or by install order.
    let bench = Bench::new();
    write_org_pack(
        &bench.worktree,
        "acme",
        "pack-a",
        &[("conventions.md", "from pack-a\n")],
    );
    write_org_pack(
        &bench.worktree,
        "globex",
        "pack-b",
        &[("conventions.md", "from pack-b\n")],
    );

    let workflow = context_workflow("      - knowledge: { layers: [org] }\n");
    let fixture = "sessions: []";

    let (terminal, state) = bench.run(&workflow, fixture).await;
    match &state.nodes.get("ask") {
        Some(yunta_engine::NodeState::Failed { outcome, .. }) => {
            assert!(outcome.contains("conventions.md"), "got: {outcome}");
            assert!(outcome.contains("acme/pack-a"), "got: {outcome}");
            assert!(outcome.contains("globex/pack-b"), "got: {outcome}");
        }
        other => panic!("expected `ask` to fail naming both packs, got {other:?}"),
    }
    match terminal {
        RunTerminal::Paused { .. } => {}
        other => panic!("expected the run to pause, got {other:?}"),
    }
}

#[tokio::test]
async fn layers_repo_only_never_mounts_an_installed_org_pack() {
    let bench = Bench::new();
    write_org_pack(
        &bench.worktree,
        "acme",
        "org-knowledge",
        &[("conventions.md", "MARKER-ORG-MUST-NOT-APPEAR\n")],
    );
    std::fs::create_dir_all(bench.worktree.join(".yunta/knowledge")).unwrap();
    std::fs::write(
        bench.worktree.join(".yunta/knowledge/local.md"),
        "MARKER-REPO-LOCAL\n",
    )
    .unwrap();

    let workflow = context_workflow("      - knowledge: { layers: [repo] }\n");
    let fixture = "sessions:\n  - match_prompt_contains: \"MARKER-REPO-LOCAL\"\n    outcome: { type: completed, summary: ok }\n";

    let (terminal, state) = bench.run(&workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished, "state: {state:?}");

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let sources = context_sources(&events, "ask");
    let path = bench
        .run_dir()
        .join("context")
        .join(&sources[0].content_hash)
        .join("content");
    let content = std::fs::read_to_string(path).unwrap();
    assert!(
        !content.contains("MARKER-ORG-MUST-NOT-APPEAR"),
        "`layers: [repo]` must not mount the org pack: {content}"
    );
}

#[tokio::test]
async fn an_org_layer_with_no_packs_installed_resolves_empty_not_an_error() {
    // With a real resolver behind it, an empty org layer is a
    // true answer — same as `user` with no `~/.yunta/knowledge` — not
    // the degradation-with-error refusal a stub without a resolver would give.
    let bench = Bench::new();
    let workflow = context_workflow("      - knowledge: { layers: [org] }\n");
    let fixture = "sessions:\n  - outcome: { type: completed, summary: ok }\n";

    let (terminal, state) = bench.run(&workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished, "state: {state:?}");
}

// --- HumanInteraction — gates ----------------------------------

struct ScriptedInteraction {
    resolution: yunta_core::events::GateResolvedPayload,
}

#[async_trait::async_trait]
impl yunta_engine::HumanInteraction for ScriptedInteraction {
    async fn resolve(
        &self,
        _escalation: &yunta_core::events::GateWaitingPayload,
    ) -> Option<yunta_core::events::GateResolvedPayload> {
        Some(self.resolution.clone())
    }
}

/// A never-refuses `on_failure.goto` corrective node whose *second*
/// attempt actually fixes what its first attempt didn't — so a human
/// authorizing exactly one extra retry at the exhausted-reroutes gate is
/// what turns this workflow from perpetually failing into finished.
const HOPELESS_UNTIL_RETRIED_WORKFLOW: &str = r#"
name: hopeless-until-retried
nodes:
  - id: lint
    kind: bash
    run: "test -f fixed.txt"
    on_failure: { goto: fix-lint, max_reroutes: 1 }
  - id: fix-lint
    kind: prompt
    runner: executor
    prompt: "Try to fix it."
"#;

const HOPELESS_UNTIL_RETRIED_FIXTURE: &str = r#"
sessions:
  - outcome: { type: completed, summary: "did nothing useful" }
  - effects:
      - { path: fixed.txt, content: "fixed" }
    outcome: { type: completed, summary: "actually fixed it this time" }
"#;

#[tokio::test]
async fn a_gate_resolved_to_retry_reroutes_to_the_indicated_node_and_can_still_finish() {
    let bench = Bench::new();
    let interaction = ScriptedInteraction {
        resolution: yunta_core::events::GateResolvedPayload {
            chosen_option: Some("retry".to_string()),
            resolved_by: Some("eulke".to_string()),
            free_text: None,
            approved_sha: None,
        },
    };

    let (terminal, state) = bench
        .run_with_interaction(
            HOPELESS_UNTIL_RETRIED_WORKFLOW,
            HOPELESS_UNTIL_RETRIED_FIXTURE,
            &interaction,
        )
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.get("lint"),
        Some(yunta_engine::NodeState::Finished { .. })
    ));

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let reroutes = events
        .iter()
        .filter(|e| {
            matches!(
                e.payload(),
                Some(yunta_core::events::EventPayload::NodeRerouted(_))
            )
        })
        .count();
    assert_eq!(
        reroutes, 2,
        "the automatic reroute plus the gate-authorized one"
    );
    assert!(
        events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::GateWaiting(_))
        )),
        "the escalation itself must be on the log, not just its resolution"
    );
    let resolved = events.iter().find_map(|e| match e.payload() {
        Some(yunta_core::events::EventPayload::GateResolved(p)) => Some(p),
        _ => None,
    });
    assert_eq!(
        resolved.and_then(|p| p.chosen_option.as_deref()),
        Some("retry")
    );
}

#[tokio::test]
async fn a_gate_resolved_to_abort_pauses_citing_the_decision_and_free_text() {
    let bench = Bench::new();
    let interaction = ScriptedInteraction {
        resolution: yunta_core::events::GateResolvedPayload {
            chosen_option: Some("abort".to_string()),
            resolved_by: Some("eulke".to_string()),
            free_text: Some("not worth chasing today".to_string()),
            approved_sha: None,
        },
    };

    let (terminal, _) = bench
        .run_with_interaction(
            HOPELESS_UNTIL_RETRIED_WORKFLOW,
            HOPELESS_UNTIL_RETRIED_FIXTURE,
            &interaction,
        )
        .await;

    match terminal {
        RunTerminal::Paused { reason } => {
            assert!(reason.contains("abort"), "got: {reason}");
            assert!(reason.contains("not worth chasing today"), "got: {reason}");
        }
        other => panic!("expected Paused, got {other:?}"),
    }
}

#[tokio::test]
async fn a_gate_with_no_live_interaction_degrades_to_pausing_exactly_as_before_t7_2() {
    // Regression: `NoInteraction` (what every other test in this suite
    // already uses) must reproduce the same pause behavior byte for
    // byte — a gate existing must never change what an unattended run
    // does.
    let bench = Bench::new();
    let (terminal, _) = bench
        .run(
            HOPELESS_UNTIL_RETRIED_WORKFLOW,
            HOPELESS_UNTIL_RETRIED_FIXTURE,
        )
        .await;

    match terminal {
        RunTerminal::Paused { reason } => {
            assert!(reason.contains("re-route"), "got: {reason}");
        }
        other => panic!("expected Paused, got {other:?}"),
    }
}

// --- gate interno genérico (message/options/on) --------

/// Resolves gates from a scripted sequence, one per call — `None` once
/// the script runs out (so an unexpected extra ask degrades to pause
/// instead of hanging a test).
struct SequencedInteraction {
    resolutions:
        std::sync::Mutex<std::collections::VecDeque<yunta_core::events::GateResolvedPayload>>,
}

impl SequencedInteraction {
    fn choosing(options: &[&str]) -> Self {
        Self {
            resolutions: std::sync::Mutex::new(
                options
                    .iter()
                    .map(|option| yunta_core::events::GateResolvedPayload {
                        chosen_option: Some(option.to_string()),
                        resolved_by: Some("lead".to_string()),
                        free_text: None,
                        approved_sha: None,
                    })
                    .collect(),
            ),
        }
    }
}

#[async_trait::async_trait]
impl yunta_engine::HumanInteraction for SequencedInteraction {
    async fn resolve(
        &self,
        _escalation: &yunta_core::events::GateWaitingPayload,
    ) -> Option<yunta_core::events::GateResolvedPayload> {
        self.resolutions.lock().unwrap().pop_front()
    }
}

const INTERNAL_GATE_WORKFLOW: &str = r#"
name: internal-gate
nodes:
  - id: plan
    kind: bash
    run: "sh -c 'echo run >> plan-runs.txt'"
  - id: approve
    kind: gate
    depends_on: [plan]
    assignee: lead
    message: "Approve the plan?"
    options: [aprobar, ajustar]
    on: { ajustar: plan }
  - id: ship
    kind: bash
    depends_on: [approve]
    run: "true"
"#;

fn plan_run_count(worktree: &std::path::Path) -> usize {
    std::fs::read_to_string(worktree.join("plan-runs.txt"))
        .map(|s| s.lines().count())
        .unwrap_or(0)
}

#[tokio::test]
async fn an_internal_gate_approved_resolves_and_the_dag_continues() {
    let bench = Bench::new();
    let interaction = SequencedInteraction::choosing(&["aprobar"]);
    let (terminal, state) = bench
        .run_with_interaction(INTERNAL_GATE_WORKFLOW, "sessions: []\n", &interaction)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    match state.nodes.get("approve") {
        Some(yunta_engine::NodeState::Finished { outcome, .. }) => {
            assert_eq!(outcome, "aprobar")
        }
        other => panic!("expected the gate finished with the chosen option, got {other:?}"),
    }
    assert_eq!(plan_run_count(&bench.worktree), 1);

    // The recorded escalation carries the declared options plus the
    // engine-appended abort, each with a tradeoff.
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let waiting = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::GateWaiting(p)) => Some(p),
            _ => None,
        })
        .expect("the resolved interaction must be on the log");
    let ids: Vec<&str> = waiting.options.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, vec!["aprobar", "ajustar", "abort"]);
    assert!(waiting.options.iter().all(|o| !o.tradeoff.is_empty()));
    assert_eq!(waiting.summary, "Approve the plan?");
}

#[tokio::test]
async fn an_internal_gate_option_mapped_in_on_reroutes_and_asks_again() {
    // Re-route semantics through a human choice: `ajustar` re-routes to
    // `plan`, plan re-runs, and the gate asks AGAIN — the second answer
    // (`aprobar`) lets the DAG continue.
    let bench = Bench::new();
    let interaction = SequencedInteraction::choosing(&["ajustar", "aprobar"]);
    let (terminal, state) = bench
        .run_with_interaction(INTERNAL_GATE_WORKFLOW, "sessions: []\n", &interaction)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.get("approve"),
        Some(yunta_engine::NodeState::Finished { .. })
    ));
    assert_eq!(
        plan_run_count(&bench.worktree),
        2,
        "`ajustar` must re-run plan before the gate asks again"
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(events.iter().any(|e| matches!(e.payload(), Some(yunta_core::events::EventPayload::NodeRerouted(p)) if p.to_node.as_str() == "plan" && e.node_id.as_ref().map(|n| n.as_str()) == Some("approve"))));
}

#[tokio::test]
async fn an_internal_gate_with_no_surface_pauses_and_a_resume_re_asks() {
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");
    let _ = artifacts_dir; // same Bench shape as every other e2e here
    let workflow: yunta_core::Workflow = serde_yaml::from_str(INTERNAL_GATE_WORKFLOW).unwrap();
    let config: yunta_core::ConfigLayer = serde_yaml::from_str(CONFIG).unwrap();
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();

    let adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::from([(
        "mock".into(),
        Arc::new(MockAdapter::from_yaml("sessions: []").unwrap()) as Arc<dyn Adapter>,
    )]);
    let first = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &adapters,
        storage: &bench.storage.async_handle(),
        clock: &FixedClock,
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
    })
    .await
    .unwrap();
    match &first.terminal {
        RunTerminal::Paused { reason } => assert!(reason.contains("approve"), "got: {reason}"),
        other => panic!("headless internal gate must pause, got {other:?}"),
    }
    // Unresolved: nothing recorded (re-asks on resume, same gate convention).
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(!events.iter().any(|e| matches!(
        e.payload(),
        Some(yunta_core::events::EventPayload::GateWaiting(_))
    )));

    let interaction = SequencedInteraction::choosing(&["aprobar"]);
    let resumed = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &adapters,
        storage: &bench.storage.async_handle(),
        clock: &FixedClock,
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &interaction,
        forge: None,
        cancel: None,
        adapter_override: None,
    })
    .await
    .unwrap();
    assert_eq!(resumed.terminal, RunTerminal::Finished);
}

// --- run token budget (limits.max_tokens_per_run) ---------------

const BUDGET_CONFIG: &str = r#"
runners:
  executor:
    - { adapter: mock, model: mock-model }
limits:
  max_tokens_per_run: 100
"#;

/// A compliant session can never push the run past its cap — its own
/// equal-share `Budget` (etapa 3) stops it first — so the run-level
/// check's real scenario is an *overshoot*: one usage burst blows both
/// the session share and the whole run cap at once, the session is
/// killed, the node fails, and its `on_failure` re-route asks the
/// scheduler for more work while `spent >= cap`.
const BUDGET_WORKFLOW: &str = r#"
name: budget
nodes:
  - id: first
    kind: prompt
    runner: executor
    prompt: "Do the first thing."
    on_failure: { goto: fix, max_reroutes: 2 }
  - id: fix
    kind: prompt
    runner: executor
    prompt: "Fix it."
"#;

/// Session 1 bursts 200 tokens against a share of 50 (cap 100 across 2
/// nodes); sessions 2 and 3 (the corrective, then `first`'s retry) only
/// ever run if a human lets the run continue past the cap.
const BUDGET_FIXTURE: &str = r#"
sessions:
  - steps:
      - { type: usage, input_tokens: 150, output_tokens: 50 }
    outcome: { type: completed, summary: "spent a lot" }
  - outcome: { type: completed, summary: "fixed" }
  - outcome: { type: completed, summary: "did it" }
"#;

#[tokio::test]
async fn a_run_over_its_token_budget_pauses_with_reason_budget_when_headless() {
    let bench = Bench::new();
    let (terminal, state) = bench
        .run_with_config(BUDGET_WORKFLOW, BUDGET_FIXTURE, BUDGET_CONFIG)
        .await;

    match &terminal {
        RunTerminal::Paused { reason } => {
            assert!(reason.contains("budget"), "got: {reason}");
        }
        other => panic!("an exhausted budget with no surface must pause, got {other:?}"),
    }
    // The corrective node never started — the cap is checked before the
    // re-route hands it work.
    assert!(matches!(
        state.nodes.get("first"),
        Some(NodeState::Failed { .. })
    ));
    assert_eq!(state.nodes.get("fix"), None);
    // Unresolved: nothing recorded (resume re-asks, same convention as
    // every other gate).
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(!events.iter().any(|e| matches!(
        e.payload(),
        Some(yunta_core::events::EventPayload::GateWaiting(_))
    )));
}

#[tokio::test]
async fn authorizing_continue_lifts_the_cap_and_records_a_run_level_gate_pair() {
    let bench = Bench::new();
    let interaction = SequencedInteraction::choosing(&["continue"]);
    let (terminal, state) = bench
        .run_full(BUDGET_WORKFLOW, BUDGET_FIXTURE, BUDGET_CONFIG, &interaction)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    // The corrective ran and control returned to `first`, which finished
    // on its retry — all of it past the cap, under the one authorization.
    for node in ["first", "fix"] {
        assert!(
            matches!(state.nodes.get(node), Some(NodeState::Finished { .. })),
            "node `{node}` should be finished, got {:?}",
            state.nodes.get(node)
        );
    }

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let waiting = events
        .iter()
        .find(|e| {
            matches!(
                e.payload(),
                Some(yunta_core::events::EventPayload::GateWaiting(_))
            )
        })
        .expect("the budget escalation must be recorded");
    assert_eq!(
        waiting.node_id, None,
        "the budget gate belongs to the run, not to any node"
    );
    let resolved = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::GateResolved(p)) => Some((e.node_id.clone(), p)),
            _ => None,
        })
        .expect("the authorization must be recorded");
    assert_eq!(resolved.0, None);
    assert_eq!(resolved.1.chosen_option.as_deref(), Some("continue"));
}

#[tokio::test]
async fn choosing_abort_on_the_budget_escalation_pauses_with_the_decision_recorded() {
    let bench = Bench::new();
    let interaction = SequencedInteraction::choosing(&["abort"]);
    let (terminal, state) = bench
        .run_full(BUDGET_WORKFLOW, BUDGET_FIXTURE, BUDGET_CONFIG, &interaction)
        .await;

    match &terminal {
        RunTerminal::Paused { reason } => assert!(reason.contains("budget"), "got: {reason}"),
        other => panic!("abort must pause the run, got {other:?}"),
    }
    assert_eq!(state.nodes.get("fix"), None);
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(
        events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::GateResolved(p)) if p.chosen_option.as_deref() == Some("abort")
        )),
        "the abort decision must be auditable in the log"
    );
}

#[tokio::test]
async fn a_run_under_its_token_budget_never_escalates() {
    let bench = Bench::new();
    let config = r#"
runners:
  executor:
    - { adapter: mock, model: mock-model }
limits:
  max_tokens_per_run: 1000000
"#;
    let (terminal, _) = bench
        .run_with_config(BUDGET_WORKFLOW, BUDGET_FIXTURE, config)
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(!events.iter().any(|e| matches!(
        e.payload(),
        Some(yunta_core::events::EventPayload::GateWaiting(_))
    )));
}

#[tokio::test]
async fn budget_authorization_is_per_invocation_a_resume_asks_again() {
    // The first invocation pauses headless; the resume gets its own
    // "continue" — proving the ask happens per invocation and nothing in
    // the log pre-authorizes new spend.
    let bench = Bench::new();
    let workflow: yunta_core::Workflow = serde_yaml::from_str(BUDGET_WORKFLOW).unwrap();
    let config: yunta_core::ConfigLayer = serde_yaml::from_str(BUDGET_CONFIG).unwrap();
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();
    let adapter = MockAdapter::from_yaml(BUDGET_FIXTURE).unwrap();
    let mut adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
    adapters.insert("mock".into(), Arc::new(adapter));

    let first = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &adapters,
        storage: &bench.storage.async_handle(),
        clock: &FixedClock,
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
    })
    .await
    .unwrap();
    match &first.terminal {
        RunTerminal::Paused { reason } => assert!(reason.contains("budget"), "got: {reason}"),
        other => panic!("expected the headless invocation to pause, got {other:?}"),
    }

    let interaction = SequencedInteraction::choosing(&["continue"]);
    let resumed = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &adapters,
        storage: &bench.storage.async_handle(),
        clock: &FixedClock,
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &interaction,
        forge: None,
        cancel: None,
        adapter_override: None,
    })
    .await
    .unwrap();
    assert_eq!(resumed.terminal, RunTerminal::Finished);
}

#[test]
fn session_token_budget_is_an_equal_share_bounded_by_what_remains() {
    // Fresh run, 4 non-terminal nodes: each session gets cap/4.
    assert_eq!(yunta_engine::session_token_budget(1000, 0, 4), 250);
    // Late in the run, what actually remains is the bound.
    assert_eq!(yunta_engine::session_token_budget(1000, 900, 4), 100);
    // Overspent never underflows.
    assert_eq!(yunta_engine::session_token_budget(1000, 2000, 4), 0);
    // A degenerate node count never divides by zero.
    assert_eq!(yunta_engine::session_token_budget(1000, 0, 0), 1000);
}

// --- limits.max_loop_iterations ------------------------

/// Three sequential tasks at concurrency 1 need four loop iterations
/// (one per batch plus the closing empty-batch check) — a cap of 2 trips
/// mid-ledger.
const LOOP_CAP_WORKFLOW: &str = r#"
name: loop-cap
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
"#;

const LOOP_CAP_CONFIG: &str = r#"
runners:
  planner:
    - { adapter: mock, model: mock-model }
  executor:
    - { adapter: mock, model: mock-model }
limits:
  max_loop_iterations: 2
"#;

fn loop_cap_fixture(artifacts_dir: &std::path::Path) -> String {
    format!(
        r#"
sessions:
  - effects:
      - {{ path: "{artifacts}/plan.yaml", content: "tasks:\n  - id: T001\n    title: \"a\"\n    scope: [\"a.txt\"]\n    criteria:\n      - cmd: \"test -f a.txt\"\n  - id: T002\n    title: \"b\"\n    scope: [\"b.txt\"]\n    criteria:\n      - cmd: \"test -f b.txt\"\n    depends_on: [T001]\n  - id: T003\n    title: \"c\"\n    scope: [\"c.txt\"]\n    criteria:\n      - cmd: \"test -f c.txt\"\n    depends_on: [T002]\n" }}
    outcome: {{ type: completed, summary: "planned" }}
  - effects:
      - {{ path: a.txt, content: "a" }}
    outcome: {{ type: completed, summary: "did T001" }}
  - effects:
      - {{ path: b.txt, content: "b" }}
    outcome: {{ type: completed, summary: "did T002" }}
  - effects:
      - {{ path: c.txt, content: "c" }}
    outcome: {{ type: completed, summary: "did T003" }}
"#,
        artifacts = artifacts_dir.display()
    )
}

#[tokio::test]
async fn a_loop_over_its_iteration_cap_fails_with_the_limit_named_when_headless() {
    let bench = Bench::new();
    let fixture = loop_cap_fixture(&bench.run_dir().join("artifacts"));
    let (terminal, state) = bench
        .run_with_config(LOOP_CAP_WORKFLOW, &fixture, LOOP_CAP_CONFIG)
        .await;

    match &terminal {
        RunTerminal::Paused { reason } => assert!(
            reason.contains("max_loop_iterations"),
            "the pause must name the limit: {reason}"
        ),
        other => panic!("an exhausted iteration cap with no surface must pause, got {other:?}"),
    }
    assert!(matches!(
        state.nodes.get("implement"),
        Some(NodeState::Failed { .. })
    ));
    // T003 never ran: iteration 3 was refused, so it stays registered
    // but untouched.
    assert_eq!(
        state.tasks.get("T003"),
        Some(&yunta_core::events::TaskStatus::Pending)
    );
}

#[tokio::test]
async fn authorizing_continue_lifts_the_iteration_cap_for_this_invocation() {
    let bench = Bench::new();
    let fixture = loop_cap_fixture(&bench.run_dir().join("artifacts"));
    let interaction = SequencedInteraction::choosing(&["continue"]);
    let (terminal, state) = bench
        .run_full(LOOP_CAP_WORKFLOW, &fixture, LOOP_CAP_CONFIG, &interaction)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        state.tasks.get("T003"),
        Some(&yunta_core::events::TaskStatus::Done)
    );
    // One authorization covers the whole invocation — the script had a
    // single `continue`, and iterations 3 AND 4 both ran on it.
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let resolutions = events
        .iter()
        .filter(|e| {
            matches!(
                e.payload(),
                Some(yunta_core::events::EventPayload::GateResolved(_))
            )
        })
        .count();
    assert_eq!(resolutions, 1, "asked once, not once per iteration");
}

#[tokio::test]
async fn a_ledger_within_the_default_iteration_cap_runs_unasked() {
    // No `limits:` declared — the reference default (12) covers a
    // three-task ledger with room to spare, and nothing escalates.
    let bench = Bench::new();
    let fixture = loop_cap_fixture(&bench.run_dir().join("artifacts"));
    let (terminal, _) = bench.run(LOOP_CAP_WORKFLOW, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(!events.iter().any(|e| matches!(
        e.payload(),
        Some(yunta_core::events::EventPayload::GateWaiting(_))
    )));
}

// --- limits.inline_context_bytes -------------------------

/// A ~60-byte file source: inlined under the reference default (32000),
/// referenced by pointer when the configured threshold is below it.
const INLINE_CONTEXT_WORKFLOW: &str = r#"
name: inline-context
nodes:
  - id: ask
    kind: prompt
    runner: executor
    prompt: "Use the context above."
    context:
      - files: ["notes.txt"]
"#;

#[tokio::test]
async fn a_source_over_the_configured_inline_threshold_is_referenced_not_inlined() {
    let bench = Bench::new();
    std::fs::write(
        bench.worktree.join("notes.txt"),
        "MARKER-NOTES-CONTENT repeated enough to pass ten bytes",
    )
    .unwrap();
    let config = r#"
runners:
  executor:
    - { adapter: mock, model: mock-model }
limits:
  inline_context_bytes: 10
"#;
    // The only script matches the pointer wording — if the content were
    // inlined instead, no session would match and the node would fail.
    let fixture = "sessions:\n  - match_prompt_contains: \"bytes, referenced\"\n    outcome: { type: completed, summary: ok }\n";
    let (terminal, _) = bench
        .run_with_config(INLINE_CONTEXT_WORKFLOW, fixture, config)
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
}

#[tokio::test]
async fn a_source_under_the_default_inline_threshold_is_inlined() {
    let bench = Bench::new();
    std::fs::write(
        bench.worktree.join("notes.txt"),
        "MARKER-NOTES-CONTENT repeated enough to pass ten bytes",
    )
    .unwrap();
    // No `limits:` — the reference default (32000 bytes) inlines it.
    let fixture = "sessions:\n  - match_prompt_contains: \"MARKER-NOTES-CONTENT\"\n    outcome: { type: completed, summary: ok }\n";
    let (terminal, _) = bench.run(INLINE_CONTEXT_WORKFLOW, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);
}

// --- session events (agent_session_opened / agent_message) ------------

const SESSION_EVENTS_WORKFLOW: &str = r#"
name: session-events
nodes:
  - id: work
    kind: prompt
    runner: executor
    prompt: "Do the thing."
"#;

#[tokio::test]
async fn a_session_leaves_agent_session_opened_in_the_log_with_its_session_id() {
    let bench = Bench::new();
    let fixture = r#"
sessions:
  - outcome: { type: completed, summary: "done" }
"#;
    let (terminal, _) = bench.run(SESSION_EVENTS_WORKFLOW, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let opened = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::AgentSessionOpened(p)) => {
                Some((e.node_id.clone(), p.clone()))
            }
            _ => None,
        })
        .expect("the session must leave agent_session_opened in the log");
    assert_eq!(opened.0.as_ref().map(|n| n.as_str()), Some("work"));
    assert!(!opened.1.session_id.as_str().is_empty());
    assert_eq!(
        opened.1.model.as_ref().map(|model| model.as_str()),
        Some("mock-model")
    );
}

#[tokio::test]
async fn agent_messages_are_bounded_summaries_that_never_carry_note_content() {
    let bench = Bench::new();
    let fixture = r#"
sessions:
  - steps:
      - { type: note, text: "thinking about SECRET-TOKEN-123 carefully" }
      - { type: usage, input_tokens: 40, output_tokens: 10 }
      - { type: tool_use, name: edit, target_digest: abc123 }
    outcome: { type: completed, summary: "done" }
"#;
    let (terminal, _) = bench.run(SESSION_EVENTS_WORKFLOW, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let messages: Vec<&yunta_core::events::AgentMessagePayload> = events
        .iter()
        .filter_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::AgentMessage(p)) => Some(p),
            _ => None,
        })
        .collect();
    assert_eq!(messages.len(), 3, "one agent_message per adapter event");

    // Never the content — a mechanical size+digest summary only.
    let jsonl = serde_json::to_string(&messages).unwrap();
    assert!(
        !jsonl.contains("SECRET-TOKEN-123"),
        "note content must never be persisted: {jsonl}"
    );
    let note = messages
        .iter()
        .find(|m| m.message_type == yunta_core::events::AgentMessageType::Note)
        .unwrap();
    let summary = note.text.as_deref().unwrap_or_default();
    assert!(summary.contains("bytes"), "got: {summary}");

    let usage = messages
        .iter()
        .find(|m| m.message_type == yunta_core::events::AgentMessageType::Usage)
        .unwrap();
    assert_eq!(usage.input_tokens, Some(40));
    assert_eq!(usage.output_tokens, Some(10));

    let tool = messages
        .iter()
        .find(|m| m.message_type == yunta_core::events::AgentMessageType::ToolUse)
        .unwrap();
    assert_eq!(tool.tool_name.as_deref(), Some("edit"));
    assert_eq!(tool.target_digest.as_deref(), Some("abc123"));
}

// --- loop/check cancellation under join: any --------------------------

#[tokio::test]
async fn a_join_any_race_cancels_a_slow_loop_child_when_a_sibling_wins() {
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");

    let workflow = r#"
name: race-loop
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the ledger to {{run.dir}}/artifacts/plan.yaml."
    artifacts:
      produces:
        - { name: plan.yaml, kind: task-ledger }
  - id: race
    kind: parallel
    depends_on: [plan]
    join: any
    nodes:
      - id: quick
        kind: bash
        run: "true"
      - id: slow-loop
        kind: loop
        runner: executor
        until: all_tasks_complete
        prompt: "Implement your task."
"#;

    // The task session stalls 4s before doing anything — far longer than
    // `quick` needs to win. Without cancellation the loop would sit out
    // the whole delay and finish anyway.
    let fixture = format!(
        r#"
sessions:
  - effects:
      - {{ path: "{artifacts}/plan.yaml", content: "tasks:\n  - id: T001\n    title: \"slow\"\n    scope: [\"slow.txt\"]\n    criteria:\n      - cmd: \"test -f slow.txt\"\n" }}
    outcome: {{ type: completed, summary: "planned" }}
  - steps:
      - {{ type: note, text: "stalling", after_ms: 4000 }}
    effects:
      - {{ path: slow.txt, content: "slow" }}
    outcome: {{ type: completed, summary: "did T001" }}
"#,
        artifacts = artifacts_dir.display()
    );

    let started = std::time::Instant::now();
    let (terminal, state) = bench.run(workflow, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(3),
        "the loser must die with the race, not sit out its stall: {:?}",
        started.elapsed()
    );

    assert!(matches!(
        state.nodes.get("race"),
        Some(NodeState::Finished { .. })
    ));
    match state.nodes.get("slow-loop") {
        Some(NodeState::Failed { outcome, .. }) => {
            assert!(outcome.contains("interrupted"), "got: {outcome}");
        }
        other => panic!("the losing loop must be recorded interrupted, got {other:?}"),
    }
}

#[tokio::test]
async fn a_join_any_race_cancels_a_slow_check_child_when_a_sibling_wins() {
    let bench = Bench::new();
    let config = r#"
runners:
  executor:
    - { adapter: mock, model: mock-model }
baseline:
  suite: "sleep 4"
"#;
    let workflow = r#"
name: race-check
nodes:
  - id: race
    kind: parallel
    join: any
    nodes:
      - id: quick
        kind: bash
        run: "true"
      - id: slow-check
        kind: check
        builtin: baseline_compare
"#;

    let started = std::time::Instant::now();
    let (terminal, state) = bench
        .run_with_config(workflow, "sessions: []\n", config)
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(3),
        "the check must die with the race: {:?}",
        started.elapsed()
    );
    match state.nodes.get("slow-check") {
        Some(NodeState::Failed { outcome, .. }) => {
            assert!(outcome.contains("interrupted"), "got: {outcome}");
        }
        other => panic!("the losing check must be recorded interrupted, got {other:?}"),
    }
}

// --- skills chain (resolution → SessionRequest → degradation) ---------

const SKILLS_CONFIG: &str = r#"
runners:
  executor:
    - { adapter: mock, model: mock-model }
skills:
  paths: [.yunta/skills]
  always: [conventions]
"#;

const SKILLS_WORKFLOW: &str = r#"
name: skilled
nodes:
  - id: work
    kind: prompt
    runner: executor
    skills: [grill]
    prompt: "Do the thing."
"#;

fn install_skill(worktree: &std::path::Path, name: &str) {
    let dir = worktree.join(".yunta/skills").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("SKILL.md"), format!("# {name}\n")).unwrap();
}

/// Runs one workflow with a hand-held mock so the test can ask it what
/// skills each spawn carried.
async fn run_with_recording_mock(
    bench: &Bench,
    workflow_yaml: &str,
    fixture_yaml: &str,
    config_yaml: &str,
) -> (RunTerminal, yunta_engine::RunState, Arc<MockAdapter>) {
    let workflow: Workflow = serde_yaml::from_str(workflow_yaml).unwrap();
    let config: ConfigLayer = serde_yaml::from_str(config_yaml).unwrap();
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();
    let adapter = Arc::new(MockAdapter::from_yaml(fixture_yaml).unwrap());
    let mut adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
    adapters.insert("mock".into(), adapter.clone());
    let report = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &adapters,
        storage: &bench.storage.async_handle(),
        clock: &FixedClock,
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
    })
    .await
    .unwrap();
    (report.terminal, report.state, adapter)
}

#[tokio::test]
async fn resolved_skills_reach_the_session_request_always_first() {
    let bench = Bench::new();
    install_skill(&bench.worktree, "conventions");
    install_skill(&bench.worktree, "grill");
    let fixture = r#"
capabilities: { skills: true }
sessions:
  - outcome: { type: completed, summary: "done" }
"#;
    let (terminal, _, adapter) =
        run_with_recording_mock(&bench, SKILLS_WORKFLOW, fixture, SKILLS_CONFIG).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let seen = adapter.skills_seen();
    assert_eq!(seen.len(), 1);
    assert_eq!(
        seen[0],
        vec![
            bench.worktree.join(".yunta/skills/conventions"),
            bench.worktree.join(".yunta/skills/grill"),
        ],
        "skills.always mounts first, then the node's own list"
    );
}

#[tokio::test]
async fn a_missing_skill_name_fails_the_node_with_where_it_looked() {
    let bench = Bench::new();
    install_skill(&bench.worktree, "conventions");
    // `grill` is never installed.
    let fixture = r#"
capabilities: { skills: true }
sessions:
  - outcome: { type: completed, summary: "never reached" }
"#;
    let (terminal, state, adapter) =
        run_with_recording_mock(&bench, SKILLS_WORKFLOW, fixture, SKILLS_CONFIG).await;
    match &terminal {
        RunTerminal::Paused { reason } => {
            assert!(reason.contains("grill"), "got: {reason}");
            assert!(reason.contains("skills.paths"), "got: {reason}");
        }
        other => panic!("a missing skill must fail the node, got {other:?}"),
    }
    assert!(matches!(
        state.nodes.get("work"),
        Some(NodeState::Failed { .. })
    ));
    assert!(adapter.skills_seen().is_empty(), "no session was spawned");
}

#[tokio::test]
async fn an_adapter_without_the_skills_capability_degrades_with_an_event() {
    let bench = Bench::new();
    install_skill(&bench.worktree, "conventions");
    install_skill(&bench.worktree, "grill");
    // Default capabilities: `skills: false`.
    let fixture = r#"
sessions:
  - outcome: { type: completed, summary: "done" }
"#;
    let (terminal, _, adapter) =
        run_with_recording_mock(&bench, SKILLS_WORKFLOW, fixture, SKILLS_CONFIG).await;
    assert_eq!(
        terminal,
        RunTerminal::Finished,
        "a skill is not correctness"
    );

    assert_eq!(
        adapter.skills_seen(),
        vec![Vec::<std::path::PathBuf>::new()],
        "the engine never populates skills an adapter didn't declare"
    );
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let degraded = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::CapabilityDegraded(p)) => Some(p),
            _ => None,
        })
        .expect("the degradation must be an event, never silence");
    assert_eq!(degraded.capability, "skills");
    assert_eq!(degraded.adapter, "mock");
}

// --- on_finish.distill — deterministic knowledge distillation ---------

const DISTILL_WORKFLOW: &str = r#"
name: distiller
nodes:
  - id: plan
    kind: prompt
    runner: executor
    prompt: "Write the plan to {{run.dir}}/artifacts/plan.md."
    artifacts:
      produces: [plan.md]
on_finish:
  - distill: [plan.md]
"#;

fn distill_fixture(artifacts_dir: &std::path::Path) -> String {
    format!(
        r#"
sessions:
  - effects:
      - {{ path: "{artifacts}/plan.md", content: "DISTILLED-MARKER: the durable decision\n" }}
    outcome: {{ type: completed, summary: "planned" }}
"#,
        artifacts = artifacts_dir.display()
    )
}

#[tokio::test]
async fn distill_copies_declared_artifacts_with_provenance_and_commits() {
    let bench = Bench::new();
    let fixture = distill_fixture(&bench.run_dir().join("artifacts"));
    let (terminal, _) = bench.run(DISTILL_WORKFLOW, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let dest = bench
        .worktree
        .join(".yunta/knowledge/distilled/distiller")
        .join(bench.run_id.as_str());
    let copied = std::fs::read_to_string(dest.join("plan.md")).expect("the artifact must land");
    assert!(copied.contains("DISTILLED-MARKER"));

    let provenance: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(dest.join("provenance.yaml")).unwrap())
            .unwrap();
    assert_eq!(
        provenance["source_run"].as_str(),
        Some(bench.run_id.as_str())
    );
    assert_eq!(provenance["workflow"].as_str(), Some("distiller"));
    assert!(provenance["artifacts"][0]["content_hash"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
    assert_eq!(
        provenance["verification"]["findings"]["blocking"].as_u64(),
        Some(0)
    );

    // The knowledge travels on the run's own branch: a conventional
    // commit exists in the worktree.
    let log = std::process::Command::new("git")
        .args(["log", "--oneline", "-3"])
        .current_dir(&bench.worktree)
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&log.stdout);
    assert!(
        log.contains(&format!("docs(knowledge): distill from {}", bench.run_id)),
        "got: {log}"
    );
}

#[tokio::test]
async fn a_distill_path_never_produced_becomes_a_finding_and_the_rest_lands() {
    let bench = Bench::new();
    let workflow = r#"
name: distiller
nodes:
  - id: plan
    kind: prompt
    runner: executor
    prompt: "Write the plan to {{run.dir}}/artifacts/plan.md."
    artifacts:
      produces: [plan.md]
  - id: notes
    kind: prompt
    runner: executor
    depends_on: [plan]
    prompt: "Maybe write notes."
    artifacts:
      produces: [notes.md]
on_finish:
  - distill: [plan.md, notes.md]
"#;
    // `notes` fails before producing its artifact — but with a re-route
    // budget of zero the run pauses... instead: notes produces, then we
    // delete it? Simpler: notes' fixture writes the artifact and the run
    // finishes, then this test only covers the produced path. The
    // missing-path case uses a workflow whose declared artifact the
    // session legitimately produced but distill names one more — which
    // check would refuse. So: simulate runtime-missing by removing the
    // file after the run? No — distill runs inside execute_run. The
    // honest runtime-missing case: `notes` is mode-excluded.
    let workflow = workflow.replace(
        "on_finish:",
        "modes:\n  quick: { include: [plan] }\n  full: { include: all }\non_finish:",
    );
    let artifacts = bench.run_dir().join("artifacts");
    let fixture = format!(
        r#"
sessions:
  - effects:
      - {{ path: "{artifacts}/plan.md", content: "plan\n" }}
    outcome: {{ type: completed, summary: "planned" }}
"#,
        artifacts = artifacts.display()
    );
    // Run in `quick` mode: `notes` never runs, its artifact never
    // exists, but distill declares it.
    let workflow_parsed: Workflow = serde_yaml::from_str(&workflow).unwrap();
    let config: ConfigLayer = serde_yaml::from_str(CONFIG).unwrap();
    let manifest = build_manifest(
        &workflow_parsed,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"quick".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();
    let adapter = MockAdapter::from_yaml(&fixture).unwrap();
    let mut adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
    adapters.insert("mock".into(), Arc::new(adapter));
    let report = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &adapters,
        storage: &bench.storage.async_handle(),
        clock: &FixedClock,
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
    })
    .await
    .unwrap();
    assert_eq!(report.terminal, RunTerminal::Finished);

    let dest = bench
        .worktree
        .join(".yunta/knowledge/distilled/distiller")
        .join(bench.run_id.as_str());
    assert!(dest.join("plan.md").exists(), "the produced path lands");
    assert!(!dest.join("notes.md").exists());

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let finding = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::FindingPosted(p)) => Some(&p.finding),
            _ => None,
        })
        .expect("the missing path must become a finding, never be lost");
    assert!(finding.title.contains("notes.md"), "got: {finding:?}");
    assert_eq!(finding.severity, yunta_core::events::FindingSeverity::Minor);

    let provenance: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(dest.join("provenance.yaml")).unwrap())
            .unwrap();
    let listed: Vec<&str> = provenance["artifacts"]
        .as_sequence()
        .unwrap()
        .iter()
        .map(|a| a["name"].as_str().unwrap())
        .collect();
    assert_eq!(listed, vec!["plan.md", "notes.md"]);
    assert_eq!(provenance["artifacts"][1]["missing"].as_bool(), Some(true));
}

#[tokio::test]
async fn a_paused_run_distills_nothing() {
    let bench = Bench::new();
    let workflow = r#"
name: distiller
nodes:
  - id: plan
    kind: prompt
    runner: executor
    prompt: "Write the plan to {{run.dir}}/artifacts/plan.md."
    artifacts:
      produces: [plan.md]
  - id: boom
    kind: bash
    depends_on: [plan]
    run: "false"
on_finish:
  - distill: [plan.md]
"#;
    let fixture = distill_fixture(&bench.run_dir().join("artifacts"));
    let (terminal, _) = bench.run(workflow, &fixture).await;
    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    assert!(
        !bench.worktree.join(".yunta/knowledge").exists(),
        "a paused run did not close — nothing distills"
    );
}

#[tokio::test]
async fn a_later_run_mounts_the_distilled_knowledge() {
    let bench = Bench::new();
    let fixture = distill_fixture(&bench.run_dir().join("artifacts"));
    let (terminal, _) = bench.run(DISTILL_WORKFLOW, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    // Second run, same checkout: a knowledge context source must see
    // the distilled file — the loop closed, from budget limit to knowledge layering.
    let second_workflow = r#"
name: consumer
nodes:
  - id: ask
    kind: prompt
    runner: executor
    context:
      - knowledge: {}
    prompt: "Use what the team learned."
"#;
    let second_fixture = r#"
sessions:
  - match_prompt_contains: "DISTILLED-MARKER"
    outcome: { type: completed, summary: "informed" }
"#;
    let workflow: Workflow = serde_yaml::from_str(second_workflow).unwrap();
    let config: ConfigLayer = serde_yaml::from_str(CONFIG).unwrap();
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let second_id = RunId::from("run-test-2");
    let run_dir = create_run(
        CreateRunParams {
            run_id: &second_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();
    let adapter = MockAdapter::from_yaml(second_fixture).unwrap();
    let mut adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
    adapters.insert("mock".into(), Arc::new(adapter));
    let report = execute_run(RunEnv {
        run_id: &second_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &adapters,
        storage: &bench.storage.async_handle(),
        clock: &FixedClock,
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
    })
    .await
    .unwrap();
    assert_eq!(
        report.terminal,
        RunTerminal::Finished,
        "the consumer session only matches if the distilled content reached its prompt"
    );
}

// --- runners fan-out end-to-end + node-level agent ---------------------

#[tokio::test]
async fn a_fanout_review_runs_one_session_per_role_with_rendered_artifacts() {
    let bench = Bench::new();
    let workflow = r#"
name: fanout
nodes:
  - id: review
    kind: prompt
    runners: [reviewer, reviewer-alt]
    prompt: "Audit as {{runner.role}}; write {{run.dir}}/artifacts/findings-{{runner.role}}.md"
    artifacts:
      produces: ["findings-{{runner.role}}.md"]
"#;
    let config = r#"
runners:
  reviewer:
    - { adapter: mock, model: mock-model }
  reviewer-alt:
    - { adapter: mock, model: mock-model }
"#;
    let artifacts = bench.run_dir().join("artifacts");
    let fixture = format!(
        r#"
sessions:
  - match_prompt_contains: "Audit as reviewer;"
    effects:
      - {{ path: "{artifacts}/findings-reviewer.md", content: "r1\n" }}
    outcome: {{ type: completed, summary: "reviewed" }}
  - match_prompt_contains: "Audit as reviewer-alt"
    effects:
      - {{ path: "{artifacts}/findings-reviewer-alt.md", content: "r2\n" }}
    outcome: {{ type: completed, summary: "reviewed-alt" }}
"#,
        artifacts = artifacts.display()
    );

    let (terminal, state) = bench.run_with_config(workflow, &fixture, config).await;
    assert_eq!(terminal, RunTerminal::Finished);
    for node in ["review@reviewer", "review@reviewer-alt"] {
        assert!(
            matches!(state.nodes.get(node), Some(NodeState::Finished { .. })),
            "node `{node}` should be finished, got {:?}",
            state.nodes.get(node)
        );
    }
    // The templated artifact names rendered per expanded node.
    assert!(artifacts.join("findings-reviewer.md").exists());
    assert!(artifacts.join("findings-reviewer-alt.md").exists());
}

#[tokio::test]
async fn a_node_level_agent_overrides_the_runner_candidate_s_agent() {
    let bench = Bench::new();
    let workflow = r#"
name: agent-override
nodes:
  - id: audit
    kind: prompt
    runner: reviewer
    agent: security-auditor
    prompt: "Audit."
"#;
    let config = r#"
runners:
  reviewer:
    - { adapter: mock, model: mock-model, agent: benito }
"#;
    let fixture = r#"
capabilities: { custom_agents: true }
sessions:
  - outcome: { type: completed, summary: "audited" }
"#;
    let (terminal, _, adapter) = run_with_recording_mock(&bench, workflow, fixture, config).await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        adapter.agents_seen(),
        vec![Some("security-auditor".into())],
        "the node's own agent wins over the candidate's"
    );
}

#[tokio::test]
async fn max_per_run_holds_exactly_under_a_fully_concurrent_batch() {
    // Four tasks in ONE batch (`concurrency: 4`) all request an
    // expansion under `rules` with `max_per_run: 2`. The cap window
    // (read count → decide → commit) is atomic across the batch, so the
    // count is deterministic — exactly 2 granted, 2 escalated — never
    // "up to concurrency - 1 over".
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");

    let workflow = r#"
name: capped-concurrency
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
    concurrency: 4
    prompt: "Read your task from the ledger and implement it."
    scope_expansion:
      mode: rules
      within: ["extra-*.txt"]
      max_per_run: 2
"#;
    let mut ledger = String::from("tasks:\n");
    for n in 1..=4 {
        ledger.push_str(&task_yaml(
            &format!("task-{n}"),
            &format!("t{n}"),
            &format!("a{n}.txt"),
            &format!("test -f a{n}.txt"),
        ));
    }

    let mut fixture = plan_session(&artifacts_dir, &ledger);
    for n in 1..=4 {
        let request_yaml = format!(
            "paths:\n  - extra-{n}.txt\nreason: \"needs the extra file\"\nproposed_criterion:\n  cmd: \"test -f extra-{n}.txt\"\n"
        );
        fixture.push_str(&format!(
            "  - match_prompt_contains: \"task-{n}\"\n    effects:\n      - {{ path: a{n}.txt, content: \"a\" }}\n      - {{ path: {:?}, content: {:?} }}\n    outcome: {{ type: completed, summary: did-{n} }}\n",
            yunta_engine::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE,
            request_yaml,
        ));
    }

    let (_terminal, _state) = bench.run(workflow, &fixture).await;

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let requested = events
        .iter()
        .filter(|e| {
            matches!(
                e.payload(),
                Some(yunta_core::events::EventPayload::ScopeExpansionRequested(_))
            )
        })
        .count();
    let granted = events
        .iter()
        .filter(|e| {
            matches!(
                e.payload(),
                Some(yunta_core::events::EventPayload::ScopeExpansionGranted(_))
            )
        })
        .count();
    assert_eq!(
        requested, 4,
        "every batch member's request must be recorded"
    );
    assert_eq!(
        granted, 2,
        "max_per_run: 2 must hold exactly under a concurrent batch"
    );
}

// --- `context:` at loop level ----------------------------------------

#[tokio::test]
async fn a_loop_s_context_reaches_every_task_s_brief() {
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");
    std::fs::write(bench.worktree.join("notes.md"), "the-shared-notes").unwrap();

    let workflow = r#"
name: loop-context
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
    context:
      - files: ["notes.md"]
    prompt: "Read your task from the ledger and implement it."
"#;
    let ledger = format!(
        "tasks:\n{}{}",
        task_yaml("task-1", "one", "one.txt", "test -f one.txt"),
        task_yaml("task-2", "two", "two.txt", "test -f two.txt"),
    );

    // The executor sessions only match if their prompt actually carries
    // the context block's content — a brief without it dispatches no
    // session and the run fails, so a Finished terminal IS the proof.
    let mut fixture = plan_session(&artifacts_dir, &ledger);
    for n in 1..=2 {
        let file = if n == 1 { "one.txt" } else { "two.txt" };
        fixture.push_str(&format!(
            "  - match_prompt_contains: \"the-shared-notes\"\n    effects:\n      - {{ path: {file}, content: \"x\" }}\n    outcome: {{ type: completed, summary: did-{n} }}\n",
        ));
    }

    let (terminal, _state) = bench.run(workflow, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    // One `context_assembled` per task brief, each naming its task.
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let assembled_tasks: Vec<String> = events
        .iter()
        .filter_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::ContextAssembled(p)) => Some(
                p.task_id
                    .as_ref()
                    .map(|t| t.to_string())
                    .unwrap_or_default(),
            ),
            _ => None,
        })
        .collect();
    let mut sorted = assembled_tasks.clone();
    sorted.sort();
    assert_eq!(
        sorted,
        vec!["task-1".to_string(), "task-2".to_string()],
        "one context_assembled per task, each carrying its task_id: {assembled_tasks:?}"
    );
}

// --- events.jsonl on the Broken path ----------------------------------

#[tokio::test]
async fn a_broken_log_still_exports_events_jsonl_for_forensics() {
    let bench = Bench::new();
    let workflow = r#"
name: broken
nodes:
  - id: build
    kind: bash
    run: "true"
"#;
    // Corrupt the log by hand: a node_finished with no node_started —
    // exactly the class of inconsistency `derive` refuses to guess over.
    let wf: Workflow = serde_yaml::from_str(workflow).unwrap();
    let config: ConfigLayer = serde_yaml::from_str(CONFIG).unwrap();
    let manifest = build_manifest(
        &wf,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();
    bench
        .storage
        .append(
            &yunta_core::events::EventDraft {
                run_id: bench.run_id.clone(),
                node_id: Some("ghost".into()),
                payload: yunta_core::events::EventPayload::NodeFinished(
                    yunta_core::events::NodeFinishedPayload {
                        outcome: "??".to_string(),
                        tokens_used: yunta_core::events::TokenUsage::default(),
                    },
                ),
            },
            &yunta_core::SystemClock,
        )
        .unwrap();

    let adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
    let result = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &adapters,
        storage: &bench.storage.async_handle(),
        clock: &FixedClock,
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
    })
    .await;

    assert!(
        matches!(result, Err(yunta_engine::RunError::Broken { .. })),
        "a corrupt log is a Broken error, got {result:?}"
    );
    // The corrupt log is exactly the one you most want exported — the
    // forensic copy exists even though the run errored.
    let exported = std::fs::read_to_string(run_dir.join("events.jsonl")).unwrap();
    assert!(exported.contains("node_finished"));
    assert!(exported.contains("run_created"));
}

// --- on_interrupt: resume_session -------------------------------------

/// Crafts an interrupted run: `run_created` + a `node_started` (and
/// optionally an open `agent_session_opened`) with no terminal event —
/// exactly what a mid-session crash leaves — then resumes it with a
/// recording mock.
async fn resume_orphan_with_mock(
    workflow_yaml: &str,
    fixture_yaml: &str,
    orphan_session: Option<&str>,
) -> (
    RunTerminal,
    Vec<yunta_core::events::StoredEvent>,
    Arc<MockAdapter>,
) {
    let bench = Bench::new();
    let workflow: Workflow = serde_yaml::from_str(workflow_yaml).unwrap();
    let config: ConfigLayer = serde_yaml::from_str(CONFIG).unwrap();
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();
    let emit = |node: &str, payload: yunta_core::events::EventPayload| {
        bench
            .storage
            .append(
                &yunta_core::events::EventDraft {
                    run_id: bench.run_id.clone(),
                    node_id: Some(node.into()),
                    payload,
                },
                &yunta_core::SystemClock,
            )
            .unwrap();
    };
    emit(
        "work",
        yunta_core::events::EventPayload::NodeStarted(yunta_core::events::NodeStartedPayload {
            attempt: 1,
        }),
    );
    if let Some(session_id) = orphan_session {
        emit(
            "work",
            yunta_core::events::EventPayload::AgentSessionOpened(
                yunta_core::events::AgentSessionOpenedPayload {
                    session_id: session_id.into(),
                    agent: None,
                    model: Some("mock-model".into()),
                    capabilities: yunta_core::Capabilities::default(),
                },
            ),
        );
    }

    let adapter = Arc::new(MockAdapter::from_yaml(fixture_yaml).unwrap());
    let mut adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
    adapters.insert("mock".into(), adapter.clone());
    let report = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &bench.runs_root.join(bench.run_id.as_str()),
        worktree: &bench.worktree,
        adapters: &adapters,
        storage: &bench.storage.async_handle(),
        clock: &FixedClock,
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
    })
    .await
    .unwrap();
    let _ = run_dir;
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    (report.terminal, events, adapter)
}

const RESUME_WORKFLOW: &str = r#"
name: resumable
nodes:
  - id: work
    kind: prompt
    runner: executor
    on_interrupt: resume_session
    prompt: "Do the thing."
"#;

#[tokio::test]
async fn an_orphaned_prompt_with_resume_session_continues_the_same_session() {
    let fixture = r#"
capabilities: { resume_session: true }
sessions:
  - outcome: { type: completed, summary: "picked up where it left off" }
"#;
    let (terminal, events, adapter) =
        resume_orphan_with_mock(RESUME_WORKFLOW, fixture, Some("mock-session-orig")).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        adapter.resumes_seen(),
        vec![yunta_core::SessionId::from("mock-session-orig")],
        "the cut session must be resumed, not replaced"
    );
    assert!(
        !events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::CapabilityDegraded(p)) if p.capability == "resume_session"
        )),
        "a successful resume degrades nothing"
    );
}

#[tokio::test]
async fn resume_session_without_the_capability_degrades_to_restart_with_an_event() {
    let fixture = r#"
sessions:
  - outcome: { type: completed, summary: "fresh session" }
"#;
    let (terminal, events, adapter) =
        resume_orphan_with_mock(RESUME_WORKFLOW, fixture, Some("mock-session-orig")).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(adapter.resumes_seen().is_empty());
    assert!(
        events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::CapabilityDegraded(p)) if p.capability == "resume_session"
                    && p.policy_applied.contains("restart_node")
        )),
        "degrading to a fresh session must be an event, never a silence"
    );
}

#[tokio::test]
async fn resume_session_with_no_recorded_session_restarts_with_an_event() {
    let fixture = r#"
capabilities: { resume_session: true }
sessions:
  - outcome: { type: completed, summary: "fresh session" }
"#;
    let (terminal, events, adapter) = resume_orphan_with_mock(RESUME_WORKFLOW, fixture, None).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(adapter.resumes_seen().is_empty());
    assert!(
        events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::CapabilityDegraded(p)) if p.capability == "resume_session"
                    && p.policy_applied.contains("no session")
        )),
        "a crash before the session opened restarts WITH an explicit event"
    );
}

#[tokio::test]
async fn a_clean_first_run_under_resume_session_spawns_normally_without_events() {
    let fixture = r#"
capabilities: { resume_session: true }
sessions:
  - outcome: { type: completed, summary: "first run" }
"#;
    let bench = Bench::new();
    let (terminal, _state, adapter) =
        run_with_recording_mock(&bench, RESUME_WORKFLOW, fixture, CONFIG).await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert!(adapter.resumes_seen().is_empty());
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(!events.iter().any(|e| matches!(
        e.payload(),
        Some(yunta_core::events::EventPayload::CapabilityDegraded(_))
    )));
}

// --- a run is born once, whole -----------------------------------------------

struct BirthBench {
    _root: tempfile::TempDir,
    runs_root: std::path::PathBuf,
    storage: Storage,
    manifest: yunta_core::Manifest,
}

impl BirthBench {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let worktree = root.path().join("worktree");
        std::fs::create_dir_all(&worktree).unwrap();
        init_repo(&worktree);
        let storage = Storage::open(&root.path().join("yunta.db")).unwrap();
        let workflow: Workflow = serde_yaml::from_str(
            "name: birth\nnodes:\n  - id: a\n    kind: bash\n    run: \"true\"\n",
        )
        .unwrap();
        let manifest = build_manifest(
            &workflow,
            &ConfigLayer::default(),
            &worktree,
            &worktree,
            &HashMap::new(),
        )
        .unwrap();
        BirthBench {
            runs_root: root.path().join("runs"),
            _root: root,
            storage,
            manifest,
        }
    }

    async fn create(
        &self,
        run_id: &RunId,
        artifacts: &[yunta_engine::BirthArtifact],
    ) -> Result<std::path::PathBuf, yunta_engine::RunError> {
        create_run(
            CreateRunParams {
                run_id,
                manifest: &self.manifest,
                runs_root: &self.runs_root,
                mode: &"default".into(),
                promoted_from: None,
                artifacts,
            },
            &self.storage.async_handle(),
            &FixedClock,
        )
        .await
    }
}

#[tokio::test]
async fn create_run_refuses_an_existing_run_dir() {
    let bench = BirthBench::new();
    let run_id = RunId::from("run-once");
    bench.create(&run_id, &[]).await.unwrap();

    let error = bench.create(&run_id, &[]).await.unwrap_err();
    assert!(
        matches!(
            &error,
            yunta_engine::RunError::RunDirExists { path } if *path == bench.runs_root.join("run-once")
        ),
        "{error:?}"
    );
    // Refused before anything is written: the log still holds one birth.
    assert_eq!(bench.storage.events_for_run(&run_id).unwrap().len(), 1);
}

#[tokio::test]
async fn create_run_writes_the_birth_artifacts_before_the_run_exists_in_the_log() {
    let bench = BirthBench::new();
    let run_id = RunId::from("run-with-brief");
    let artifacts = vec![yunta_engine::BirthArtifact {
        name: "brief/plan.md".to_string(),
        bytes: b"hello".to_vec(),
    }];

    let run_dir = bench.create(&run_id, &artifacts).await.unwrap();

    assert_eq!(
        std::fs::read(run_dir.join("artifacts").join("brief").join("plan.md")).unwrap(),
        b"hello"
    );
    let events = bench.storage.events_for_run(&run_id).unwrap();
    assert!(
        matches!(
            events.as_slice(),
            [only] if matches!(only.payload(), Some(yunta_core::events::EventPayload::RunCreated(_)))
        ),
        "the birth is one run_created after the directory is complete"
    );
}

#[tokio::test]
async fn run_created_freezes_resolved_inputs() {
    let bench = Bench::new();
    let workflow: Workflow = serde_yaml::from_str(
        r#"
name: with-inputs
inputs:
  idea:
    type: string
    required: true
  greeting:
    type: string
    default: hola
nodes:
  - id: only
    kind: bash
    run: "true"
"#,
    )
    .unwrap();
    let config: ConfigLayer = serde_yaml::from_str(CONFIG).unwrap();
    let provided = HashMap::from([("idea".to_string(), "ship it".to_string())]);
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &provided,
    )
    .unwrap();
    create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let created = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::RunCreated(p)) => Some(p.clone()),
            _ => None,
        })
        .expect("run_created is the first event");
    let frozen: std::collections::BTreeMap<String, serde_json::Value> = manifest
        .inputs
        .iter()
        .map(|(name, value)| (name.clone(), serde_json::Value::String(value.clone())))
        .collect();
    assert_eq!(
        created.inputs, frozen,
        "run_created carries every input the manifest froze, the default included"
    );
    assert_eq!(created.inputs["greeting"], "hola");
    assert_eq!(created.inputs["idea"], "ship it");
}
