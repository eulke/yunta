use std::path::Path;

use yunta_core::Isolation;
use yunta_engine::{prepare_worktree, release_worktree, WorktreeError};

fn git(dir: &Path, args: &[&str]) -> std::process::Output {
    std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn git_ok(dir: &Path, args: &[&str]) {
    let output = git(dir, args);
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_repo(dir: &Path) {
    git_ok(dir, &["init", "-q"]);
    git_ok(dir, &["config", "user.email", "test@example.com"]);
    git_ok(dir, &["config", "user.name", "Test"]);
    std::fs::write(dir.join(".gitkeep"), "").unwrap();
    git_ok(dir, &["add", "."]);
    git_ok(dir, &["commit", "-q", "-m", "initial"]);
}

fn head(dir: &Path) -> String {
    let output = git(dir, &["rev-parse", "HEAD"]);
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[tokio::test]
async fn worktree_isolation_creates_a_real_git_worktree_at_base_commit() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let base_commit = head(&repo);
    let worktree_path = root.path().join("worktrees/run-1");

    prepare_worktree(
        &repo,
        &worktree_path,
        &base_commit,
        "yunta/run-1",
        Isolation::Worktree,
    )
    .await
    .unwrap();

    assert!(worktree_path.join(".gitkeep").exists());
    assert_eq!(head(&worktree_path), base_commit);
    // It's a real worktree of the same repo, not a detached clone.
    let common_dir = git(&worktree_path, &["rev-parse", "--git-common-dir"]);
    let common_dir = String::from_utf8_lossy(&common_dir.stdout);
    assert!(common_dir
        .trim()
        .starts_with(repo.join(".git").to_str().unwrap()));
}

#[tokio::test]
async fn two_worktree_isolated_runs_on_the_same_repo_never_collide() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let base_commit = head(&repo);

    let wt1 = root.path().join("worktrees/run-1");
    let wt2 = root.path().join("worktrees/run-2");
    prepare_worktree(
        &repo,
        &wt1,
        &base_commit,
        "yunta/run-1",
        Isolation::Worktree,
    )
    .await
    .unwrap();
    prepare_worktree(
        &repo,
        &wt2,
        &base_commit,
        "yunta/run-2",
        Isolation::Worktree,
    )
    .await
    .unwrap();

    std::fs::write(wt1.join("only-in-1.txt"), "one").unwrap();
    std::fs::write(wt2.join("only-in-2.txt"), "two").unwrap();

    assert!(!wt1.join("only-in-2.txt").exists());
    assert!(!wt2.join("only-in-1.txt").exists());
    assert!(!repo.join("only-in-1.txt").exists());
    assert!(!repo.join("only-in-2.txt").exists());
}

#[tokio::test]
async fn none_isolation_with_a_clean_tree_succeeds_and_locks_the_repo() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let base_commit = head(&repo);

    prepare_worktree(&repo, &repo, &base_commit, "unused", Isolation::None)
        .await
        .unwrap();

    // A second run on the same repo must be refused while the first
    // holds the lock — this is the "no concurrent runs" guarantee §7.3
    // requires for `none`.
    let err = prepare_worktree(&repo, &repo, &base_commit, "unused", Isolation::None)
        .await
        .unwrap_err();
    assert!(matches!(err, WorktreeError::Locked { .. }));
}

#[tokio::test]
async fn none_isolation_with_a_dirty_tree_is_refused_before_anything_runs() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let base_commit = head(&repo);
    std::fs::write(repo.join("uncommitted.txt"), "dirty").unwrap();

    let err = prepare_worktree(&repo, &repo, &base_commit, "unused", Isolation::None)
        .await
        .unwrap_err();
    assert!(matches!(err, WorktreeError::DirtyTree { .. }));
}

#[tokio::test]
async fn releasing_a_none_isolation_lock_lets_a_later_run_proceed() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let base_commit = head(&repo);

    prepare_worktree(&repo, &repo, &base_commit, "unused", Isolation::None)
        .await
        .unwrap();
    release_worktree(&repo, Isolation::None).await.unwrap();

    // No longer locked — a fresh run may proceed.
    prepare_worktree(&repo, &repo, &base_commit, "unused", Isolation::None)
        .await
        .unwrap();
}

#[tokio::test]
async fn releasing_a_worktree_isolated_run_leaves_the_worktree_on_disk() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let base_commit = head(&repo);
    let worktree_path = root.path().join("worktrees/run-1");

    prepare_worktree(
        &repo,
        &worktree_path,
        &base_commit,
        "yunta/run-1",
        Isolation::Worktree,
    )
    .await
    .unwrap();
    release_worktree(&repo, Isolation::Worktree).await.unwrap();

    // Worktrees are left in place for inspection — cleanup is a
    // separate, not-yet-built concern (on_finish, out of M-0).
    assert!(worktree_path.join(".gitkeep").exists());
}
