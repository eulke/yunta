//! The repair cycle: an interpreted artifact that could not be read gets
//! a session of its own to rewrite it, instead of ending the node.
//!
//! The cycle belongs to every node that closes artifacts, not to one
//! kind. What decides it is whether the node resolves a runner to
//! dispatch the repair on — the file is already on disk, so the session
//! that fixes it needs the shape and the problems, never the work the
//! node did to produce it.

use std::collections::HashMap;
use std::sync::Arc;

use yunta_adapters::{Adapter, MockAdapter};
use yunta_core::diagnostic::ArtifactFailure;
use yunta_core::events::{EventPayload, Failure};
use yunta_core::{AdapterId, ArtifactKind, ConfigLayer, Workflow};
use yunta_engine::{
    build_manifest, create_run, execute_run, CreateRunParams, NoInteraction, RunEnv, RunError,
    RunTerminal, DEFAULT_MAX_RETRIES,
};
use yunta_testkit::{Bench, FixedClock, MOCK_CONFIG};

mod common;
use common::IDS;

const PLAN_ONLY: &str = r#"
name: plan-only
nodes:
  - id: plan
    kind: prompt
    runner: executor
    prompt: "Write a task ledger."
    artifacts:
      produces: [{ name: plan.yaml, kind: task-ledger }]
"#;

/// What an agent writes when it has never seen the shape: a key that
/// reads right and a criterion as plain text.
const WRONG_LEDGER: &str = "tasks:\\n  - id: t1\\n    title: Work\\n    description: the toggle\\n    scope: [\\\"src/**\\\"]\\n    criteria:\\n      - cargo test\\n";

const RIGHT_LEDGER: &str =
    "tasks:\\n  - id: t1\\n    title: Work\\n    scope: [\\\"src/**\\\"]\\n    criteria:\\n      - cmd: \\\"cargo test\\\"\\n";

/// The config a test uses to say "this node fails terminally on a bad
/// artifact": no repair is bought at all.
const NO_REPAIRS: &str = "\
runners:
  planner:
    - { adapter: mock, model: mock-model }
  executor:
    - { adapter: mock, model: mock-model }
limits:
  max_artifact_repairs: 0
";

fn plan_path(bench: &Bench) -> String {
    bench
        .run_dir()
        .join("artifacts")
        .join("plan.yaml")
        .display()
        .to_string()
}

/// Every attempt this node announced.
fn attempts(events: &[yunta_core::events::StoredEvent], node: &str) -> usize {
    events
        .iter()
        .filter(|e| e.node_id.as_ref().is_some_and(|id| id.as_str() == node))
        .filter(|e| matches!(e.payload(), Some(EventPayload::NodeStarted(_))))
        .count()
}

/// The node's last recorded failure.
fn last_failure(
    events: &[yunta_core::events::StoredEvent],
) -> yunta_core::events::NodeFailedPayload {
    events
        .iter()
        .rev()
        .find_map(|e| match e.payload() {
            Some(EventPayload::NodeFailed(p)) => Some(p.clone()),
            _ => None,
        })
        .expect("the node failed")
}

