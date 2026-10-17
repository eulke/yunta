//! Isolation on a real repository: the checkout a run works in and the
//! lock that says it is a run's.
//!
//! `worktree` gives each run its own checkout at the base commit;
//! `none` takes the repository itself and refuses a dirty tree. A lock
//! is taken from a dead owner and reported, never from a live one, and
//! never from an owner this process cannot verify — a reused pid is not
//! the owner, and an unreadable record refuses rather than guesses.

mod lock;
mod mutation;
mod unit;

use std::path::Path;
use std::sync::Arc;

use yunta_core::{CommitSha, Isolation};
use yunta_engine::{prepare_worktree, release_worktree, WorktreeError};
use yunta_testkit::{git_output, init_repo, Owner};

fn head(dir: &Path) -> CommitSha {
    git_output(dir, &["rev-parse", "HEAD"])
        .trim()
        .parse()
        .unwrap()
}

/// The pool of `root`'s checkouts for the run `holder` is, reading time
/// from a fixed clock.
fn pool_of(root: &Path, repo: &Path, holder: &Path) -> Arc<yunta_engine::CheckoutPool> {
    yunta_engine::CheckoutPool::new(root, repo, holder, Arc::new(yunta_testkit_core::FixedClock))
}

#[tokio::test]
async fn worktree_isolation_creates_a_real_git_worktree_at_base_commit() {
    let owner = Owner::new();
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
        owner.supervision(),
    )
    .await
    .unwrap();

    assert!(worktree_path.join(".gitkeep").exists());
    assert_eq!(head(&worktree_path), base_commit);
    // It's a real worktree of the same repo, not a detached clone.
    let common_dir_output = git_output(&worktree_path, &["rev-parse", "--git-common-dir"]);
    let common_dir = Path::new(common_dir_output.trim());
    let common_dir = if common_dir.is_absolute() {
        common_dir.to_path_buf()
    } else {
        worktree_path.join(common_dir)
    }
    .canonicalize()
    .unwrap();
    let repo_git = repo.join(".git").canonicalize().unwrap();
    assert!(common_dir.starts_with(repo_git));
}

#[tokio::test]
async fn two_worktree_isolated_runs_on_the_same_repo_never_collide() {
    let owner = Owner::new();
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
        owner.supervision(),
    )
    .await
    .unwrap();
    prepare_worktree(
        &repo,
        &wt2,
        &base_commit,
        "yunta/run-2",
        Isolation::Worktree,
        owner.supervision(),
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
    let owner = Owner::new();
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let base_commit = head(&repo);

    prepare_worktree(
        &repo,
        &repo,
        &base_commit,
        "unused",
        Isolation::None,
        owner.supervision(),
    )
    .await
    .unwrap();

    // A second run on the same repo must be refused while the first
    // holds the lock — this is the "no concurrent runs" guarantee
    // required for `none`.
    let err = prepare_worktree(
        &repo,
        &repo,
        &base_commit,
        "unused",
        Isolation::None,
        owner.supervision(),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, WorktreeError::Locked { .. }));
}

#[tokio::test]
async fn none_isolation_with_a_dirty_tree_is_refused_before_anything_runs() {
    let owner = Owner::new();
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let base_commit = head(&repo);
    std::fs::write(repo.join("uncommitted.txt"), "dirty").unwrap();

    let err = prepare_worktree(
        &repo,
        &repo,
        &base_commit,
        "unused",
        Isolation::None,
        owner.supervision(),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, WorktreeError::DirtyTree { .. }));
}

#[tokio::test]
async fn releasing_a_none_isolation_lock_lets_a_later_run_proceed() {
    let owner = Owner::new();
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let base_commit = head(&repo);

    prepare_worktree(
        &repo,
        &repo,
        &base_commit,
        "unused",
        Isolation::None,
        owner.supervision(),
    )
    .await
    .unwrap();
    release_worktree(&repo, Isolation::None, owner.supervision())
        .await
        .unwrap();

    // No longer locked — a fresh run may proceed.
    prepare_worktree(
        &repo,
        &repo,
        &base_commit,
        "unused",
        Isolation::None,
        owner.supervision(),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn releasing_a_worktree_isolated_run_leaves_the_worktree_on_disk() {
    let owner = Owner::new();
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
        owner.supervision(),
    )
    .await
    .unwrap();
    release_worktree(&repo, Isolation::Worktree, owner.supervision())
        .await
        .unwrap();

    // Worktrees are left in place for inspection — cleanup is a
    // separate, not-yet-built concern (on_finish).
    assert!(worktree_path.join(".gitkeep").exists());
}
