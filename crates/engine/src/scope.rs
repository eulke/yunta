//! Scope post-check by `git diff`: what a task
//! actually touched, checked against what its `scope` globs declared it
//! could touch. Unlike the tasks document's overlap heuristic (which
//! compares two *patterns* to each other with no library that does
//! that), this checks real *paths* against real globs — exactly what
//! `globset` is for, so it's used here instead of a hand-rolled
//! approximation.

use std::path::{Path, PathBuf};

use crate::process::Supervision;
use yunta_core::fence::Coverage;
use yunta_core::{Location, RelativePath, ScopeGlob};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScopeCheckError {
    #[error("failed to {action}")]
    Io {
        action: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{}", crate::git::exited_with(.command, .status, .stderr))]
    GitFailed {
        command: String,
        status: i32,
        stderr: String,
    },
    /// Every pattern compiled on its own when it was parsed, so the
    /// only failure left is the set's own limit on how many it holds.
    #[error("this scope has more globs than one set can hold")]
    GlobSet {
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
    scope: &[ScopeGlob],
    staged: &[PathBuf],
    supervision: Supervision<'_>,
) -> Result<ScopeCheckResult, ScopeCheckError> {
    let set =
        yunta_core::scope_globset(scope).map_err(|source| ScopeCheckError::GlobSet { source })?;

    let mut diff = git_diff_names(cwd, supervision).await?;
    diff.extend(git_untracked(cwd, supervision).await?);
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

async fn git_diff_names(
    cwd: &Path,
    supervision: Supervision<'_>,
) -> Result<Vec<PathBuf>, ScopeCheckError> {
    let bytes = git_bytes(cwd, &["diff", "--name-only", "-z", "HEAD"], supervision).await?;
    Ok(nul_separated_paths(&bytes))
}

async fn git_untracked(
    cwd: &Path,
    supervision: Supervision<'_>,
) -> Result<Vec<PathBuf>, ScopeCheckError> {
    let bytes = git_bytes(
        cwd,
        &["ls-files", "--others", "--exclude-standard", "-z"],
        supervision,
    )
    .await?;
    Ok(nul_separated_paths(&bytes))
}

async fn git_bytes(
    cwd: &Path,
    args: &[&str],
    supervision: Supervision<'_>,
) -> Result<Vec<u8>, ScopeCheckError> {
    crate::git::output_bytes(cwd, args, supervision)
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
pub fn audited_scope(node: &yunta_core::Node) -> Option<&[ScopeGlob]> {
    if node.permissions == Some(yunta_core::NodePermissions::ReadOnly) {
        return Some(&[]);
    }
    (!node.scope.is_empty()).then_some(node.scope.as_slice())
}

/// A write that reached the diff despite an exact fence.
///
/// Only an exact fence makes this a finding: the adapter said it judged
/// every write before it happened and one got through anyway, which is
/// something to look at in the adapter. Under a widened or tool-only
/// coverage a violation is what it always was — the task fails with the
/// list, and nobody claimed more.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Breach {
    pub paths: Vec<PathBuf>,
}

/// Pure: what the coverage and the diff together mean.
pub fn fence_breach(coverage: Option<&Coverage>, result: &ScopeCheckResult) -> Option<Breach> {
    if coverage != Some(&Coverage::Exact) || result.violations.is_empty() {
        return None;
    }
    Some(Breach {
        paths: result.violations.clone(),
    })
}

impl Breach {
    /// The id every fence breach is filed under, in the `engine-*`
    /// scheme the engine's own findings use.
    pub const ID: &'static str = "engine-fence-breach";

    /// What the finding says, for one adapter.
    pub fn title() -> String {
        "the fence declared exact let a write through".to_string()
    }

    /// The first path, which is where a reader looks. Under the work:
    /// a breach is a write to the project that the fence said it had
    /// stopped.
    pub fn location(&self) -> Location {
        Location::work(RelativePath::of(self.paths.first()), None)
    }

    pub fn detail(&self, adapter: &yunta_core::AdapterId) -> String {
        format!(
            "fence exact on {adapter}; {} paths reached the diff outside it: {}",
            self.paths.len(),
            self.paths
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}
