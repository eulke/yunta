//! `current_escalation` (M8/T8.1): reconstructs the §5.3 escalation
//! object a paused run is currently waiting on — purely from the
//! manifest and its own log, no live process required. This is what
//! lets `resolve_gate` (a separate `yunta mcp` invocation, not the
//! process that paused the run) know what it's answering.

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Utc};
use yunta_adapters::{Adapter, MockAdapter};
use yunta_core::{Clock, ConfigLayer, RunId, Workflow};
use yunta_engine::{
    build_manifest, create_run, current_escalation, execute_run, CreateRunParams, NoInteraction,
    RunTerminal, DEFAULT_MAX_RETRIES,
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
  executor:
    - { adapter: mock, model: mock-model }
"#;

/// Runs `workflow_yaml` with `NoInteraction` to a pause, and returns the
/// run's frozen manifest alongside its log — exactly the two inputs a
/// separate `resolve_gate` invocation would read off disk (manifest.yaml
/// + storage), with nothing else.
async fn paused_manifest_and_events(
    workflow_yaml: &str,
    fixture_yaml: &str,
) -> (yunta_core::Manifest, Vec<yunta_core::events::Event>) {
    let root = tempfile::tempdir().unwrap();
    let worktree = root.path().join("worktree");
    std::fs::create_dir_all(&worktree).unwrap();
    init_repo(&worktree);
    let runs_root = root.path().join("runs");
    let storage = Storage::open(&root.path().join("yunta.db")).unwrap();
    let run_id = RunId::from("run-escalation-1");

    let workflow: Workflow = serde_yaml::from_str(workflow_yaml).unwrap();
    let config: ConfigLayer = serde_yaml::from_str(CONFIG).unwrap();
    let manifest =
        build_manifest(&workflow, &config, &worktree, &worktree, &HashMap::new()).unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &run_id,
            manifest: &manifest,
            runs_root: &runs_root,
            mode: "default",
            promoted_from: None,
        },
        &storage,
        &FixedClock,
    )
    .unwrap();

    let adapter = MockAdapter::from_yaml(fixture_yaml).unwrap();
    let mut adapters: HashMap<String, std::sync::Arc<dyn Adapter>> = HashMap::new();
    adapters.insert("mock".to_string(), std::sync::Arc::new(adapter));

    let report = execute_run(
        &run_id,
        &manifest,
        &run_dir,
        &worktree,
        &adapters,
        &storage,
        &FixedClock,
        DEFAULT_MAX_RETRIES,
        &NoInteraction,
        None,
        None,
    )
    .await
    .unwrap();
    assert!(
        matches!(report.terminal, RunTerminal::Paused { .. }),
        "expected the run to pause, got {:?}",
        report.terminal
    );

    let events = storage.events_for_run(&run_id).unwrap();
    (manifest, events)
}

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
"#;

#[tokio::test]
async fn reconstructs_an_exhausted_reroute_escalation_with_retry_and_abort() {
    let (manifest, events) = paused_manifest_and_events(
        HOPELESS_UNTIL_RETRIED_WORKFLOW,
        HOPELESS_UNTIL_RETRIED_FIXTURE,
    )
    .await;

    // Nothing was ever logged for this pause (no live surface) — the
    // whole point is that `current_escalation` rebuilds it without one.
    assert!(!events
        .iter()
        .any(|e| matches!(e.payload, yunta_core::events::EventPayload::GateWaiting(_))));

    let (node_id, escalation) = current_escalation(&manifest, &events)
        .expect("an exhausted re-route must reconstruct an escalation");
    assert_eq!(node_id.as_str(), "lint");
    assert!(escalation.summary.contains("lint"));
    assert!(escalation.summary.contains("fix-lint"));
    let ids: Vec<&str> = escalation.options.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, vec!["retry", "abort"]);
    assert!(escalation.options.iter().all(|o| !o.tradeoff.is_empty()));
}

const INTERNAL_GATE_WORKFLOW: &str = r#"
name: internal-gate
nodes:
  - id: plan
    kind: bash
    run: "true"
  - id: approve
    kind: gate
    depends_on: [plan]
    assignee: lead
    message: "Approve the plan?"
    options: [aprobar, ajustar]
    on: { ajustar: plan }
"#;

#[tokio::test]
async fn reconstructs_an_internal_gate_escalation_with_its_declared_options() {
    let (manifest, events) =
        paused_manifest_and_events(INTERNAL_GATE_WORKFLOW, "sessions: []\n").await;

    assert!(!events
        .iter()
        .any(|e| matches!(e.payload, yunta_core::events::EventPayload::GateWaiting(_))));

    let (node_id, escalation) = current_escalation(&manifest, &events)
        .expect("an unresolved internal gate must reconstruct an escalation");
    assert_eq!(node_id.as_str(), "approve");
    assert_eq!(escalation.summary, "Approve the plan?");
    let ids: Vec<&str> = escalation.options.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, vec!["aprobar", "ajustar", "abort"]);
    assert!(escalation.options.iter().all(|o| !o.tradeoff.is_empty()));
}

