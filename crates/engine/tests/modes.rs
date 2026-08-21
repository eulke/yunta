//! `modes:` (§10.1, D44, T9.1) exercised end-to-end: a node a mode
//! excludes is never scheduled, an in-mode node's dependency on an
//! excluded node is treated as already satisfied (§10.1's own
//! "quick"/"standard"/"full" example does exactly this — `implement`
//! depends_on the excluded `approve-plan` in "quick"), and the run
//! finishes once every *included* node is done, never waiting on one
//! that was never going to run.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use yunta_adapters::{Adapter, MockAdapter};
use yunta_core::{Clock, ConfigLayer, RunId, Workflow};
use yunta_engine::{
    build_manifest, create_run, execute_run, CreateRunParams, NoInteraction, NodeState, RunError,
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
        let workflow: Workflow = serde_yaml::from_str(WORKFLOW).unwrap();
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
                mode,
                promoted_from: None,
            },
            &self.storage,
            &FixedClock,
        )?;

        let adapter = MockAdapter::from_yaml(FIXTURE).unwrap();
        let mut adapters: HashMap<String, Arc<dyn Adapter>> = HashMap::new();
        adapters.insert("mock".to_string(), Arc::new(adapter));

        let report = execute_run(
            &run_id,
            &manifest,
            &run_dir,
            &self.worktree,
            &adapters,
            &self.storage,
            &FixedClock,
            DEFAULT_MAX_RETRIES,
            &NoInteraction,
            None,
            None,
        )
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
        state.nodes.get(&"start".into()),
        Some(NodeState::Finished { .. })
    ));
    assert!(matches!(
        state.nodes.get(&"ship".into()),
        Some(NodeState::Finished { .. })
    ));
    assert!(
        !state.nodes.contains_key(&"extra".into()),
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
            matches!(
                state.nodes.get(&id.into()),
                Some(NodeState::Finished { .. })
            ),
            "node `{id}` should have finished under `full`, got {:?}",
            state.nodes.get(&id.into())
        );
    }
}

#[tokio::test]
async fn the_default_sentinel_ignores_modes_and_runs_everything() {
    // `"default"` never validates against `modes:` and never filters —
    // `yunta test` (T7.9) relies on exactly this to exercise a moded
    // workflow's full graph without picking one mode out from under it.
    let bench = Bench::new();
    let (terminal, state) = bench.run("run-default", "default").await.unwrap();
    assert_eq!(terminal, RunTerminal::Finished);
    for id in ["start", "extra", "ship"] {
        assert!(matches!(
            state.nodes.get(&id.into()),
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
    assert!(bench.storage.list_run_ids().unwrap().is_empty());
}
