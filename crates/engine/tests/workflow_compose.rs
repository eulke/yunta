//! `kind: workflow` — composition as linked runs: each
//! sub-workflow is a **complete run** (own run_id, manifest, log,
//! run.dir), the parent records `child_run_created`/`child_run_finished`
//! and treats the child's terminal state as the node's result, resume
//! reaches orphaned children recursively, and history pins the child's
//! frozen manifest through its `child_run_id` — never re-resolving the
//! workflow name.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use yunta_adapters::{Adapter, MockAdapter};
use yunta_core::events::{EventPayload, TerminalState};
use yunta_core::{AdapterId, Clock, ConfigLayer, Manifest, RunId, Workflow};
use yunta_engine::{
    build_manifest, create_run, execute_run, CreateRunParams, NoInteraction, NodeState, RunEnv,
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

/// A bench whose worktree carries a committed `.yunta/workflows/`
/// catalog — what `use:` resolves against at child birth.
struct Bench {
    _root: tempfile::TempDir,
    worktree: PathBuf,
    runs_root: PathBuf,
    storage: Storage,
}

impl Bench {
    fn new(catalog: &[(&str, &str)]) -> Self {
        let root = tempfile::tempdir().unwrap();
        let worktree = root.path().join("worktree");
        std::fs::create_dir_all(&worktree).unwrap();
        init_repo(&worktree);
        let workflows = worktree.join(".yunta/workflows");
        std::fs::create_dir_all(&workflows).unwrap();
        for (name, yaml) in catalog {
            std::fs::write(workflows.join(format!("{name}.yaml")), yaml).unwrap();
        }
        git(&worktree, &["add", "."]);
        git(
            &worktree,
            &["commit", "-q", "--allow-empty", "-m", "catalog"],
        );
        let runs_root = root.path().join("runs");
        let storage = Storage::open(&root.path().join("yunta.db")).unwrap();
        Bench {
            _root: root,
            worktree,
            runs_root,
            storage,
        }
    }

    fn recommit_catalog(&self, name: &str, yaml: &str) {
        std::fs::write(
            self.worktree
                .join(".yunta/workflows")
                .join(format!("{name}.yaml")),
            yaml,
        )
        .unwrap();
        git(&self.worktree, &["add", "."]);
        git(&self.worktree, &["commit", "-q", "-m", "update catalog"]);
    }

    async fn run(
        &self,
        run_id: &RunId,
        parent_yaml: &str,
        config_yaml: &str,
        inputs: &HashMap<String, String>,
        fixture_yaml: &str,
        human_interaction: &dyn yunta_engine::HumanInteraction,
    ) -> (RunTerminal, yunta_engine::RunState) {
        let manifest = self.create(run_id, parent_yaml, config_yaml, inputs).await;
        self.execute(run_id, &manifest, fixture_yaml, human_interaction)
            .await
    }

    async fn create(
        &self,
        run_id: &RunId,
        parent_yaml: &str,
        config_yaml: &str,
        inputs: &HashMap<String, String>,
    ) -> Manifest {
        let workflow: Workflow = serde_yaml::from_str(parent_yaml).unwrap();
        let config: ConfigLayer = serde_yaml::from_str(config_yaml).unwrap();
        let manifest =
            build_manifest(&workflow, &config, &self.worktree, &self.worktree, inputs).unwrap();
        create_run(
            CreateRunParams {
                run_id,
                manifest: &manifest,
                runs_root: &self.runs_root,
                mode: &"default".into(),
                promoted_from: None,
            },
            &self.storage.async_handle(),
            &FixedClock,
        )
        .await
        .unwrap();
        manifest
    }

    async fn execute(
        &self,
        run_id: &RunId,
        manifest: &Manifest,
        fixture_yaml: &str,
        human_interaction: &dyn yunta_engine::HumanInteraction,
    ) -> (RunTerminal, yunta_engine::RunState) {
        let adapter = MockAdapter::from_yaml(fixture_yaml).unwrap();
        let mut adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
        adapters.insert("mock".into(), Arc::new(adapter));
        let run_dir = self.runs_root.join(run_id.as_str());
        let report = execute_run(RunEnv {
            run_id,
            manifest,
            run_dir: &run_dir,
            worktree: &self.worktree,
            adapters: &adapters,
            storage: &self.storage.async_handle(),
            clock: &FixedClock,
            max_task_retries: DEFAULT_MAX_RETRIES,
            human_interaction,
            forge: None,
            cancel: None,
            adapter_override: None,
        })
        .await
        .unwrap();
        (report.terminal, report.state)
    }

    /// Every `(child_run_id, child_workflow_hash)` the parent recorded,
    /// in log order.
    fn children_created(&self, run_id: &RunId) -> Vec<(RunId, String)> {
        self.storage
            .events_for_run(run_id)
            .unwrap()
            .into_iter()
            .filter_map(|e| match e.payload() {
                Some(EventPayload::ChildRunCreated(p)) => {
                    Some((p.child_run_id.clone(), p.child_workflow_hash.clone()))
                }
                _ => None,
            })
            .collect()
    }

    fn children_finished(&self, run_id: &RunId) -> Vec<(RunId, TerminalState)> {
        self.storage
            .events_for_run(run_id)
            .unwrap()
            .into_iter()
            .filter_map(|e| match e.payload() {
                Some(EventPayload::ChildRunFinished(p)) => {
                    Some((p.child_run_id.clone(), p.terminal_state))
                }
                _ => None,
            })
            .collect()
    }

    fn child_manifest(&self, child_id: &RunId) -> Manifest {
        let path = self.runs_root.join(child_id.as_str()).join("manifest.yaml");
        serde_yaml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
    }
}

const EMPTY_FIXTURE: &str = "sessions: []\n";

// --- The child is a complete, linked run -------------------------------------

#[tokio::test]
async fn a_workflow_node_runs_its_child_as_a_complete_linked_run() {
    let bench = Bench::new(&[(
        "child-wf",
        r#"
name: child-wf
inputs:
  idea: { type: string, required: true }
nodes:
  - id: work
    kind: bash
    run: "echo {{inputs.idea}} > out.txt"
"#,
    )]);
    let parent = r#"
name: parent
inputs:
  thing: { type: string, required: true }
nodes:
  - id: feat
    kind: workflow
    use: child-wf
    inputs: { idea: "{{inputs.thing}}" }
"#;
    let run_id = RunId::from("run-parent-1");
    let inputs = HashMap::from([("thing".to_string(), "hola".to_string())]);
    let (terminal, state) = bench
        .run(
            &run_id,
            parent,
            CONFIG,
            &inputs,
            EMPTY_FIXTURE,
            &NoInteraction,
        )
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.get("feat"),
        Some(NodeState::Finished { .. })
    ));

    // The parent's log carries the link pair.
    let created = bench.children_created(&run_id);
    assert_eq!(created.len(), 1);
    let (child_id, recorded_hash) = &created[0];
    assert_eq!(child_id.as_str(), "run-parent-1-feat");
    assert_eq!(
        bench.children_finished(&run_id),
        vec![(child_id.clone(), TerminalState::Done)]
    );

    // The child is a complete run: own log (created + finished), own
    // frozen manifest with the parent-rendered inputs.
    let child_events = bench.storage.events_for_run(child_id).unwrap();
    assert!(matches!(
        child_events.first().and_then(|e| e.payload()),
        Some(EventPayload::RunCreated(_))
    ));
    assert!(child_events
        .iter()
        .any(|e| matches!(e.payload(), Some(EventPayload::RunFinished(_)))));
    let child_manifest = bench.child_manifest(child_id);
    assert_eq!(child_manifest.workflow.name, "child-wf");
    assert_eq!(&child_manifest.workflow_hash, recorded_hash);
    assert_eq!(
        child_manifest.inputs.get("idea").map(String::as_str),
        Some("hola")
    );
    assert_eq!(child_manifest.isolation, yunta_core::Isolation::Worktree);

    // The child worked in its own tree, branched off the parent's HEAD.
    let child_tree = bench._root.path().join("worktrees").join(child_id.as_str());
    assert_eq!(
        std::fs::read_to_string(child_tree.join("out.txt"))
            .unwrap()
            .trim(),
        "hola"
    );
    // The parent's own tree never saw the child's write.
    assert!(!bench.worktree.join("out.txt").exists());
}

