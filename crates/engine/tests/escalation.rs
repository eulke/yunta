//! `current_escalation` reconstructs the escalation
//! object a paused run is currently waiting on — purely from the
//! manifest and its own log, no live process required. This is what
//! lets `resolve_gate` (a separate `yunta mcp` invocation, not the
//! process that paused the run) know what it's answering.

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Utc};
use yunta_adapters::{Adapter, MockAdapter};
use yunta_core::SeqIdSource;
use yunta_core::{AdapterId, Clock, ConfigLayer, RunId, Workflow};
use yunta_engine::{
    build_manifest, create_run, current_escalation, execute_run, CreateRunParams, NoInteraction,
    RunEnv, RunTerminal, DEFAULT_MAX_RETRIES,
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
) -> (yunta_core::Manifest, Vec<yunta_core::events::StoredEvent>) {
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
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();

    let adapter = MockAdapter::from_yaml(fixture_yaml).unwrap();
    let mut adapters: HashMap<AdapterId, std::sync::Arc<dyn Adapter>> = HashMap::new();
    adapters.insert("mock".into(), std::sync::Arc::new(adapter));

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
    assert!(!events.iter().any(|e| matches!(
        e.payload(),
        Some(yunta_core::events::EventPayload::GateWaiting(_))
    )));

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

    assert!(!events.iter().any(|e| matches!(
        e.payload(),
        Some(yunta_core::events::EventPayload::GateWaiting(_))
    )));

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

// --- resolve_gate writes ONLY the decision; the engine
// consumes it on wake through its one existing consequence path.

use yunta_engine::{resolve_gate, ResolveGateError, RunState};

struct GateBench {
    _root: tempfile::TempDir,
    worktree: std::path::PathBuf,
    storage: Storage,
    run_id: RunId,
    manifest: yunta_core::Manifest,
    run_dir: std::path::PathBuf,
}

impl GateBench {
    /// Creates the run and drives it with `NoInteraction` to its first
    /// stop — asserting it paused, since every test here starts from a
    /// parked run.
    async fn paused(workflow_yaml: &str, fixture_yaml: &str) -> Self {
        let bench = Self::created(workflow_yaml).await;
        let (terminal, _) = bench.execute(fixture_yaml, &NoInteraction).await;
        assert!(
            matches!(terminal, RunTerminal::Paused { .. }),
            "expected the run to pause, got {terminal:?}"
        );
        bench
    }

    async fn created(workflow_yaml: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let worktree = root.path().join("worktree");
        std::fs::create_dir_all(&worktree).unwrap();
        init_repo(&worktree);
        let runs_root = root.path().join("runs");
        let storage = Storage::open(&root.path().join("yunta.db")).unwrap();
        let run_id = RunId::from("run-gate-bench");
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
        GateBench {
            _root: root,
            worktree,
            storage,
            run_id,
            manifest,
            run_dir,
        }
    }

    async fn execute(
        &self,
        fixture_yaml: &str,
        interaction: &dyn yunta_engine::HumanInteraction,
    ) -> (RunTerminal, RunState) {
        let adapter = MockAdapter::from_yaml(fixture_yaml).unwrap();
        let mut adapters: HashMap<AdapterId, std::sync::Arc<dyn Adapter>> = HashMap::new();
        adapters.insert("mock".into(), std::sync::Arc::new(adapter));
        let report = execute_run(RunEnv {
            run_id: &self.run_id,
            manifest: &self.manifest,
            run_dir: &self.run_dir,
            worktree: &self.worktree,
            adapters: &adapters,
            storage: &self.storage.async_handle(),
            clock: std::sync::Arc::new(FixedClock),
            ids: &IDS,
            max_task_retries: DEFAULT_MAX_RETRIES,
            human_interaction: interaction,
            forge: None,
            cancel: None,
            adapter_override: None,
        })
        .await
        .unwrap();
        (report.terminal, report.state)
    }

    async fn resolve(&self, option: &str) -> Result<(), ResolveGateError> {
        resolve_gate(
            &self.manifest,
            &self.storage.async_handle(),
            &self.run_id,
            &FixedClock,
            option,
            Some("mcp".to_string()),
            None,
        )
        .await
    }

    fn events(&self) -> Vec<yunta_core::events::StoredEvent> {
        self.storage.events_for_run(&self.run_id).unwrap()
    }

    fn count(&self, pred: impl Fn(&yunta_core::events::EventPayload) -> bool) -> usize {
        self.events()
            .iter()
            .filter(|e| e.payload().is_some_and(&pred))
            .count()
    }
}

const RETRY_FIX_FIXTURE: &str = r#"
sessions:
  - effects:
      - { path: fixed.txt, content: "fixed" }
    outcome: { type: completed, summary: "actually fixed it this time" }
"#;

#[tokio::test]
async fn a_pre_seeded_retry_is_consumed_by_a_plain_resume_and_finishes() {
    let bench = GateBench::paused(
        HOPELESS_UNTIL_RETRIED_WORKFLOW,
        HOPELESS_UNTIL_RETRIED_FIXTURE,
    )
    .await;

    bench.resolve("retry").await.unwrap();
    // resolve_gate writes ONLY the decision pair — the reroute
    // consequence is the engine's to apply, not this function's.
    assert_eq!(
        bench.count(|p| matches!(p, yunta_core::events::EventPayload::NodeRerouted(_))),
        1,
        "only the run's own automatic reroute is on the log before the resume"
    );

    let (terminal, state) = bench.execute(RETRY_FIX_FIXTURE, &NoInteraction).await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.get("lint"),
        Some(yunta_engine::NodeState::Finished { .. })
    ));
    // The consuming engine never re-emits the recorded pair.
    assert_eq!(
        bench.count(|p| matches!(p, yunta_core::events::EventPayload::GateWaiting(_))),
        1
    );
    assert_eq!(
        bench.count(|p| matches!(p, yunta_core::events::EventPayload::GateResolved(_))),
        1
    );
}

