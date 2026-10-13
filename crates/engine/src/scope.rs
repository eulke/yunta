//! Scope post-check by tree comparison: what a unit of work actually
//! touched between the tree it started from and the tree it left,
//! checked against what its `scope` globs declared it could touch.
//! Unlike the tasks document's overlap heuristic (which
//! compares two *patterns* to each other with no library that does
//! that), this checks real *paths* against real globs — exactly what
//! `globset` is for, so it's used here instead of a hand-rolled
//! approximation.

use std::path::{Path, PathBuf};

use crate::git::GitError;
use crate::process::Supervision;
use yunta_core::fence::Coverage;
use yunta_core::protected::{Closing, Pathspecs, Protected, Protection, Trespass};
use yunta_core::{Location, NodeScope, RelativePath, ScopeGlob, TreeId};

use crate::replay::RunState;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScopeCheckError {
    /// git could not say what changed: it failed, or the token stopped it.
    #[error(transparent)]
    Git(#[from] GitError),
    /// Every pattern compiled on its own when it was parsed, so the
    /// only failure left is the set's own limit on how many it holds.
    #[error("this scope has more globs than one set can hold")]
    GlobSet {
        #[source]
        source: globset::Error,
    },
    /// The checkout would not say what it holds. Asking a tree where it
    /// stands belongs to the worktree module; an audit only reports what
    /// came back.
    #[error(transparent)]
    Worktree(#[from] crate::worktree::WorktreeError),
}

impl ScopeCheckError {
    /// Whether the token stopped the git or the checkout this audit asked.
    pub fn cancelled(&self) -> bool {
        match self {
            ScopeCheckError::Git(git) => git.cancelled(),
            ScopeCheckError::Worktree(worktree) => worktree.cancelled(),
            ScopeCheckError::GlobSet { .. } => false,
        }
    }
}

/// What a unit of work changed between the tree it started from and the
/// tree it left, and which of those paths it may not have changed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScopeCheckResult {
    pub diff: Vec<PathBuf>,
    /// Every path of the diff the work may not have changed: outside its
    /// scope, or protected from every session of the run.
    pub violations: Vec<PathBuf>,
    /// The violations of protected paths: what no session of the run may
    /// write, whatever its scope says. A person's grant lifts only the
    /// instruction files, and the caller's set already carries that lift.
    pub denied: Vec<PathBuf>,
    /// Why each of `denied` is protected, in the same order.
    pub denied_by: Vec<Protection>,
}

impl ScopeCheckResult {
    /// The refused paths with the reason each is protected, in order.
    pub fn trespasses(&self) -> Vec<Trespass> {
        self.denied
            .iter()
            .zip(&self.denied_by)
            .map(|(path, reason)| Trespass {
                path: path.clone(),
                reason: *reason,
            })
            .collect()
    }
}

/// What a unit of work is held to: the scope it may change, and what no
/// session of the run may write whatever that scope allows, judged as
/// `closing` says.
#[derive(Debug, Clone, Copy)]
pub struct Ceiling<'a> {
    pub scope: &'a [ScopeGlob],
    pub protected: &'a Protected,
    pub closing: Closing,
}

/// What changed in `cwd` since `from`: the paths a unit of work is
/// answerable for, and nothing that was already there when it began.
///
/// Both ends are trees, so an untracked file needs no case of its own: a
/// file the unit created is in the second tree and not the first, and a
/// file that was already lying there untracked is in both. Asking git
/// for untracked paths separately would answer about the checkout rather
/// than about the unit, which is the whole distinction this makes.
///
/// Both ends come from [`capture_tree`](crate::capture_tree), which is
/// the one place a checkout is asked what it holds.
pub async fn changed_since(
    cwd: &Path,
    from: &TreeId,
    index: &Path,
    covered: &Pathspecs,
    supervision: Supervision<'_>,
) -> Result<Vec<PathBuf>, ScopeCheckError> {
    let now = crate::worktree::capture_tree(cwd, index, covered, supervision).await?;
    changed_between(cwd, from.as_str(), now.as_str(), supervision).await
}