#[tokio::test]
async fn child_usage_aggregates_into_the_parent_total() {
    let bench = Bench::new(&[(
        "spender",
        r#"
name: spender
nodes:
  - id: talk
    kind: prompt
    runner: executor
    prompt: "Do the thing."
"#,
    )]);
    let parent = r#"
name: parent
nodes:
  - id: feat
    kind: workflow
    use: spender
"#;
    let fixture = r#"
sessions:
  - steps:
      - { type: usage, input_tokens: 100, output_tokens: 20 }
    outcome: { type: completed, summary: "done" }
"#;
    let run_id = RunId::from("run-parent-tokens");
    let (terminal, state) = bench
        .run(
            &run_id,
            parent,
            CONFIG,
            &HashMap::new(),
            fixture,
            &NoInteraction,
        )
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    // The children's Usage aggregates upward — each chain member's
    // whole spend rides its own `child_run_finished.tokens`, so
    // the parent's derived total (what `limits.max_tokens_per_run`
    // compares against) includes it exactly once. The node's own close
    // deliberately carries none — it would double-count.
    assert_eq!(state.total_tokens.input, 100);
    assert_eq!(state.total_tokens.output, 20);
    match state.nodes.get("feat") {
        Some(NodeState::Finished { tokens, .. }) => {
            assert_eq!(
                tokens.input, 0,
                "the node close must not re-count the child"
            );
            assert_eq!(tokens.output, 0);
        }
        other => panic!("expected feat finished, got {other:?}"),
    }
    let events = bench.storage.events_for_run(&run_id).unwrap();
    let recorded = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(EventPayload::ChildRunFinished(p)) => Some(p.tokens),
            _ => None,
        })
        .expect("child_run_finished must carry the child's spend");
    assert_eq!(recorded.input, 100);
    assert_eq!(recorded.output, 20);
}

