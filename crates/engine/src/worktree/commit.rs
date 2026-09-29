//! Committing the run's own tree: what a node left in it, or what a node
//! found in it when it started, as a commit on the run's branch.

use std::path::Path;

use yunta_core::{CommitSha, InvalidId, TreeId};

use super::{capture_tree, head_commit, head_tree, run_git, WorktreeError};
use crate::process::Supervision;

/// Commits everything `repo` holds — its `HEAD` plus whatever lies in it
/// uncommitted, less what git ignores — on top of `HEAD`, moving its
/// branch, and answers with the commit and the tree it holds. `None`
/// when the tree is `HEAD`'s already: nothing changed, nothing to commit.
///
/// No hook runs. The tree committed is exactly the one captured, the
/// same tree id the log names, and a hook that reformats a file or fails
/// cannot make the branch disagree with it. The branch moves by
/// compare-and-swap from the `HEAD` this read, and the checkout's own
/// index follows so `git status` reads clean; its files are not touched.
pub async fn commit_tree(
    repo: &Path,
    index: &Path,
    message: &str,
    supervision: Supervision<'_>,
) -> Result<Option<(CommitSha, TreeId)>, WorktreeError> {
    let tree = capture_tree(repo, index, supervision).await?;
    if tree == head_tree(repo, supervision).await? {
        return Ok(None);
    }
    let head = head_commit(repo, supervision).await?;
    let printed = run_git(
        repo,
        &[
            "commit-tree",
            tree.as_str(),
            "-p",
            head.as_str(),
            "-m",
            message,
        ],
        supervision,
    )
    .await?;
    let commit: CommitSha =
        printed
            .trim()
            .parse()
            .map_err(|source: InvalidId| WorktreeError::NotACommit {
                args: "commit-tree".to_string(),
                cwd: repo.to_path_buf(),
                source,
            })?;
    run_git(
        repo,
        &[
            "update-ref",
            "-m",
            message,
            "HEAD",
            commit.as_str(),
            head.as_str(),
        ],
        supervision,
    )
    .await?;
    run_git(repo, &["reset", "-q"], supervision).await?;
    Ok(Some((commit, tree)))
}
