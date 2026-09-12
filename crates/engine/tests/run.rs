//! End-to-end runs with the mock adapter: the bootstrap
//! shape — a prompt plan node that produces the tasks document, a loop that
//! implements it task by task, a bash gate — plus re-routes, hooks,
//! findings and the birth of a run, all derived from the event log alone.

use std::collections::HashMap;

use yunta_core::{ConfigLayer, RunId, Workflow};
use yunta_engine::{
    build_manifest, create_run, execute_run, CreateRunParams, NoInteraction, NodeState, RunEnv,
    RunTerminal, DEFAULT_MAX_RETRIES,
};
use yunta_storage::Storage;
use yunta_testkit::{init_repo, Bench, FixedClock, MOCK_CONFIG};

mod common;
use common::*;

#[tokio::test]
async fn the_bootstrap_shape_runs_end_to_end_plan_loop_and_gate() {
    let bench = Bench::new();

    let workflow = r#"
name: bootstrap
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the tasks document."
    artifacts:
      produces:
        - { name: plan.yaml, kind: tasks }
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Read your task from the tasks document and implement it."
  - id: verify
    kind: bash
    depends_on: [implement]
    run: "test -f hello.txt && test -f world.txt"
"#;

    // Session 1 is the planner: it hands the tasks document over through its run
    // tools. Sessions 2 and 3 are one executor session per task, each
    // writing the file its task is scoped to.
    let fixture = r#"
capabilities: { run_tools: true }
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_tasks
        arguments:
          name: plan.yaml
          document:
            tasks:
              - id: T001
                title: "Create hello"
                scope: ["hello.txt"]
                criteria:
                  - cmd: "test -f hello.txt"
              - id: T002
                title: "Create world"
                scope: ["world.txt"]
                criteria:
                  - cmd: "test -f world.txt"
                depends_on: [T001]
    outcome: { type: completed, summary: "planned" }
  - effects:
      - { path: hello.txt, content: "hello" }
    steps:
      - { type: usage, input_tokens: 100, output_tokens: 20 }
    outcome: { type: completed, summary: "did T001" }
  - effects:
      - { path: world.txt, content: "world" }
    steps:
      - { type: usage, input_tokens: 80, output_tokens: 10 }
    outcome: { type: completed, summary: "did T002" }
"#;

    let (terminal, state) = bench.run(workflow, fixture).await;

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
            assert_eq!(
                reason,
                "node `lint` failed and its 1 re-route(s) to `fix-lint` are exhausted: exit 1: "
            );
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
        RunTerminal::Paused { reason } => assert_eq!(reason, "node `build` failed: exit 3: "),
        other => panic!("expected Paused, got {other:?}"),
    }
    assert!(matches!(
        state.nodes.get("build"),
        Some(NodeState::Failed { .. })
    ));
}