// --- Pause and recursive resume ----------------------------------------------

#[tokio::test]
async fn a_paused_child_pauses_the_parent_and_resume_reaches_it_recursively() {
    let bench = Bench::new(&[(
        "gated",
        r#"
name: gated
nodes:
  - id: approve
    kind: gate
    assignee: lead
  - id: work
    kind: bash
    depends_on: [approve]
    run: "echo done > child-out.txt"
"#,
    )]);
    // `isolation: inherit`: the child works in the parent's own tree, so
    // the child's write is visible right there after the close.
    let parent = r#"
name: parent
nodes:
  - id: feat
    kind: workflow
    use: gated
    isolation: inherit
"#;
    let run_id = RunId::from("run-parent-resume");
    let manifest = bench.create(&run_id, parent, CONFIG, &HashMap::new()).await;
    // No interaction surface: the child's internal gate has nobody to
    // ask, so the child pauses waiting — and the parent pauses with it,
    // since a parent run spends most of its life waiting on its children.
    let (terminal, _) = bench
        .execute(&run_id, &manifest, EMPTY_FIXTURE, &NoInteraction)
        .await;

    let RunTerminal::Paused { reason } = terminal else {
        panic!("expected the parent to pause on its paused child, got {terminal:?}");
    };
    assert!(
        reason.contains("run-parent-resume-feat"),
        "the pause reason must name the child run: {reason}"
    );
    // The child paused on its own log; the parent's node stays open (no
    // child_run_finished) so resume knows to go back in.
    assert_eq!(bench.children_created(&run_id).len(), 1);
    assert!(bench.children_finished(&run_id).is_empty());
    let child_id = RunId::from("run-parent-resume-feat");
    assert!(bench
        .storage
        .events_for_run(&child_id)
        .unwrap()
        .iter()
        .any(|e| matches!(e.payload(), Some(EventPayload::RunPaused(_)))));

    // Resume the parent with a surface that answers: the SAME child run
    // resumes (no second child_run_created), its gate resolves, and
    // everything closes.
    let (terminal, state) = bench
        .execute(&run_id, &manifest, EMPTY_FIXTURE, &ApproveEverything)
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.get("feat"),
        Some(NodeState::Finished { .. })
    ));
    assert_eq!(bench.children_created(&run_id).len(), 1);
    assert_eq!(
        bench.children_finished(&run_id),
        vec![(child_id.clone(), TerminalState::Done)]
    );
    assert!(bench
        .storage
        .events_for_run(&child_id)
        .unwrap()
        .iter()
        .any(|e| matches!(e.payload(), Some(EventPayload::RunResumed(_)))));
    assert_eq!(
        std::fs::read_to_string(bench.worktree.join("child-out.txt"))
            .unwrap()
            .trim(),
        "done"
    );
}

