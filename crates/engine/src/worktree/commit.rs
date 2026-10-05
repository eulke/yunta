//! Committing the run's own tree: what a node left in it, or what a node
//! found in it when it started, as a commit on the run's branch.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use yunta_core::{CommitSha, InvalidId, TreeId};

use super::{capture_tree, head_commit, head_tree, run_git, WorktreeError};
use crate::process::Supervision;

/// What a checkout held when it was committed: the tree, read once, and
/// the commit made of it — `None` when the tree was `HEAD`'s already.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Captured {
    pub tree: TreeId,
    pub commit: Option<CommitSha>,
}

/// Commits everything `repo` holds — its `HEAD` plus whatever lies in it
/// uncommitted, less what git ignores — on top of `HEAD`, moving its
/// branch, and answers with the tree it holds and the commit made of it.
/// A tree that is `HEAD`'s already makes no commit: nothing changed. The
/// tree is answered either way, so nobody reads it again.
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
) -> Result<Captured, WorktreeError> {
    let tree = capture_tree(repo, index, supervision).await?;
    if tree == head_tree(repo, supervision).await? {
        return Ok(Captured { tree, commit: None });
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
    Ok(Captured {
        tree,
        commit: Some(commit),
    })
}

/// Puts `paths` in `repo` back as its `HEAD` holds them: a path `HEAD`
/// has is checked out from it, and one it does not have is removed.
/// What a run refuses to commit goes back to where the branch had it,
/// so nothing later commits it by accident.
pub async fn restore(
    repo: &Path,
    paths: &[PathBuf],
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    if paths.is_empty() {
        return Ok(());
    }
    let mut listing: Vec<OsString> = ["ls-tree", "-r", "-z", "--name-only", "HEAD", "--"]
        .map(OsString::from)
        .to_vec();
    listing.extend(paths.iter().map(|path| path.as_os_str().to_owned()));
    let bytes = crate::git::output_bytes(repo, &listing, supervision).await?;
    let tracked = crate::scope::nul_separated_paths(&bytes);
    if !tracked.is_empty() {
        let mut checkout: Vec<OsString> = ["checkout", "HEAD", "--"].map(OsString::from).to_vec();
        checkout.extend(tracked.iter().map(|path| path.as_os_str().to_owned()));
        crate::git::output(repo, &checkout, supervision).await?;
    }
    for path in paths.iter().filter(|path| !tracked.contains(path)) {
        match tokio::fs::remove_file(repo.join(path)).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(WorktreeError::Io {
                    action: "remove a path the run refused to commit".to_string(),
                    path: repo.join(path),
                    source,
                })
            }
        }
    }
    Ok(())
}
