//! `modes:` exercised end-to-end: a node a mode excludes is never
//! scheduled; an in-mode node that depended on an excluded node waits
//! for the excluded node's own in-mode dependencies instead (the
//! "quick"/"standard"/"full" example does exactly this — `implement`
//! depends_on the excluded `approve-plan` in "quick", so it waits for
//! `plan`); and the run finishes once every *included* node is done,
//! never waiting on one that was never going to run.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use yunta_adapters::{Adapter, MockAdapter};
use yunta_core::{AdapterId, Clock, ConfigLayer, ModeName, RunId, Workflow};
use yunta_engine::{
    build_manifest, create_run, execute_run, CreateRunParams, NoInteraction, NodeState, RunEnv,
    RunError, RunTerminal, DEFAULT_MAX_RETRIES,
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

const WORKFLOW: &str = r#"
name: mode-scenario
modes:
  quick:  { include: [start, ship] }
  full:   { include: all }
nodes:
  - id: start
    kind: bash
    run: "true"
  - id: extra
    kind: bash
    depends_on: [start]
    run: "true"
  - id: ship
    kind: bash
    depends_on: [extra]
    run: "true"
"#;

const FIXTURE: &str = "sessions: []\n";

struct Bench {
    _root: tempfile::TempDir,
    worktree: std::path::PathBuf,
    runs_root: std::path::PathBuf,
    storage: Storage,
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
        }
    }

    /// Creates and drives a run in `mode` to completion (or pause) in
    /// one call — every test here needs exactly one wake.
    async fn run(
        &self,
        run_id: &str,
        mode: &str,
    ) -> Result<(RunTerminal, yunta_engine::RunState), RunError> {
        self.run_workflow(WORKFLOW, run_id, mode).await
    }

    async fn run_workflow(
        &self,
        workflow_yaml: &str,
        run_id: &str,
        mode: &str,
    ) -> Result<(RunTerminal, yunta_engine::RunState), RunError> {
        let workflow: Workflow = serde_yaml::from_str(workflow_yaml).unwrap();
        let manifest = build_manifest(
            &workflow,
            &ConfigLayer::default(),
            &self.worktree,
            &self.worktree,
            &HashMap::new(),
        )
        .unwrap();
        let run_id = RunId::from(run_id);
        let run_dir = create_run(
            CreateRunParams {
                run_id: &run_id,
                manifest: &manifest,
                runs_root: &self.runs_root,
                mode: &ModeName::from(mode),
                promoted_from: None,
            },
            &self.storage,
            &FixedClock,
        )?;

        let adapter = MockAdapter::from_yaml(FIXTURE).unwrap();
        let mut adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
        adapters.insert("mock".into(), Arc::new(adapter));

        let report = execute_run(RunEnv {
            run_id: &run_id,
            manifest: &manifest,
            run_dir: &run_dir,
            worktree: &self.worktree,
            adapters: &adapters,
            storage: &self.storage,
            clock: &FixedClock,
            max_task_retries: DEFAULT_MAX_RETRIES,
            human_interaction: &NoInteraction,
            forge: None,
            cancel: None,
            adapter_override: None,
        })
        .await
        .unwrap();
        Ok((report.terminal, report.state))
    }
}

#[tokio::test]
async fn quick_mode_skips_the_excluded_node_and_still_finishes() {
    let bench = Bench::new();
    let (terminal, state) = bench.run("run-quick", "quick").await.unwrap();
    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.get("start"),
        Some(NodeState::Finished { .. })
    ));
    assert!(matches!(
        state.nodes.get("ship"),
        Some(NodeState::Finished { .. })
    ));
    assert!(
        !state.nodes.contains_key("extra"),
        "a node excluded from the run's mode must never be scheduled at all"
    );
}

#[tokio::test]
async fn full_mode_runs_every_node() {
    let bench = Bench::new();
    let (terminal, state) = bench.run("run-full", "full").await.unwrap();
    assert_eq!(terminal, RunTerminal::Finished);
    for id in ["start", "extra", "ship"] {
        assert!(
            matches!(state.nodes.get(id), Some(NodeState::Finished { .. })),
            "node `{id}` should have finished under `full`, got {:?}",
            state.nodes.get(id)
        );
    }
}