// --- Historical reproducibility ---------------------------

#[tokio::test]
async fn history_pins_the_child_manifest_and_never_re_resolves_the_name() {
    const CHILD_V1: &str = r#"
name: evolving
nodes:
  - id: work
    kind: bash
    run: "true"
"#;
    const CHILD_V2: &str = r#"
name: evolving
nodes:
  - id: work
    kind: bash
    run: "echo v2 > v2.txt"
"#;
    let bench = Bench::new(&[("evolving", CHILD_V1)]);
    let parent = r#"
name: parent
nodes:
  - id: feat
    kind: workflow
    use: evolving
"#;

    let first = RunId::from("run-parent-v1");
    let (terminal, _) = bench
        .run(
            &first,
            parent,
            CONFIG,
            &HashMap::new(),
            EMPTY_FIXTURE,
            &NoInteraction,
        )
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
    let (first_child, v1_hash) = bench.children_created(&first)[0].clone();

    // The child workflow evolves between parent executions (a long
    // process picks up improvements)...
    bench.recommit_catalog("evolving", CHILD_V2);
    let second = RunId::from("run-parent-v2");
    let (terminal, _) = bench
        .run(
            &second,
            parent,
            CONFIG,
            &HashMap::new(),
            EMPTY_FIXTURE,
            &NoInteraction,
        )
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
    let (_, v2_hash) = bench.children_created(&second)[0].clone();

    // ...so the second child froze the new file...
    assert_ne!(
        v1_hash, v2_hash,
        "the second run must resolve the current file"
    );

    // ...while history stays pinned: following the recorded
    // child_run_id reaches the child's own frozen manifest, which still
    // hashes to v1 — reproducing the old parent never resolves
    // `evolving@current`.
    let pinned = bench.child_manifest(&first_child);
    assert_eq!(pinned.workflow_hash, v1_hash);
    let v1_workflow: Workflow = serde_yaml::from_str(CHILD_V1).unwrap();
    assert_eq!(pinned.workflow.nodes[0], v1_workflow.nodes[0]);
}

// --- The composed reference workflow --------------------

struct ApproveEverything;

#[async_trait::async_trait]
impl yunta_engine::HumanInteraction for ApproveEverything {
    async fn resolve(
        &self,
        escalation: &yunta_core::events::GateWaitingPayload,
    ) -> Option<yunta_core::events::GateResolvedPayload> {
        Some(yunta_core::events::GateResolvedPayload {
            chosen_option: escalation.options.first().map(|o| o.id.clone()),
            resolved_by: Some("test".to_string()),
            free_text: None,
            approved_sha: None,
        })
    }
}