const PROMOTABLE_WORKFLOW: &str = r#"
name: promotable
modes:
  quick: { include: [lint, fix-lint] }
  full:  { include: [ship] }
nodes:
  - id: lint
    kind: bash
    run: "test -f fixed.txt"
    on_failure: { goto: fix-lint, max_reroutes: 0 }
  - id: fix-lint
    kind: bash
    run: "true"
  - id: ship
    kind: bash
    run: "echo shipped > shipped.txt"
"#;

#[tokio::test]
async fn a_pre_seeded_promote_closes_the_run_as_promoted_on_resume() {
    // The paused run sits in mode `quick` with `full` later in the
    // declaration order — `promote` is on its menu. resolve_gate
    // records the choice; the resuming engine (a live process, exactly
    // what promotion's distill+close needs) applies it.
    let root = tempfile::tempdir().unwrap();
    let worktree = root.path().join("worktree");
    std::fs::create_dir_all(&worktree).unwrap();
    init_repo(&worktree);
    let storage = Storage::open(&root.path().join("yunta.db")).unwrap();
    let run_id = RunId::from("run-preseed-promote");
    let workflow: Workflow = serde_yaml::from_str(PROMOTABLE_WORKFLOW).unwrap();
    let config: ConfigLayer = serde_yaml::from_str(CONFIG).unwrap();
    let manifest =
        build_manifest(&workflow, &config, &worktree, &worktree, &HashMap::new()).unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &run_id,
            manifest: &manifest,
            runs_root: &root.path().join("runs"),
            mode: &"quick".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();
    async fn drive(
        run_id: &RunId,
        manifest: &yunta_core::Manifest,
        run_dir: &std::path::Path,
        worktree: &std::path::Path,
        storage: &Storage,
    ) -> yunta_engine::RunReport {
        let adapter = MockAdapter::from_yaml("sessions: []\n").unwrap();
        let mut adapters: HashMap<AdapterId, std::sync::Arc<dyn Adapter>> = HashMap::new();
        adapters.insert("mock".into(), std::sync::Arc::new(adapter));
        execute_run(RunEnv {
            run_id,
            manifest,
            run_dir,
            worktree,
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
        .unwrap()
    }

    let report = drive(&run_id, &manifest, &run_dir, &worktree, &storage).await;
    assert!(matches!(report.terminal, RunTerminal::Paused { .. }));

    resolve_gate(
        &manifest,
        &storage.async_handle(),
        &run_id,
        &FixedClock,
        "promote",
        Some("mcp".to_string()),
        None,
    )
    .await
    .unwrap();

    let report = drive(&run_id, &manifest, &run_dir, &worktree, &storage).await;
    assert_eq!(
        report.terminal,
        RunTerminal::Promoted {
            suggested_mode: "full".into()
        }
    );
    let events = storage.events_for_run(&run_id).unwrap();
    assert!(events.iter().any(|e| matches!(
        e.payload(),
        Some(yunta_core::events::EventPayload::PromotionSignaled(_))
    )));
    assert!(events.iter().any(|e| matches!(
        e.payload(),
        Some(yunta_core::events::EventPayload::RunFinished(p)) if p.terminal_state == yunta_core::events::TerminalState::Promoted
    )));
}

const INTERNAL_GATE_DAG_WORKFLOW: &str = r#"
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
    run: "echo done > shipped.txt"
"#;

fn plan_runs(worktree: &std::path::Path) -> usize {
    std::fs::read_to_string(worktree.join("plan-runs.txt"))
        .map(|s| s.lines().count())
        .unwrap_or(0)
}

#[tokio::test]
async fn a_pre_seeded_internal_gate_unmapped_option_finishes_the_gate_on_resume() {
    let bench = GateBench::paused(INTERNAL_GATE_DAG_WORKFLOW, "sessions: []\n").await;

    bench.resolve("aprobar").await.unwrap();
    let (terminal, state) = bench.execute("sessions: []\n", &NoInteraction).await;

    assert_eq!(terminal, RunTerminal::Finished);
    match state.nodes.get("approve") {
        Some(yunta_engine::NodeState::Finished { outcome, .. }) => assert_eq!(outcome, "aprobar"),
        other => panic!("expected the gate finished with the chosen option, got {other:?}"),
    }
    assert!(bench.worktree.join("shipped.txt").exists());
    // One recorded pair — the consuming engine never re-emits it.
    assert_eq!(
        bench.count(|p| matches!(p, yunta_core::events::EventPayload::GateWaiting(_))),
        1
    );
    assert_eq!(
        bench.count(|p| matches!(p, yunta_core::events::EventPayload::GateResolved(_))),
        1
    );
}

#[tokio::test]
async fn a_pre_seeded_internal_gate_mapped_option_reroutes_and_asks_again() {
    let bench = GateBench::paused(INTERNAL_GATE_DAG_WORKFLOW, "sessions: []\n").await;
    assert_eq!(plan_runs(&bench.worktree), 1);

    bench.resolve("ajustar").await.unwrap();
    let (terminal, _) = bench.execute("sessions: []\n", &NoInteraction).await;

    // A reroute through a pre-seeded choice: plan re-ran, the gate came
    // back to ask again, and with no live surface the run parks there.
    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    assert_eq!(plan_runs(&bench.worktree), 2);
}

#[tokio::test]
async fn a_pre_seeded_abort_is_consumed_exactly_once() {
    let bench = GateBench::paused(
        HOPELESS_UNTIL_RETRIED_WORKFLOW,
        HOPELESS_UNTIL_RETRIED_FIXTURE,
    )
    .await;

    bench.resolve("abort").await.unwrap();
    let (terminal, _) = bench.execute("sessions: []\n", &NoInteraction).await;
    let RunTerminal::Paused { reason } = terminal else {
        panic!("expected the consumed abort to pause, got {terminal:?}");
    };
    assert!(reason.contains("abort"), "got: {reason}");

    // A later manual resume must NOT re-apply the stale abort — the
    // decision was consumed; with no live surface it parks on the
    // escalation again, exactly as an interactive abort does today.
    let (terminal, _) = bench.execute("sessions: []\n", &NoInteraction).await;
    let RunTerminal::Paused { reason } = terminal else {
        panic!("expected the second resume to pause, got {terminal:?}");
    };
    assert!(
        !reason.contains("abort") && reason.contains("exhausted"),
        "a stale abort must not be re-applied: {reason}"
    );
    assert_eq!(
        bench.count(|p| matches!(p, yunta_core::events::EventPayload::GateResolved(_))),
        1
    );
}

#[tokio::test]
async fn live_and_pre_seeded_retry_reach_the_same_final_state() {
    // As a property: the surface is presentation,
    // the decision is the data — answering live or by a later separate
    // process must land the run in the identical derived state.
    struct RetryOnce;
    #[async_trait::async_trait]
    impl yunta_engine::HumanInteraction for RetryOnce {
        async fn resolve(
            &self,
            _escalation: &yunta_core::events::GateWaitingPayload,
        ) -> Option<yunta_core::events::GateResolvedPayload> {
            Some(yunta_core::events::GateResolvedPayload {
                chosen_option: Some("retry".to_string()),
                resolved_by: Some("mcp".to_string()),
                free_text: None,
                approved_sha: None,
            })
        }
    }

    const TWO_SESSION_FIXTURE: &str = r#"
sessions:
  - outcome: { type: completed, summary: "did nothing useful" }
  - effects:
      - { path: fixed.txt, content: "fixed" }
    outcome: { type: completed, summary: "actually fixed it this time" }
"#;

    let live = GateBench::created(HOPELESS_UNTIL_RETRIED_WORKFLOW).await;
    let (live_terminal, live_state) = live.execute(TWO_SESSION_FIXTURE, &RetryOnce).await;

    let seeded = GateBench::paused(
        HOPELESS_UNTIL_RETRIED_WORKFLOW,
        HOPELESS_UNTIL_RETRIED_FIXTURE,
    )
    .await;
    seeded.resolve("retry").await.unwrap();
    let (seeded_terminal, seeded_state) = seeded.execute(RETRY_FIX_FIXTURE, &NoInteraction).await;

    assert_eq!(live_terminal, seeded_terminal);
    let project = |state: &RunState| -> std::collections::BTreeMap<String, String> {
        state
            .nodes
            .iter()
            .map(|(id, node)| (id.to_string(), format!("{node:?}")))
            .collect()
    };
    assert_eq!(project(&live_state), project(&seeded_state));
}

#[tokio::test]
async fn resolve_gate_refuses_a_run_that_is_not_parked() {
    let bench = GateBench::created(
        r#"
name: fine
nodes:
  - id: ok
    kind: bash
    run: "true"
"#,
    )
    .await;
    let (terminal, _) = bench.execute("sessions: []\n", &NoInteraction).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let err = bench.resolve("retry").await.unwrap_err();
    assert!(matches!(err, ResolveGateError::NotPaused));
}

#[tokio::test]
async fn resolve_gate_rejects_an_option_not_on_the_menu() {
    let bench = GateBench::paused(
        HOPELESS_UNTIL_RETRIED_WORKFLOW,
        HOPELESS_UNTIL_RETRIED_FIXTURE,
    )
    .await;
    let err = bench.resolve("nonexistent-option").await.unwrap_err();
    match err {
        ResolveGateError::UnknownOption { chosen, declared } => {
            assert_eq!(chosen, "nonexistent-option");
            assert!(declared.contains("retry") && declared.contains("abort"));
        }
        other => panic!("expected UnknownOption, got {other:?}"),
    }
    // A refusal is a no-op on the log.
    assert_eq!(
        bench.count(|p| matches!(p, yunta_core::events::EventPayload::GateResolved(_))),
        0
    );
}

#[tokio::test]
async fn resolve_gate_on_a_run_paused_without_a_menu_errors() {
    let bench = GateBench::paused(
        r#"
name: plain-failure
nodes:
  - id: broken
    kind: bash
    run: "false"
"#,
        "sessions: []\n",
    )
    .await;
    let err = bench.resolve("anything").await.unwrap_err();
    assert!(matches!(err, ResolveGateError::NothingToResolve));
}