/// The kind of every context source each session was given, one entry
/// per `context_assembled`, in log order.
fn assembled_context(events: &[yunta_core::events::StoredEvent]) -> Vec<Vec<String>> {
    events
        .iter()
        .filter_map(|e| match e.payload() {
            Some(EventPayload::ContextAssembled(p)) => {
                Some(p.sources.iter().map(|s| s.kind.clone()).collect())
            }
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn a_ledger_that_could_not_be_read_is_written_again_and_the_node_finishes() {
    let bench = Bench::new();
    let path = plan_path(&bench);
    // The second script only matches a prompt carrying the diagnostics,
    // so the run finishing at all proves they reached the repair session.
    let fixture = format!(
        "sessions:\n  \
         - effects:\n      - {{ path: \"{path}\", content: \"{WRONG_LEDGER}\" }}\n    \
           outcome: {{ type: completed, summary: planned }}\n  \
         - match_prompt_contains: \"could not be read\"\n    \
           effects:\n      - {{ path: \"{path}\", content: \"{RIGHT_LEDGER}\" }}\n    \
           outcome: {{ type: completed, summary: repaired }}\n"
    );

    let (terminal, state) = bench.run(PLAN_ONLY, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert!(
        state.tasks.keys().any(|id| id.as_str() == "t1"),
        "the repaired ledger registered its task: {state:?}"
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert_eq!(
        attempts(&events, "plan"),
        2,
        "a repair is a fresh attempt on the log, visible in status and stats"
    );
}

#[tokio::test]
async fn the_repair_session_is_told_what_was_wrong() {
    let bench = Bench::new();
    let path = plan_path(&bench);
    // A needle the first prompt cannot contain: the problem named in the
    // document's own vocabulary, about the entry it belongs to.
    let fixture = format!(
        "sessions:\n  \
         - effects:\n      - {{ path: \"{path}\", content: \"{WRONG_LEDGER}\" }}\n    \
           outcome: {{ type: completed, summary: planned }}\n  \
         - match_prompt_contains: \"task `t1`, criterion 1\"\n    \
           effects:\n      - {{ path: \"{path}\", content: \"{RIGHT_LEDGER}\" }}\n    \
           outcome: {{ type: completed, summary: repaired }}\n"
    );
    let (terminal, _state) = bench.run(PLAN_ONLY, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);
}

const PLAN_WITH_CONTEXT: &str = r#"
name: plan-with-context
nodes:
  - id: plan
    kind: prompt
    runner: executor
    prompt: "Write a task ledger."
    context:
      - files: ["notes.md"]
    artifacts:
      produces: [{ name: plan.yaml, kind: task-ledger }]
"#;

#[tokio::test]
async fn the_repair_session_gets_the_shape_and_the_problems_and_nothing_else() {
    let bench = Bench::new();
    let path = plan_path(&bench);
    std::fs::write(
        bench.worktree.join("notes.md"),
        "the toggle is behind a flag",
    )
    .unwrap();
    let fixture = format!(
        "sessions:\n  \
         - effects:\n      - {{ path: \"{path}\", content: \"{WRONG_LEDGER}\" }}\n    \
           outcome: {{ type: completed, summary: planned }}\n  \
         - match_prompt_contains: \"could not be read\"\n    \
           effects:\n      - {{ path: \"{path}\", content: \"{RIGHT_LEDGER}\" }}\n    \
           outcome: {{ type: completed, summary: repaired }}\n"
    );
    let (terminal, _state) = bench.run(PLAN_WITH_CONTEXT, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    // The node's own session gets the shape and the author's `context:`.
    // The repair session gets the shape alone: it is rewriting a file it
    // already has on disk, and paying for the author's context again
    // would buy the session the node already had.
    assert_eq!(
        assembled_context(&bench.storage.events_for_run(&bench.run_id).unwrap()),
        vec![vec!["artifact-shape", "files"], vec!["artifact-shape"]],
    );
}

#[tokio::test]
async fn a_node_that_never_gets_it_right_fails_once_the_budget_is_spent() {
    let bench = Bench::new();
    let path = plan_path(&bench);
    // Both scripts write the same unreadable file. The default budget is
    // one repair, so exactly two sessions run and then the node fails.
    let script = format!(
        "  - effects:\n      - {{ path: \"{path}\", content: \"{WRONG_LEDGER}\" }}\n    \
           outcome: {{ type: completed, summary: planned }}\n"
    );
    let fixture = format!("sessions:\n{script}{script}");

    let (terminal, _state) = bench.run(PLAN_ONLY, &fixture).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "the budget is a budget: {terminal:?}"
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert_eq!(attempts(&events, "plan"), 2, "one attempt, then one repair");

    let failure = last_failure(&events);
    // Nothing is going to attempt this node again, and the log says so
    // rather than promising a repair the budget can no longer buy.
    assert!(!failure.retryable, "{failure:?}");
    // The failure a person reads still carries every violation, produced
    // from the facts on read.
    assert!(
        failure
            .failure
            .to_string()
            .contains("unknown key `description`"),
        "{failure:?}"
    );
}

#[tokio::test]
async fn a_run_that_buys_no_repairs_fails_the_node_on_its_first_bad_artifact() {
    let bench = Bench::new();
    let path = plan_path(&bench);
    let fixture = format!(
        "sessions:\n  \
         - effects:\n      - {{ path: \"{path}\", content: \"{WRONG_LEDGER}\" }}\n    \
           outcome: {{ type: completed, summary: planned }}\n"
    );

    let (terminal, _state) = bench.run_with_config(PLAN_ONLY, &fixture, NO_REPAIRS).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert_eq!(
        attempts(&events, "plan"),
        1,
        "`limits.max_artifact_repairs: 0` buys no second session"
    );
    assert!(!last_failure(&events).retryable);
}

#[tokio::test]
async fn a_node_failure_round_trips_through_the_log_with_the_document_of_every_diagnostic() {
    let bench = Bench::new();
    let path = plan_path(&bench);
    let fixture = format!(
        "sessions:\n  \
         - effects:\n      - {{ path: \"{path}\", content: \"{WRONG_LEDGER}\" }}\n    \
           outcome: {{ type: completed, summary: planned }}\n"
    );
    let (_terminal, _state) = bench.run_with_config(PLAN_ONLY, &fixture, NO_REPAIRS).await;

    // Read back out of storage, so this is what a later reader — a
    // receipt, `yunta status`, another process — actually gets.
    let failure = last_failure(&bench.storage.events_for_run(&bench.run_id).unwrap());
    let Failure::Artifacts { artifacts } = &failure.failure else {
        panic!("an artifact failure: {failure:?}");
    };
    let report = artifacts[0].report().expect("a problem with the content");
    // The document each diagnostic belongs to survives the log: a
    // receipt can tell `duplicate-id` in a ledger from `duplicate-id` in
    // a findings file without reading prose.
    assert_eq!(report.document.kind, ArtifactKind::TaskLedger);
    assert_eq!(report.document.path, "artifacts/plan.yaml");
    assert!(
        report.diagnostics.iter().any(|d| d.code() == "unknown-key"),
        "{report:?}"
    );
}

#[tokio::test]
async fn an_artifact_that_was_never_written_does_not_enter_the_cycle() {
    let bench = Bench::new();
    // Nothing to correct: the session wrote no file at all. Asking for
    // it again is a different question from asking for it in the right
    // shape, and the ledger's own budget is not the place to answer it.
    let fixture = "sessions:\n  - outcome: { type: completed, summary: planned }\n";

    let (terminal, _state) = bench.run(PLAN_ONLY, fixture).await;
    assert!(matches!(terminal, RunTerminal::Paused { .. }));

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert_eq!(
        attempts(&events, "plan"),
        1,
        "a file that does not exist is not a file to rewrite"
    );
    let failure = last_failure(&events);
    assert!(!failure.retryable, "{failure:?}");
    assert!(
        matches!(
            failure.failure.failures().next(),
            Some(ArtifactFailure::File { .. })
        ),
        "{failure:?}"
    );
}

#[tokio::test]
async fn a_node_whose_artifact_reads_first_time_runs_exactly_one_session() {
    let bench = Bench::new();
    let path = plan_path(&bench);
    let fixture = format!(
        "sessions:\n  \
         - effects:\n      - {{ path: \"{path}\", content: \"{RIGHT_LEDGER}\" }}\n    \
           outcome: {{ type: completed, summary: planned }}\n"
    );

    let (terminal, _state) = bench.run(PLAN_ONLY, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert_eq!(
        attempts(&events, "plan"),
        1,
        "the cycle costs nothing when nothing is wrong"
    );
}

// --- every kind that closes artifacts, not just `kind: prompt` ---------------

const LOOP_FINDINGS: &str = r#"
name: loop-findings
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the ledger."
    artifacts:
      produces: [{ name: plan.yaml, kind: task-ledger }]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Implement your task and report what you found."
    artifacts:
      produces: [{ name: findings.yaml, kind: findings }]
"#;

const WRONG_FINDINGS: &str = "findings:\\n  - id: f1\\n    severity: major\\n    title: \\\"Unchecked error\\\"\\n    where: \\\"src/lib.rs:10\\\"\\n    detail: \\\"The Result is discarded.\\\"\\n";

const RIGHT_FINDINGS: &str = "findings:\\n  - id: f1\\n    severity: major\\n    title: \\\"Unchecked error\\\"\\n    location: \\\"src/lib.rs:10\\\"\\n    detail: \\\"The Result is discarded.\\\"\\n";

/// The asymmetry the cycle exists to remove: a `kind: loop` node's
/// artifact is written by a task session the node no longer holds, so
/// re-running the node could never fix it — but a session of its own,
/// handed the shape and the problems, can.
#[tokio::test]
async fn a_loop_node_whose_findings_could_not_be_read_gets_its_second_attempt() {
    let bench = Bench::new();
    let artifacts = bench.run_dir().join("artifacts");
    let findings = artifacts.join("findings.yaml").display().to_string();
    let plan = artifacts.join("plan.yaml").display().to_string();
    let fixture = format!(
        "sessions:\n  \
         - effects:\n      - {{ path: \"{plan}\", content: \"tasks:\\n  - id: T001\\n    title: \\\"Create hello\\\"\\n    scope: [\\\"hello.txt\\\"]\\n    criteria:\\n      - cmd: \\\"test -f hello.txt\\\"\\n\" }}\n    \
           outcome: {{ type: completed, summary: planned }}\n  \
         - effects:\n      - {{ path: hello.txt, content: hello }}\n      \
             - {{ path: \"{findings}\", content: \"{WRONG_FINDINGS}\" }}\n    \
           outcome: {{ type: completed, summary: \"did T001\" }}\n  \
         - match_prompt_contains: \"could not be read\"\n    \
           effects:\n      - {{ path: \"{findings}\", content: \"{RIGHT_FINDINGS}\" }}\n    \
           outcome: {{ type: completed, summary: repaired }}\n"
    );

    let (terminal, state) = bench.run(LOOP_FINDINGS, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished, "{state:?}");

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert_eq!(
        attempts(&events, "implement"),
        2,
        "the loop node got a repair session of its own"
    );
    // The repaired file was read, so its findings reached the log.
    assert!(
        state.findings.iter().any(|f| f.id.as_str() == "f1"),
        "{state:?}"
    );
}

const BASH_LEDGER: &str = r#"
name: bash-ledger
nodes:
  - id: plan
    kind: bash
    run: "mkdir -p {{run.dir}}/artifacts && cp wrong.yaml {{run.dir}}/artifacts/plan.yaml"
    artifacts:
      produces: [{ name: plan.yaml, kind: task-ledger }]
"#;

const WRONG_LEDGER_FILE: &str = "tasks:\n  - id: t1\n    title: Work\n    description: the toggle\n    scope: [\"src/**\"]\n    criteria:\n      - cmd: \"cargo test\"\n";

#[tokio::test]
async fn a_node_that_resolves_no_runner_fails_without_a_repair_session() {
    let bench = Bench::new();
    std::fs::write(bench.worktree.join("wrong.yaml"), WRONG_LEDGER_FILE).unwrap();

    // This `kind: bash` node declares no `runner:` and the config
    // declares no `defaults.runner`, so there is no agent to instruct —
    // a limit of the system, not an omission. Its command is its whole
    // instruction, and running it again would be a retry, not a repair.
    let (terminal, _state) = bench.run(BASH_LEDGER, "sessions: []\n").await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert_eq!(attempts(&events, "plan"), 1, "no session was dispatched");
    let failure = last_failure(&events);
    assert!(!failure.retryable, "{failure:?}");
    // The failure is still the whole picture, as data — what the node
    // cannot do is act on it by itself.
    let report = failure
        .failure
        .reports()
        .next()
        .expect("the document's own problems");
    assert_eq!(report.document.kind, ArtifactKind::TaskLedger);
}

// --- the second door onto an interpreted artifact ----------------------------

const CORRUPTED_LEDGER: &str = r#"
name: corrupted-ledger
nodes:
  - id: plan
    kind: prompt
    runner: executor
    prompt: "Write a task ledger."
    artifacts:
      produces: [{ name: plan.yaml, kind: task-ledger }]
  - id: corrupt
    kind: bash
    depends_on: [plan]
    run: "cp wrong.yaml {{run.dir}}/artifacts/plan.yaml"
  - id: implement
    kind: loop
    runner: executor
    depends_on: [corrupt]
    until: all_tasks_complete
    prompt: "Implement your task."
"#;

#[tokio::test]
async fn an_interpreted_artifact_the_loop_cannot_read_fails_with_a_diagnostic() {
    let bench = Bench::new();
    let path = plan_path(&bench);
    std::fs::write(bench.worktree.join("wrong.yaml"), WRONG_LEDGER_FILE).unwrap();
    let fixture = format!(
        "sessions:\n  \
         - effects:\n      - {{ path: \"{path}\", content: \"{RIGHT_LEDGER}\" }}\n    \
           outcome: {{ type: completed, summary: planned }}\n"
    );

    let workflow: Workflow = serde_norway::from_str(CORRUPTED_LEDGER).unwrap();
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
    let mut adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
    adapters.insert(
        "mock".into(),
        Arc::new(MockAdapter::from_yaml(&fixture).unwrap()),
    );

    let error = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &adapters,
        storage: &bench.storage.async_handle(),
        clock: Arc::new(FixedClock),
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
        ambient: None,
        observer: None,
    })
    .await
    .expect_err("the loop cannot read the ledger it was given");

    let RunError::UnreadableArtifact(report) = &error else {
        panic!("the document's own report: {error:?}");
    };
    assert_eq!(report.document.kind, ArtifactKind::TaskLedger);
    let text = error.to_string();
    // The document's own vocabulary, never the deserializer's: a reader
    // is told which key is not part of a task, not that an "unknown
    // field" was found while deserializing a struct.
    assert!(text.contains("unknown key `description`"), "{text}");
    assert!(!text.contains("unknown field"), "{text}");
}
