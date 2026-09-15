//! `kind: workflow` — composition as linked runs: each
//! sub-workflow is a **complete run** (own run_id, manifest, log,
//! run.dir), the parent records `child_run_created`/`child_run_finished`
//! and treats the child's terminal state as the node's result, resume
//! reaches orphaned children recursively, and history pins the child's
//! frozen manifest through its `child_run_id` — never re-resolving the
//! workflow name.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use yunta_core::diagnostic::ArtifactFailure;
use yunta_core::events::{ArtifactId, EventPayload, StoredEvent, TerminalState};
use yunta_core::events::{ChildEvent, FindingEvent, RunEvent, TaskEvent};
use yunta_core::{ContentHash, Manifest, NodeId, RunId, Workflow};
use yunta_engine::{NodeState, RunReport, RunTerminal};
use yunta_testkit::{git, write, ApproveEverything, Bench, ScriptedInteraction};

/// Every `(child_run_id, child_workflow_hash)` a parent recorded, in
/// log order.
fn children_created(events: &[StoredEvent]) -> Vec<(RunId, ContentHash)> {
    events
        .iter()
        .filter_map(|e| match e.payload() {
            Some(EventPayload::Children(ChildEvent::Created(p))) => {
                Some((p.child_run_id.clone(), p.child_workflow_hash.clone()))
            }
            _ => None,
        })
        .collect()
}