#[tokio::test]
async fn the_release_cycle_reference_runs_with_mock() {
    let release_cycle = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/release-cycle.yaml"
    ))
    .unwrap();
    let prompt_child = |name: &str, inputs: &str| {
        format!(
            r#"
name: {name}
{inputs}
nodes:
  - id: work
    kind: prompt
    runner: executor
    prompt: "Do the {name} work."
"#
        )
    };
    let design_review = prompt_child(
        "design-review",
        "inputs:\n  rfc: { type: path, required: true }",
    );
    let build_feature = prompt_child(
        "build-feature",
        "inputs:\n  idea: { type: string, required: true }",
    );
    let qa_review = prompt_child("qa-review", "");
    let bench = Bench::new(&[
        ("design-review", design_review.as_str()),
        ("build-feature", build_feature.as_str()),
        ("qa-review", qa_review.as_str()),
    ]);
    std::fs::write(bench.worktree.join("rfc.md"), "the rfc").unwrap();
    git(&bench.worktree, &["add", "."]);
    git(&bench.worktree, &["commit", "-q", "-m", "rfc"]);

    // Four child runs, one mock session each (identical on purpose —
    // feat-a/feat-b race in parallel, so fixture order between them is
    // not deterministic).
    let fixture = r#"
sessions:
  - outcome: { type: completed, summary: "worked" }
  - outcome: { type: completed, summary: "worked" }
  - outcome: { type: completed, summary: "worked" }
  - outcome: { type: completed, summary: "worked" }
"#;
    let run_id = RunId::from("run-release");
    let inputs = HashMap::from([
        ("rfc".to_string(), "rfc.md".to_string()),
        ("feat_a".to_string(), "feature a".to_string()),
        ("feat_b".to_string(), "feature b".to_string()),
    ]);
    let (terminal, state) = bench
        .run(
            &run_id,
            &release_cycle,
            CONFIG,
            &inputs,
            fixture,
            &ApproveEverything,
        )
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    for node in [
        "design",
        "approve-design",
        "build",
        "feat-a",
        "feat-b",
        "qa",
        "ship",
    ] {
        assert!(
            matches!(state.nodes.get(node), Some(NodeState::Finished { .. })),
            "node `{node}` should be finished, got {:?}",
            state.nodes.get(node)
        );
    }
    let created = bench.children_created(&run_id);
    let mut child_ids: Vec<&str> = created.iter().map(|(id, _)| id.as_str()).collect();
    child_ids.sort();
    assert_eq!(
        child_ids,
        vec![
            "run-release-design",
            "run-release-feat-a",
            "run-release-feat-b",
            "run-release-qa",
        ]
    );
    let finished = bench.children_finished(&run_id);
    assert_eq!(finished.len(), 4);
    assert!(finished
        .iter()
        .all(|(_, terminal)| *terminal == TerminalState::Done));
}

// --- Deliberate limits -------------------------------------------------------

#[tokio::test]
async fn workflow_nesting_depth_is_capped_at_runtime() {
    let bench = Bench::new(&[
        (
            "mid",
            r#"
name: mid
nodes:
  - id: deeper
    kind: workflow
    use: leaf
"#,
        ),
        (
            "leaf",
            r#"
name: leaf
nodes:
  - id: work
    kind: bash
    run: "true"
"#,
        ),
    ]);
    let parent = r#"
name: parent
nodes:
  - id: feat
    kind: workflow
    use: mid
"#;
    let config = r#"
runners:
  executor:
    - { adapter: mock, model: mock-model }
limits:
  max_workflow_depth: 1
"#;
    let run_id = RunId::from("run-deep");
    let (terminal, _) = bench
        .run(
            &run_id,
            parent,
            config,
            &HashMap::new(),
            EMPTY_FIXTURE,
            &NoInteraction,
        )
        .await;
    let RunTerminal::Paused { reason } = terminal else {
        panic!("expected the over-deep composition to pause, got {terminal:?}");
    };
    assert!(
        reason.contains("max_workflow_depth"),
        "the pause must name the limit: {reason}"
    );
}

#[tokio::test]
async fn a_missing_child_workflow_fails_the_node_naming_the_path() {
    let bench = Bench::new(&[]);
    let parent = r#"
name: parent
nodes:
  - id: feat
    kind: workflow
    use: nope
"#;
    let run_id = RunId::from("run-missing");
    let (terminal, state) = bench
        .run(
            &run_id,
            parent,
            CONFIG,
            &HashMap::new(),
            EMPTY_FIXTURE,
            &NoInteraction,
        )
        .await;
    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    match state.nodes.get("feat") {
        Some(NodeState::Failed { outcome, .. }) => {
            assert!(
                outcome.contains(".yunta/workflows/nope.yaml"),
                "the diagnostic must name the resolved path: {outcome}"
            );
        }
        other => panic!("expected feat failed, got {other:?}"),
    }
    assert!(bench.children_created(&run_id).is_empty());
}

