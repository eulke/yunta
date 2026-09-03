//! `Bench` — the canonical run harness. It drives the real engine against
//! the mock adapter from a `(workflow, fixture)` pair, so an end-to-end run
//! is one call and every test observes the same production recipe:
//! `build_manifest` → `create_run` → `execute_run`, all derived from the
//! event log.

use std::collections::HashMap;
use std::sync::Arc;

use tempfile::TempDir;
use yunta_adapters::{Adapter, MockAdapter};
use yunta_core::events::StoredEvent;
use yunta_core::{AdapterId, ConfigLayer, RunId, SeqIdSource, Workflow};
use yunta_engine::{
    build_manifest, create_run, execute_run, CreateRunParams, HumanInteraction, NoInteraction,
    RunEnv, RunState, RunTerminal, DEFAULT_MAX_RETRIES,
};
use yunta_storage::Storage;

use crate::clock::FixedClock;

/// The `runners:` config every mock-backed run resolves against — one
/// `planner` and one `executor`, both bound to the mock adapter.
pub const MOCK_CONFIG: &str = "\
runners:
  planner:
    - { adapter: mock, model: mock-model }
  executor:
    - { adapter: mock, model: mock-model }
";

/// A single run's world: a fresh temp repo, its runs root, and an open
/// event store. Everything a run needs is derived from this.
pub struct Bench {
    _root: TempDir,
    /// The checkout a run's nodes execute against.
    pub worktree: std::path::PathBuf,
    /// The directory run directories are created under.
    pub runs_root: std::path::PathBuf,
    /// The event store the run appends to.
    pub storage: Storage,
    /// The id of the run this bench creates.
    pub run_id: RunId,
    ids: SeqIdSource,
}

impl Default for Bench {
    fn default() -> Self {
        Self::new()
    }
}

impl Bench {
    /// A bench whose run is `run-test-1`.
    pub fn new() -> Self {
        Self::with_run_id("run-test-1")
    }

    /// A bench whose run carries a caller-chosen id — for a test that reads
    /// or asserts on the id, or runs several benches side by side.
    pub fn with_run_id(run_id: &str) -> Self {
        let root = tempfile::tempdir().expect("tempdir");
        let worktree = root.path().join("worktree");
        std::fs::create_dir_all(&worktree).expect("create worktree");
        crate::repo::init_repo(&worktree);
        let runs_root = root.path().join("runs");
        let storage = Storage::open(&root.path().join("yunta.db")).expect("open storage");
        Bench {
            _root: root,
            worktree,
            runs_root,
            storage,
            run_id: RunId::from(run_id),
            ids: SeqIdSource::new("minted"),
        }
    }

    /// The absolute run dir this bench's run uses — known before the run
    /// exists, so a fixture can embed the absolute artifact paths a real
    /// agent would write after reading `{{run.dir}}`.
    pub fn run_dir(&self) -> std::path::PathBuf {
        self.runs_root.join(self.run_id.as_str())
    }

    /// Every event this bench's run has appended.
    pub fn events(&self) -> Vec<StoredEvent> {
        self.storage
            .events_for_run(&self.run_id)
            .expect("read events")
    }

    /// Runs `(workflow, fixture)` against the default [`MOCK_CONFIG`] with
    /// no human present (a gate degrades to pause).
    pub async fn run(&self, workflow_yaml: &str, fixture_yaml: &str) -> (RunTerminal, RunState) {
        self.run_full(workflow_yaml, fixture_yaml, MOCK_CONFIG, &NoInteraction)
            .await
    }

    /// Runs with a caller-chosen config layer — for a test that needs
    /// `baseline:`/`coverage:`/`limits:` alongside the usual `runners:`.
    pub async fn run_with_config(
        &self,
        workflow_yaml: &str,
        fixture_yaml: &str,
        config_yaml: &str,
    ) -> (RunTerminal, RunState) {
        self.run_full(workflow_yaml, fixture_yaml, config_yaml, &NoInteraction)
            .await
    }

    /// Runs with a caller-chosen interaction surface — for a test that
    /// scripts a gate's resolution instead of degrading to pause.
    pub async fn run_with_interaction(
        &self,
        workflow_yaml: &str,
        fixture_yaml: &str,
        human_interaction: &dyn HumanInteraction,
    ) -> (RunTerminal, RunState) {
        self.run_full(workflow_yaml, fixture_yaml, MOCK_CONFIG, human_interaction)
            .await
    }

    /// The fully parameterized run every other `run*` helper delegates to.
    pub async fn run_full(
        &self,
        workflow_yaml: &str,
        fixture_yaml: &str,
        config_yaml: &str,
        human_interaction: &dyn HumanInteraction,
    ) -> (RunTerminal, RunState) {
        let workflow: Workflow = serde_yaml::from_str(workflow_yaml).expect("parse workflow");
        let config: ConfigLayer = serde_yaml::from_str(config_yaml).expect("parse config");
        let manifest = build_manifest(
            &workflow,
            &config,
            &self.worktree,
            &self.worktree,
            &HashMap::new(),
        )
        .expect("build manifest");

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
        .expect("create run");

        let adapter = MockAdapter::from_yaml(fixture_yaml).expect("parse mock fixture");
        let mut adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
        adapters.insert("mock".into(), Arc::new(adapter));

        let report = execute_run(RunEnv {
            run_id: &self.run_id,
            manifest: &manifest,
            run_dir: &run_dir,
            worktree: &self.worktree,
            adapters: &adapters,
            storage: &self.storage.async_handle(),
            clock: Arc::new(FixedClock),
            ids: &self.ids,
            max_task_retries: DEFAULT_MAX_RETRIES,
            human_interaction,
            forge: None,
            cancel: None,
            adapter_override: None,
        })
        .await
        .expect("execute run");
        (report.terminal, report.state)
    }
}
