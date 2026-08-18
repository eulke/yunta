use std::path::Path;

use yunta_core::{ConfigLayer, Workflow};
use yunta_engine::{build_manifest, ManifestError};

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

fn workflow(yaml: &str) -> Workflow {
    serde_yaml::from_str(yaml).unwrap()
}

fn config(yaml: &str) -> ConfigLayer {
    serde_yaml::from_str(yaml).unwrap()
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

    let a = build_manifest(&workflow(WORKFLOW), &config(CONFIG), dir.path(), dir.path()).unwrap();
    let b = build_manifest(&workflow(WORKFLOW), &config(CONFIG), dir.path(), dir.path()).unwrap();

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

    let frozen = build_manifest(&wf, &config(CONFIG), dir.path(), dir.path()).unwrap();
    assert_eq!(
        frozen.prompts.get(&"plan".into()).map(String::as_str),
        Some("first version")
    );

    // Editing the file after freezing must not affect the frozen manifest,
    // and a rebuild must see the new content as a different manifest.
    std::fs::write(dir.path().join("prompts/plan.md"), "edited later").unwrap();
    assert_eq!(
        frozen.prompts.get(&"plan".into()).map(String::as_str),
        Some("first version")
    );
    let rebuilt = build_manifest(&wf, &config(CONFIG), dir.path(), dir.path()).unwrap();
    assert_ne!(frozen.manifest_hash(), rebuilt.manifest_hash());
}

#[test]
fn an_inline_prompt_freezes_nothing_from_disk() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let manifest =
        build_manifest(&workflow(WORKFLOW), &config(CONFIG), dir.path(), dir.path()).unwrap();

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

    let err = build_manifest(&wf, &config(CONFIG), dir.path(), dir.path()).unwrap_err();
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

    let head = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let head = String::from_utf8(head.stdout).unwrap().trim().to_string();

    let manifest =
        build_manifest(&workflow(WORKFLOW), &config(CONFIG), dir.path(), dir.path()).unwrap();

    assert_eq!(manifest.base_commit, head);
}

#[test]
fn a_non_git_directory_is_a_typed_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap(); // no git init

    let err =
        build_manifest(&workflow(WORKFLOW), &config(CONFIG), dir.path(), dir.path()).unwrap_err();
    assert!(matches!(err, ManifestError::Git { .. }));
}

#[test]
fn each_content_hash_reacts_only_to_its_own_content() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let base =
        build_manifest(&workflow(WORKFLOW), &config(CONFIG), dir.path(), dir.path()).unwrap();

    let other_config = config(
        r#"
runners:
  executor:
    - { adapter: mock, model: a-different-model }
"#,
    );
    let changed =
        build_manifest(&workflow(WORKFLOW), &other_config, dir.path(), dir.path()).unwrap();

    assert_eq!(base.workflow_hash, changed.workflow_hash);
    assert_ne!(base.config_hash, changed.config_hash);
    assert_ne!(base.manifest_hash(), changed.manifest_hash());
}

#[test]
fn a_manifest_survives_yaml_round_trip_with_the_same_hash() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let manifest =
        build_manifest(&workflow(WORKFLOW), &config(CONFIG), dir.path(), dir.path()).unwrap();

    let yaml = serde_yaml::to_string(&manifest).unwrap();
    let reread: yunta_core::Manifest = serde_yaml::from_str(&yaml).unwrap();

    assert_eq!(manifest.manifest_hash(), reread.manifest_hash());
    assert_eq!(manifest, reread);
}