// --- a promoted child chains into its successor -----------------------

struct AlwaysPromote;

#[async_trait::async_trait]
impl yunta_engine::HumanInteraction for AlwaysPromote {
    async fn resolve(
        &self,
        _escalation: &yunta_core::events::GateWaitingPayload,
    ) -> Option<yunta_core::events::GateResolvedPayload> {
        Some(yunta_core::events::GateResolvedPayload {
            chosen_option: Some("promote".to_string()),
            resolved_by: Some("test".to_string()),
            free_text: None,
            approved_sha: None,
        })
    }
}

#[tokio::test]
async fn a_promoted_child_chains_into_its_successor_automatically() {
    let bench = Bench::new(&[(
        "promotable",
        r#"
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
"#,
    )]);
    let parent = r#"
name: parent
nodes:
  - id: feat
    kind: workflow
    use: promotable
"#;
    // The child starts in `quick` (the floor), exhausts its re-route,
    // and the scripted human promotes — the parent must then create and
    // drive the successor child (`full`) instead of failing the node.
    let run_id = RunId::from("run-parent-chain");
    let (terminal, state) = bench
        .run(
            &run_id,
            parent,
            CONFIG,
            &HashMap::new(),
            EMPTY_FIXTURE,
            &AlwaysPromote,
        )
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.get("feat"),
        Some(NodeState::Finished { .. })
    ));

    // Both chain members are linked children of the same node, in order.
    let created = bench.children_created(&run_id);
    let ids: Vec<&str> = created.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["run-parent-chain-feat", "run-parent-chain-feat-promoted"],
        "the successor must be a new linked child, recorded on the parent log"
    );
    let finished = bench.children_finished(&run_id);
    assert_eq!(
        finished
            .iter()
            .map(|(id, t)| (id.as_str(), *t))
            .collect::<Vec<_>>(),
        vec![
            ("run-parent-chain-feat", TerminalState::Promoted),
            ("run-parent-chain-feat-promoted", TerminalState::Done),
        ]
    );

    // The successor's own log carries the audited chain and mode.
    let successor = RunId::from("run-parent-chain-feat-promoted");
    let successor_created = bench
        .storage
        .events_for_run(&successor)
        .unwrap()
        .into_iter()
        .find_map(|e| match e.payload() {
            Some(EventPayload::RunCreated(p)) => Some(p.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        successor_created.promoted_from,
        Some(RunId::from("run-parent-chain-feat"))
    );
    assert_eq!(successor_created.mode, "full");

    // And the successor really did the `full` work, in its own tree.
    let successor_tree = bench
        ._root
        .path()
        .join("worktrees")
        .join(successor.as_str());
    assert!(successor_tree.join("shipped.txt").exists());
}

// --- cross-run artifact mounts -----------------------------------

#[tokio::test]
async fn mounts_copy_parent_and_sibling_artifacts_into_the_child_at_birth() {
    let bench = Bench::new(&[
        (
            "producer",
            r#"
name: producer
nodes:
  - id: work
    kind: bash
    run: "echo the-report > {{run.dir}}/artifacts/report.md"
    artifacts: { produces: [report.md] }
"#,
        ),
        (
            "consumer",
            r#"
name: consumer
nodes:
  - id: verify
    kind: bash
    run: "test -f {{run.dir}}/artifacts/report.md && test -f {{run.dir}}/artifacts/brief.md"
"#,
        ),
    ]);
    let parent = r#"
name: parent
nodes:
  - id: plan
    kind: bash
    run: "echo the-plan > {{run.dir}}/artifacts/plan.yaml"
    artifacts: { produces: [plan.yaml] }
  - id: prod
    kind: workflow
    use: producer
  - id: cons
    kind: workflow
    use: consumer
    mounts:
      - artifact: { node: prod, name: report.md }
      - artifact: { node: plan, name: plan.yaml, as: brief.md }
"#;
    let run_id = RunId::from("run-mounts");
    let manifest = bench.create(&run_id, parent, CONFIG, &HashMap::new()).await;

    // Each mount implies depends_on — visible in the frozen graph.
    let cons = manifest
        .workflow
        .nodes
        .iter()
        .find(|n| n.id.as_str() == "cons")
        .unwrap();
    assert!(cons.depends_on.contains(&"prod".into()));
    assert!(cons.depends_on.contains(&"plan".into()));

    let (terminal, state) = bench
        .execute(&run_id, &manifest, EMPTY_FIXTURE, &NoInteraction)
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.get("cons"),
        Some(NodeState::Finished { .. })
    ));

    // The copies landed in the child's own run.dir at birth: the
    // sibling's artifact through the recorded link, the parent's own
    // under its `as:` rename.
    let cons_artifacts = bench.runs_root.join("run-mounts-cons").join("artifacts");
    assert_eq!(
        std::fs::read_to_string(cons_artifacts.join("report.md"))
            .unwrap()
            .trim(),
        "the-report"
    );
    assert_eq!(
        std::fs::read_to_string(cons_artifacts.join("brief.md"))
            .unwrap()
            .trim(),
        "the-plan"
    );
}

