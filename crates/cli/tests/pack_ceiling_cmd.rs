//! End to end: the executor-confirmation gate on `pack add`
//! and `declares.permissions` enforced as a ceiling by
//! `yunta check` — both against the real compiled binary.

use std::path::Path;

use yunta_testkit::{git, init_repo, stderr, stdout, yunta_in};

fn write_pack_with_executor(dir: &Path) {
    std::fs::create_dir_all(dir.join("workflows")).unwrap();
    std::fs::write(
        dir.join("pack.yaml"),
        "name: automation-pack\n\
         publisher: acme\n\
         version: 1.0.0\n\
         declares:\n  permissions: edit\n  network: false\n  executors: [runner.py]\n\
         contents:\n  workflows: [workflows/build.yaml]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("workflows/build.yaml"),
        "name: build\nnodes:\n  - id: package\n    kind: executor\n    executor: runner.py\n",
    )
    .unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "v1"]);
}

fn write_pack_with_ceiling_violation(dir: &Path) {
    std::fs::create_dir_all(dir.join("workflows")).unwrap();
    std::fs::write(
        dir.join("pack.yaml"),
        "name: review-pack\n\
         publisher: acme\n\
         version: 1.0.0\n\
         declares:\n  permissions: read-only\n  network: false\n  executors: []\n\
         contents:\n  workflows: [workflows/review.yaml]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("workflows/review.yaml"),
        "name: review\nnodes:\n  - id: draft\n    kind: prompt\n    prompt: hi\n    permissions: edit\n",
    )
    .unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "v1"]);
}

fn setup(
    write_upstream: impl FnOnce(&Path),
) -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let upstream = root.path().join("upstream");
    std::fs::create_dir_all(&upstream).unwrap();
    init_repo(&upstream);
    write_upstream(&upstream);

    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    let home = root.path().join("state");
    (root, upstream, home)
}

#[test]
fn add_refuses_a_pack_with_executors_without_yes() {
    let (root, upstream, home) = setup(write_pack_with_executor);
    let repo = root.path().join("repo");

    let out = yunta_in!(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("--yes"), "{}", stderr(&out));
    assert!(
        !repo.join(".yunta/packs/acme/automation-pack").exists(),
        "a refused add must not vendor anything"
    );
}

#[test]
fn add_with_yes_installs_a_pack_with_executors() {
    let (root, upstream, home) = setup(write_pack_with_executor);
    let repo = root.path().join("repo");

    let out = yunta_in!(
        &repo,
        &home,
        &["pack", "add", upstream.to_str().unwrap(), "--yes"]
    );
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stdout(&out).contains("installed acme/automation-pack"));
    assert!(repo.join(".yunta/packs/acme/automation-pack").exists());
}

#[test]
fn check_refuses_a_node_that_exceeds_the_packs_declared_ceiling() {
    let (root, upstream, home) = setup(write_pack_with_ceiling_violation);
    let repo = root.path().join("repo");

    let add_out = yunta_in!(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
    assert!(add_out.status.success(), "{}", stderr(&add_out));

    let check_out = yunta_in!(&repo, &home, &["check", "acme/review"]);
    assert!(!check_out.status.success());
    let text = stderr(&check_out);
    assert!(
        text.lines().any(|l| l
            == "  node `draft` requests permissions `edit` but pack `acme/review-pack` declares a ceiling of `read-only` — lower the node's permissions or raise the pack's declared ceiling"),
        "check names the node, the pack, and the ceiling the node exceeds: {text}"
    );
}