#[tokio::test]
async fn a_plain_failure_with_no_on_failure_has_no_escalation_to_reconstruct() {
    // A pause with no menu of options (no on_failure, no gate) — nothing
    // for `resolve_gate` to answer; the caller keeps its plain pause
    // reason instead of inventing a decision that doesn't exist.
    let workflow = r#"
name: plain-failure
nodes:
  - id: broken
    kind: bash
    run: "false"
"#;
    let (manifest, events) = paused_manifest_and_events(workflow, "sessions: []\n").await;
    assert!(current_escalation(&manifest, &events).is_none());
}

// --- resolve_gate (M8/T8.1.3): answers a paused run's escalation purely
// by appending to its log — the caller drives it forward separately.

use yunta_engine::{resolve_gate, ResolveGateError};

#[tokio::test]
async fn resolve_gate_answers_an_exhausted_reroute_and_a_plain_resume_finishes_it() {
    let root = tempfile::tempdir().unwrap();
    let worktree = root.path().join("worktree");
    std::fs::create_dir_all(&worktree).unwrap();
    init_repo(&worktree);
    let runs_root = root.path().join("runs");
    let storage = Storage::open(&root.path().join("yunta.db")).unwrap();
    let run_id = RunId::from("run-resolve-1");

    let workflow: Workflow = serde_yaml::from_str(HOPELESS_UNTIL_RETRIED_WORKFLOW).unwrap();
    let config: ConfigLayer = serde_yaml::from_str(CONFIG).unwrap();
    let manifest =
        build_manifest(&workflow, &config, &worktree, &worktree, &HashMap::new()).unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &run_id,
            manifest: &manifest,
            runs_root: &runs_root,
            mode: "default",
            promoted_from: None,
        },
        &storage,
        &FixedClock,
    )
    .unwrap();

    let adapter = MockAdapter::from_yaml(HOPELESS_UNTIL_RETRIED_FIXTURE).unwrap();
    let mut adapters: HashMap<String, std::sync::Arc<dyn Adapter>> = HashMap::new();
    adapters.insert("mock".to_string(), std::sync::Arc::new(adapter));
    let report = execute_run(
        &run_id,
        &manifest,
        &run_dir,
        &worktree,
        &adapters,
        &storage,
        &FixedClock,
        DEFAULT_MAX_RETRIES,
        &NoInteraction,
        None,
        None,
    )
    .await
    .unwrap();
    assert!(matches!(report.terminal, RunTerminal::Paused { .. }));

    // A separate "process" (same test, but nothing here reuses any live
    // state from the paused `execute_run` call above) resolves it.
    resolve_gate(
        &manifest,
        &storage,
        &run_id,
        &FixedClock,
        "retry",
        Some("mcp".to_string()),
        Some("looked fine to me".to_string()),
    )
    .unwrap();

    let events = storage.events_for_run(&run_id).unwrap();
    assert!(events
        .iter()
        .any(|e| matches!(e.payload, yunta_core::events::EventPayload::GateWaiting(_))));
    let resolved = events
        .iter()
        .find_map(|e| match &e.payload {
            yunta_core::events::EventPayload::GateResolved(p) => Some(p),
            _ => None,
        })
        .expect("gate_resolved must be on the log");
    assert_eq!(resolved.chosen_option.as_deref(), Some("retry"));
    assert_eq!(resolved.resolved_by.as_deref(), Some("mcp"));
    assert_eq!(resolved.free_text.as_deref(), Some("looked fine to me"));

    // An ordinary resume — no live process, no special knowledge of
    // what happened — picks up the reroute and finishes.
    let adapter = MockAdapter::from_yaml(
        r#"
sessions:
  - effects:
      - { path: fixed.txt, content: "fixed" }
    outcome: { type: completed, summary: "actually fixed it this time" }
"#,
    )
    .unwrap();
    let mut adapters: HashMap<String, std::sync::Arc<dyn Adapter>> = HashMap::new();
    adapters.insert("mock".to_string(), std::sync::Arc::new(adapter));
    let report = execute_run(
        &run_id,
        &manifest,
        &run_dir,
        &worktree,
        &adapters,
        &storage,
        &FixedClock,
        DEFAULT_MAX_RETRIES,
        &NoInteraction,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(report.terminal, RunTerminal::Finished);
}

