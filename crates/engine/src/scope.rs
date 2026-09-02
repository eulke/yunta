//! Scope post-check by `git diff`: what a task
//! actually touched, checked against what its `scope` globs declared it
//! could touch. Unlike the ledger's overlap heuristic (which
//! compares two *patterns* to each other with no library that does
//! that), this checks real *paths* against real globs — exactly what
//! `globset` is for, so it's used here instead of a hand-rolled
//! approximation.

use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScopeCheckError {
    #[error("failed to {action}")]
    Io {
        action: String,
        #[source]
        source: std::io::Error,
    },
    #[error("`git {command}` exited with status {status}: {stderr}")]
    GitFailed {
        command: String,
        status: i32,
        stderr: String,
    },
    #[error("invalid scope glob `{glob}`")]
    InvalidGlob {
        glob: String,
        #[source]
        source: globset::Error,
    },
}

/// What changed in `cwd` since `HEAD` (tracked edits/deletes plus new
/// untracked files) and which of those paths fall outside every declared
/// glob.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScopeCheckResult {
    pub diff: Vec<PathBuf>,
    pub violations: Vec<PathBuf>,
}

/// `staged` is what the adapter that ran declared it wrote for its own
/// mechanics (`Adapter::staged_paths`): a change at or under one of
/// those paths is never a violation, and nothing else is left out.
pub async fn scope_check(
    cwd: &Path,
    scope: &[String],
    staged: &[PathBuf],
) -> Result<ScopeCheckResult, ScopeCheckError> {
    let set = yunta_core::scope_globset(scope)
        .map_err(|(glob, source)| ScopeCheckError::InvalidGlob { glob, source })?;

    let mut diff = git_diff_names(cwd).await?;
    diff.extend(git_untracked(cwd).await?);
    diff.sort();
    diff.dedup();

    let violations = diff
        .iter()
        .filter(|path| !staged.iter().any(|mount| path.starts_with(mount)))
        .filter(|path| !set.is_match(path))
        .cloned()
        .collect();

    Ok(ScopeCheckResult { diff, violations })
}

async fn run_git(cwd: &Path, args: &[&str]) -> Result<String, ScopeCheckError> {
    let output = tokio::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .await
        .map_err(|source| ScopeCheckError::Io {
            action: format!("run `git {}`", args.join(" ")),
            source,
        })?;

    if !output.status.success() {
        return Err(ScopeCheckError::GitFailed {
            command: args.join(" "),
            status: output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn git_diff_names(cwd: &Path) -> Result<Vec<PathBuf>, ScopeCheckError> {
    let stdout = run_git(cwd, &["diff", "--name-only", "HEAD"]).await?;
    Ok(stdout.lines().map(PathBuf::from).collect())
}

async fn git_untracked(cwd: &Path) -> Result<Vec<PathBuf>, ScopeCheckError> {
    let stdout = run_git(cwd, &["ls-files", "--others", "--exclude-standard"]).await?;
    Ok(stdout.lines().map(PathBuf::from).collect())
}
