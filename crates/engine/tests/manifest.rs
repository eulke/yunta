use std::collections::HashMap;

use yunta_core::{ConfigLayer, Workflow};
use yunta_engine::{build_manifest, ManifestError};
use yunta_testkit::{git_output, init_repo};

fn workflow(yaml: &str) -> Workflow {
    serde_norway::from_str(yaml).unwrap()
}

fn config(yaml: &str) -> ConfigLayer {
    serde_norway::from_str(yaml).unwrap()
}

const WORKFLOW: &str = r#"
name: bootstrap
nodes:
  - id: plan
    kind: bash
    run: "cp ledger.yaml artifacts/plan.yaml"
  - id: implement
    kind: loop
    until: all_tasks_complete
    prompt: "Read your task from the ledger and implement it."
    depends_on: [plan]
"#;

const CONFIG: &str = r#"
runners:
  executor:
    - { adapter: mock, model: fixture }
"#;

#[test]
fn the_same_inputs_always_produce_the_same_manifest_hash() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let a = build_manifest(
        &workflow(WORKFLOW),
        &config(CONFIG),
        dir.path(),
        dir.path(),
        &HashMap::new(),
    )
    .unwrap();
    let b = build_manifest(
        &workflow(WORKFLOW),
        &config(CONFIG),
        dir.path(),
        dir.path(),
        &HashMap::new(),
    )
    .unwrap();

    assert_eq!(a.manifest_hash(), b.manifest_hash());
    assert_eq!(a.workflow_hash, b.workflow_hash);
    assert_eq!(a.config_hash, b.config_hash);
}

#[test]
fn a_file_prompt_is_frozen_by_content_not_by_path() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    std::fs::create_dir_all(dir.path().join("prompts")).unwrap();
    std::fs::write(dir.path().join("prompts/plan.md"), "first version").unwrap();

    let wf = workflow(
        r#"
name: with-file-prompt
nodes:
  - id: plan
    kind: prompt
    prompt: { file: prompts/plan.md }
"#,
    );

    let frozen = build_manifest(
        &wf,
        &config(CONFIG),
        dir.path(),
        dir.path(),
        &HashMap::new(),
    )
    .unwrap();
    assert_eq!(
        frozen.prompts.get("plan").map(String::as_str),
        Some("first version")
    );

    // Editing the file after freezing must not affect the frozen manifest,
    // and a rebuild must see the new content as a different manifest.
    std::fs::write(dir.path().join("prompts/plan.md"), "edited later").unwrap();
    assert_eq!(
        frozen.prompts.get("plan").map(String::as_str),
        Some("first version")
    );
    let rebuilt = build_manifest(
        &wf,
        &config(CONFIG),
        dir.path(),
        dir.path(),
        &HashMap::new(),
    )
    .unwrap();
    assert_ne!(frozen.manifest_hash(), rebuilt.manifest_hash());
}

#[test]
fn an_inline_prompt_freezes_nothing_from_disk() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let manifest = build_manifest(
        &workflow(WORKFLOW),
        &config(CONFIG),
        dir.path(),
        dir.path(),
        &HashMap::new(),
    )
    .unwrap();

    assert!(manifest.prompts.is_empty());
}

#[test]
fn a_missing_prompt_file_is_a_typed_error_naming_the_node() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let wf = workflow(
        r#"
name: broken
nodes:
  - id: plan
    kind: prompt
    prompt: { file: prompts/missing.md }
"#,
    );

    let err = build_manifest(
        &wf,
        &config(CONFIG),
        dir.path(),
        dir.path(),
        &HashMap::new(),
    )
    .unwrap_err();
    match err {
        ManifestError::PromptFile { node, path, .. } => {
            assert_eq!(node.as_str(), "plan");
            assert!(path.ends_with("prompts/missing.md"));
        }
        other => panic!("expected PromptFile, got {other}"),
    }
}

#[test]
fn base_commit_is_the_repository_head() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let head = git_output(dir.path(), &["rev-parse", "HEAD"]);

    let manifest = build_manifest(
        &workflow(WORKFLOW),
        &config(CONFIG),
        dir.path(),
        dir.path(),
        &HashMap::new(),
    )
    .unwrap();

    assert_eq!(manifest.base_commit.as_str(), head);
}

#[test]
fn a_non_git_directory_is_a_typed_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap(); // no git init

    let err = build_manifest(
        &workflow(WORKFLOW),
        &config(CONFIG),
        dir.path(),
        dir.path(),
        &HashMap::new(),
    )
    .unwrap_err();
    assert!(matches!(err, ManifestError::Git { .. }));
}

#[test]
fn each_content_hash_reacts_only_to_its_own_content() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let base = build_manifest(
        &workflow(WORKFLOW),
        &config(CONFIG),
        dir.path(),
        dir.path(),
        &HashMap::new(),
    )
    .unwrap();

    let other_config = config(
        r#"
runners:
  executor:
    - { adapter: mock, model: a-different-model }
"#,
    );
    let changed = build_manifest(
        &workflow(WORKFLOW),
        &other_config,
        dir.path(),
        dir.path(),
        &HashMap::new(),
    )
    .unwrap();

    assert_eq!(base.workflow_hash, changed.workflow_hash);
    assert_ne!(base.config_hash, changed.config_hash);
    assert_ne!(base.manifest_hash(), changed.manifest_hash());
}

