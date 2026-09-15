//! `Bench` — the canonical run harness. It drives the real engine against
//! the mock adapter from a `(workflow, fixture)` pair, so an end-to-end run
//! is one call and every test observes the same production recipe:
//! `build_manifest` → `create_run` → `execute_run`, all derived from the
//! event log.
//!
//! The world a run happens in is stated here; [`driving`] creates the
//! run and wakes it, and [`reading`] answers what it left behind.

mod driving;
mod reading;

use std::collections::HashMap;
use std::sync::Arc;

use tempfile::TempDir;
use yunta_adapters::MockAdapter;
use yunta_core::port::Adapter;
use yunta_core::{AdapterId, Clock, RunId};
use yunta_engine::{BirthArtifact, RunObserver};
use yunta_storage::Storage;
use yunta_testkit_core::{FixedClock, SeqIdSource};

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
    /// The tree this bench owns — absent in a bench that shares
    /// another's world, which outlives neither it nor the tree.
    _root: Option<TempDir>,
    /// Where the event store lives, so a second bench in this world
    /// opens the same one.
    db: std::path::PathBuf,
    /// The checkout a run's nodes execute against.
    pub worktree: std::path::PathBuf,
    /// The directory run directories are created under.
    pub runs_root: std::path::PathBuf,
    /// The event store the run appends to.
    pub storage: Storage,
    /// The id of the run this bench creates.
    pub run_id: RunId,
    ids: Arc<dyn yunta_core::IdSource>,
    clock: Arc<dyn Clock>,
    forge: Option<Arc<dyn yunta_core::port::Forge>>,
    cancel: Option<tokio_util::sync::CancellationToken>,
    mode: yunta_core::ModeName,
    /// What the invocation hands the workflow's `inputs:`.
    inputs: HashMap<yunta_core::InputName, String>,
    /// Where a `prompt: { file: … }` resolves from and where a pack's
    /// provenance is read — the worktree unless a test names a pack.
    workflow_dir: Option<std::path::PathBuf>,
    ambient: Option<yunta_core::Env>,
    observer: Option<Arc<dyn RunObserver>>,
    /// What the run is born holding — a document its `inputs:` named,
    /// or what another run hands over.
    birth: Vec<BirthArtifact>,
    /// What the last run left behind. A bench answers about the run it
    /// drove, and wakes it again, without the test carrying any of it:
    /// one reading, taken under one lock, so two answers can never come
    /// from two different runs.
    driven: std::sync::Mutex<Option<Driven>>,
}

/// The run a bench created, as its next wake needs it.
pub(super) struct Driven {
    manifest: yunta_core::Manifest,
    run_dir: std::path::PathBuf,
    adapters: HashMap<AdapterId, Arc<dyn Adapter>>,
    /// The adapter the run used, so a test can ask what each session was
    /// actually handed.
    mock: Arc<MockAdapter>,
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
        let db = root.path().join("yunta.db");
        let storage = Storage::open(&db).expect("open storage");
        Bench {
            _root: Some(root),
            db,
            worktree,
            runs_root,
            storage,
            run_id: RunId::from(run_id),
            ids: Arc::new(SeqIdSource::new("minted")),
            clock: Arc::new(FixedClock),
            forge: None,
            cancel: None,
            mode: yunta_core::ModeName::default(),
            inputs: HashMap::new(),
            workflow_dir: None,
            ambient: None,
            observer: None,
            birth: Vec::new(),
            driven: std::sync::Mutex::new(None),
        }
    }

    /// A second run in this bench's world: the same worktree, runs root
    /// and event store, under a run id of its own — what a run that
    /// hands something over to the next one needs, since both sides
    /// have to read one log.
    ///
    /// The tree belongs to the bench it was made from, which has to
    /// outlive this one.
    pub fn beside(&self, run_id: &str) -> Self {
        Bench {
            _root: None,
            db: self.db.clone(),
            worktree: self.worktree.clone(),
            runs_root: self.runs_root.clone(),
            storage: Storage::open(&self.db).expect("open storage"),
            run_id: RunId::from(run_id),
            ids: self.ids.clone(),
            clock: self.clock.clone(),
            forge: self.forge.clone(),
            cancel: self.cancel.clone(),
            mode: self.mode.clone(),
            inputs: self.inputs.clone(),
            workflow_dir: self.workflow_dir.clone(),
            ambient: self.ambient.clone(),
            observer: self.observer.clone(),
            birth: Vec::new(),
            driven: std::sync::Mutex::new(None),
        }
    }

    /// Stamps this bench's run with `clock` instead of the frozen
    /// default — for a test that asserts on the instants its events
    /// carry, or that needs two runs stamped apart.
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    /// Mints the ids of the runs this one gives birth to from `ids` —
    /// for a test that asserts on a child's or a successor's id.
    pub fn with_ids(self, ids: SeqIdSource) -> Self {
        self.with_id_source(Arc::new(ids))
    }

    /// The same, from any source of ids — for a test about what a run
    /// does when the ids it is handed collide or run out.
    pub fn with_id_source(mut self, ids: Arc<dyn yunta_core::IdSource>) -> Self {
        self.ids = ids;
        self
    }

    /// Hands the workflow's `inputs:` what the invocation provides —
    /// a `yunta run -i name=value`, as the manifest freezes it.
    pub fn with_inputs(mut self, inputs: &[(&str, &str)]) -> Self {
        self.inputs = inputs
            .iter()
            .map(|(name, value)| ((*name).into(), (*value).to_string()))
            .collect();
        self
    }

    /// Freezes the workflow from `dir` rather than from the worktree —
    /// a pack's root, which is where its `prompt: { file: … }` and its
    /// provenance are read from.
    pub fn with_workflow_dir(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
        self.workflow_dir = Some(dir.into());
        self
    }

    /// Layers `vars` onto every subprocess the run spawns — what puts a
    /// stub on `PATH` for a bash node without mutating this process.
    pub fn with_subprocess_vars(mut self, vars: Vec<(String, String)>) -> Self {
        self.ambient
            .get_or_insert_with(Default::default)
            .subprocess_vars = vars;
        self
    }

    /// Cancels the run when `cancel` fires — the token a `yunta cancel`
    /// from another process trips.
    pub fn with_cancel(mut self, cancel: tokio_util::sync::CancellationToken) -> Self {
        self.cancel = Some(cancel);
        self
    }

    /// Creates the run in `mode` — the subset of the graph a `yunta run
    /// --mode` drives, instead of the whole of it.
    pub fn in_mode(mut self, mode: &str) -> Self {
        self.mode = yunta_core::ModeName::from(mode);
        self
    }

    /// Puts `forge` behind every wake of this run — what an external
    /// gate publishes to and later reads its resolution from.
    ///
    /// The forge belongs to the bench's world rather than to one call
    /// because a gate spans wakes: the wake that publishes and the wake
    /// that finds the answer are separate processes reading one forge.
    pub fn with_forge(mut self, forge: Arc<dyn yunta_core::port::Forge>) -> Self {
        self.forge = Some(forge);
        self
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

    /// Reads what the last run left behind.
    pub(super) fn driven<T>(&self, read: impl FnOnce(&Driven) -> T) -> T {
        read(
            self.driven
                .lock()
                .expect("the bench's own lock")
                .as_ref()
                .expect("a run has to happen before the bench can be asked about it"),
        )
    }
}