/// Every `(node_id, child_run_id)` a parent recorded, in log order —
/// which node each child was born from.
fn children_by_node(events: &[StoredEvent]) -> Vec<(String, RunId)> {
    events
        .iter()
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

/// Every `(child_run_id, terminal_state)` a parent recorded, in log
/// order.
fn children_finished(events: &[StoredEvent]) -> Vec<(RunId, TerminalState)> {
    events
        .iter()
        .filter_map(|e| match e.payload() {
            Some(EventPayload::Children(ChildEvent::Finished(p))) => {
                Some((p.child_run_id.clone(), p.terminal_state))
            }
            _ => None,
        })
        .collect()
}

/// The manifest a run froze at birth, read from its run directory.
fn manifest_of(runs_root: &Path, run_id: &RunId) -> Manifest {
    let path = runs_root.join(run_id.as_str()).join("manifest.yaml");
    serde_norway::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

/// The root a child run's tree goes under: the `runs` sibling
/// `worktrees` directory.
fn child_trees(runs_root: &Path) -> PathBuf {
    runs_root
        .parent()
        .expect("the runs root sits beside the worktrees root")
        .join("worktrees")
}

const EMPTY_FIXTURE: &str = "sessions: []\n";

// --- The child is a complete, linked run -------------------------------------

#[tokio::test]
async fn a_workflow_node_runs_its_child_as_a_complete_linked_run() {
    let bench = Bench::with_run_id("run-parent-1")
        .with_workflow(
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
        )
        .with_inputs(&[("thing", "hola")]);
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
    let RunReport { terminal, state } = bench.run(parent, EMPTY_FIXTURE).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.state("feat"),
        Some(NodeState::Finished { .. })
    ));

    // The parent's log carries the link pair.
    let parent_events = bench.events();
    let created = children_created(&parent_events);
    assert_eq!(created.len(), 1);
    let (child_id, recorded_hash) = &created[0];
    assert_eq!(
        children_finished(&parent_events),
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
    let child_manifest = manifest_of(&bench.runs_root, child_id);
    assert_eq!(child_manifest.workflow.name, "child-wf");
    assert_eq!(&child_manifest.workflow_hash, recorded_hash);
    assert_eq!(
        child_manifest.inputs.get("idea").map(String::as_str),
        Some("hola")
    );
    assert_eq!(child_manifest.isolation, yunta_core::Isolation::Worktree);

    // The child worked in its own tree, branched off the parent's HEAD.
    let child_tree = child_trees(&bench.runs_root).join(child_id.as_str());
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
    let bench = Bench::with_run_id("run-parent-ulid")
        .with_workflow(
            "child-wf",
            r#"
name: child-wf
nodes:
  - id: work
    kind: bash
    run: "true"
"#,
        )
        .with_id_source(Arc::new(yunta_core::SystemIdSource));
    let parent = r#"
name: parent
nodes:
  - id: feat
    kind: workflow
    use: child-wf
"#;
    let RunReport { terminal, .. } = bench.run(parent, EMPTY_FIXTURE).await;
    assert_eq!(terminal, RunTerminal::Finished);

    // The link is the log's, not the name's: the child id is a fresh
    // ULID, and nothing in it is derived from the parent or the node.
    let created = children_created(&bench.events());
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
    let bench = Bench::with_run_id("run-parent-tokens").with_workflow(
        "spender",
        r#"
name: spender
nodes:
  - id: talk
    kind: prompt
    runner: executor
    prompt: "Do the thing."
"#,
    );
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
    let RunReport { terminal, state } = bench.run(parent, fixture).await;

    assert_eq!(terminal, RunTerminal::Finished);
    // The children's Usage aggregates upward — each chain member's
    // whole spend rides its own `child_run_finished.tokens`, so
    // the parent's derived total (what `limits.max_tokens_per_run`
    // compares against) includes it exactly once. The node's own close
    // deliberately carries none — it would double-count.
    assert_eq!(state.total_tokens().input, 100);
    assert_eq!(state.total_tokens().output, 20);
    match state.nodes.state("feat") {
        Some(NodeState::Finished { tokens, .. }) => {
            assert_eq!(
                tokens.input, 0,
                "the node close must not re-count the child"
            );
            assert_eq!(tokens.output, 0);
        }
        other => panic!("expected feat finished, got {other:?}"),
    }
    let events = bench.events();
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
    let bench = Bench::with_run_id("run-parent-resume").with_workflow(
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
    );
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
    // No interaction surface: the child's internal gate has nobody to
    // ask, so the child pauses waiting — and the parent pauses with it,
    // since a parent run spends most of its life waiting on its children.
    let RunReport { terminal, .. } = bench.run(parent, EMPTY_FIXTURE).await;

    let RunTerminal::Paused { reason } = terminal else {
        panic!("expected the parent to pause on its paused child, got {terminal:?}");
    };
    // The child paused on its own log; the parent's node stays open (no
    // child_run_finished) so resume knows to go back in.
    let created = children_created(&bench.events());
    assert_eq!(created.len(), 1);
    assert!(children_finished(&bench.events()).is_empty());
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
    let RunReport { terminal, state } = bench.wake_answering(&ApproveEverything::new("test")).await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.state("feat"),
        Some(NodeState::Finished { .. })
    ));
    assert_eq!(children_created(&bench.events()).len(), 1);
    assert_eq!(
        children_finished(&bench.events()),
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
    let parent = r#"
name: parent
nodes:
  - id: feat
    kind: workflow
    use: evolving
"#;

    let first = Bench::with_run_id("run-parent-v1").with_workflow("evolving", CHILD_V1);
    let RunReport { terminal, .. } = first.run(parent, EMPTY_FIXTURE).await;
    assert_eq!(terminal, RunTerminal::Finished);
    let (first_child, v1_hash) = children_created(&first.events())[0].clone();

    // The child workflow evolves between parent executions (a long
    // process picks up improvements)...
    let second = first
        .beside("run-parent-v2")
        .with_workflow("evolving", CHILD_V2);
    let RunReport { terminal, .. } = second.run(parent, EMPTY_FIXTURE).await;
    assert_eq!(terminal, RunTerminal::Finished);
    let (_, v2_hash) = children_created(&second.events())[0].clone();

    // ...so the second child froze the new file...
    assert_ne!(
        v1_hash, v2_hash,
        "the second run must resolve the current file"
    );

    // ...while history stays pinned: following the recorded
    // child_run_id reaches the child's own frozen manifest, which still
    // hashes to v1 — reproducing the old parent never resolves
    // `evolving@current`.
    let pinned = manifest_of(&first.runs_root, &first_child);
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
    let bench = Bench::with_run_id("run-release")
        .with_workflow("design-review", &design_review)
        .with_workflow("build-feature", &build_feature)
        .with_workflow("qa-review", &qa_review)
        .with_inputs(&[
            ("rfc", "rfc.md"),
            ("feat_a", "feature a"),
            ("feat_b", "feature b"),
        ]);
    write(&bench.worktree.join("rfc.md"), "the rfc");
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
    let RunReport { terminal, state } = bench
        .run_with_interaction(&release_cycle, fixture, &ApproveEverything::new("test"))
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
            matches!(state.nodes.state(node), Some(NodeState::Finished { .. })),
            "node `{node}` should be finished, got {:?}",
            state.nodes.state(node)
        );
    }
    // One child per `kind: workflow` node, each with an id of its own.
    let parent_events = bench.events();
    let created = children_by_node(&parent_events);
    let mut nodes: Vec<&str> = created.iter().map(|(node, _)| node.as_str()).collect();
    nodes.sort();
    assert_eq!(nodes, vec!["design", "feat-a", "feat-b", "qa"]);
    let mut child_ids: Vec<&RunId> = created.iter().map(|(_, id)| id).collect();
    child_ids.sort();
    child_ids.dedup();
    assert_eq!(child_ids.len(), 4, "every child has an id of its own");
    let finished = children_finished(&parent_events);
    assert_eq!(finished.len(), 4);
    assert!(finished
        .iter()
        .all(|(_, terminal)| *terminal == TerminalState::Done));
}

