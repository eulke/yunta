//! End-to-end runs with the mock adapter: the bootstrap
//! shape — a prompt plan node that produces the tasks document, a loop that
//! implements it task by task, a bash gate — plus re-routes, hooks,
//! findings and the birth of a run, all derived from the event log alone.

use yunta_core::RunId;
use yunta_engine::{NodeState, RunReport, RunTerminal};
use yunta_testkit::{Bench, MOCK_CONFIG};
use yunta_testkit_core::FixedClock;

mod common;
use common::*;
use yunta_core::events::{ArtifactEvent, NodeEvent, RecordedOrigin, RunEvent};

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
      produces: [tasks]
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

    let RunReport { terminal, state } = bench.run(workflow, fixture).await;

    assert_eq!(terminal, RunTerminal::Finished);
    for node in ["plan", "implement", "verify"] {
        assert!(
            matches!(state.nodes.state(node), Some(NodeState::Finished { .. })),
            "node `{node}` should be finished, got {:?}",
            state.nodes.state(node)
        );
    }
    assert_eq!(
        state.tasks.status("T001"),
        Some(yunta_core::events::TaskStatus::Done)
    );
    assert_eq!(
        state.tasks.status("T002"),
        Some(yunta_core::events::TaskStatus::Done)
    );
    // Tokens from both executor sessions were attributed to the run.
    assert_eq!(state.total_tokens().input, 180);
    assert_eq!(state.total_tokens().output, 30);
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

    let RunReport { terminal, state } = bench.run(workflow, fixture).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.state("lint"),
        Some(NodeState::Finished { .. })
    ));
    assert!(matches!(
        state.nodes.state("fix-lint"),
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

    let RunReport { terminal, state } = bench.run(workflow, fixture).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.state("lint"),
        Some(NodeState::Finished { .. })
    ));
    assert_eq!(
        state.nodes.state("fix-lint"),
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

    let RunReport { terminal, state } = bench
        .run_with_interaction(
            workflow,
            "sessions: []",
            &SequencedInteraction::choosing(&["ship"]),
        )
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        state.nodes.state("redo-node"),
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

    let RunReport { terminal, .. } = bench.run(workflow, fixture).await;

    match terminal {
        RunTerminal::Paused { reason } => {
            assert_eq!(
                reason,
                "node `lint` failed and its 1 re-route(s) to `fix-lint` are exhausted — exit 1"
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

    let RunReport { terminal, state } = bench.run(workflow, "sessions: []").await;

    match terminal {
        RunTerminal::Paused { reason } => assert_eq!(reason, "node `build` failed: exit 3"),
        other => panic!("expected Paused, got {other:?}"),
    }
    assert!(matches!(
        state.nodes.state("build"),
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
      produces: [tasks]
"#;

    // The session claims success but submits nothing — the engine
    // verifies, and the document nobody handed over fails the node. It
    // names the node and what that node owes: no file was ever going to
    // be there, so the failure points at nothing on disk.
    let fixture = r#"
capabilities: { run_tools: true }
sessions:
  - outcome: { type: completed, summary: "trust me, it is written" }
"#;

    let RunReport { terminal, state } = bench.run(workflow, fixture).await;

    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    match state.nodes.state("plan") {
        Some(NodeState::Failed { failure, .. }) => {
            assert_eq!(
                failure.to_string(),
                "node `plan`: 1 error\n  handed over no tasks document — \
                 produce it before the node ends, or stop declaring it here"
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

    let RunReport { terminal, .. } = bench.run(workflow, "sessions: []").await;
    assert_eq!(terminal, RunTerminal::Finished);

    // Second execution: same log, still Finished — and no duplicate
    // node execution (the log would show a second start).
    let RunReport { terminal, .. } = bench.wake().await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.events();
    let starts = events
        .iter()
        .filter(|e| {
            matches!(
                e.payload(),
                Some(yunta_core::events::EventPayload::Node(NodeEvent::Started(
                    _
                )))
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
    let RunReport { terminal, .. } = bench.run(workflow, "sessions: []").await;

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

    let RunReport { terminal, .. } = bench.run(workflow, "sessions: []").await;

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

    let RunReport { terminal, .. } = bench.run(workflow, "sessions: []").await;
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

    let RunReport { terminal, .. } = bench.run(workflow, "sessions: []").await;

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

    let RunReport { terminal, .. } = bench.run(workflow, "sessions: []").await;
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

    let RunReport { terminal, .. } = bench.run(workflow, "sessions: []").await;
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

    let RunReport { terminal, .. } = bench.run(&workflow, "sessions: []").await;
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
      produces: [findings]
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

    let RunReport { terminal, state } = bench.run(workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(state.effective_findings().len(), 1);
    assert_eq!(state.effective_findings()[0].id, "f1");
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
    let RunReport { terminal, .. } = bench
        .run(
            workflow,
            "sessions:\n  - outcome: { type: completed, summary: ok }\n",
        )
        .await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.events();
    let source = events
        .iter()
        .find_map(|e| match (&e.node_id, e.payload()) {
            (
                Some(n),
                Some(yunta_core::events::EventPayload::Node(NodeEvent::ContextAssembled(p))),
            ) if n.as_str() == "read" => p.sources.iter().find(|s| s.kind == "run-events").cloned(),
            _ => None,
        })
        .expect("a run-events context source assembled for `read`");

    let content = String::from_utf8(bench.object(&source.content_hash).unwrap()).unwrap();
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
    run: "echo the-report > {{node.artifacts}}/report.md"
    artifacts: { produces: [report.md] }
"#;
    let RunReport { terminal, state } = bench.run(workflow, "sessions: []").await;
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
    assert_eq!(held[0].origin, RecordedOrigin::Ingested);
    assert_eq!(
        bench.object(&held[0].content_hash).expect("the object"),
        b"the-report\n"
    );
    assert_eq!(
        bench
            .projection(Some("report"), "report.md")
            .expect("the view"),
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
    run: "printf '# the plan\ntasks:\n- {id: alpha, title: First, scope: [src/**], criteria: [{cmd: cargo test}]}\n' > {{node.artifacts}}/tasks.yaml"
    artifacts:
      produces: [tasks]
"#;
    let RunReport { terminal, state } = bench.run(workflow, "sessions: []").await;
    assert_eq!(terminal, RunTerminal::Finished, "{state:?}");

    let held = bench.accepted();
    assert_eq!(held.len(), 1, "{held:?}");
    assert_eq!(held[0].origin, RecordedOrigin::Ingested);
    assert_eq!(
        held[0].artifact,
        yunta_core::events::ArtifactId::Interpreted {
            kind: yunta_core::ArtifactKind::Tasks
        }
    );

    // What the run stores is the document, rendered the way the engine
    // renders every document of that kind — not the spelling the node
    // happened to write.
    let written =
        std::fs::read(bench.staging("plan").join("tasks.yaml")).expect("the node wrote it");
    let stored = bench.object(&held[0].content_hash).expect("the object");
    assert_ne!(stored, written, "the file was not canonical to begin with");
    let parsed: yunta_core::TasksFile =
        yunta_core::shape::read(&stored, "tasks.yaml").expect("a canonical tasks document");
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

/// A workflow with a single node that always passes: what a test about a
/// birth points a run at, so what it reads is the birth itself.
const BIRTH_WORKFLOW: &str = "name: birth\nnodes:\n  - { id: a, kind: bash, run: \"true\" }\n";

#[tokio::test]
async fn create_run_refuses_an_existing_run_dir() {
    let bench = Bench::with_run_id("run-once");
    bench.birth(BIRTH_WORKFLOW).await.unwrap();

    let error = bench.birth(BIRTH_WORKFLOW).await.unwrap_err();
    assert!(
        matches!(
            &error,
            yunta_engine::RunError::RunDirExists { path } if *path == bench.runs_root.join("run-once")
        ),
        "{error:?}"
    );
    // Refused before anything is written: the log still holds one birth.
    assert_eq!(bench.events().len(), 1);
}

#[tokio::test]
async fn a_run_born_holding_artifacts_names_each_one_after_run_created() {
    let from = RunId::from("run-predecessor");
    let bench = Bench::new().born_holding(vec![yunta_engine::BirthArtifact {
        artifact: yunta_core::events::ArtifactId::Opaque {
            name: "brief/plan.md".to_string(),
        },
        origin: yunta_engine::BirthOrigin::Inherited {
            run: from.clone(),
            producer: Some("write".into()),
        },
        bytes: b"hello".to_vec(),
    }]);

    bench.run(BIRTH_WORKFLOW, "sessions: []").await;

    assert_eq!(
        bench.projection(None, "brief/plan.md").unwrap(),
        b"hello",
        "the view of what the run was handed sits at the root: it has no producer here"
    );
    let events = bench.events();
    assert!(
        matches!(
            events.first().and_then(|e| e.payload()),
            Some(yunta_core::events::EventPayload::Run(RunEvent::Created(_)))
        ),
        "the run exists in the log before anything is said about it"
    );
    let accepted = bench.accepted();
    assert_eq!(accepted.len(), 1, "{accepted:?}");
    assert_eq!(
        accepted[0].producer, None,
        "no node of this run produced it"
    );
    assert_eq!(
        accepted[0].origin,
        RecordedOrigin::Inherited {
            run: from,
            producer: Some("write".into()),
        }
    );
    assert_eq!(bench.object(&accepted[0].content_hash).unwrap(), b"hello");
}

#[tokio::test]
async fn run_created_freezes_resolved_inputs() {
    let bench = Bench::new().with_inputs(&[("idea", "ship it")]);
    bench
        .birth(
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
        .await
        .unwrap();

    let created = bench
        .events()
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::Run(RunEvent::Created(p))) => Some(p.clone()),
            _ => None,
        })
        .expect("run_created is the first event");
    let frozen: std::collections::BTreeMap<String, serde_json::Value> = bench
        .manifest()
        .inputs
        .iter()
        .map(|(name, value)| (name.to_string(), serde_json::Value::String(value.clone())))
        .collect();
    assert_eq!(
        created.inputs, frozen,
        "run_created carries every input the manifest froze, the default included"
    );
    assert_eq!(created.inputs["greeting"], "hola");
    assert_eq!(created.inputs["idea"], "ship it");
}

/// A hand-written tasks document: what a person points a run at.
const TASKS_ON_DISK: &str = "tasks:\n  - id: greeting\n    title: 'Add the greeting'\n    \
     scope: ['src/**']\n    criteria:\n      - {cmd: 'true'}\n";

#[tokio::test]
async fn a_document_input_is_born_as_an_artifact_and_frozen_as_its_hash() {
    let bench = Bench::new().with_inputs(&[("tasks", "plan.yaml")]);
    std::fs::write(bench.worktree.join("plan.yaml"), TASKS_ON_DISK).unwrap();
    bench
        .birth(
            r#"
name: with-a-document
inputs:
  tasks:
    type: document
    kind: tasks
nodes:
  - id: only
    kind: bash
    run: "true"
"#,
        )
        .await
        .expect("a run holding the document its inputs named");

    let events = bench.events();
    assert!(
        matches!(
            events.first().and_then(|e| e.payload()),
            Some(yunta_core::events::EventPayload::Run(RunEvent::Created(_)))
        ),
        "the run exists in the log before the document it holds is stated"
    );
    let accepted = yunta_testkit::accepted(&events);
    assert_eq!(accepted.len(), 1, "{accepted:?}");
    assert_eq!(
        accepted[0].producer, None,
        "no node of this run produced it: it came in as an input"
    );
    assert_eq!(
        accepted[0].origin,
        RecordedOrigin::Input {
            input: "tasks".into()
        }
    );
    assert_eq!(
        accepted[0].artifact,
        yunta_core::events::ArtifactId::Interpreted {
            kind: yunta_core::ArtifactKind::Tasks
        }
    );

    let frozen_value = format!("sha256:{}", accepted[0].content_hash);
    assert_eq!(
        bench.manifest().inputs["tasks"],
        frozen_value,
        "the manifest freezes the document the run holds, not the path it was read from"
    );
    let created = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::Run(RunEvent::Created(p))) => Some(p.clone()),
            _ => None,
        })
        .expect("run_created is the first event");
    assert_eq!(created.inputs["tasks"], frozen_value);

    assert_eq!(
        yunta_engine::derive(&events)
            .tasks
            .iter()
            .map(|(id, record)| (id.clone(), record.status))
            .collect::<Vec<_>>(),
        vec![(
            yunta_core::TaskId::from("greeting"),
            yunta_core::events::TaskStatus::Pending
        )],
        "the tasks of a document the run was given are tasks of the run: no node produces it, \
         so its birth is where they are registered"
    );
}

#[tokio::test]
async fn run_resumed_records_the_policy_each_orphan_resolved_to() {
    let bench = Bench::new();
    let workflow = r#"
name: resumable
nodes:
  - id: only
    kind: bash
    on_interrupt: fail_if_uncertain
    run: "true"
"#;

    let RunReport { terminal, .. } = bench
        .run_sabotaged(workflow, "sessions: []", |_| {
            // A crash mid-node: node_started with no terminal event.
            bench
                .storage
                .append(
                    &yunta_core::events::EventDraft {
                        run_id: bench.run_id.clone(),
                        node_id: Some("only".into()),
                        payload: yunta_core::events::EventPayload::Node(NodeEvent::Started(
                            yunta_core::events::NodeStartedPayload::attempt(1),
                        )),
                    },
                    &yunta_core::SystemClock,
                )
                .unwrap();
        })
        .await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "fail_if_uncertain pauses instead of guessing: {terminal:?}"
    );

    let events = bench.events();
    let resumed = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::Run(RunEvent::Resumed(p))) => Some(p.clone()),
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
    // The view is the engine's own directory, so the node that deletes
    // it names it by its absolute path rather than through a template no
    // workflow has for it.
    let workflow = format!(
        r#"
name: tasks-from-the-log
nodes:
  - id: plan
    kind: bash
    run: "printf 'tasks:\n  - id: T001\n    title: Create hello\n    scope: [hello.txt]\n    criteria:\n      - cmd: test -f hello.txt\n' > {{{{node.artifacts}}}}/tasks.yaml"
    artifacts:
      produces: [tasks]
  - id: wipe
    kind: bash
    depends_on: [plan]
    run: "rm -rf {view}"
  - id: implement
    kind: loop
    runner: executor
    depends_on: [wipe]
    until: all_tasks_complete
    prompt: "Read your task from the tasks document and implement it."
"#,
        view = bench.run_dir().join(yunta_core::ARTIFACTS_DIR).display()
    );
    let fixture = r#"
sessions:
  - effects:
      - { path: hello.txt, content: "hello" }
    outcome: { type: completed, summary: "did T001" }
"#;

    let RunReport { terminal, state } = bench.run(&workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished, "{state:?}");
    assert_eq!(
        state.tasks.status("T001"),
        Some(yunta_core::events::TaskStatus::Done)
    );
}

#[tokio::test]
async fn a_file_left_by_a_failed_attempt_is_not_ingested_by_the_next_one() {
    // `report` writes its artifact and fails; the re-route fixes what
    // made it fail and `report` runs again, this time writing nothing.
    // The second attempt owes an artifact it never produced, because it
    // opened on an empty directory — what the first attempt left behind
    // is not this attempt's work.
    let bench = Bench::new();
    let workflow = r#"
name: stale-staging
nodes:
  - id: report
    kind: bash
    run: "test -f marker || (echo the-report > {{node.artifacts}}/report.md; false)"
    artifacts: { produces: [report.md] }
    on_failure: { goto: fix, max_reroutes: 1 }
  - id: fix
    kind: bash
    run: "touch marker"
"#;
    let RunReport { terminal, state } = bench.run(workflow, "sessions: []").await;
    let RunTerminal::Paused { reason } = &terminal else {
        panic!("the second attempt owes an artifact it never wrote: {terminal:?} {state:?}");
    };
    assert!(
        reason.contains("report.md") && reason.contains("never produced"),
        "the second attempt is judged on what it produced: {reason}"
    );
    assert_eq!(
        bench.accepted(),
        vec![],
        "nothing a failed attempt left behind becomes the next attempt's artifact"
    );
}

// --- a run answers for the tasks of every document it holds ------------

/// The bytes of a document naming one task, as a run stores it: read
/// through its own door and rendered canonically.
fn one_task_document() -> Vec<u8> {
    yunta_core::shape::render(&yunta_testkit::tasks_document(&[(
        "T001",
        "done.txt",
        "test -f done.txt",
    )]))
    .expect("the canonical rendering")
    .into_bytes()
}

#[tokio::test]
async fn a_run_inheriting_a_tasks_document_from_a_log_that_does_not_replay_is_never_created() {
    let source = RunId::from("run-unreadable-source");
    let bench = Bench::with_run_id("run-from-a-broken-source").born_holding(vec![
        yunta_engine::BirthArtifact {
            artifact: yunta_core::events::ArtifactId::Interpreted {
                kind: yunta_core::ArtifactKind::Tasks,
            },
            origin: yunta_engine::BirthOrigin::Inherited {
                run: source.clone(),
                producer: None,
            },
            bytes: one_task_document(),
        },
    ]);
    // A status about a task nobody registered: a log replay stops at.
    let planted =
        yunta_testkit::SourceLog::open(&bench.storage, &source, std::sync::Arc::new(FixedClock));
    planted.record(yunta_testkit::task_status_changed(
        &"T001".into(),
        yunta_core::events::TaskStatus::Done,
        None,
        1u64.into(),
    ));

    let error = bench.birth(BIRTH_WORKFLOW).await.unwrap_err();

    let yunta_engine::RunError::Broken { diagnostic } = &error else {
        panic!("a source whose log does not replay refuses the birth: {error:?}");
    };
    assert!(
        diagnostic.contains(source.as_str()),
        "the diagnostic names the run that cannot answer: {diagnostic}"
    );
    assert!(
        !bench.run_dir().exists(),
        "the run has no directory, because it was never born"
    );
    assert!(bench.events().is_empty(), "nor a single event");
}

const LOOP_ONLY_WORKFLOW: &str = r#"
name: loop-only
nodes:
  - id: implement
    kind: loop
    runner: executor
    until: all_tasks_complete
    prompt: "Read your task from the tasks document and implement it."
"#;

#[tokio::test]
async fn a_loop_over_a_tasks_document_the_run_never_registered_is_broken_not_stuck() {
    let bench = Bench::new();
    let run_dir = bench.birth(LOOP_ONLY_WORKFLOW).await.unwrap();

    // A document the run holds and never said what to do about — the
    // one shape that reaches a loop with no registration behind it.
    let content_hash = yunta_engine::ObjectStore::at(&run_dir)
        .put(&one_task_document())
        .await
        .unwrap();
    bench
        .storage
        .append(
            &yunta_core::events::EventDraft {
                run_id: bench.run_id.clone(),
                node_id: None,
                payload: yunta_core::events::EventPayload::Artifacts(ArtifactEvent::Accepted(
                    yunta_core::events::ArtifactAcceptedPayload::new(
                        yunta_core::events::ArtifactId::Interpreted {
                            kind: yunta_core::ArtifactKind::Tasks,
                        },
                        content_hash,
                        RecordedOrigin::Inherited {
                            run: "run-elsewhere".into(),
                            producer: None,
                        },
                    ),
                )),
            },
            &yunta_core::SystemClock,
        )
        .unwrap();

    let error = bench.try_wake().await.unwrap_err();

    let yunta_engine::RunError::Broken { diagnostic } = &error else {
        panic!("a document with no registration behind it is broken: {error:?}");
    };
    assert!(
        diagnostic.contains("tasks.yaml") && diagnostic.contains("T001"),
        "the diagnostic names the document and every task missing from the log: {diagnostic}"
    );
}

/// However a run ends, its log carries exactly one `run_finished`, and
/// that event is the run's own — never a node's. One writer, one close:
/// a green run, a run whose failure aborted it, and a promotion all take
/// the same path out.
#[tokio::test]
async fn a_run_closes_with_one_run_finished_whatever_way_it_closes() {
    let green = r#"
name: green
nodes:
  - { id: ok, kind: bash, run: "true" }
"#;
    let red = r#"
name: red
nodes:
  - { id: nope, kind: bash, run: "exit 1" }
"#;
    for (name, workflow, config, expected) in [
        ("a run that finished", green, MOCK_CONFIG, "done"),
        (
            "a run a failure aborted",
            red,
            "defaults:\n  on_failure: abort\n",
            "failed",
        ),
    ] {
        let bench = Bench::new();
        let RunReport { terminal, .. } = bench
            .run_with_config(workflow, "sessions: []\n", config)
            .await;
        let events = bench.events();
        let closes: Vec<&yunta_core::events::StoredEvent> = events
            .iter()
            .filter(|e| e.body.kind_name() == "run_finished")
            .collect();
        assert_eq!(closes.len(), 1, "{name} closes once: {terminal:?}");
        assert_eq!(
            closes[0].node_id, None,
            "{name}: the close is the run's, not a node's"
        );
        let Some(yunta_core::events::EventPayload::Run(RunEvent::Finished(p))) =
            closes[0].payload()
        else {
            panic!("{name}: the close carries a run_finished payload");
        };
        assert_eq!(
            format!("{:?}", p.terminal_state).to_lowercase(),
            expected,
            "{name} names how it closed"
        );
        assert_eq!(
            closes[0].seq,
            events.last().expect("a log with events").seq,
            "{name}: nothing is written after the close"
        );
    }
}

/// A `loop` node whose executor session dies reports the death, not the
/// tail that says no task is ready: what a reader has to act on is the
/// CLI that would not start, and the criteria were never run at all.
#[tokio::test]
async fn a_loop_node_whose_session_died_fails_naming_the_adapter_and_the_exit() {
    let bench = Bench::new();

    let workflow = r#"
name: dying-loop
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the tasks document."
    artifacts:
      produces: [tasks]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Read your task from the tasks document and implement it."
"#;

    let fixture = r#"
capabilities: { run_tools: true }
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_tasks
        arguments:
          document:
            tasks:
              - id: T001
                title: "Create hello"
                scope: ["hello.txt"]
                criteria:
                  - cmd: "test -f hello.txt"
    outcome: { type: completed, summary: "planned" }
  - outcome: { type: crash }
"#;

    let RunReport { terminal, state: _ } = bench.run(workflow, fixture).await;
    assert!(
        !matches!(terminal, RunTerminal::Finished),
        "a loop whose session died does not finish: {terminal:?}"
    );

    let failure = bench
        .events()
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::Node(NodeEvent::Failed(p))) => Some(p.clone()),
            _ => None,
        })
        .expect("the loop node failed");
    let yunta_core::events::Failure::SessionDied { died } = &failure.failure else {
        panic!("the death is the fact, not a tail: {:?}", failure.failure);
    };
    assert_eq!(died.adapter, "mock");
    assert!(
        failure.retryable,
        "a CLI that would not start is worth another run"
    );
}