#[tokio::test]
async fn a_mount_whose_source_was_never_produced_fails_the_node_before_the_child_exists() {
    let bench = Bench::new(&[(
        "consumer",
        r#"
name: consumer
nodes:
  - id: verify
    kind: bash
    run: "true"
"#,
    )]);
    let parent = r#"
name: parent
nodes:
  - id: plan
    kind: bash
    run: "true"
  - id: cons
    kind: workflow
    use: consumer
    mounts:
      - artifact: { node: plan, name: plan.yaml }
"#;
    let run_id = RunId::from("run-mount-missing");
    let (terminal, state) = bench
        .run(
            &run_id,
            parent,
            CONFIG,
            &HashMap::new(),
            EMPTY_FIXTURE,
            &NoInteraction,
        )
        .await;
    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    match state.nodes.get("cons") {
        Some(NodeState::Failed { outcome, .. }) => {
            assert!(
                outcome.contains("plan.yaml") && outcome.contains("plan"),
                "the diagnostic must name the artifact and its source node: {outcome}"
            );
        }
        other => panic!("expected cons failed, got {other:?}"),
    }
    // The failure happened before the link: no dangling child run.
    assert!(bench.children_created(&run_id).is_empty());
}

#[tokio::test]
async fn a_child_consumes_a_mounted_artifact_through_context_without_naming_a_node() {
    let bench = Bench::new(&[(
        "consumer",
        r#"
name: consumer
nodes:
  - id: talk
    kind: prompt
    runner: executor
    prompt: "Use the brief."
    context:
      - artifact: { name: brief.md }
"#,
    )]);
    let parent = r#"
name: parent
nodes:
  - id: plan
    kind: bash
    run: "echo the-plan > {{run.dir}}/artifacts/plan.yaml"
    artifacts: { produces: [plan.yaml] }
  - id: cons
    kind: workflow
    use: consumer
    mounts:
      - artifact: { node: plan, name: plan.yaml, as: brief.md }
"#;
    let fixture = r#"
sessions:
  - outcome: { type: completed, summary: "read it" }
"#;
    let run_id = RunId::from("run-mount-context");
    let (terminal, state) = bench
        .run(
            &run_id,
            parent,
            CONFIG,
            &HashMap::new(),
            fixture,
            &NoInteraction,
        )
        .await;
    // The node-less artifact source resolved against the child's own
    // run.dir — a missing file would have failed the child's node, so a
    // clean finish is the proof the mount fed the context.
    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.get("cons"),
        Some(NodeState::Finished { .. })
    ));
}