#[tokio::test]
async fn a_session_that_never_hands_over_its_declared_document_fails_the_node() {
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
        - { name: plan.yaml, kind: tasks }
"#;

    // The session claims success but submits nothing — the engine
    // verifies, and the missing document fails the node.
    let fixture = r#"
capabilities: { run_tools: true }
sessions:
  - outcome: { type: completed, summary: "trust me, it is written" }
"#;

    let (terminal, state) = bench.run(workflow, fixture).await;

    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    match state.nodes.get("plan") {
        Some(NodeState::Failed { failure, .. }) => {
            assert_eq!(
                failure.to_string(),
                "artifacts/plan.yaml: 1 error\n  \
         the document was declared by node `plan` and never produced"
            );
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
    let workflow: Workflow = serde_norway::from_str(workflow).unwrap();
    let config: ConfigLayer = serde_norway::from_str(MOCK_CONFIG).unwrap();
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
        clock: std::sync::Arc::new(FixedClock),
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
        ambient: None,
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
        RunTerminal::Paused { reason } => assert_eq!(
            reason,
            "node `implement` failed: before hook `exit 1` failed"
        ),
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
        RunTerminal::Paused { reason } => {
            assert_eq!(reason, "node `only` failed: after hook `exit 1` failed")
        }
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

    // The hook blocks forever; only its own 1s timeout ends it. Were the
    // timeout not honored the run would hang here, so reaching the pause at
    // all is the proof it was cut short — no wall-clock assertion needed.
    let workflow = r#"
name: hook-timeout
nodes:
  - id: only
    kind: bash
    run: "true"
    hooks:
      before:
        - run: "tail -f /dev/null"
          timeout_seconds: 1
"#;

    let (terminal, _) = bench.run(workflow, "sessions: []").await;

    match terminal {
        RunTerminal::Paused { reason } => assert_eq!(
            reason,
            "node `only` failed: before hook `tail -f /dev/null` failed"
        ),
        other => panic!("expected Paused, got {other:?}"),
    }
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

    let fixture = r#"
capabilities: { run_tools: true }
sessions:
  - steps:
      - type: run_tool
        tool: yunta_post_finding
        arguments:
          id: f1
          severity: major
          title: "Unchecked error"
          location: "src/lib.rs:10"
          detail: "The Result is discarded."
    outcome: { type: completed, summary: "reviewed" }
"#;

    let (terminal, state) = bench.run(workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(state.findings.len(), 1);
    assert_eq!(state.findings[0].id, "f1");
}

#[tokio::test]
async fn run_events_context_is_canonical_jsonl() {
    // A `run-events` context is the same canonical JSONL the run's own
    // `events.jsonl` export writes — one JSON object per line — never the
    // Rust `Debug` rendering, which would drift with any struct change.
    let bench = Bench::new();
    let workflow = r#"
name: run-events-ctx
nodes:
  - id: seed
    kind: bash
    run: "true"
  - id: read
    kind: prompt
    runner: executor
    depends_on: [seed]
    prompt: "Review the log."
    context:
      - run-events: {}
"#;
    let (terminal, _) = bench
        .run(
            workflow,
            "sessions:\n  - outcome: { type: completed, summary: ok }\n",
        )
        .await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let source = events
        .iter()
        .find_map(|e| match (&e.node_id, e.payload()) {
            (Some(n), Some(yunta_core::events::EventPayload::ContextAssembled(p)))
                if n.as_str() == "read" =>
            {
                p.sources.iter().find(|s| s.kind == "run-events").cloned()
            }
            _ => None,
        })
        .expect("a run-events context source assembled for `read`");

    let content = std::fs::read_to_string(
        bench
            .run_dir()
            .join("objects")
            .join(source.content_hash.as_str()),
    )
    .unwrap();
    assert!(
        !content.contains("StoredEvent {"),
        "the context must not be Rust Debug output: {content}"
    );
    for line in content.lines().filter(|l| !l.is_empty()) {
        let value: serde_json::Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("each line is canonical JSON ({line:?}): {e}"));
        assert!(
            value.get("kind").is_some(),
            "each event line names its kind: {line}"
        );
    }
    assert!(
        content.lines().any(|l| l.contains("run_created")),
        "the run's own events are present in the context"
    );
}

// --- what a command node writes enters the run at its close ------------------

#[tokio::test]
async fn a_file_a_command_node_writes_enters_the_run_as_ingested() {
    let bench = Bench::new();
    let workflow = r#"
name: ingest
nodes:
  - id: report
    kind: bash
    run: "echo the-report > {{run.dir}}/artifacts/report.md"
    artifacts: { produces: [report.md] }
"#;
    let (terminal, state) = bench.run(workflow, "sessions: []").await;
    assert_eq!(terminal, RunTerminal::Finished, "{state:?}");

    let held = bench.accepted();
    assert_eq!(held.len(), 1, "{held:?}");
    assert_eq!(
        held[0].producer.as_ref().map(|n| n.to_string()),
        Some("report".to_string())
    );
    assert_eq!(
        held[0].artifact,
        yunta_core::events::ArtifactId::Opaque {
            name: "report.md".to_string()
        }
    );
    assert_eq!(held[0].origin, yunta_core::events::ArtifactOrigin::Ingested);
    assert_eq!(
        bench.object(&held[0].content_hash).expect("the object"),
        b"the-report\n"
    );
    assert_eq!(
        std::fs::read(bench.run_dir().join("artifacts/report/report.md")).expect("the view"),
        b"the-report\n",
        "the view sits under the node that produced it"
    );
}

#[tokio::test]
async fn an_interpreted_artifact_a_node_wrote_is_stored_canonical() {
    let bench = Bench::new();
    // A valid tasks document in the node's own spelling: a comment and a
    // flow style nothing canonical writes.
    let workflow = r#"
name: ingest-typed
nodes:
  - id: plan
    kind: bash
    run: "printf '# the plan\ntasks:\n- {id: alpha, title: First, scope: [src/**], criteria: [{cmd: cargo test}]}\n' > {{run.dir}}/artifacts/plan.yaml"
    artifacts:
      produces: [{ name: plan.yaml, kind: tasks }]
"#;
    let (terminal, state) = bench.run(workflow, "sessions: []").await;
    assert_eq!(terminal, RunTerminal::Finished, "{state:?}");

    let held = bench.accepted();
    assert_eq!(held.len(), 1, "{held:?}");
    assert_eq!(held[0].origin, yunta_core::events::ArtifactOrigin::Ingested);
    assert_eq!(
        held[0].artifact,
        yunta_core::events::ArtifactId::Interpreted {
            kind: yunta_core::ArtifactKind::Tasks
        }
    );

    // What the run stores is the document, rendered the way the engine
    // renders every document of that kind — not the spelling the node
    // happened to write.
    let written = bench
        .projection(None, "plan.yaml")
        .expect("the node wrote it");
    let stored = bench.object(&held[0].content_hash).expect("the object");
    assert_ne!(stored, written, "the file was not canonical to begin with");
    let parsed: yunta_core::TasksFile =
        yunta_core::shape::read(&stored, "plan.yaml").expect("a canonical tasks document");
    assert_eq!(
        stored,
        yunta_core::shape::render(&parsed).unwrap().into_bytes(),
        "the stored bytes re-render to themselves"
    );
    assert_eq!(
        parsed
            .tasks
            .iter()
            .map(|t| t.id.to_string())
            .collect::<Vec<_>>(),
        vec!["alpha".to_string()]
    );
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
        let workflow: Workflow = serde_norway::from_str(
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
async fn a_run_born_holding_artifacts_names_each_one_after_run_created() {
    let bench = BirthBench::new();
    let run_id = RunId::from("run-with-brief");
    let from = RunId::from("run-predecessor");
    let artifacts = vec![yunta_engine::BirthArtifact {
        name: "brief/plan.md".to_string(),
        artifact: yunta_core::events::ArtifactId::Opaque {
            name: "brief/plan.md".to_string(),
        },
        origin: yunta_core::events::ArtifactOrigin::Inherited {
            run: from.clone(),
            producer: Some("write".into()),
        },
        bytes: b"hello".to_vec(),
    }];

    let run_dir = bench.create(&run_id, &artifacts).await.unwrap();

    assert_eq!(
        std::fs::read(run_dir.join("artifacts").join("brief").join("plan.md")).unwrap(),
        b"hello",
        "the view of what the run was handed sits at the root: it has no producer here"
    );
    let events = bench.storage.events_for_run(&run_id).unwrap();
    assert!(
        matches!(
            events.first().and_then(|e| e.payload()),
            Some(yunta_core::events::EventPayload::RunCreated(_))
        ),
        "the run exists in the log before anything is said about it"
    );
    let accepted = yunta_testkit::accepted(&events);
    assert_eq!(accepted.len(), 1, "{accepted:?}");
    assert_eq!(
        accepted[0].producer, None,
        "no node of this run produced it"
    );
    assert_eq!(
        accepted[0].origin,
        yunta_core::events::ArtifactOrigin::Inherited {
            run: from,
            producer: Some("write".into()),
        }
    );
    assert_eq!(
        std::fs::read(
            run_dir
                .join("objects")
                .join(accepted[0].content_hash.as_str())
        )
        .unwrap(),
        b"hello"
    );
}

#[tokio::test]
async fn run_created_freezes_resolved_inputs() {
    let bench = Bench::new();
    let workflow: Workflow = serde_norway::from_str(
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
    let config: ConfigLayer = serde_norway::from_str(MOCK_CONFIG).unwrap();
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

#[tokio::test]
async fn run_resumed_records_the_policy_each_orphan_resolved_to() {
    let bench = Bench::new();
    let workflow: Workflow = serde_norway::from_str(
        r#"
name: resumable
nodes:
  - id: only
    kind: bash
    on_interrupt: fail_if_uncertain
    run: "true"
"#,
    )
    .unwrap();
    let config: ConfigLayer = serde_norway::from_str(MOCK_CONFIG).unwrap();
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
    // A crash mid-node: node_started with no terminal event.
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
        clock: std::sync::Arc::new(FixedClock),
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &yunta_engine::NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
        ambient: None,
    })
    .await
    .unwrap();
    assert!(
        matches!(report.terminal, RunTerminal::Paused { .. }),
        "fail_if_uncertain pauses instead of guessing: {:?}",
        report.terminal
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let resumed = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::RunResumed(p)) => Some(p.clone()),
            _ => None,
        })
        .expect("a second invocation records run_resumed");
    assert_eq!(
        resumed.policies,
        vec![yunta_core::events::ResumePolicy {
            node: "only".into(),
            on_interrupt: yunta_core::OnInterrupt::FailIfUncertain,
        }],
        "the orphan and the policy it resolved to"
    );
    assert_eq!(
        resumed.resume_policy_applied.as_deref(),
        Some("fail_if_uncertain"),
        "the one policy every orphan shares, never a fixed literal"
    );
}

#[tokio::test]
async fn the_loop_finds_its_tasks_after_the_view_of_them_is_deleted() {
    // The tasks document is a fact of the log and bytes in the store.
    // Deleting the whole `artifacts/` view leaves both untouched, so the
    // loop still has its tasks.
    let bench = Bench::new();
    let workflow = r#"
name: tasks-from-the-log
nodes:
  - id: plan
    kind: bash
    run: "printf 'tasks:\n  - id: T001\n    title: Create hello\n    scope: [hello.txt]\n    criteria:\n      - cmd: test -f hello.txt\n' > {{run.dir}}/artifacts/plan.yaml"
    artifacts:
      produces: [{ name: plan.yaml, kind: tasks }]
  - id: wipe
    kind: bash
    depends_on: [plan]
    run: "rm -rf {{run.dir}}/artifacts"
  - id: implement
    kind: loop
    runner: executor
    depends_on: [wipe]
    until: all_tasks_complete
    prompt: "Read your task from the tasks document and implement it."
"#;
    let fixture = r#"
sessions:
  - effects:
      - { path: hello.txt, content: "hello" }
    outcome: { type: completed, summary: "did T001" }
"#;

    let (terminal, state) = bench.run(workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished, "{state:?}");
    assert_eq!(
        state.tasks.get("T001"),
        Some(&yunta_core::events::TaskStatus::Done)
    );
}
