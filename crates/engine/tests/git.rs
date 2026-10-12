//! How a failed `git` invocation reads, on the engine's own [`GitError`]
//! and on the two modules that map it into an error of their own. Git
//! exits non-zero saying nothing often enough — a `diff` in a directory
//! that is not a repository, a `worktree remove` on a path already
//! gone — that what those messages do with an empty stderr is behavior,
//! not an edge case.

use std::path::PathBuf;

use tokio_util::sync::CancellationToken;
use yunta_core::describe;
use yunta_engine::git::{Cause, GitError, Invocation};
use yunta_engine::process::Supervision;
use yunta_engine::scope_expansion::ScopeExpansionError;
use yunta_engine::{RunError, ScopeCheckError, WorktreeError};

/// A git run in `/repo` with the given arguments.
fn invocation(args: &[&str]) -> Invocation {
    Invocation {
        dir: PathBuf::from("/repo"),
        args: args.iter().map(|arg| arg.to_string()).collect(),
    }
}

fn exited_non_zero(stderr: &str) -> GitError {
    GitError {
        invocation: invocation(&["status", "--porcelain"]),
        cause: Cause::Exit {
            code: Some(128),
            stderr: stderr.to_string(),
        },
    }
}

#[test]
fn a_git_failure_that_wrote_to_stderr_reads_as_what_ran_and_what_git_said() {
    assert_eq!(
        exited_non_zero("fatal: not a git repository").to_string(),
        "git status --porcelain in `/repo` failed: fatal: not a git repository"
    );
}

#[test]
fn a_git_failure_that_said_nothing_carries_no_colon_promising_a_reason() {
    assert_eq!(
        exited_non_zero("").to_string(),
        "git status --porcelain in `/repo` failed"
    );
    assert_eq!(
        exited_non_zero("\n").to_string(),
        "git status --porcelain in `/repo` failed"
    );
}

#[test]
fn a_scope_check_git_failure_that_said_nothing_names_the_command_alone() {
    let error = ScopeCheckError::Git(GitError {
        invocation: invocation(&["diff", "--name-only", "HEAD"]),
        cause: Cause::Exit {
            code: Some(128),
            stderr: String::new(),
        },
    });
    assert_eq!(
        error.to_string(),
        "git diff --name-only HEAD in `/repo` failed"
    );
}

#[test]
fn a_scope_expansion_git_failure_reads_exactly_as_a_scope_check_one() {
    let expansion = ScopeExpansionError::Git(GitError {
        invocation: invocation(&["diff", "--name-only", "HEAD"]),
        cause: Cause::Exit {
            code: Some(128),
            stderr: "fatal: bad revision".to_string(),
        },
    });
    let check = ScopeCheckError::Git(GitError {
        invocation: invocation(&["diff", "--name-only", "HEAD"]),
        cause: Cause::Exit {
            code: Some(128),
            stderr: "fatal: bad revision".to_string(),
        },
    });
    assert_eq!(expansion.to_string(), check.to_string());
    assert_eq!(
        expansion.to_string(),
        "git diff --name-only HEAD in `/repo` failed: fatal: bad revision"
    );
}

/// A git failure that crosses a module boundary keeps the `GitError` it
/// came from rather than a copy of its words: the sentence is composed
/// once, by `GitError` itself, and a module that carries it adds nothing
/// and loses nothing.
#[test]
fn a_git_failure_keeps_its_cause() {
    let sentence = exited_non_zero("fatal: not a git repository").to_string();

    let run: RunError = exited_non_zero("fatal: not a git repository").into();
    assert_eq!(describe(&run), sentence);

    let worktree: WorktreeError = exited_non_zero("fatal: not a git repository").into();
    assert_eq!(describe(&worktree), sentence);
}

/// The spawn that never ran keeps the `io::Error` that stopped it as its
/// cause: the sentence is written once, from the error itself, rather than
/// retyped by whoever reports it.
#[test]
fn a_git_that_never_ran_keeps_the_io_error_under_it() {
    let run: RunError = GitError {
        invocation: invocation(&["status", "--porcelain"]),
        cause: Cause::Spawn(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no such file or directory",
        )),
    }
    .into();
    assert_eq!(
        describe(&run),
        "git status --porcelain in `/repo` failed: no such file or directory"
    );
}

/// A repository with one commit, under a directory the test owns.
fn a_repo() -> (tempfile::TempDir, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    yunta_testkit::init_repo(&repo);
    (root, repo)
}

#[tokio::test]
async fn a_linked_worktree_names_the_repository_its_main_checkout_names() {
    let owner = yunta_testkit::Owner::new();
    let (root, repo) = a_repo();
    let linked = root.path().join("linked");
    yunta_testkit::git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            linked.to_str().unwrap(),
            "-b",
            "side",
        ],
    );

    let main = yunta_engine::git::common_dir(&repo, owner.supervision())
        .await
        .unwrap();
    let from_linked = yunta_engine::git::common_dir(&linked, owner.supervision())
        .await
        .unwrap();
    assert_eq!(main, from_linked);
    assert!(main.is_absolute(), "{}", main.display());
}

#[tokio::test]
async fn run_branches_lists_every_run_branch_in_one_call() {
    let owner = yunta_testkit::Owner::new();
    let (_root, repo) = a_repo();
    let first = yunta_core::RunId::from("01JQ0000000000000000000001");
    let second = yunta_core::RunId::from("01JQ0000000000000000000002");
    for branch in [
        yunta_engine::run_branch(&first),
        yunta_engine::run_branch(&second),
        "yunta/unit/01JQ0000000000000000000001/task/t1/1".to_string(),
        "feature/unrelated".to_string(),
    ] {
        yunta_testkit::git(&repo, &["branch", &branch]);
    }

    let runs = yunta_engine::git::run_branches(&repo, owner.supervision())
        .await
        .unwrap();
    assert_eq!(runs.into_iter().collect::<Vec<_>>(), vec![first, second]);
}

#[tokio::test]
async fn a_git_that_cannot_start_keeps_its_spawn_error_as_the_cause() {
    let empty = tempfile::tempdir().unwrap();
    let env = [("PATH".to_string(), empty.path().display().to_string())];
    let cancel = CancellationToken::new();
    let supervision = Supervision {
        env: &env,
        ..Supervision::outside_any_run(&cancel, &yunta_testkit_core::FixedClock)
    };
    let err = yunta_engine::git::read(empty.path(), &["status"], supervision)
        .await
        .unwrap_err();
    assert!(
        matches!(err.cause, Cause::Spawn(ref e) if e.kind() == std::io::ErrorKind::NotFound),
        "got {err:?}"
    );
}

#[tokio::test]
async fn a_cancelled_scope_check_is_not_a_git_failure() {
    let repo = tempfile::tempdir().unwrap();
    yunta_testkit::init_repo(repo.path());
    let cancel = CancellationToken::new();
    cancel.cancel();
    let err = yunta_engine::changed_between(
        repo.path(),
        "HEAD",
        "HEAD",
        Supervision::outside_any_run(&cancel, &yunta_testkit_core::FixedClock),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, ScopeCheckError::Git(ref e) if e.cancelled()),
        "got {err:?}"
    );
}