#[test]
fn a_manifest_survives_yaml_round_trip_with_the_same_hash() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let manifest = build_manifest(
        &workflow(WORKFLOW),
        &config(CONFIG),
        dir.path(),
        dir.path(),
        &HashMap::new(),
    )
    .unwrap();

    let yaml = serde_norway::to_string(&manifest).unwrap();
    let reread: yunta_core::Manifest = serde_norway::from_str(&yaml).unwrap();

    assert_eq!(manifest.manifest_hash(), reread.manifest_hash());
    assert_eq!(manifest, reread);
}

#[test]
fn isolation_defaults_to_worktree_and_freezes_into_the_manifest() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let manifest = build_manifest(
        &workflow(WORKFLOW),
        &config(CONFIG),
        dir.path(),
        dir.path(),
        &HashMap::new(),
    )
    .unwrap();

    assert_eq!(manifest.isolation, yunta_core::Isolation::Worktree);
}

#[test]
fn an_explicit_none_isolation_freezes_as_none() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    let cfg = config("defaults:\n  isolation: none\n");

    let manifest = build_manifest(
        &workflow(WORKFLOW),
        &cfg,
        dir.path(),
        dir.path(),
        &HashMap::new(),
    )
    .unwrap();

    assert_eq!(manifest.isolation, yunta_core::Isolation::None);
}

#[test]
fn max_parallel_nodes_defaults_to_1_and_freezes_into_the_manifest() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let manifest = build_manifest(
        &workflow(WORKFLOW),
        &config(CONFIG),
        dir.path(),
        dir.path(),
        &HashMap::new(),
    )
    .unwrap();

    assert_eq!(manifest.max_parallel_nodes, 1);
}

#[test]
fn an_explicit_max_parallel_nodes_freezes_that_value() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    let cfg = config("defaults:\n  max_parallel_nodes: 4\n");

    let manifest = build_manifest(
        &workflow(WORKFLOW),
        &cfg,
        dir.path(),
        dir.path(),
        &HashMap::new(),
    )
    .unwrap();

    assert_eq!(manifest.max_parallel_nodes, 4);
}

// --- inputs: freeze into the manifest -----------------------------------------

const WORKFLOW_WITH_INPUT: &str = r#"
name: with-input
inputs:
  idea:
    type: string
    required: true
nodes:
  - id: plan
    kind: bash
    run: "true"
"#;

#[test]
fn a_required_input_with_no_value_refuses_before_any_worktree_work() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let err = build_manifest(
        &workflow(WORKFLOW_WITH_INPUT),
        &config(CONFIG),
        dir.path(),
        dir.path(),
        &HashMap::new(),
    )
    .unwrap_err();

    assert!(matches!(err, ManifestError::Inputs(_)));
    assert!(err.to_string().contains("idea"), "got: {err}");
}

#[test]
fn a_provided_input_value_freezes_into_the_manifest() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let provided = HashMap::from([("idea".to_string(), "build a thing".to_string())]);
    let manifest = build_manifest(
        &workflow(WORKFLOW_WITH_INPUT),
        &config(CONFIG),
        dir.path(),
        dir.path(),
        &provided,
    )
    .unwrap();

    assert_eq!(
        manifest.inputs.get("idea").map(String::as_str),
        Some("build a thing")
    );
}

// --- `runners:` fan-out expands statically in the manifest --------------------

#[test]
fn a_runners_fanout_node_expands_into_one_node_per_role() {
    let yaml = r#"
name: fanout
modes:
  quick: { include: [work, review] }
  full: { include: all }
nodes:
  - id: work
    kind: bash
    run: "true"
  - id: review
    kind: prompt
    runners: [reviewer, reviewer-alt]
    depends_on: [work]
    prompt: "Audit as {{runner.role}}."
  - id: ship
    kind: bash
    depends_on: [review]
    run: "true"
"#;
    let workflow: yunta_core::Workflow = serde_norway::from_str(yaml).unwrap();
    let config: yunta_core::ConfigLayer = serde_norway::from_str(
        "runners:\n  reviewer:\n    - { adapter: mock, model: m }\n  reviewer-alt:\n    - { adapter: mock, model: m }\n",
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    let manifest = yunta_engine::build_manifest(
        &workflow,
        &config,
        dir.path(),
        dir.path(),
        &std::collections::HashMap::new(),
    )
    .unwrap();

    let ids: Vec<&str> = manifest
        .workflow
        .nodes
        .iter()
        .map(|n| n.id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["work", "review@reviewer", "review@reviewer-alt", "ship"]
    );

    let expanded = &manifest.workflow.nodes[1];
    assert_eq!(
        expanded.runner.as_ref().map(|value| value.as_str()),
        Some("reviewer")
    );
    assert!(expanded.runners.is_empty());

    // Downstream dependencies rewire onto every expanded sibling.
    let ship = manifest
        .workflow
        .nodes
        .iter()
        .find(|n| n.id.as_str() == "ship")
        .unwrap();
    let deps: Vec<&str> = ship.depends_on.iter().map(|d| d.as_str()).collect();
    assert_eq!(deps, vec!["review@reviewer", "review@reviewer-alt"]);

    // Mode include lists rewrite too — `quick` still covers the review.
    let quick = &manifest.workflow.modes.as_ref().unwrap()["quick"];
    match &quick.include {
        yunta_core::ModeInclude::Nodes(nodes) => {
            let names: Vec<&str> = nodes.iter().map(|n| n.as_str()).collect();
            assert_eq!(
                names,
                vec!["work", "review@reviewer", "review@reviewer-alt"]
            );
        }
        other => panic!("got {other:?}"),
    }
}