/// What a diff and a declared scope mean together — a function of its
/// arguments and nothing else, so the verdict is the same wherever it is
/// reached.
///
/// `staged` is what the adapter that ran declared it wrote for its own
/// mechanics (`Adapter::staged_paths`): a change at or under one of
/// those paths is never a violation, and nothing else is left out.
pub fn violations(
    diff: &[PathBuf],
    scope: &[ScopeGlob],
    staged: &[PathBuf],
) -> Result<Vec<PathBuf>, ScopeCheckError> {
    let set =
        yunta_core::scope_globset(scope).map_err(|source| ScopeCheckError::GlobSet { source })?;
    Ok(diff
        .iter()
        .filter(|path| !staged.iter().any(|mount| path.starts_with(mount)))
        .filter(|path| !set.is_match(path))
        .cloned()
        .collect())
}

/// The diff and its verdict together, for a caller that wants both.
pub async fn audit(
    cwd: &Path,
    from: &TreeId,
    index: &Path,
    ceiling: Ceiling<'_>,
    staged: &[PathBuf],
    supervision: Supervision<'_>,
) -> Result<ScopeCheckResult, ScopeCheckError> {
    let pathspecs = ceiling.protected.pathspecs();
    let now = crate::worktree::capture_tree(cwd, index, &pathspecs, supervision).await?;
    audit_tree(cwd, from, &now, ceiling, staged, supervision).await
}

/// [`audit`] of the tree `now`, which the caller already captured from
/// `cwd` — a check that read the checkout a moment ago does not read it
/// again.
pub async fn audit_tree(
    cwd: &Path,
    from: &TreeId,
    now: &TreeId,
    ceiling: Ceiling<'_>,
    staged: &[PathBuf],
    supervision: Supervision<'_>,
) -> Result<ScopeCheckResult, ScopeCheckError> {
    let diff = changed_between(cwd, from.as_str(), now.as_str(), supervision).await?;
    let trespasses = ceiling.protected.trespasses(&diff, staged, ceiling.closing);
    let mut violations = violations(&diff, ceiling.scope, staged)?;
    let mut denied = Vec::with_capacity(trespasses.len());
    let mut denied_by = Vec::with_capacity(trespasses.len());
    for trespass in trespasses {
        if !violations.contains(&trespass.path) {
            violations.push(trespass.path.clone());
        }
        denied.push(trespass.path);
        denied_by.push(trespass.reason);
    }
    Ok(ScopeCheckResult {
        diff,
        violations,
        denied,
        denied_by,
    })
}

/// The protection half of [`audit_tree`], for a unit whose scope is not
/// audited: what the diff from `from` to the checkout reaches that
/// `protected` refuses, as `closing` judges it. Each such path is a violation,
/// as an audit reports it, and the diff is the one the audit would have read.
pub async fn protection_audit(
    cwd: &Path,
    from: &str,
    index: &Path,
    protected: &Protected,
    closing: Closing,
    staged: &[PathBuf],
    supervision: Supervision<'_>,
) -> Result<ScopeCheckResult, ScopeCheckError> {
    let pathspecs = protected.pathspecs();
    let now = crate::worktree::capture_tree(cwd, index, &pathspecs, supervision).await?;
    let diff = changed_between(cwd, from, now.as_str(), supervision).await?;
    let (denied, denied_by): (Vec<PathBuf>, Vec<Protection>) = protected
        .trespasses(&diff, staged, closing)
        .into_iter()
        .map(|trespass| (trespass.path, trespass.reason))
        .unzip();
    Ok(ScopeCheckResult {
        diff,
        violations: denied.clone(),
        denied,
        denied_by,
    })
}

/// The paths from a `-z` git listing: NUL-separated, and — because `-z`
/// turns off git's own path quoting — byte-for-byte, so a non-ASCII path
/// reaches the globs unescaped instead of as a `"caf\303\251.rs"` string
/// no glob would match.
pub(crate) fn nul_separated_paths(bytes: &[u8]) -> Vec<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    bytes
        .split(|byte| *byte == 0)
        .filter(|segment| !segment.is_empty())
        .map(|segment| PathBuf::from(std::ffi::OsStr::from_bytes(segment)))
        .collect()
}

