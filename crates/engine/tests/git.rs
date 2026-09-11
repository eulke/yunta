//! How a failed `git` invocation reads, on the engine's own [`GitError`]
//! and on the two modules that map it into an error of their own. Git
//! exits non-zero saying nothing often enough — a `diff` in a directory
//! that is not a repository, a `worktree remove` on a path already
//! gone — that what those messages do with an empty stderr is behavior,
//! not an edge case.

use std::path::PathBuf;

use yunta_engine::scope_expansion::ScopeExpansionError;
use yunta_engine::{GitError, ScopeCheckError};

fn exited_non_zero(stderr: &str) -> GitError {
    GitError {
        args: "status --porcelain".to_string(),
        cwd: PathBuf::from("/repo"),
        stderr: stderr.to_string(),
        code: Some(128),
        source: None,
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
fn a_scope_check_git_failure_that_said_nothing_names_the_command_and_status_alone() {
    let error = ScopeCheckError::GitFailed {
        command: "diff --name-only HEAD".to_string(),
        status: 128,
        stderr: String::new(),
    };
    assert_eq!(
        error.to_string(),
        "`git diff --name-only HEAD` exited with status 128"
    );
}

#[test]
fn a_scope_expansion_git_failure_reads_exactly_as_a_scope_check_one() {
    let command = "diff --name-only HEAD".to_string();
    let expansion = ScopeExpansionError::GitFailed {
        command: command.clone(),
        status: 128,
        stderr: "fatal: bad revision".to_string(),
    };
    let check = ScopeCheckError::GitFailed {
        command,
        status: 128,
        stderr: "fatal: bad revision".to_string(),
    };
    assert_eq!(expansion.to_string(), check.to_string());
    assert_eq!(
        expansion.to_string(),
        "`git diff --name-only HEAD` exited with status 128: fatal: bad revision"
    );
}