#[tokio::test]
async fn the_default_sentinel_ignores_modes_and_runs_everything() {
    // `"default"` never validates against `modes:` and never filters —
    // `yunta test` relies on exactly this to exercise a moded
    // workflow's full graph without picking one mode out from under it.
    let bench = Bench::new();
    let (terminal, state) = bench.run("run-default", "default").await.unwrap();
    assert_eq!(terminal, RunTerminal::Finished);
    for id in ["start", "extra", "ship"] {
        assert!(matches!(
            state.nodes.get(id),
            Some(NodeState::Finished { .. })
        ));
    }
}

#[tokio::test]
async fn an_unknown_mode_name_is_refused_before_the_run_is_created() {
    let bench = Bench::new();
    let err = bench.run("run-bogus", "nonexistent").await.unwrap_err();
    assert!(matches!(err, RunError::UnknownMode { .. }), "got: {err:?}");
    // Refused before anything was written — no run directory, no event.
    assert!(bench.storage.list_runs().unwrap().is_empty());
}

/// Declared so that the dependent of the excluded node comes *first* in
/// declaration order: an edge merely "treated as satisfied" would make
/// `ship` ready before `start` ran at all.
const ORDER_WORKFLOW: &str = r#"
name: mode-order
modes:
  quick:  { include: [ship, start] }
  full:   { include: all }
nodes:
  - id: ship
    kind: bash
    depends_on: [extra]
    run: "true"
  - id: extra
    kind: bash
    depends_on: [start]
    run: "true"
  - id: start
    kind: bash
    run: "true"
"#;

const GATE_WORKFLOW: &str = r#"
name: mode-gate
modes:
  quick:  { include: [start, ship] }
  full:   { include: all }
nodes:
  - id: start
    kind: bash
    run: "true"
  - id: extra
    kind: bash
    depends_on: [start]
    run: "true"
  - id: ship
    kind: gate
    depends_on: [extra]
    assignee: lead
    message: "Ship it?"
"#;

fn seq_of(
    events: &[yunta_core::events::StoredEvent],
    node: &str,
    pick: impl Fn(&yunta_core::events::EventPayload) -> bool,
) -> u64 {
    events
        .iter()
        .find(|e| {
            e.node_id.as_ref().is_some_and(|id| id.as_str() == node)
                && e.payload().is_some_and(&pick)
        })
        .unwrap_or_else(|| panic!("no matching event for node `{node}`"))
        .seq
        .get()
}

#[tokio::test]
async fn a_dependent_of_an_excluded_node_waits_for_that_nodes_own_dependencies() {
    let bench = Bench::new();
    let (terminal, _) = bench
        .run_workflow(ORDER_WORKFLOW, "run-order", "quick")
        .await
        .unwrap();
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench
        .storage
        .events_for_run(&RunId::from("run-order"))
        .unwrap();
    use yunta_core::events::EventPayload;
    let start_finished = seq_of(&events, "start", |p| {
        matches!(p, EventPayload::NodeFinished(_))
    });
    let ship_started = seq_of(&events, "ship", |p| {
        matches!(p, EventPayload::NodeStarted(_))
    });
    assert!(
        start_finished < ship_started,
        "`ship` (depends_on the excluded `extra`, which depends_on `start`) started at seq \
         {ship_started}, before `start` finished at seq {start_finished}"
    );
}

#[tokio::test]
async fn a_gate_behind_an_excluded_node_waits_for_that_nodes_own_dependencies() {
    // With nobody to answer it, the internal gate pauses the run — but
    // only once everything it effectively depends on has run.
    let bench = Bench::new();
    let (terminal, state) = bench
        .run_workflow(GATE_WORKFLOW, "run-gate", "quick")
        .await
        .unwrap();
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "expected the unanswered gate to pause the run, got {terminal:?}"
    );
    assert!(
        matches!(state.nodes.get("start"), Some(NodeState::Finished { .. })),
        "`start` must finish before the gate that transitively depends on it is asked; got {:?}",
        state.nodes.get("start")
    );
}
