//! Every degradation the engine hits is recorded on the run's log — a
//! `finding_posted` or a `capability_degraded`, never a bare `tracing`
//! warning that leaves the log silent about what the engine could not do.
//! These runs provoke each degradation deterministically and read the
//! event back off storage.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use yunta_core::events::FindingEvent;
use yunta_core::events::{EventPayload, Finding, StoredEvent};
use yunta_core::port::Adapter;
use yunta_core::{AdapterId, ConfigLayer, RunId, Workflow};
use yunta_engine::{
    build_manifest, create_run, execute_run, CreateRunParams, NoInteraction, RunEnv, RunTerminal,
    DEFAULT_MAX_RETRIES,
};
use yunta_storage::Storage;
use yunta_testkit::{git, init_repo};
use yunta_testkit_core::FixedClock;
use yunta_testkit_core::SeqIdSource;

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
        .await
        .unwrap()
        .manifest;

        let run_dir = create_run(
            CreateRunParams {
                run_id: &self.run_id,
                manifest: &manifest,
                runs_root: &self.runs_root,
                mode: &"default".into(),
                worktree: &self.worktree,
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
            secrets: None,
            observer: None,
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
        Some(EventPayload::Findings(FindingEvent::Posted(p))) if p.finding.id.as_str() == id => {
            Some(p.finding.clone())
        }
        _ => None,
    })
}

#[tokio::test]
async fn an_unwritable_process_registry_is_recorded_as_a_finding() {
    // `create_run` makes `scratch/`; replacing `engine.json` with a
    // directory makes the registry's atomic write fail — the run must
    // record that its process tree became invisible from outside, not
    // warn and vanish.
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
        finding.detail.contains("process tree"),
        "the finding says what became invisible: {}",
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
      - { node: build, name: notes.md }
"#;
    let terminal = bench.run(workflow).await;

    assert!(matches!(terminal, RunTerminal::Finished));
    assert!(
        finding(&bench.events(), "distill-commit").is_some(),
        "a distill commit blocked by a failing hook must be a finding"
    );
}

/// A fallback the whole run works under is stated once. `edit_hooks` is
/// the run's condition, not a node's choice: every node with a declared
/// scope runs unguarded on an adapter that has none, and repeating that
/// per node says nothing new and buries what does.
#[tokio::test]
async fn a_run_on_an_adapter_without_edit_hooks_says_so_once() {
    let bench = yunta_testkit::Bench::new();
    let workflow = r#"
name: two-scoped-nodes
nodes:
  - id: first
    kind: prompt
    runner: executor
    prompt: "Do the first thing."
    scope: ["a.txt"]
  - id: second
    kind: prompt
    runner: executor
    depends_on: [first]
    prompt: "Do the second thing."
    scope: ["b.txt"]
"#;
    let fixture = r#"
sessions:
  - outcome: { type: completed, summary: "first" }
  - outcome: { type: completed, summary: "second" }
"#;
    let (terminal, _) = bench.run(workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let stated = degradations_of(&bench, yunta_core::Capability::EditHooks);
    assert_eq!(
        stated.len(),
        1,
        "two scoped nodes, one adapter that cannot hold them: {stated:?}"
    );
    assert_eq!(
        stated[0],
        yunta_core::events::Policy::PostCheckOnly.to_string()
    );
}

/// A run whose adapter reports no usage cannot count what it spends, so
/// it neither hands sessions a cap it cannot enforce nor stays silent
/// about the cap it was given.
#[tokio::test]
async fn a_run_on_an_adapter_without_usage_reporting_says_it_has_no_token_budget() {
    let bench = yunta_testkit::Bench::new();
    let workflow = r#"
name: capped
nodes:
  - id: only
    kind: prompt
    runner: executor
    prompt: "Do the thing."
"#;
    let config = format!("{}\nlimits:\n  max_tokens_per_run: 100000\n", CONFIG);
    let fixture = r#"
sessions:
  - outcome: { type: completed, summary: "done" }
"#;
    let (terminal, _) = bench.run_with_config(workflow, fixture, &config).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let stated = degradations_of(&bench, yunta_core::Capability::UsageReporting);
    assert_eq!(stated.len(), 1, "{stated:?}");
    assert_eq!(
        stated[0],
        yunta_core::events::Policy::NoTokenBudget.to_string()
    );

    // And the cap it cannot count against never reaches a session.
    let budgets: Vec<Option<u64>> = bench
        .mock()
        .requests_seen()
        .into_iter()
        .map(|request| request.budget.max_tokens)
        .collect();
    assert_eq!(
        budgets,
        vec![None],
        "a run that cannot count tokens hands out no token cap"
    );
}

/// The `policy_applied` of every `capability_degraded` the run recorded
/// for `capability`, in log order.
fn degradations_of(
    bench: &yunta_testkit::Bench,
    capability: yunta_core::Capability,
) -> Vec<String> {
    bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Session(yunta_core::events::SessionEvent::CapabilityDegraded(
                p,
            ))) if p.capability == capability => Some(p.policy_applied().to_string()),
            _ => None,
        })
        .collect()
}

/// A secret the config names never reaches the log.
///
/// It reaches the session's environment on purpose, and the session may
/// then say it back — a note quoting a command line it ran, an error
/// repeating a URL with a token in it. The log is the run's permanent
/// record, so what the config called a secret is taken back out on the
/// way in, at the one door every event goes through.
#[tokio::test]
async fn a_secret_the_config_names_never_reaches_the_log() {
    const VALUE: &str = "hunter2-the-whole-token";

    let bench = yunta_testkit::Bench::new();
    let config = format!("{CONFIG}\nsecrets: [YUNTA_TEST_TOKEN]\n");
    let workflow = r#"
name: leaky
nodes:
  - id: talk
    kind: prompt
    runner: executor
    prompt: "Do the thing."
"#;
    // The session says the secret back, twice over: once as a note, and
    // once as the outcome its close records.
    let fixture = format!(
        "sessions:\n  - steps:\n      - {{ type: note, text: \"ran psql with {VALUE}\" }}\n    outcome: {{ type: completed, summary: \"used {VALUE}\" }}\n"
    );

    let (_terminal, _) = bench
        .run_with_secrets(workflow, &fixture, &config, &[("YUNTA_TEST_TOKEN", VALUE)])
        .await;

    let log = format!("{:?}", bench.events());
    assert!(
        !log.contains(VALUE),
        "the secret reached the log:\n{}",
        log.lines()
            .filter(|line| line.contains(VALUE))
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        log.contains(yunta_core::REDACTED),
        "and the log says where it was: {log}"
    );
}
