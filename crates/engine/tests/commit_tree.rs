//! Committing the run's own tree: exactly the tree captured, on top of
//! `HEAD`, with nothing run on the way.

use yunta_engine::{capture_tree, commit_tree, head_commit};
use yunta_testkit::{git_output, init_repo, Owner};

async fn a_repo() -> (tempfile::TempDir, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    tokio::fs::create_dir_all(&repo).await.unwrap();
    init_repo(&repo);
    (root, repo)
}

#[tokio::test]
async fn committing_a_clean_tree_makes_no_commit() {
    let owner = Owner::new();
    let (root, repo) = a_repo().await;
    let before = head_commit(&repo, owner.supervision()).await.unwrap();

    let made = commit_tree(
        &repo,
        &root.path().join("index"),
        "nothing",
        owner.supervision(),
    )
    .await
    .unwrap();

    assert_eq!(made, None);
    assert_eq!(
        head_commit(&repo, owner.supervision()).await.unwrap(),
        before
    );
}

/// The commit holds the tree the capture names, its parent is the `HEAD`
/// it found, and the checkout reads clean after — its files untouched.
#[tokio::test]
async fn a_committed_tree_leaves_its_checkout_clean_and_its_tree_id_unchanged() {
    let owner = Owner::new();
    let (root, repo) = a_repo().await;
    let before = head_commit(&repo, owner.supervision()).await.unwrap();
    tokio::fs::write(repo.join("made.txt"), "made\n")
        .await
        .unwrap();
    let captured = capture_tree(&repo, &root.path().join("probe"), owner.supervision())
        .await
        .unwrap();

    let (commit, tree) = commit_tree(
        &repo,
        &root.path().join("index"),
        "made it",
        owner.supervision(),
    )
    .await
    .unwrap()
    .expect("a changed tree is committed");

    assert_eq!(tree, captured);
    assert_eq!(
        git_output(&repo, &["rev-parse", "HEAD"]).trim(),
        commit.as_str()
    );
    assert_eq!(
        git_output(&repo, &["rev-parse", "HEAD^"]).trim(),
        before.as_str()
    );
    assert_eq!(git_output(&repo, &["status", "--porcelain"]), "");
    assert_eq!(
        git_output(&repo, &["log", "-1", "--format=%s"]).trim(),
        "made it"
    );
}

#[tokio::test]
async fn a_commit_of_the_tree_runs_no_hook() {
    let owner = Owner::new();
    let (root, repo) = a_repo().await;
    let hook = repo.join(".git/hooks/pre-commit");
    tokio::fs::write(&hook, "#!/bin/sh\nexit 1\n")
        .await
        .unwrap();
    std::process::Command::new("chmod")
        .args(["+x", hook.to_str().unwrap()])
        .status()
        .unwrap();
    tokio::fs::write(repo.join("made.txt"), "made\n")
        .await
        .unwrap();

    let made = commit_tree(
        &repo,
        &root.path().join("index"),
        "made it",
        owner.supervision(),
    )
    .await
    .unwrap();

    assert!(
        made.is_some(),
        "a hook that refuses every commit refused nothing here"
    );
}
