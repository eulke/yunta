//! Committing the run's own tree: exactly the tree captured, on top of
//! `HEAD`, with nothing run on the way.

use yunta_core::protected::{Pathspecs, Protected};
use yunta_engine::{capture_tree, commit_tree, head_commit};
use yunta_testkit::{git, git_output, init_repo, Owner};

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

    assert_eq!(made.commit, None);
    assert_eq!(
        head_commit(&repo, owner.supervision()).await.unwrap(),
        before
    );
    let head_tree = git_output(&repo, &["rev-parse", "HEAD^{tree}"]);
    assert_eq!(
        made.tree.as_str(),
        head_tree.trim(),
        "the tree is answered whole"
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
    let captured = capture_tree(
        &repo,
        &root.path().join("probe"),
        &Pathspecs::default(),
        owner.supervision(),
    )
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
    let (commit, tree) = (made.commit.expect("a changed tree is committed"), made.tree);

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
        made.commit.is_some(),
        "a hook that refuses every commit refused nothing here"
    );
}

/// Commits `.gitignore` with `ignored` as its lines, so the files the test
/// writes after it are ones git ignores.
fn ignoring(repo: &std::path::Path, ignored: &str) {
    std::fs::write(repo.join(".gitignore"), ignored).unwrap();
    git(repo, &["add", ".gitignore"]);
    git(repo, &["commit", "-q", "-m", "ignore"]);
}

/// The names a tree holds, one to a line.
fn names_in(repo: &std::path::Path, tree: &str) -> Vec<String> {
    git_output(repo, &["ls-tree", "-r", "--name-only", tree])
        .lines()
        .map(str::to_string)
        .collect()
}

/// An ignored path under a protected glob belongs to the tree a close
/// judges, never to a commit: a commit through the same index holds only
/// what git sees.
#[tokio::test]
async fn a_commit_never_holds_an_ignored_path_a_capture_staged() {
    let owner = Owner::new();
    let (root, repo) = a_repo().await;
    ignoring(&repo, "settings.local.json\n");
    tokio::fs::create_dir_all(repo.join(".claude"))
        .await
        .unwrap();
    tokio::fs::write(repo.join(".claude/settings.local.json"), "{}\n")
        .await
        .unwrap();
    tokio::fs::write(repo.join("made.txt"), "made\n")
        .await
        .unwrap();
    let index = root.path().join("index");
    let covered = Protected::new(&[], &[]).unwrap().pathspecs();

    let judged = capture_tree(&repo, &index, &covered, owner.supervision())
        .await
        .unwrap();
    let made = commit_tree(&repo, &index, "made it", owner.supervision())
        .await
        .unwrap();

    let local = ".claude/settings.local.json".to_string();
    assert!(names_in(&repo, judged.as_str()).contains(&local));
    let committed = names_in(&repo, made.tree.as_str());
    assert!(committed.contains(&"made.txt".to_string()), "{committed:?}");
    assert!(!committed.contains(&local), "{committed:?}");
}

/// A glob that names no directory never sends git into a directory the
/// project ignores whole: an ignored build output stays out of the tree,
/// and an ignored file git reaches without entering one is in it.
#[tokio::test]
async fn a_capture_never_enters_an_ignored_directory_for_a_glob_that_names_none() {
    let owner = Owner::new();
    let (root, repo) = a_repo().await;
    ignoring(&repo, "build/\nCLAUDE.md\n");
    tokio::fs::create_dir_all(repo.join("build/out"))
        .await
        .unwrap();
    tokio::fs::write(repo.join("build/out/AGENTS.md"), "a dependency's own\n")
        .await
        .unwrap();
    tokio::fs::write(repo.join("CLAUDE.md"), "local notes\n")
        .await
        .unwrap();
    let covered = Protected::new(&[], &[]).unwrap().pathspecs();

    let tree = capture_tree(
        &repo,
        &root.path().join("index"),
        &covered,
        owner.supervision(),
    )
    .await
    .unwrap();

    let held = names_in(&repo, tree.as_str());
    assert!(held.contains(&"CLAUDE.md".to_string()), "{held:?}");
    assert!(
        !held.iter().any(|name| name.starts_with("build/")),
        "{held:?}"
    );
}

#[tokio::test]
async fn the_request_file_is_not_part_of_the_captured_tree() {
    let owner = Owner::new();
    let (root, repo) = a_repo().await;
    let probe = root.path().join("probe");
    let spec = Protected::default().pathspecs();
    let without = capture_tree(&repo, &probe, &spec, owner.supervision())
        .await
        .unwrap();
    tokio::fs::write(
        repo.join(yunta_engine::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE),
        "paths: [b.txt]\nreason: r\n",
    )
    .await
    .unwrap();
    assert_eq!(
        capture_tree(&repo, &probe, &spec, owner.supervision())
            .await
            .unwrap(),
        without
    );
}
