//! `kind: workflow` — composition as linked runs: each
//! sub-workflow is a **complete run** (own run_id, manifest, log,
//! run.dir), the parent records `child_run_created`/`child_run_finished`
//! and treats the child's terminal state as the node's result, resume
//! reaches orphaned children recursively, and history pins the child's
//! frozen manifest through its `child_run_id` — never re-resolving the
//! workflow name.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use yunta_adapters::MockAdapter;
use yunta_core::diagnostic::ArtifactFailure;
use yunta_core::events::{ArtifactId, EventPayload, TerminalState};
use yunta_core::events::{ChildEvent, FindingEvent, RunEvent, TaskEvent};
use yunta_core::port::Adapter;
use yunta_core::{AdapterId, ConfigLayer, IdSource, Manifest, NodeId, RunId, Workflow};
use yunta_engine::{
    build_manifest, create_run, execute_run, CreateRunParams, NoInteraction, NodeState, RunEnv,
    RunTerminal, DEFAULT_MAX_RETRIES,
};
use yunta_storage::Storage;
use yunta_testkit::{git, init_repo, ApproveEverything};
use yunta_testkit_core::FixedClock;
use yunta_testkit_core::SeqIdSource;

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
    /// The ids of every child and successor born in this bench, in
    /// minting order: `minted-1`, `minted-2`, …
    ids: SeqIdSource,
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
            ids: SeqIdSource::new("minted"),
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
        let workflow: Workflow = serde_norway::from_str(parent_yaml).unwrap();
        let config: ConfigLayer = serde_norway::from_str(config_yaml).unwrap();
        let manifest = build_manifest(&workflow, &config, &self.worktree, &self.worktree, inputs)
            .unwrap()
            .manifest;
        create_run(
            CreateRunParams {
                run_id,
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
        manifest
    }

    async fn execute(
        &self,
        run_id: &RunId,
        manifest: &Manifest,
        fixture_yaml: &str,
        human_interaction: &dyn yunta_engine::HumanInteraction,
    ) -> (RunTerminal, yunta_engine::RunState) {
        self.execute_with(run_id, manifest, fixture_yaml, human_interaction, &self.ids)
            .await
    }

    async fn execute_with(
        &self,
        run_id: &RunId,
        manifest: &Manifest,
        fixture_yaml: &str,
        human_interaction: &dyn yunta_engine::HumanInteraction,
        ids: &dyn IdSource,
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
            clock: std::sync::Arc::new(FixedClock),
            ids,
            max_task_retries: DEFAULT_MAX_RETRIES,
            human_interaction,
            forge: None,
            cancel: None,
            adapter_override: None,
            ambient: None,
            observer: None,
        })
        .await
        .unwrap();
        (report.terminal, report.state)
    }

    /// Every `(child_run_id, child_workflow_hash)` the parent recorded,
    /// in log order.
    fn children_created(&self, run_id: &RunId) -> Vec<(RunId, yunta_core::ContentHash)> {
        self.storage
            .events_for_run(run_id)
            .unwrap()
            .into_iter()
            .filter_map(|e| match e.payload() {
                Some(EventPayload::Children(ChildEvent::Created(p))) => {
                    Some((p.child_run_id.clone(), p.child_workflow_hash.clone()))
                }
                _ => None,
            })
            .collect()
    }

    /// Every `(node_id, child_run_id)` the parent recorded, in log
    /// order — which node each child was born from.
    fn children_by_node(&self, run_id: &RunId) -> Vec<(String, RunId)> {
        self.storage
            .events_for_run(run_id)
            .unwrap()
            .into_iter()
            .filter_map(|e| match e.payload() {
                Some(EventPayload::Children(ChildEvent::Created(p))) => Some((
                    e.node_id
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_default(),
                    p.child_run_id.clone(),
                )),
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
                Some(EventPayload::Children(ChildEvent::Finished(p))) => {
                    Some((p.child_run_id.clone(), p.terminal_state))
                }
                _ => None,
            })
            .collect()
    }

    fn child_manifest(&self, child_id: &RunId) -> Manifest {
        let path = self.runs_root.join(child_id.as_str()).join("manifest.yaml");
        serde_norway::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
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
    assert_eq!(
        bench.children_finished(&run_id),
        vec![(child_id.clone(), TerminalState::Done)]
    );

    // The child is a complete run: own log (created + finished), own
    // frozen manifest with the parent-rendered inputs.
    let child_events = bench.storage.events_for_run(child_id).unwrap();
    assert!(matches!(
        child_events.first().and_then(|e| e.payload()),
        Some(EventPayload::Run(RunEvent::Created(_)))
    ));
    assert!(child_events
        .iter()
        .any(|e| matches!(e.payload(), Some(EventPayload::Run(RunEvent::Finished(_))))));
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
async fn child_and_successor_run_ids_are_ulids_from_the_id_source() {
    let bench = Bench::new(&[(
        "child-wf",
        r#"
name: child-wf
nodes:
  - id: work
    kind: bash
    run: "true"
"#,
    )]);
    let parent = r#"
name: parent
nodes:
  - id: feat
    kind: workflow
    use: child-wf
"#;
    let run_id = RunId::from("run-parent-ulid");
    let manifest = bench.create(&run_id, parent, CONFIG, &HashMap::new()).await;
    let (terminal, _) = bench
        .execute_with(
            &run_id,
            &manifest,
            EMPTY_FIXTURE,
            &NoInteraction,
            &yunta_core::SystemIdSource,
        )
        .await;
    assert_eq!(terminal, RunTerminal::Finished);

    // The link is the log's, not the name's: the child id is a fresh
    // ULID, and nothing in it is derived from the parent or the node.
    let created = bench.children_created(&run_id);
    assert_eq!(created.len(), 1);
    let child_id = created[0].0.as_str();
    assert_eq!(child_id.len(), 26, "{child_id}");
    assert!(
        child_id
            .chars()
            .all(|c| "0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(c)),
        "{child_id} is not a ULID"
    );
    assert!(!child_id.contains("run-parent-ulid") && !child_id.contains("feat"));
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
            Some(EventPayload::Children(ChildEvent::Finished(p))) => Some(p.tokens),
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
    // The child paused on its own log; the parent's node stays open (no
    // child_run_finished) so resume knows to go back in.
    let created = bench.children_created(&run_id);
    assert_eq!(created.len(), 1);
    assert!(bench.children_finished(&run_id).is_empty());
    let child_id = created[0].0.clone();
    assert!(
        reason.contains(child_id.as_str()),
        "the pause reason must name the child run: {reason}"
    );
    assert!(bench
        .storage
        .events_for_run(&child_id)
        .unwrap()
        .iter()
        .any(|e| matches!(e.payload(), Some(EventPayload::Run(RunEvent::Paused(_))))));

    // Resume the parent with a surface that answers: the SAME child run
    // resumes (no second child_run_created), its gate resolves, and
    // everything closes.
    let (terminal, state) = bench
        .execute(
            &run_id,
            &manifest,
            EMPTY_FIXTURE,
            &ApproveEverything::new("test"),
        )
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
        .any(|e| matches!(e.payload(), Some(EventPayload::Run(RunEvent::Resumed(_))))));
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
    let v1_workflow: Workflow = serde_norway::from_str(CHILD_V1).unwrap();
    assert_eq!(pinned.workflow.nodes[0], v1_workflow.nodes[0]);
}

// --- The composed reference workflow --------------------

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
            &ApproveEverything::new("test"),
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
    // One child per `kind: workflow` node, each with an id of its own.
    let created = bench.children_by_node(&run_id);
    let mut nodes: Vec<&str> = created.iter().map(|(node, _)| node.as_str()).collect();
    nodes.sort();
    assert_eq!(nodes, vec!["design", "feat-a", "feat-b", "qa"]);
    let mut child_ids: Vec<&RunId> = created.iter().map(|(_, id)| id).collect();
    child_ids.sort();
    child_ids.dedup();
    assert_eq!(child_ids.len(), 4, "every child has an id of its own");
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
        Some(NodeState::Failed { failure, .. }) => {
            let outcome = failure.to_string();
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
    ) -> Option<yunta_core::events::HumanChoice> {
        Some(yunta_core::events::HumanChoice {
            option: "promote".into(),
            by: "test".into(),
            free_text: None,
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

    // Both chain members are linked children of the same node, in
    // order, each with an id of its own.
    let created = bench.children_created(&run_id);
    let ids: Vec<RunId> = created.iter().map(|(id, _)| id.clone()).collect();
    assert_eq!(
        ids.len(),
        2,
        "the successor must be a new linked child, recorded on the parent log"
    );
    assert_ne!(ids[0], ids[1]);
    let finished = bench.children_finished(&run_id);
    assert_eq!(
        finished,
        vec![
            (ids[0].clone(), TerminalState::Promoted),
            (ids[1].clone(), TerminalState::Done),
        ]
    );

    // The successor's own log carries the audited chain and mode.
    let successor = ids[1].clone();
    let successor_created = bench
        .storage
        .events_for_run(&successor)
        .unwrap()
        .into_iter()
        .find_map(|e| match e.payload() {
            Some(EventPayload::Run(RunEvent::Created(p))) => Some(p.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(successor_created.promoted_from, Some(ids[0].clone()));
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
    run: "echo the-report > {{node.artifacts}}/report.md"
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
    run: "true"
"#,
        ),
    ]);
    let parent = r#"
name: parent
nodes:
  - id: plan
    kind: bash
    run: "echo the-plan > {{node.artifacts}}/plan.yaml"
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
    let cons_id = bench
        .children_by_node(&run_id)
        .into_iter()
        .find(|(node, _)| node == "cons")
        .map(|(_, id)| id)
        .expect("the cons child is linked on the parent's log");
    let cons_artifacts = bench.runs_root.join(cons_id.as_str()).join("artifacts");
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

    // The child's own log names each one, after its `run_created` and
    // with no node of its own behind it: the child produced neither.
    let child_events = bench.storage.events_for_run(&cons_id).unwrap();
    assert!(
        matches!(
            child_events.first().and_then(|e| e.payload()),
            Some(EventPayload::Run(RunEvent::Created(_)))
        ),
        "the child exists in its log before anything is said about it"
    );
    let mounted = yunta_testkit::accepted(&child_events);
    assert_eq!(mounted.len(), 2, "{mounted:?}");
    assert!(
        mounted.iter().all(|held| held.producer.is_none()),
        "a mount has no producer in the run that receives it: {mounted:?}"
    );
    assert_eq!(
        mounted
            .iter()
            .map(|held| held.artifact.to_string())
            .collect::<Vec<_>>(),
        vec!["report.md".to_string(), "brief.md".to_string()],
        "each is opaque under the name the mount carries it as"
    );
    // The parent's own artifact comes from the parent's log; the
    // sibling's comes from the child run that produced it.
    let from_parent = yunta_core::events::ArtifactOrigin::Inherited {
        run: run_id.clone(),
        producer: Some("plan".into()),
    };
    assert_eq!(mounted[1].origin, from_parent);
    let prod_id = bench
        .children_by_node(&run_id)
        .into_iter()
        .find(|(node, _)| node == "prod")
        .map(|(_, id)| id)
        .expect("the prod child is linked on the parent's log");
    assert_eq!(
        mounted[0].origin,
        yunta_core::events::ArtifactOrigin::Inherited {
            run: prod_id,
            producer: Some("work".into()),
        }
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
        Some(NodeState::Failed { failure, .. }) => {
            // A source the run does not hold is a declared artifact that
            // did not close, so it reaches every surface as one entry
            // with its own code — never a sentence to be taken apart.
            let entries: Vec<&ArtifactFailure> = failure.failures().collect();
            assert_eq!(entries.len(), 1, "one artifact did not close: {failure}");
            assert_eq!(entries[0].code(), Some("artifact-unheld"));
            assert!(
                matches!(
                    entries[0],
                    ArtifactFailure::Unheld { run, producer, artifact }
                        if *run == run_id
                            && producer.as_ref() == Some(&NodeId::from("plan"))
                            && *artifact == ArtifactId::of("plan.yaml", None)
                ),
                "the entry names the run asked, the node and the artifact: {:?}",
                entries[0]
            );
            let outcome = failure.to_string();
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
    run: "echo the-plan > {{node.artifacts}}/plan.yaml"
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

#[tokio::test]
async fn a_mount_carries_the_bytes_the_log_names_even_with_no_view_left() {
    // A mount resolves through the source run's log and its object
    // store, so a view somebody deleted between the producer and the
    // mount changes nothing the child receives.
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
    // The view belongs to the engine, so the node that deletes it names
    // it by its absolute path rather than through a template no workflow
    // has for it.
    let run_id = RunId::from("run-mount-from-log");
    let parent = format!(
        r#"
name: parent
nodes:
  - id: plan
    kind: bash
    run: "echo the-plan > {{{{node.artifacts}}}}/plan.yaml"
    artifacts: {{ produces: [plan.yaml] }}
  - id: wipe
    kind: bash
    depends_on: [plan]
    run: "rm -rf {view}"
  - id: cons
    kind: workflow
    use: consumer
    depends_on: [wipe]
    mounts:
      - artifact: {{ node: plan, name: plan.yaml, as: brief.md }}
"#,
        view = bench
            .runs_root
            .join(run_id.as_str())
            .join(yunta_core::ARTIFACTS_DIR)
            .display()
    );
    let (terminal, state) = bench
        .run(
            &run_id,
            &parent,
            CONFIG,
            &HashMap::new(),
            EMPTY_FIXTURE,
            &NoInteraction,
        )
        .await;
    assert_eq!(terminal, RunTerminal::Finished, "{state:?}");

    let cons_id = bench
        .children_by_node(&run_id)
        .into_iter()
        .find(|(node, _)| node == "cons")
        .map(|(_, id)| id)
        .expect("the cons child is linked on the parent's log");
    let child_events = bench.storage.events_for_run(&cons_id).unwrap();
    let mounted = yunta_testkit::accepted(&child_events);
    assert_eq!(mounted.len(), 1, "{mounted:?}");
    assert_eq!(
        mounted[0].origin,
        yunta_core::events::ArtifactOrigin::Inherited {
            run: run_id.clone(),
            producer: Some("plan".into()),
        },
        "the mount carries where it came from and who produced it there"
    );
    let parent_events = bench.storage.events_for_run(&run_id).unwrap();
    let parent_held = yunta_testkit::accepted(&parent_events);
    assert_eq!(
        mounted[0].content_hash, parent_held[0].content_hash,
        "the child holds exactly the bytes the parent's log names"
    );
}

// --- what a workflow node acquires from its child ---------------------

#[tokio::test]
async fn a_workflow_node_acquires_the_artifact_its_child_produced() {
    let bench = Bench::new(&[(
        "producer",
        r#"
name: producer
nodes:
  - id: work
    kind: bash
    run: "echo the-report > {{node.artifacts}}/report.md"
    artifacts: { produces: [report.md] }
"#,
    )]);
    let parent = r#"
name: parent
nodes:
  - id: feat
    kind: workflow
    use: producer
    artifacts: { produces: [report.md] }
"#;
    let run_id = RunId::from("run-acquire");
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

    assert_eq!(terminal, RunTerminal::Finished, "{state:?}");
    assert!(
        matches!(state.nodes.get("feat"), Some(NodeState::Finished { .. })),
        "a node whose declared artifact its child produced finishes: {:?}",
        state.nodes.get("feat")
    );

    let child_id = bench
        .children_by_node(&run_id)
        .into_iter()
        .find(|(node, _)| node == "feat")
        .map(|(_, id)| id)
        .expect("the child is linked on the parent's log");

    // The parent holds it as its node's own, stating where it came from.
    let parent_held = yunta_testkit::accepted(&bench.storage.events_for_run(&run_id).unwrap());
    assert_eq!(parent_held.len(), 1, "{parent_held:?}");
    assert_eq!(parent_held[0].producer, Some("feat".into()));
    assert_eq!(parent_held[0].artifact.to_string(), "report.md");
    assert_eq!(
        parent_held[0].origin,
        yunta_core::events::ArtifactOrigin::Inherited {
            run: child_id.clone(),
            producer: Some("work".into()),
        },
        "the acquisition names the child run and the node that produced it there"
    );

    // And the bytes are the ones the child's own log names.
    let child_held = yunta_testkit::accepted(&bench.storage.events_for_run(&child_id).unwrap());
    assert_eq!(child_held.len(), 1, "{child_held:?}");
    assert_eq!(
        parent_held[0].content_hash, child_held[0].content_hash,
        "the parent holds exactly the bytes the child's log names"
    );
    let object = bench
        .runs_root
        .join(run_id.as_str())
        .join("objects")
        .join(parent_held[0].content_hash.as_str());
    assert_eq!(
        std::fs::read_to_string(&object).unwrap().trim(),
        "the-report"
    );
}

#[tokio::test]
async fn a_workflow_node_declaring_what_its_child_never_produced_fails_naming_both() {
    let bench = Bench::new(&[(
        "producer",
        r#"
name: producer
nodes:
  - id: work
    kind: bash
    run: "true"
"#,
    )]);
    let parent = r#"
name: parent
nodes:
  - id: feat
    kind: workflow
    use: producer
    artifacts: { produces: [report.md] }
"#;
    let run_id = RunId::from("run-acquire-missing");
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

    assert!(matches!(terminal, RunTerminal::Paused { .. }), "{state:?}");
    let child_id = bench
        .children_by_node(&run_id)
        .into_iter()
        .find(|(node, _)| node == "feat")
        .map(|(_, id)| id)
        .expect("the child is linked on the parent's log");
    match state.nodes.get("feat") {
        Some(NodeState::Failed { failure, .. }) => {
            // The child run holding none of what this node declares is
            // exactly one declared artifact that did not close: the
            // failure carries it as an entry, with the run it was
            // missing from.
            let entries: Vec<&ArtifactFailure> = failure.failures().collect();
            assert_eq!(entries.len(), 1, "one artifact did not close: {failure}");
            assert_eq!(entries[0].code(), Some("artifact-unheld"));
            assert!(
                matches!(
                    entries[0],
                    ArtifactFailure::Unheld { run, producer, artifact }
                        if *run == child_id
                            && producer.is_none()
                            && *artifact == ArtifactId::of("report.md", None)
                ),
                "the entry names the child run and the artifact: {:?}",
                entries[0]
            );
            let outcome = failure.to_string();
            assert!(
                outcome.contains("report.md") && outcome.contains(child_id.as_str()),
                "the diagnostic must name the artifact and the child run: {outcome}"
            );
        }
        other => panic!("expected feat failed, got {other:?}"),
    }
    // Nothing entered the parent: an artifact it could not acquire is
    // not a fact of the run.
    assert!(
        yunta_testkit::accepted(&bench.storage.events_for_run(&run_id).unwrap()).is_empty(),
        "the parent holds nothing it never acquired"
    );
}

#[tokio::test]
async fn the_findings_of_a_child_run_stand_as_the_workflow_nodes_own() {
    let bench = Bench::new(&[(
        "reviewer",
        r#"
name: reviewer
nodes:
  - id: review
    kind: prompt
    runner: executor
    prompt: "Review it."
    artifacts: { produces: [findings] }
"#,
    )]);
    let parent = r#"
name: parent
nodes:
  - id: feat
    kind: workflow
    use: reviewer
    artifacts: { produces: [findings] }
"#;
    let fixture = r#"
capabilities: { run_tools: true }
sessions:
  - steps:
      - type: run_tool
        tool: yunta_post_finding
        arguments:
          id: null-deref
          severity: blocking
          title: "Resize handler dereferences a null pointer"
          location: "src/ui/resize.rs:142"
          detail: "Resizing before the first paint reaches a null surface."
    outcome: { type: completed, summary: "reviewed" }
"#;
    let run_id = RunId::from("run-acquire-findings");
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

    assert_eq!(terminal, RunTerminal::Finished, "{state:?}");
    assert!(matches!(
        state.nodes.get("feat"),
        Some(NodeState::Finished { .. })
    ));

    // The child states the finding twice — once as the posting its
    // session made, once inside the findings document the engine derived
    // from it — and the parent learns it from the document alone.
    let child_id = bench
        .children_by_node(&run_id)
        .into_iter()
        .find(|(node, _)| node == "feat")
        .map(|(_, id)| id)
        .expect("the child is linked on the parent's log");
    let child_events = bench.storage.events_for_run(&child_id).unwrap();
    assert_eq!(
        child_events
            .iter()
            .filter(|e| matches!(
                e.payload(),
                Some(EventPayload::Findings(FindingEvent::Posted(_)))
            ))
            .count(),
        1
    );

    // The child's findings are on the PARENT's log, as that node's own.
    let parent_events = bench.storage.events_for_run(&run_id).unwrap();
    assert_eq!(
        yunta_testkit::accepted(&parent_events).len(),
        1,
        "one acquisition, whatever the child said about it"
    );
    let posted: Vec<(Option<String>, String)> = parent_events
        .iter()
        .filter_map(|e| match e.payload() {
            Some(EventPayload::Findings(FindingEvent::Posted(p))) => Some((
                e.node_id.as_ref().map(ToString::to_string),
                p.finding.id.to_string(),
            )),
            _ => None,
        })
        .collect();
    assert_eq!(
        posted,
        vec![(Some("feat".to_string()), "null-deref".to_string())],
        "posted once, under the workflow node that acquired them"
    );

    // And the parent's effective set is what a reader of the parent sees.
    let effective = yunta_core::events::findings::FindingLedger::of(&parent_events).effective();
    assert_eq!(effective.len(), 1, "{effective:?}");
    assert_eq!(effective[0].node, Some("feat".into()));
    assert_eq!(effective[0].finding.id.to_string(), "null-deref");
}

// --- a run owns the tasks of every tasks document it acquires ---------

/// A command node writing the one-task document the composition works
/// from, where that node writes the files it declares.
const WRITES_ONE_TASK: &str = r#"
  - id: plan
    kind: bash
    run: |
      cat > {{node.artifacts}}/tasks.yaml <<'YAML'
      tasks:
        - id: T001
          title: "Write done.txt"
          scope: ["done.txt"]
          criteria:
            - cmd: "test -f done.txt"
      YAML
    artifacts: { produces: [tasks] }
"#;

/// The one executor session the loop dispatches for `T001`: it writes
/// what the task's criterion checks for.
const DOES_ONE_TASK: &str = r#"
sessions:
  - match_prompt_contains: "T001"
    effects:
      - { path: done.txt, content: "done" }
    outcome: { type: completed, summary: "did T001" }
"#;

/// Every `(task_id, status)` a run's log records, in order.
fn task_statuses(bench: &Bench, run_id: &RunId) -> Vec<(String, yunta_core::events::TaskStatus)> {
    bench
        .storage
        .events_for_run(run_id)
        .unwrap()
        .into_iter()
        .filter_map(|e| match e.payload() {
            Some(EventPayload::Tasks(TaskEvent::StatusChanged(p))) => {
                Some((p.task_id.to_string(), p.new_status))
            }
            _ => None,
        })
        .collect()
}

/// Every `(node_id, task_id)` a run's log registers, in order.
fn task_registrations(bench: &Bench, run_id: &RunId) -> Vec<(Option<String>, String)> {
    bench
        .storage
        .events_for_run(run_id)
        .unwrap()
        .into_iter()
        .filter_map(|e| match e.payload() {
            Some(EventPayload::Tasks(TaskEvent::Registered(p))) => Some((
                e.node_id.as_ref().map(ToString::to_string),
                p.task_id.to_string(),
            )),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn a_child_born_with_a_mounted_tasks_document_registers_its_tasks_at_birth() {
    let bench = Bench::new(&[(
        "implement-them",
        r#"
name: implement-them
nodes:
  - id: implement
    kind: loop
    runner: executor
    until: all_tasks_complete
    prompt: "Read your task from the tasks document and implement it."
"#,
    )]);
    let parent = format!(
        r#"
name: parent
nodes:
{WRITES_ONE_TASK}
  - id: do
    kind: workflow
    use: implement-them
    mounts:
      - artifact: {{ node: plan, kind: tasks }}
"#
    );

    let run_id = RunId::from("run-mounted-tasks");
    let (terminal, state) = bench
        .run(
            &run_id,
            &parent,
            CONFIG,
            &HashMap::new(),
            DOES_ONE_TASK,
            &NoInteraction,
        )
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.get("do"),
        Some(NodeState::Finished { .. })
    ));

    let child = bench
        .children_by_node(&run_id)
        .into_iter()
        .find(|(node, _)| node == "do")
        .map(|(_, id)| id)
        .expect("the child is linked on the parent's log");
    assert_eq!(
        task_registrations(&bench, &child),
        vec![(None, "T001".to_string())],
        "a mounted document's tasks are the child's from birth, with no node behind them"
    );
    let child_state = yunta_engine::derive(&bench.storage.events_for_run(&child).unwrap());
    assert_eq!(
        child_state.tasks.get("T001"),
        Some(&yunta_core::events::TaskStatus::Done)
    );
}

#[tokio::test]
async fn a_parent_does_not_hold_done_what_its_child_did_in_a_tree_of_its_own() {
    let bench = Bench::new(&[(
        "plan-and-do",
        &format!(
            r#"
name: plan-and-do
nodes:
{WRITES_ONE_TASK}
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Read your task from the tasks document and implement it."
"#
        ),
    )]);
    // The default isolation gives the child a tree of its own, and
    // nothing merges it back: the child's commits live on the child's
    // branch, so the parent's tree does not have the work `done` names.
    let parent = r#"
name: parent
nodes:
  - id: feat
    kind: workflow
    use: plan-and-do
    artifacts: { produces: [tasks] }
"#;

    let run_id = RunId::from("run-acquires-tasks");
    let (terminal, state) = bench
        .run(
            &run_id,
            parent,
            CONFIG,
            &HashMap::new(),
            DOES_ONE_TASK,
            &NoInteraction,
        )
        .await;
    assert_eq!(terminal, RunTerminal::Finished);

    assert_eq!(
        task_registrations(&bench, &run_id),
        vec![(Some("feat".to_string()), "T001".to_string())],
        "the parent registers them under the node that acquired the document"
    );
    assert_eq!(
        task_statuses(&bench, &run_id),
        vec![],
        "a done whose commit the parent's tree does not have follows no registration"
    );
    assert_eq!(
        state.tasks.get("T001"),
        Some(&yunta_core::events::TaskStatus::Pending),
        "the parent holds the task open: the work is in a tree it never took"
    );
}

#[tokio::test]
async fn a_parent_sharing_its_tree_with_its_child_holds_its_child_s_work_done() {
    let bench = Bench::new(&[(
        "plan-and-do",
        &format!(
            r#"
name: plan-and-do
nodes:
{WRITES_ONE_TASK}
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Read your task from the tasks document and implement it."
"#
        ),
    )]);
    // `inherit` puts the child's integration commits in the parent's own
    // tree, which is what makes the child's `done` answerable here.
    let parent = r#"
name: parent
nodes:
  - id: feat
    kind: workflow
    use: plan-and-do
    isolation: inherit
    artifacts: { produces: [tasks] }
"#;

    let run_id = RunId::from("run-shares-its-tree");
    let (terminal, state) = bench
        .run(
            &run_id,
            parent,
            CONFIG,
            &HashMap::new(),
            DOES_ONE_TASK,
            &NoInteraction,
        )
        .await;
    assert_eq!(terminal, RunTerminal::Finished);

    assert_eq!(
        state.tasks.get("T001"),
        Some(&yunta_core::events::TaskStatus::Done),
        "what the child finished in this very tree is finished here"
    );
    assert_eq!(
        task_statuses(&bench, &run_id),
        vec![("T001".to_string(), yunta_core::events::TaskStatus::Done)],
    );
}

#[tokio::test]
async fn a_sibling_mounting_a_finished_child_s_tasks_starts_them_over() {
    let bench = Bench::new(&[
        (
            "plan-and-do",
            &format!(
                r#"
name: plan-and-do
nodes:
{WRITES_ONE_TASK}
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Read your task from the tasks document and implement it."
"#
            ),
        ),
        (
            "hold-them",
            r#"
name: hold-them
nodes:
  - id: note
    kind: bash
    run: "true"
"#,
        ),
    ]);
    // A fan-out: one child plans and implements in its own tree, a
    // sibling mounts the document it left. The sibling's tree branches
    // from the parent's, which never took the first child's branch, so
    // the work that document calls done is not there to stand on and
    // the sibling starts its tasks where any task starts.
    let parent = r#"
name: parent
nodes:
  - id: feat
    kind: workflow
    use: plan-and-do
    artifacts: { produces: [tasks] }
  - id: audit
    kind: workflow
    use: hold-them
    depends_on: [feat]
    mounts:
      - artifact: { node: feat, kind: tasks }
"#;

    let run_id = RunId::from("run-fan-out-tasks");
    let (terminal, _state) = bench
        .run(
            &run_id,
            parent,
            CONFIG,
            &HashMap::new(),
            DOES_ONE_TASK,
            &NoInteraction,
        )
        .await;
    assert_eq!(terminal, RunTerminal::Finished);

    let sibling = bench
        .children_by_node(&run_id)
        .into_iter()
        .find(|(node, _)| node == "audit")
        .map(|(_, id)| id)
        .expect("the sibling is linked on the parent's log");
    assert_eq!(
        task_registrations(&bench, &sibling),
        vec![(None, "T001".to_string())],
        "the mounted document's tasks are the sibling's from birth"
    );
    assert_eq!(
        task_statuses(&bench, &sibling),
        vec![],
        "and none of them crosses: the sibling's tree has no commit the document's done names"
    );
    let state = yunta_engine::derive(&bench.storage.events_for_run(&sibling).unwrap());
    assert_eq!(
        state.tasks.get("T001"),
        Some(&yunta_core::events::TaskStatus::Pending),
        "the sibling has the task to do, not behind it"
    );
}

#[tokio::test]
async fn a_promoted_child_s_successor_does_not_redo_what_its_predecessor_finished() {
    let bench = Bench::new(&[(
        "promotable-tasks",
        r#"
name: promotable-tasks
modes:
  quick: { include: [implement, lint, fix-lint] }
  full:  { include: [implement, ship] }
nodes:
  - id: implement
    kind: loop
    runner: executor
    until: all_tasks_complete
    prompt: "Read your task from the tasks document and implement it."
  - id: lint
    kind: bash
    depends_on: [implement]
    run: "test -f fixed.txt"
    on_failure: { goto: fix-lint, max_reroutes: 0 }
  - id: fix-lint
    kind: bash
    run: "true"
  - id: ship
    kind: bash
    depends_on: [implement]
    run: "echo shipped > shipped.txt"
"#,
    )]);
    let parent = format!(
        r#"
name: parent
nodes:
{WRITES_ONE_TASK}
  - id: feat
    kind: workflow
    use: promotable-tasks
    mounts:
      - artifact: {{ node: plan, kind: tasks }}
"#
    );

    let run_id = RunId::from("run-promoted-tasks");
    let (terminal, _state) = bench
        .run(
            &run_id,
            &parent,
            CONFIG,
            &HashMap::new(),
            DOES_ONE_TASK,
            &AlwaysPromote,
        )
        .await;
    assert_eq!(terminal, RunTerminal::Finished);

    let ids: Vec<RunId> = bench
        .children_created(&run_id)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(ids.len(), 2, "the promotion chains into a second child");
    assert_eq!(
        bench.children_finished(&run_id),
        vec![
            (ids[0].clone(), TerminalState::Promoted),
            (ids[1].clone(), TerminalState::Done),
        ]
    );

    let successor = &ids[1];
    assert_eq!(
        task_statuses(&bench, successor),
        vec![("T001".to_string(), yunta_core::events::TaskStatus::Done)],
        "the successor is born with the work done, and never dispatches it again"
    );
    assert!(
        bench
            ._root
            .path()
            .join("worktrees")
            .join(successor.as_str())
            .join("shipped.txt")
            .exists(),
        "the successor's own mode ran past the loop it had nothing left to do"
    );
}