// --- Deliberate limits -------------------------------------------------------

#[tokio::test]
async fn workflow_nesting_depth_is_capped_at_runtime() {
    let bench = Bench::with_run_id("run-deep")
        .with_workflow(
            "mid",
            r#"
name: mid
nodes:
  - id: deeper
    kind: workflow
    use: leaf
"#,
        )
        .with_workflow(
            "leaf",
            r#"
name: leaf
nodes:
  - id: work
    kind: bash
    run: "true"
"#,
        );
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
    let RunReport { terminal, .. } = bench.run_with_config(parent, EMPTY_FIXTURE, config).await;
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
    let bench = Bench::with_run_id("run-missing");
    let parent = r#"
name: parent
nodes:
  - id: feat
    kind: workflow
    use: nope
"#;
    let RunReport { terminal, state } = bench.run(parent, EMPTY_FIXTURE).await;
    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    match state.nodes.state("feat") {
        Some(NodeState::Failed { failure, .. }) => {
            let outcome = failure.to_string();
            assert!(
                outcome.contains(".yunta/workflows/nope.yaml"),
                "the diagnostic must name the resolved path: {outcome}"
            );
        }
        other => panic!("expected feat failed, got {other:?}"),
    }
    assert!(children_created(&bench.events()).is_empty());
}

// --- a promoted child chains into its successor -----------------------

#[tokio::test]
async fn a_promoted_child_chains_into_its_successor_automatically() {
    let bench = Bench::with_run_id("run-parent-chain").with_workflow(
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
    );
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
    let RunReport { terminal, state } = bench
        .run_with_interaction(
            parent,
            EMPTY_FIXTURE,
            &ScriptedInteraction::choose("promote"),
        )
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.state("feat"),
        Some(NodeState::Finished { .. })
    ));

    // Both chain members are linked children of the same node, in
    // order, each with an id of its own.
    let created = children_created(&bench.events());
    let ids: Vec<RunId> = created.iter().map(|(id, _)| id.clone()).collect();
    assert_eq!(
        ids.len(),
        2,
        "the successor must be a new linked child, recorded on the parent log"
    );
    assert_ne!(ids[0], ids[1]);
    let finished = children_finished(&bench.events());
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
    let successor_tree = child_trees(&bench.runs_root).join(successor.as_str());
    assert!(successor_tree.join("shipped.txt").exists());
}

// --- cross-run artifact mounts -----------------------------------

#[tokio::test]
async fn mounts_copy_parent_and_sibling_artifacts_into_the_child_at_birth() {
    let bench = Bench::with_run_id("run-mounts")
        .with_workflow(
            "producer",
            r#"
name: producer
nodes:
  - id: work
    kind: bash
    run: "echo the-report > {{node.artifacts}}/report.md"
    artifacts: { produces: [report.md] }
"#,
        )
        .with_workflow(
            "consumer",
            r#"
name: consumer
nodes:
  - id: verify
    kind: bash
    run: "true"
"#,
        );
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
    let RunReport { terminal, state } = bench.run(parent, EMPTY_FIXTURE).await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.state("cons"),
        Some(NodeState::Finished { .. })
    ));

    // Each mount implies depends_on — visible in the frozen graph.
    let manifest = bench.manifest();
    let cons = manifest
        .workflow
        .nodes
        .iter()
        .find(|n| n.id.as_str() == "cons")
        .unwrap();
    assert!(cons.depends_on.contains(&"prod".into()));
    assert!(cons.depends_on.contains(&"plan".into()));

    // The copies landed in the child's own run.dir at birth: the
    // sibling's artifact through the recorded link, the parent's own
    // under its `as:` rename.
    let cons_id = children_by_node(&bench.events())
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
    let from_parent = yunta_core::events::RecordedOrigin::Inherited {
        run: bench.run_id.clone(),
        producer: Some("plan".into()),
    };
    assert_eq!(mounted[1].origin, from_parent);
    let prod_id = children_by_node(&bench.events())
        .into_iter()
        .find(|(node, _)| node == "prod")
        .map(|(_, id)| id)
        .expect("the prod child is linked on the parent's log");
    assert_eq!(
        mounted[0].origin,
        yunta_core::events::RecordedOrigin::Inherited {
            run: prod_id,
            producer: Some("work".into()),
        }
    );
}

