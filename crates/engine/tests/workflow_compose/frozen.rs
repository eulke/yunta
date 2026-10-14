//! What a run freezes of the workflows it reaches: a child from a pack is
//! born with the provenance its parent froze, a prompt a pack workflow
//! names stays inside its pack, and a child that fans out over runners is
//! born with each clone's prompt.

use std::collections::HashMap;
use std::path::Path;

use yunta_core::{ConfigLayer, Workflow};
use yunta_engine::{build_manifest, ManifestError, RunReport, RunTerminal};
use yunta_testkit::{git, init_repo, read, write, Bench, Owner};

use super::{children_created, manifest_of, EMPTY_FIXTURE};

const LEAF: &str = "name: leaf\nnodes:\n  - { id: work, kind: bash, run: \"true\" }\n";

/// One session that completes, for a child whose only node is a prompt.
const ONE_SESSION: &str =
    "dialect: claude-code\nsessions:\n  - outcome: { type: completed, summary: planned }\n";

/// A parent that calls the `pub/leaf` workflow of the pack.
const CALLS_PACK_CHILD: &str =
    "name: parent\nnodes:\n  - { id: call, kind: workflow, use: pub/leaf }\n";

/// A prompt whose file lies outside its pack: `secret.txt` in the repo, five
/// directories up from a pack's `workflows/` directory.
const CLIMBING_PROMPT: &str = "name: climbing\nnodes:\n  - { id: plan, kind: prompt, runner: executor, prompt: { file: ../../../../../secret.txt } }\n";

/// A prompt that climbs one directory and stays inside its pack.
const INSIDE_PROMPT: &str = "name: inside\nnodes:\n  - { id: plan, kind: prompt, runner: executor, prompt: { file: ../prompts/plan.md } }\n";

/// The `pub/leaf` child of a pack, whose prompt climbs one directory and stays inside it.
const INSIDE_PROMPT_CHILD: &str = "name: leaf\nnodes:\n  - { id: plan, kind: prompt, runner: executor, prompt: { file: ../prompts/plan.md } }\n";

/// A child whose prompt node fans out over two runners, each reading the same file.
const FANNED_CHILD: &str = "name: fanned\nnodes:\n  - { id: review, kind: prompt, runners: [alpha, beta], prompt: { file: prompts/review.md } }\n";

/// The manifest of the `pub/tools` pack at `version`, declaring `workflow` among its workflows.
fn pack_manifest(version: &str, workflow: &str) -> String {
    format!(
        "publisher: pub\nname: tools\nversion: {version}\ndeclares:\n  permissions: read-only\n\
         contents:\n  workflows: [{workflow}]\n"
    )
}

/// Writes `text` to `relative`, a path inside the `pub/tools` pack of `root`.
fn pack_file(root: &Path, relative: &str, text: &str) {
    write(&root.join(".yunta/packs/pub/tools").join(relative), text);
}

/// Commits everything a test wrote under `root`.
fn commit(root: &Path) {
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "fixture"]);
}

fn workflow(text: &str) -> Workflow {
    yunta_core::yaml::parse(text).unwrap()
}

/// The prompt each session of a run was sent, in the order the mock saw them.
fn prompts_sent(bench: &Bench) -> Vec<String> {
    bench
        .mock()
        .requests_seen()
        .into_iter()
        .map(|request| request.prompt)
        .collect()
}

#[tokio::test]
async fn a_child_from_a_pack_is_born_with_the_provenance_its_parent_froze() {
    let bench = Bench::with_run_id("run-pack-child-provenance");
    pack_file(
        &bench.worktree,
        "pack.yaml",
        &pack_manifest("1.0.0", "leaf.yaml"),
    );
    pack_file(&bench.worktree, "leaf.yaml", LEAF);
    commit(&bench.worktree);
    // The pack moves to version 2.0.0 after the parent is born and before
    // the child is.
    let parent = r#"
name: parent
nodes:
  - id: bump
    kind: bash
    run: "sed -i.bak 's/version: 1.0.0/version: 2.0.0/' .yunta/packs/pub/tools/pack.yaml"
  - id: call
    kind: workflow
    use: pub/leaf
    depends_on: [bump]
"#;
    let RunReport { terminal, .. } = bench.run(parent, EMPTY_FIXTURE).await;
    assert_eq!(terminal, RunTerminal::Finished);
    let moved = read(&bench.worktree.join(".yunta/packs/pub/tools/pack.yaml"));
    assert!(
        moved.contains("version: 2.0.0"),
        "the pack moved in the checkout: {moved}"
    );

    let (child, _) = children_created(&bench.events())[0].clone();
    let provenance = manifest_of(&bench.runs_root, &child)
        .pack
        .expect("a child from a pack is born with that pack");
    assert_eq!(
        provenance.version, "1.0.0",
        "the child keeps the version its parent froze"
    );
}

