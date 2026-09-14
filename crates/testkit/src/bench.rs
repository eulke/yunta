//! `Bench` — the canonical run harness. It drives the real engine against
//! the mock adapter from a `(workflow, fixture)` pair, so an end-to-end run
//! is one call and every test observes the same production recipe:
//! `build_manifest` → `create_run` → `execute_run`, all derived from the
//! event log.

use std::collections::HashMap;
use std::sync::Arc;

use tempfile::TempDir;
use yunta_adapters::{MockAdapter, MockFixture, RunPaths};
use yunta_core::events::StoredEvent;
use yunta_core::port::Adapter;
use yunta_core::{AdapterId, ConfigLayer, RunId, Workflow};
use yunta_engine::{
    build_manifest, create_run, execute_run, BirthArtifact, CreateRunParams, HumanInteraction,
    NoInteraction, RunEnv, RunObserver, RunState, RunTerminal, DEFAULT_MAX_RETRIES,
};
use yunta_storage::Storage;
use yunta_testkit_core::SeqIdSource;

use yunta_testkit_core::FixedClock;

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
    ambient: Option<yunta_core::Env>,
    observer: Option<Arc<dyn RunObserver>>,
    /// What the run is born holding — a document its `inputs:` named,
    /// or what another run hands over.
    birth: Vec<BirthArtifact>,
    /// The adapter the last run used, so a test can ask what each
    /// session was actually handed.
    mock: std::sync::Mutex<Option<Arc<MockAdapter>>>,
    /// The workflow the last run was driven with, which is what reads a
    /// declared artifact name as the identity the log holds it under.
    workflow: std::sync::Mutex<Option<Workflow>>,
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
            ambient: None,
            observer: None,
            birth: Vec::new(),
            mock: std::sync::Mutex::new(None),
            workflow: std::sync::Mutex::new(None),
        }
    }

    /// Points the run's user state root at `root`, so a `knowledge:
    /// { layers: [user] }` source resolves `root/knowledge` — injected here
    /// rather than through a mutated `YUNTA_HOME`.
    pub fn with_user_state_root(mut self, root: impl Into<std::path::PathBuf>) -> Self {
        self.ambient = Some(yunta_core::Env {
            yunta_home: Some(root.into()),
            ..Default::default()
        });
        self
    }

    /// Gives the run `artifacts` from birth — what a `type: document`
    /// input, a mount or a promotion hands a run before any of its nodes
    /// runs.
    pub fn born_holding(mut self, artifacts: Vec<BirthArtifact>) -> Self {
        self.birth = artifacts;
        self
    }

    /// Commits `yaml` as `.yunta/workflows/<name>.yaml` in the bench's
    /// worktree, which is what a `kind: workflow` node's `use: <name>`
    /// resolves against when it gives birth to its child run.
    pub fn with_workflow(self, name: &str, yaml: &str) -> Self {
        let catalog = self.worktree.join(".yunta").join("workflows");
        std::fs::create_dir_all(&catalog).expect("create workflow catalog");
        std::fs::write(catalog.join(format!("{name}.yaml")), yaml).expect("write child workflow");
        crate::repo::git(&self.worktree, &["add", "."]);
        crate::repo::git(&self.worktree, &["commit", "-q", "-m", "catalog"]);
        self
    }

    /// Feeds every event this bench's run appends — the events of the
    /// `kind: workflow` children it gives birth to included — to
    /// `observer` as the engine writes it. Recording is complete when
    /// the `run*` call returns, since the engine delivers each frame
    /// inline.
    ///
    /// A bench drives one `execute_run` and stops there, so no promotion
    /// successor reports here; the chain that produces those is the
    /// CLI's `drive_promotions`.
    pub fn with_observer(mut self, observer: Arc<dyn RunObserver>) -> Self {
        self.observer = Some(observer);
        self
    }

    /// The absolute run dir this bench's run uses — known before the run
    /// exists, so a fixture can embed the absolute artifact paths a real
    /// agent would write after reading `{{run.dir}}`.
    pub fn run_dir(&self) -> std::path::PathBuf {
        self.runs_root.join(self.run_id.as_str())
    }

    /// The directory one node writes the files it declares into — known
    /// before the run exists, so a fixture can embed the absolute paths a
    /// session is granted and writes to.
    pub fn staging(&self, node: &str) -> std::path::PathBuf {
        yunta_engine::run_dir::staging(&self.run_dir(), &node.into())
    }

    /// The bytes the run holds for one artifact, by the identity it is
    /// declared under: a kind name (`tasks`) for a document the engine
    /// reads, a file name for an opaque one.
    ///
    /// What a reader of the run gets: the acceptance standing last on the
    /// run's own log for that identity, read out of its object store. A
    /// test asserts on the run's answer rather than on a file that
    /// happens to sit beside it. `None` when the run's log holds no such
    /// artifact.
    pub fn artifact(&self, declared: &str) -> Option<Vec<u8>> {
        let spec: yunta_core::ArtifactSpec = yunta_core::yaml::parse(declared)
            .expect("an artifact is named the way `artifacts.produces` names one");
        let id = yunta_core::events::ArtifactId::from(&spec);
        let held = crate::events::accepted(&self.events());
        let found = held
            .iter()
            .filter(|a| a.artifact == id)
            .max_by_key(|a| a.seq)?;
        self.object(&found.content_hash).ok()
    }

    /// The bytes of one artifact's `artifacts/` view — a producer's under
    /// its node, what the run acquired without one at the root.
    ///
    /// For a test about the projection itself, or about what a session
    /// left in the run's `artifacts/` directory; every test about what
    /// the run holds asks [`artifact`](Self::artifact).
    pub fn projection(&self, producer: Option<&str>, name: &str) -> std::io::Result<Vec<u8>> {
        let mut path = self.run_dir().join(yunta_core::ARTIFACTS_DIR);
        if let Some(node) = producer {
            path.push(node);
        }
        std::fs::read(path.join(name))
    }

    /// The bytes the run stored under `hash` — what an acceptance names,
    /// read straight out of the object store.
    pub fn object(&self, hash: &yunta_core::ContentHash) -> std::io::Result<Vec<u8>> {
        std::fs::read(self.run_dir().join("objects").join(hash.as_str()))
    }

    /// Every artifact this run's log says it accepted, in log order.
    pub fn accepted(&self) -> Vec<yunta_core::events::artifacts::ArtifactRef> {
        crate::events::accepted(&self.events())
    }

    /// Every event this bench's run has appended.
    /// The adapter the last run used — what a test asks about the
    /// requests the engine actually made.
    pub fn mock(&self) -> Arc<MockAdapter> {
        self.mock
            .lock()
            .expect("the bench's own lock")
            .clone()
            .expect("a run has to happen before its sessions can be asked about")
    }

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

    /// Runs with the secrets a test hands it, instead of reaching the
    /// process environment: the values are the test's to choose, so what
    /// happens to them is the test's to assert.
    pub async fn run_with_secrets(
        &self,
        workflow_yaml: &str,
        fixture_yaml: &str,
        config_yaml: &str,
        secrets: &[(&str, &str)],
    ) -> (RunTerminal, RunState) {
        let known: std::collections::HashMap<String, String> = secrets
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        self.run_with(
            workflow_yaml,
            fixture_yaml,
            config_yaml,
            &NoInteraction,
            Some(std::sync::Arc::new(KnownSecrets(known))),
        )
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
        self.run_with(
            workflow_yaml,
            fixture_yaml,
            config_yaml,
            human_interaction,
            None,
        )
        .await
    }

    /// The one body every `run…` helper ends in.
    async fn run_with(
        &self,
        workflow_yaml: &str,
        fixture_yaml: &str,
        config_yaml: &str,
        human_interaction: &dyn HumanInteraction,
        secrets: Option<std::sync::Arc<dyn yunta_core::SecretSource>>,
    ) -> (RunTerminal, RunState) {
        let workflow: Workflow = serde_norway::from_str(workflow_yaml).expect("parse workflow");
        *self.workflow.lock().expect("the bench's own lock") = Some(workflow.clone());
        let config: ConfigLayer = serde_norway::from_str(config_yaml).expect("parse config");
        let manifest = build_manifest(
            &workflow,
            &config,
            &self.worktree,
            &self.worktree,
            &HashMap::new(),
        )
        .await
        .expect("build manifest")
        .manifest;

        let run_dir = create_run(
            CreateRunParams {
                run_id: &self.run_id,
                manifest: &manifest,
                runs_root: &self.runs_root,
                mode: &"default".into(),
                worktree: &self.worktree,
                promoted_from: None,
                artifacts: &self.birth,
            },
            &self.storage.async_handle(),
            &FixedClock,
        )
        .await
        .expect("create run");

        let adapter = Arc::new(MockAdapter::new(
            MockFixture::parse(
                fixture_yaml,
                &RunPaths {
                    run_dir: &run_dir,
                    worktree: &self.worktree,
                    staging: &yunta_engine::run_dir::staging_root(&run_dir),
                },
            )
            .expect("parse mock fixture"),
        ));
        *self.mock.lock().expect("the bench's own lock") = Some(adapter.clone());
        let mut adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
        adapters.insert("mock".into(), adapter);

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
            ambient: self.ambient.as_ref(),
            secrets: secrets.clone(),
            // The bench runs no binary, so nothing can run a hook:
            // an adapter whose fence needs one fails the session, and
            // the mock judges in process.
            fence_hook: None,
            observer: self.observer.clone(),
        })
        .await
        .expect("execute run");
        (report.terminal, report.state)
    }
}

/// The secrets a test chose, instead of whatever the process happens to
/// carry: a test that reached the real environment would assert about a
/// machine rather than about the engine.
struct KnownSecrets(std::collections::HashMap<String, String>);

impl yunta_core::SecretSource for KnownSecrets {
    fn get(&self, name: &str) -> Option<yunta_core::Secret<String>> {
        self.0.get(name).cloned().map(yunta_core::Secret::from)
    }
}