#[tokio::test]
async fn a_mount_whose_source_was_never_produced_fails_the_node_before_the_child_exists() {
    let bench = Bench::with_run_id("run-mount-missing").with_workflow(
        "consumer",
        r#"
name: consumer
nodes:
  - id: verify
    kind: bash
    run: "true"
"#,
    );
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
    let RunReport { terminal, state } = bench.run(parent, EMPTY_FIXTURE).await;
    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    match state.nodes.state("cons") {
        Some(NodeState::Failed { failure, .. }) => {
            // A source the run does not hold is a declared artifact that
            // did not close, so it reaches every surface as one entry
            // with its own code — never a sentence to be taken apart.
            let entries: Vec<&ArtifactFailure> = failure.failures().collect();
            assert_eq!(entries.len(), 1, "one artifact did not close: {failure}");
            assert_eq!(
                entries[0]
                    .code()
                    .map(yunta_core::diagnostic::DiagnosticCode::as_str),
                Some("artifact-unheld")
            );
            assert!(
                matches!(
                    entries[0],
                    ArtifactFailure::Unheld { run, producer, artifact }
                        if *run == bench.run_id
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
    assert!(children_created(&bench.events()).is_empty());
}

#[tokio::test]
async fn a_child_consumes_a_mounted_artifact_through_context_without_naming_a_node() {
    let bench = Bench::with_run_id("run-mount-context").with_workflow(
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
    );
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
    let RunReport { terminal, state } = bench.run(parent, fixture).await;
    // The node-less artifact source resolved against the child's own
    // run.dir — a missing file would have failed the child's node, so a
    // clean finish is the proof the mount fed the context.
    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.state("cons"),
        Some(NodeState::Finished { .. })
    ));
}

#[tokio::test]
async fn a_mount_carries_the_bytes_the_log_names_even_with_no_view_left() {
    // A mount resolves through the source run's log and its object
    // store, so a view somebody deleted between the producer and the
    // mount changes nothing the child receives.
    let bench = Bench::with_run_id("run-mount-from-log").with_workflow(
        "consumer",
        r#"
name: consumer
nodes:
  - id: verify
    kind: bash
    run: "true"
"#,
    );
    // The view belongs to the engine, so the node that deletes it names
    // it by its absolute path rather than through a template no workflow
    // has for it.
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
        view = bench.run_dir().join(yunta_core::ARTIFACTS_DIR).display()
    );
    let RunReport { terminal, state } = bench.run(&parent, EMPTY_FIXTURE).await;
    assert_eq!(terminal, RunTerminal::Finished, "{state:?}");

    let cons_id = children_by_node(&bench.events())
        .into_iter()
        .find(|(node, _)| node == "cons")
        .map(|(_, id)| id)
        .expect("the cons child is linked on the parent's log");
    let child_events = bench.storage.events_for_run(&cons_id).unwrap();
    let mounted = yunta_testkit::accepted(&child_events);
    assert_eq!(mounted.len(), 1, "{mounted:?}");
    assert_eq!(
        mounted[0].origin,
        yunta_core::events::RecordedOrigin::Inherited {
            run: bench.run_id.clone(),
            producer: Some("plan".into()),
        },
        "the mount carries where it came from and who produced it there"
    );
    let parent_held = bench.accepted();
    assert_eq!(
        mounted[0].content_hash, parent_held[0].content_hash,
        "the child holds exactly the bytes the parent's log names"
    );
}

// --- what a workflow node acquires from its child ---------------------

#[tokio::test]
async fn a_workflow_node_acquires_the_artifact_its_child_produced() {
    let bench = Bench::with_run_id("run-acquire").with_workflow(
        "producer",
        r#"
name: producer
nodes:
  - id: work
    kind: bash
    run: "echo the-report > {{node.artifacts}}/report.md"
    artifacts: { produces: [report.md] }
"#,
    );
    let parent = r#"
name: parent
nodes:
  - id: feat
    kind: workflow
    use: producer
    artifacts: { produces: [report.md] }
"#;
    let RunReport { terminal, state } = bench.run(parent, EMPTY_FIXTURE).await;

    assert_eq!(terminal, RunTerminal::Finished, "{state:?}");
    assert!(
        matches!(state.nodes.state("feat"), Some(NodeState::Finished { .. })),
        "a node whose declared artifact its child produced finishes: {:?}",
        state.nodes.state("feat")
    );

    let child_id = children_by_node(&bench.events())
        .into_iter()
        .find(|(node, _)| node == "feat")
        .map(|(_, id)| id)
        .expect("the child is linked on the parent's log");

    // The parent holds it as its node's own, stating where it came from.
    let parent_held = bench.accepted();
    assert_eq!(parent_held.len(), 1, "{parent_held:?}");
    assert_eq!(parent_held[0].producer, Some("feat".into()));
    assert_eq!(parent_held[0].artifact.to_string(), "report.md");
    assert_eq!(
        parent_held[0].origin,
        yunta_core::events::RecordedOrigin::Inherited {
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
    let object = bench.object(&parent_held[0].content_hash).unwrap();
    assert_eq!(String::from_utf8(object).unwrap().trim(), "the-report");
}

#[tokio::test]
async fn a_workflow_node_declaring_what_its_child_never_produced_fails_naming_both() {
    let bench = Bench::with_run_id("run-acquire-missing").with_workflow(
        "producer",
        r#"
name: producer
nodes:
  - id: work
    kind: bash
    run: "true"
"#,
    );
    let parent = r#"
name: parent
nodes:
  - id: feat
    kind: workflow
    use: producer
    artifacts: { produces: [report.md] }
"#;
    let RunReport { terminal, state } = bench.run(parent, EMPTY_FIXTURE).await;

    assert!(matches!(terminal, RunTerminal::Paused { .. }), "{state:?}");
    let child_id = children_by_node(&bench.events())
        .into_iter()
        .find(|(node, _)| node == "feat")
        .map(|(_, id)| id)
        .expect("the child is linked on the parent's log");
    match state.nodes.state("feat") {
        Some(NodeState::Failed { failure, .. }) => {
            // The child run holding none of what this node declares is
            // exactly one declared artifact that did not close: the
            // failure carries it as an entry, with the run it was
            // missing from.
            let entries: Vec<&ArtifactFailure> = failure.failures().collect();
            assert_eq!(entries.len(), 1, "one artifact did not close: {failure}");
            assert_eq!(
                entries[0]
                    .code()
                    .map(yunta_core::diagnostic::DiagnosticCode::as_str),
                Some("artifact-unheld")
            );
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
        bench.accepted().is_empty(),
        "the parent holds nothing it never acquired"
    );
}

#[tokio::test]
async fn the_findings_of_a_child_run_stand_as_the_workflow_nodes_own() {
    let bench = Bench::with_run_id("run-acquire-findings").with_workflow(
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
    );
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
    let RunReport { terminal, state } = bench.run(parent, fixture).await;

    assert_eq!(terminal, RunTerminal::Finished, "{state:?}");
    assert!(matches!(
        state.nodes.state("feat"),
        Some(NodeState::Finished { .. })
    ));

    // The child states the finding twice — once as the posting its
    // session made, once inside the findings document the engine derived
    // from it — and the parent learns it from the document alone.
    let child_id = children_by_node(&bench.events())
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
    let parent_events = bench.events();
    assert_eq!(
        bench.accepted().len(),
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
fn task_statuses(events: &[StoredEvent]) -> Vec<(String, yunta_core::events::TaskStatus)> {
    events
        .iter()
        .filter_map(|e| match e.payload() {
            Some(EventPayload::Tasks(TaskEvent::StatusChanged(p))) => {
                Some((p.task_id.to_string(), p.new_status))
            }
            _ => None,
        })
        .collect()
}

/// Every `(node_id, task_id)` a run's log registers, in order.
fn task_registrations(events: &[StoredEvent]) -> Vec<(Option<String>, String)> {
    events
        .iter()
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
    let bench = Bench::with_run_id("run-mounted-tasks").with_workflow(
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
    );
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

    let RunReport { terminal, state } = bench.run(&parent, DOES_ONE_TASK).await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.state("do"),
        Some(NodeState::Finished { .. })
    ));

    let child = children_by_node(&bench.events())
        .into_iter()
        .find(|(node, _)| node == "do")
        .map(|(_, id)| id)
        .expect("the child is linked on the parent's log");
    assert_eq!(
        task_registrations(&bench.storage.events_for_run(&child).unwrap()),
        vec![(None, "T001".to_string())],
        "a mounted document's tasks are the child's from birth, with no node behind them"
    );
    let child_state = yunta_engine::derive(&bench.storage.events_for_run(&child).unwrap());
    assert_eq!(
        child_state.tasks.status("T001"),
        Some(yunta_core::events::TaskStatus::Done)
    );
}

#[tokio::test]
async fn a_parent_does_not_hold_done_what_its_child_did_in_a_tree_of_its_own() {
    let bench = Bench::with_run_id("run-acquires-tasks").with_workflow(
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
    );
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

    let RunReport { terminal, state } = bench.run(parent, DOES_ONE_TASK).await;
    assert_eq!(terminal, RunTerminal::Finished);

    assert_eq!(
        task_registrations(&bench.events()),
        vec![(Some("feat".to_string()), "T001".to_string())],
        "the parent registers them under the node that acquired the document"
    );
    assert_eq!(
        task_statuses(&bench.events()),
        vec![],
        "a done whose commit the parent's tree does not have follows no registration"
    );
    assert_eq!(
        state.tasks.status("T001"),
        Some(yunta_core::events::TaskStatus::Pending),
        "the parent holds the task open: the work is in a tree it never took"
    );
}

#[tokio::test]
async fn a_parent_sharing_its_tree_with_its_child_holds_its_child_s_work_done() {
    let bench = Bench::with_run_id("run-shares-its-tree").with_workflow(
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
    );
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

    let RunReport { terminal, state } = bench.run(parent, DOES_ONE_TASK).await;
    assert_eq!(terminal, RunTerminal::Finished);

    assert_eq!(
        state.tasks.status("T001"),
        Some(yunta_core::events::TaskStatus::Done),
        "what the child finished in this very tree is finished here"
    );
    assert_eq!(
        task_statuses(&bench.events()),
        vec![("T001".to_string(), yunta_core::events::TaskStatus::Done)],
    );
}

#[tokio::test]
async fn a_sibling_mounting_a_finished_child_s_tasks_starts_them_over() {
    let bench = Bench::with_run_id("run-fan-out-tasks")
        .with_workflow(
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
        )
        .with_workflow(
            "hold-them",
            r#"
name: hold-them
nodes:
  - id: note
    kind: bash
    run: "true"
"#,
        );
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

    let RunReport { terminal, .. } = bench.run(parent, DOES_ONE_TASK).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let sibling = children_by_node(&bench.events())
        .into_iter()
        .find(|(node, _)| node == "audit")
        .map(|(_, id)| id)
        .expect("the sibling is linked on the parent's log");
    let sibling_events = bench.storage.events_for_run(&sibling).unwrap();
    assert_eq!(
        task_registrations(&sibling_events),
        vec![(None, "T001".to_string())],
        "the mounted document's tasks are the sibling's from birth"
    );
    assert_eq!(
        task_statuses(&sibling_events),
        vec![],
        "and none of them crosses: the sibling's tree has no commit the document's done names"
    );
    let state = yunta_engine::derive(&sibling_events);
    assert_eq!(
        state.tasks.status("T001"),
        Some(yunta_core::events::TaskStatus::Pending),
        "the sibling has the task to do, not behind it"
    );
}

#[tokio::test]
async fn a_promoted_child_s_successor_does_not_redo_what_its_predecessor_finished() {
    let bench = Bench::with_run_id("run-promoted-tasks").with_workflow(
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
    );
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

    let RunReport { terminal, .. } = bench
        .run_with_interaction(
            &parent,
            DOES_ONE_TASK,
            &ScriptedInteraction::choose("promote"),
        )
        .await;
    assert_eq!(terminal, RunTerminal::Finished);

    let ids: Vec<RunId> = children_created(&bench.events())
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(ids.len(), 2, "the promotion chains into a second child");
    assert_eq!(
        children_finished(&bench.events()),
        vec![
            (ids[0].clone(), TerminalState::Promoted),
            (ids[1].clone(), TerminalState::Done),
        ]
    );

    let successor = &ids[1];
    assert_eq!(
        task_statuses(&bench.storage.events_for_run(successor).unwrap()),
        vec![("T001".to_string(), yunta_core::events::TaskStatus::Done)],
        "the successor is born with the work done, and never dispatches it again"
    );
    assert!(
        child_trees(&bench.runs_root)
            .join(successor.as_str())
            .join("shipped.txt")
            .exists(),
        "the successor's own mode ran past the loop it had nothing left to do"
    );
}
