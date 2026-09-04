//! Every degradation the engine hits is recorded on the run's log — a
//! `finding_posted` or a `capability_degraded`, never a bare `tracing`
//! warning that leaves the log silent about what the engine could not do.
//! These runs provoke each degradation deterministically and read the
//! event back off storage.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use yunta_adapters::Adapter;
use yunta_core::events::{EventPayload, Finding, StoredEvent};
use yunta_core::{AdapterId, ConfigLayer, RunId, SeqIdSource, Workflow};
use yunta_engine::{
    build_manifest, create_run, execute_run, CreateRunParams, NoInteraction, RunEnv, RunTerminal,
    DEFAULT_MAX_RETRIES,
};
use yunta_storage::Storage;
use yunta_testkit::{git, init_repo, FixedClock};

static IDS: SeqIdSource = SeqIdSource::new("degradation");

const CONFIG: &str = r#"
runners:
  planner:
    - { adapter: mock, model: mock-model }
  executor:
    - { adapter: mock, model: mock-model }
"#;

struct Bench {
    _root: tempfile::TempDir,
    worktree: std::path::PathBuf,
    runs_root: std::path::PathBuf,
    storage: Storage,
    run_id: RunId,
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
            run_id: RunId::from("run-degradation"),
        }
    }

    /// Freezes the manifest and creates the run directory, then hands
    /// both to `sabotage` — the window a test uses to break something the
    /// run then trips over — before executing the run to its terminal.
    async fn run_sabotaged(
        &self,
        workflow_yaml: &str,
        sabotage: impl FnOnce(&Path),
    ) -> RunTerminal {
        let workflow: Workflow = serde_norway::from_str(workflow_yaml).unwrap();
        let config: ConfigLayer = serde_norway::from_str(CONFIG).unwrap();
        let manifest = build_manifest(
            &workflow,
            &config,
            &self.worktree,
            &self.worktree,
            &HashMap::new(),
        )
        .unwrap();

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
        .unwrap();

        sabotage(&run_dir);

        // The degradation workflows here are `bash`-only, so no adapter
        // is ever asked for a session.
        let adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
        let report = execute_run(RunEnv {
            run_id: &self.run_id,
            manifest: &manifest,
            run_dir: &run_dir,
            worktree: &self.worktree,
            adapters: &adapters,
            storage: &self.storage.async_handle(),
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
        report.terminal
    }

    async fn run(&self, workflow_yaml: &str) -> RunTerminal {
        self.run_sabotaged(workflow_yaml, |_| {}).await
    }

    fn events(&self) -> Vec<StoredEvent> {
        self.storage.events_for_run(&self.run_id).unwrap()
    }
}

/// The finding with `id`, or `None` — findings are the engine's own
/// record of a degradation, so a test asserts one is present by id.
fn finding(events: &[StoredEvent], id: &str) -> Option<Finding> {
    events.iter().find_map(|event| match event.payload() {
        Some(EventPayload::FindingPosted(p)) if p.finding.id.as_str() == id => {
            Some(p.finding.clone())
        }
        _ => None,
    })
}

#[tokio::test]
async fn an_unwritable_process_registry_is_recorded_as_a_finding() {
    // `create_run` makes `scratch/`; replacing `engine.json` with a
    // directory makes the registry's atomic write fail — the run must
    // record the loss of `yunta cancel` visibility, not warn and vanish.
    let bench = Bench::new();
    let workflow = r#"
name: degradation
nodes:
  - id: build
    kind: bash
    run: "true"
"#;
    let terminal = bench
        .run_sabotaged(workflow, |run_dir| {
            std::fs::create_dir_all(run_dir.join("scratch").join("engine.json")).unwrap();
        })
        .await;

    assert!(matches!(terminal, RunTerminal::Finished));
    let events = bench.events();
    let finding = finding(&events, "engine-registry")
        .expect("the unwritable registry must be recorded as a finding");
    assert!(
        finding.detail.contains("yunta cancel"),
        "the finding says what cancel cannot do: {}",
        finding.detail
    );
}

#[tokio::test]
async fn a_cleanup_on_a_primary_checkout_is_recorded_as_a_finding() {
    // `on_finish.cleanup: worktree` on a tree that is a primary checkout
    // (not a linked worktree) touches nothing — and says so with a
    // finding, never a warning only the operator's console would see.
    let bench = Bench::new();
    let workflow = r#"
name: degradation
nodes:
  - id: build
    kind: bash
    run: "true"
on_finish:
  - cleanup: worktree
"#;
    let terminal = bench.run(workflow).await;

    assert!(matches!(terminal, RunTerminal::Finished));
    assert!(
        finding(&bench.events(), "cleanup-not-a-worktree").is_some(),
        "a cleanup that cannot run on a primary checkout must be a finding"
    );
}

#[tokio::test]
async fn a_failed_distill_commit_is_recorded_as_a_finding() {
    // A failing `pre-commit` hook makes distill's `git commit` fail; the
    // files stay on disk and the failure is a finding on the log.
    let bench = Bench::new();
    let hooks = bench.worktree.join(".git-hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    let pre_commit = hooks.join("pre-commit");
    std::fs::write(&pre_commit, "#!/bin/sh\nexit 1\n").unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&pre_commit, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    git(
        &bench.worktree,
        &["config", "core.hooksPath", hooks.to_str().unwrap()],
    );

    let workflow = r#"
name: degradation
nodes:
  - id: build
    kind: bash
    run: "true"
on_finish:
  - distill:
      - notes.md
"#;
    let terminal = bench.run(workflow).await;

    assert!(matches!(terminal, RunTerminal::Finished));
    assert!(
        finding(&bench.events(), "distill-commit").is_some(),
        "a distill commit blocked by a failing hook must be a finding"
    );
}
