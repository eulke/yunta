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

async fn git_diff_names(cwd: &Path) -> Result<Vec<PathBuf>, ScopeCheckError> {
    let bytes = git_bytes(cwd, &["diff", "--name-only", "-z", "HEAD"]).await?;
    Ok(nul_separated_paths(&bytes))
}

async fn git_untracked(cwd: &Path) -> Result<Vec<PathBuf>, ScopeCheckError> {
    let bytes = git_bytes(cwd, &["ls-files", "--others", "--exclude-standard", "-z"]).await?;
    Ok(nul_separated_paths(&bytes))
}

async fn git_bytes(cwd: &Path, args: &[&str]) -> Result<Vec<u8>, ScopeCheckError> {
    crate::git::output_bytes(cwd, args)
        .await
        .map_err(|e| match e.source {
            Some(source) => ScopeCheckError::Io {
                action: format!("run `git {}`", e.args),
                source,
            },
            None => ScopeCheckError::GitFailed {
                command: e.args,
                status: e.code.unwrap_or(-1),
                stderr: e.stderr,
            },
        })
}

/// The paths from a `-z` git listing: NUL-separated, and — because `-z`
/// turns off git's own path quoting — byte-for-byte, so a non-ASCII path
/// reaches the globs unescaped instead of as a `"caf\303\251.rs"` string
/// no glob would match.
fn nul_separated_paths(bytes: &[u8]) -> Vec<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    bytes
        .split(|byte| *byte == 0)
        .filter(|segment| !segment.is_empty())
        .map(|segment| PathBuf::from(std::ffi::OsStr::from_bytes(segment)))
        .collect()
}

/// The scope a node's own worktree diff is audited against, or `None`
/// when the node constrains nothing and no audit is owed.
///
/// A node that declares `scope:` is audited against it. A node declared
/// `read-only` is audited against nothing at all — the word says the
/// worktree comes back untouched, and the static check already takes it
/// at that word, exempting such a node from every scope-overlap rule so
/// it may run beside any other. Auditing it against the empty scope is
/// what makes that exemption true rather than assumed: whatever the
/// session's tools happened to permit, a read-only node that wrote the
/// project fails for it.
pub fn audited_scope(node: &yunta_core::Node) -> Option<&[String]> {
    if node.permissions == Some(yunta_core::NodePermissions::ReadOnly) {
        return Some(&[]);
    }
    (!node.scope.is_empty()).then_some(node.scope.as_slice())
}
