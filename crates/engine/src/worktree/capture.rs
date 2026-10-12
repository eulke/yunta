//! Capturing the tree a run or a unit works in, as git hashes it: through a
//! private index that never touches the checkout's own, and the tree a
//! checkout's `HEAD` names.

use std::path::{Path, PathBuf};

use yunta_core::{InvalidId, TreeId};

use super::{git, Effect, WorktreeError};
use crate::process::Supervision;

/// The tree `cwd` stands at right now, as the starting point a later
/// audit measures against.
///
/// Captured through an index of its own (`GIT_INDEX_FILE` under
/// `scratch`), never the repository's: a sibling unit working in the
/// same checkout at the same moment must not find this call holding
/// `.git/index`, and this call must not see half of what that sibling
/// was mid-way through staging.
///
/// `index` must sit outside `cwd`, and no two units working in one tree
/// at once may name the same one. Staging is `add -A` over the whole
/// checkout, so an index kept inside the tree it measures ends up in the
/// tree it measures; and a shared index is a lock two captures fight
/// over. `run_dir::index_for` answers both: outside the worktree,
/// named by the unit.
pub async fn capture_tree(
    cwd: &Path,
    index: &Path,
    supervision: Supervision<'_>,
) -> Result<TreeId, WorktreeError> {
    let index = private_index(index).await?;
    // A private index begins as a copy of the checkout's own, so the
    // first capture hashes only what changed since the checkout last
    // staged anything — not every file it holds.
    if !tokio::fs::try_exists(&index).await.unwrap_or(false) {
        seed_index(cwd, &index, supervision).await;
    }
    // The index travels the way every other value this engine hands a
    // child does: through the supervision it is already governed by.
    let mut env: Vec<(String, String)> = supervision.env.to_vec();
    env.push(("GIT_INDEX_FILE".to_string(), index.display().to_string()));
    let private = supervision.with_env(&env);
    // `add -A` stages what is there, untracked included, so the tree is
    // the whole visible state and not only what git already followed.
    git::change(cwd, Effect::Objects, &["add", "-A"], private).await?;
    let printed = git::change(cwd, Effect::Objects, &["write-tree"], private).await?;
    printed
        .trim()
        .parse()
        .map_err(|source: InvalidId| WorktreeError::NotATree {
            args: "write-tree".to_string(),
            cwd: cwd.to_path_buf(),
            source,
        })
}

/// Copies `cwd`'s own index to `index`. A checkout with no index of its
/// own, or one that cannot be copied, leaves the private index to start
/// from nothing, which costs time and nothing else.
async fn seed_index(cwd: &Path, index: &Path, supervision: Supervision<'_>) {
    let own = git::read(
        cwd,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
        supervision,
    )
    .await;
    let copied = match own {
        Ok(own) => tokio::fs::copy(own.trim(), index).await.map(|_| ()),
        Err(error) => Err(std::io::Error::other(error)),
    };
    if let Err(error) = copied {
        tracing::debug!(%error, "a private index starts from nothing");
    }
}

/// `index` made absolute, with its directory in place.
///
/// Absolute before anything touches it: this creates the index's
/// directory and a git child opens the file, and the two resolve a
/// relative path against different directories — here the process's,
/// there the child's. Absolute, they name the one file, and no index can
/// land inside the tree it is measuring by accident.
pub(super) async fn private_index(index: &Path) -> Result<PathBuf, WorktreeError> {
    let index = std::path::absolute(index).map_err(|source| WorktreeError::Io {
        action: "resolve the private index path".to_string(),
        path: index.to_path_buf(),
        source,
    })?;
    if let Some(parent) = index.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|source| WorktreeError::Io {
                action: "create the private index's directory".to_string(),
                path: parent.to_path_buf(),
                source,
            })?;
    }
    Ok(index)
}

/// The tree `repo`'s `HEAD` points at — the same question
/// [`super::head_commit`] asks, answered as the object a diff takes as an end.
///
/// What a unit that was handed a clean checkout of its own started
/// from: the commit it was branched from is exactly what it began with,
/// so its starting tree needs no capture. A node sharing a tree it did
/// not receive clean uses [`capture_tree`](fn@capture_tree) instead.
pub async fn head_tree(repo: &Path, supervision: Supervision<'_>) -> Result<TreeId, WorktreeError> {
    let output = git::read(repo, &["rev-parse", "HEAD^{tree}"], supervision).await?;
    output
        .trim()
        .parse()
        .map_err(|source: InvalidId| WorktreeError::NotATree {
            args: "rev-parse HEAD^{tree}".to_string(),
            cwd: repo.to_path_buf(),
            source,
        })
}