#[tokio::test]
async fn resolve_gate_rejects_an_option_not_on_the_menu() {
    let (manifest, events) = paused_manifest_and_events(
        HOPELESS_UNTIL_RETRIED_WORKFLOW,
        HOPELESS_UNTIL_RETRIED_FIXTURE,
    )
    .await;
    let _ = events;

    let root = tempfile::tempdir().unwrap();
    let storage = Storage::open(&root.path().join("yunta.db")).unwrap();
    // Re-derive the same paused state onto a fresh storage handle isn't
    // meaningful here — what's under test is pure validation against
    // `current_escalation`'s own menu, so replay the same sequence this
    // storage needs: create + drive to the same pause.
    let worktree = root.path().join("worktree");
    std::fs::create_dir_all(&worktree).unwrap();
    init_repo(&worktree);
    let run_id = RunId::from("run-resolve-bad-option");
    let run_dir = create_run(
        CreateRunParams {
            run_id: &run_id,
            manifest: &manifest,
            runs_root: &root.path().join("runs"),
            mode: "default",
            promoted_from: None,
        },
        &storage,
        &FixedClock,
    )
    .unwrap();
    let adapter = MockAdapter::from_yaml(HOPELESS_UNTIL_RETRIED_FIXTURE).unwrap();
    let mut adapters: HashMap<String, std::sync::Arc<dyn Adapter>> = HashMap::new();
    adapters.insert("mock".to_string(), std::sync::Arc::new(adapter));
    execute_run(
        &run_id,
        &manifest,
        &run_dir,
        &worktree,
        &adapters,
        &storage,
        &FixedClock,
        DEFAULT_MAX_RETRIES,
        &NoInteraction,
        None,
        None,
    )
    .await
    .unwrap();

    let err = resolve_gate(
        &manifest,
        &storage,
        &run_id,
        &FixedClock,
        "nonexistent-option",
        None,
        None,
    )
    .unwrap_err();
    match err {
        ResolveGateError::UnknownOption { chosen, declared } => {
            assert_eq!(chosen, "nonexistent-option");
            assert!(declared.contains("retry"));
            assert!(declared.contains("abort"));
        }
        other => panic!("expected UnknownOption, got {other:?}"),
    }
}

#[tokio::test]
async fn resolve_gate_on_a_run_not_waiting_on_anything_answerable_errors() {
    let workflow = r#"
name: plain-failure
nodes:
  - id: broken
    kind: bash
    run: "false"
"#;
    let (manifest, _events) = paused_manifest_and_events(workflow, "sessions: []\n").await;

    let root = tempfile::tempdir().unwrap();
    let storage = Storage::open(&root.path().join("yunta.db")).unwrap();
    let worktree = root.path().join("worktree");
    std::fs::create_dir_all(&worktree).unwrap();
    init_repo(&worktree);
    let run_id = RunId::from("run-resolve-nothing");
    let run_dir = create_run(
        CreateRunParams {
            run_id: &run_id,
            manifest: &manifest,
            runs_root: &root.path().join("runs"),
            mode: "default",
            promoted_from: None,
        },
        &storage,
        &FixedClock,
    )
    .unwrap();
    let adapter = MockAdapter::from_yaml("sessions: []\n").unwrap();
    let mut adapters: HashMap<String, std::sync::Arc<dyn Adapter>> = HashMap::new();
    adapters.insert("mock".to_string(), std::sync::Arc::new(adapter));
    execute_run(
        &run_id,
        &manifest,
        &run_dir,
        &worktree,
        &adapters,
        &storage,
        &FixedClock,
        DEFAULT_MAX_RETRIES,
        &NoInteraction,
        None,
        None,
    )
    .await
    .unwrap();

    let err = resolve_gate(
        &manifest,
        &storage,
        &run_id,
        &FixedClock,
        "anything",
        None,
        None,
    )
    .unwrap_err();
    assert!(matches!(err, ResolveGateError::NothingToResolve));
}

#[tokio::test]
async fn resolve_gate_refuses_an_unresolved_internal_gate_with_an_actionable_error() {
    let workflow = r#"
name: internal-gate
nodes:
  - id: approve
    kind: gate
    assignee: lead
    message: "Approve?"
"#;
    let (manifest, _events) = paused_manifest_and_events(workflow, "sessions: []\n").await;

    let root = tempfile::tempdir().unwrap();
    let storage = Storage::open(&root.path().join("yunta.db")).unwrap();
    let worktree = root.path().join("worktree");
    std::fs::create_dir_all(&worktree).unwrap();
    init_repo(&worktree);
    let run_id = RunId::from("run-resolve-internal-gate");
    let run_dir = create_run(
        CreateRunParams {
            run_id: &run_id,
            manifest: &manifest,
            runs_root: &root.path().join("runs"),
            mode: "default",
            promoted_from: None,
        },
        &storage,
        &FixedClock,
    )
    .unwrap();
    let adapter = MockAdapter::from_yaml("sessions: []\n").unwrap();
    let mut adapters: HashMap<String, std::sync::Arc<dyn Adapter>> = HashMap::new();
    adapters.insert("mock".to_string(), std::sync::Arc::new(adapter));
    execute_run(
        &run_id,
        &manifest,
        &run_dir,
        &worktree,
        &adapters,
        &storage,
        &FixedClock,
        DEFAULT_MAX_RETRIES,
        &NoInteraction,
        None,
        None,
    )
    .await
    .unwrap();

    let err = resolve_gate(
        &manifest,
        &storage,
        &run_id,
        &FixedClock,
        "approve",
        None,
        None,
    )
    .unwrap_err();
    assert!(matches!(err, ResolveGateError::UnsupportedGateKind));
    // No decision was recorded — a resolve_gate refusal must be a
    // no-op on the log, not a half-applied attempt.
    let events = storage.events_for_run(&run_id).unwrap();
    assert!(!events
        .iter()
        .any(|e| matches!(e.payload, yunta_core::events::EventPayload::GateResolved(_))));
}