#[tokio::test]
async fn a_pack_workflow_prompt_that_climbs_out_of_its_pack_refuses_the_run_at_birth() {
    let owner = Owner::new();
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    pack_file(
        repo.path(),
        "pack.yaml",
        &pack_manifest("1.0.0", "workflows/wf.yaml"),
    );
    write(&repo.path().join("secret.txt"), "hunter2");
    let refused = build_manifest(
        &workflow(CLIMBING_PROMPT),
        &ConfigLayer::default(),
        &repo.path().join(".yunta/packs/pub/tools/workflows"),
        repo.path(),
        &HashMap::new(),
        owner.supervision(),
        None,
    )
    .await
    .expect_err("a prompt that leaves its pack refuses the run");
    let said = refused.to_string();
    assert!(matches!(refused, ManifestError::Frozen(_)), "{said}");
    assert!(
        said.contains("../../../../../secret.txt"),
        "the refusal names the path as written: {said}"
    );
    assert!(
        !said.contains("hunter2"),
        "the file's content never reaches the refusal: {said}"
    );
}

#[tokio::test]
async fn a_pack_workflow_prompt_that_climbs_within_its_pack_is_born_with_it() {
    let owner = Owner::new();
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    pack_file(
        repo.path(),
        "pack.yaml",
        &pack_manifest("1.0.0", "workflows/wf.yaml"),
    );
    pack_file(repo.path(), "prompts/plan.md", "Plan the work.");
    let manifest = build_manifest(
        &workflow(INSIDE_PROMPT),
        &ConfigLayer::default(),
        &repo.path().join(".yunta/packs/pub/tools/workflows"),
        repo.path(),
        &HashMap::new(),
        owner.supervision(),
        None,
    )
    .await
    .unwrap()
    .manifest;
    assert_eq!(
        manifest.prompts.get("plan").map(String::as_str),
        Some("Plan the work.")
    );
}

#[tokio::test]
async fn a_pack_child_whose_prompt_climbs_out_of_its_pack_refuses_its_parent_at_birth() {
    let owner = Owner::new();
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    pack_file(
        repo.path(),
        "pack.yaml",
        &pack_manifest("1.0.0", "leaf.yaml"),
    );
    pack_file(repo.path(), "leaf.yaml", CLIMBING_PROMPT);
    write(&repo.path().join("secret.txt"), "hunter2");
    let refused = build_manifest(
        &workflow(CALLS_PACK_CHILD),
        &ConfigLayer::default(),
        repo.path(),
        repo.path(),
        &HashMap::new(),
        owner.supervision(),
        None,
    )
    .await
    .expect_err("a child whose prompt leaves its pack refuses its parent at birth");
    let said = refused.to_string();
    assert!(matches!(refused, ManifestError::Frozen(_)), "{said}");
    assert!(
        said.contains("../../../../../secret.txt"),
        "the refusal names the path as written: {said}"
    );
    assert!(
        !said.contains("hunter2"),
        "the file's content never reaches the refusal: {said}"
    );
}

#[tokio::test]
async fn a_pack_child_prompt_that_climbs_within_its_pack_is_born_with_it() {
    let bench = Bench::with_run_id("run-pack-child-prompt");
    pack_file(
        &bench.worktree,
        "pack.yaml",
        &pack_manifest("1.0.0", "workflows/leaf.yaml"),
    );
    pack_file(&bench.worktree, "workflows/leaf.yaml", INSIDE_PROMPT_CHILD);
    pack_file(&bench.worktree, "prompts/plan.md", "Plan the work.");
    commit(&bench.worktree);
    let RunReport { terminal, .. } = bench.run(CALLS_PACK_CHILD, ONE_SESSION).await;
    assert_eq!(terminal, RunTerminal::Finished);
    let prompts = prompts_sent(&bench);
    assert!(
        prompts
            .iter()
            .any(|prompt| prompt.contains("Plan the work.")),
        "the child's session was sent the prompt inside its pack: {prompts:?}"
    );
}

#[tokio::test]
async fn a_child_fanning_out_over_runners_is_born_with_each_clones_prompt() {
    let bench = Bench::with_run_id("run-fanned-child").with_workflow("fanned", FANNED_CHILD);
    write(
        &bench.worktree.join(".yunta/workflows/prompts/review.md"),
        "Review carefully.",
    );
    commit(&bench.worktree);
    let config = "runners:\n  alpha:\n    - { adapter: mock, model: m }\n  beta:\n    - { adapter: mock, model: m }\n";
    let fixture = "dialect: claude-code\nsessions:\n  - outcome: { type: completed, summary: one }\n  - outcome: { type: completed, summary: two }\n";
    let parent = "name: parent\nnodes:\n  - { id: call, kind: workflow, use: fanned }\n";
    let RunReport { terminal, .. } = bench.run_with_config(parent, fixture, config).await;
    assert_eq!(terminal, RunTerminal::Finished);
    let prompts = prompts_sent(&bench);
    assert_eq!(prompts.len(), 2, "one session per clone: {prompts:?}");
    assert!(
        prompts
            .iter()
            .all(|prompt| prompt.contains("Review carefully.")),
        "each clone's session was sent the prompt: {prompts:?}"
    );
}