/// Whether a node's own worktree diff is audited: it declares a scope,
/// or it is `read-only`.
///
/// A node that declares `scope:` is audited against it. A node declared
/// `read-only` is audited against nothing at all — the word says the
/// worktree comes back untouched, and the static check already takes it
/// at that word, exempting such a node from every scope-overlap rule so
/// it may run beside any other. Auditing it against the empty scope is
/// what makes that exemption true rather than assumed: whatever the
/// session's tools happened to permit, a read-only node that wrote the
/// project fails for it.
pub fn audits(node: &yunta_core::Node) -> bool {
    node.permissions == Some(yunta_core::NodePermissions::ReadOnly) || node.scope.is_declared()
}

/// The scope in force for a node's work on this run: the globs it declares —
/// or, scoped to the run, what the run had changed when its attempt
/// started, as `node_started` recorded it — plus every path a person
/// granted the node on the run's log after it failed on its scope.
/// `None` exactly when no audit is owed.
///
/// A read-only node is held to its word alone: nothing ever offers it a
/// grant, and one on the log would still widen nothing.
pub fn node_scope_in_force(node: &yunta_core::Node, state: &RunState) -> Option<Vec<ScopeGlob>> {
    if node.permissions == Some(yunta_core::NodePermissions::ReadOnly) {
        return Some(Vec::new());
    }
    let declared: &[ScopeGlob] = match &node.scope {
        NodeScope::Unscoped => return None,
        NodeScope::Globs(globs) => globs,
        NodeScope::Run => state.nodes.run_scope(&node.id).unwrap_or_default(),
    };
    Some(
        declared
            .iter()
            .chain(state.grants.paths_for_node(&node.id))
            .cloned()
            .collect(),
    )
}

/// The paths that differ between `from` and `to` — each a commit or a
/// tree, by git's name for it: what a run changed between its base and
/// the tree an attempt starts from, or what a unit changed since it began.
pub async fn changed_between(
    cwd: &Path,
    from: &str,
    to: &str,
    supervision: Supervision<'_>,
) -> Result<Vec<PathBuf>, ScopeCheckError> {
    let bytes =
        crate::git::read_bytes(cwd, &["diff", "--name-only", "-z", from, to], supervision).await?;
    Ok(nul_separated_paths(&bytes))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A checkout of one commit, and that commit, with a protected file and an
    /// ordinary one written on top of it.
    fn checkout_changed_since_its_commit() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        yunta_testkit::init_repo(dir.path());
        let base = yunta_testkit::git_output(dir.path(), &["rev-parse", "HEAD"]);
        std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
        std::fs::write(dir.path().join(".claude/settings.json"), "{}").unwrap();
        std::fs::write(dir.path().join("src.rs"), "fn main() {}").unwrap();
        (dir, base)
    }

    #[tokio::test]
    async fn a_protection_audit_judges_the_diff_from_the_base_it_is_given() {
        let (dir, base) = checkout_changed_since_its_commit();
        let protected = Protected::new(&[yunta_core::ScopeGlob::from(".claude/**")], &[])
            .expect("a protected set that builds");
        let cancel = tokio_util::sync::CancellationToken::new();
        let supervision =
            crate::process::Supervision::outside_any_run(&cancel, &yunta_testkit_core::FixedClock);

        let result = protection_audit(
            dir.path(),
            base.trim(),
            &dir.path().join("audit.check"),
            &protected,
            Closing::Every,
            &[],
            supervision,
        )
        .await
        .expect("the protection audit reads the checkout");

        assert_eq!(result.denied, vec![PathBuf::from(".claude/settings.json")]);
        assert_eq!(result.denied_by, vec![Protection::ProjectDeny]);
        assert_eq!(result.violations, result.denied);
        assert!(
            result.diff.contains(&PathBuf::from("src.rs")),
            "the diff holds every change from the base: {:?}",
            result.diff
        );
    }
}
