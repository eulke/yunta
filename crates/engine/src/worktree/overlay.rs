//! A task's spec in the checkout its work happens in: the files its
//! tests live in, written where the task's work starts, and the tree
//! that start is judged from.
//!
//! The files are laid over the tree the unit began from, never committed
//! on its branch: the task's own commit carries them into the run's tree
//! with its work, and every path that puts work back into a unit — a
//! carried commit, a resumed session — finds them where they were. What
//! the unit's audit measures against is that tree with the files in it,
//! so the files are never the work's change, and a change to them is.

use std::path::{Component, Path};

use yunta_core::{InvalidId, TestFile, TreeId};

use super::{private_index, WorktreeError};
use crate::process::Supervision;

/// `from` with `files` laid over it, as a tree of `repo`'s object
/// database: built through the private `index`, touching no checkout.
///
/// Each file is hashed as `git add` would hash it at its own path, so the
/// tree is the one a checkout holding `from` and those files captures.
pub async fn tree_with(
    repo: &Path,
    index: &Path,
    from: &TreeId,
    files: &[TestFile],
    supervision: Supervision<'_>,
) -> Result<TreeId, WorktreeError> {
    let index = private_index(index).await?;
    let mut env: Vec<(String, String)> = supervision.env.to_vec();
    env.push(("GIT_INDEX_FILE".to_string(), index.display().to_string()));
    let private = supervision.with_env(&env);
    crate::git::output(repo, &["read-tree", from.as_str()], private).await?;
    let scratch = index.with_extension("blob");
    for file in files {
        let path = in_repo(&file.path);
        tokio::fs::write(&scratch, &file.content)
            .await
            .map_err(|source| WorktreeError::Io {
                action: format!("stage the content of `{path}`"),
                path: scratch.clone(),
                source,
            })?;
        let blob = crate::git::output(
            repo,
            &[
                "hash-object".to_string(),
                "-w".to_string(),
                format!("--path={path}"),
                scratch.display().to_string(),
            ],
            supervision,
        )
        .await?;
        let entry = format!("100644,{},{path}", blob.trim());
        crate::git::output(
            repo,
            &["update-index", "--add", "--cacheinfo", entry.as_str()],
            private,
        )
        .await?;
    }
    let printed = crate::git::output(repo, &["write-tree"], private).await?;
    printed
        .trim()
        .parse()
        .map_err(|source: InvalidId| WorktreeError::NotATree {
            args: "write-tree".to_string(),
            cwd: repo.to_path_buf(),
            source,
        })
}

/// Writes `files` into `worktree`, each whole, over whatever is there.
pub async fn write_files(worktree: &Path, files: &[TestFile]) -> Result<(), WorktreeError> {
    for file in files {
        let path = worktree.join(in_repo(&file.path));
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|source| WorktreeError::Io {
                    action: "create the directory of a test file".to_string(),
                    path: parent.to_path_buf(),
                    source,
                })?;
        }
        tokio::fs::write(&path, &file.content)
            .await
            .map_err(|source| WorktreeError::Io {
                action: "write a test file".to_string(),
                path: path.clone(),
                source,
            })?;
    }
    Ok(())
}

/// A file's path as git names it inside the repository: its names alone,
/// joined by `/` — `./tests/a.sh` is `tests/a.sh`.
pub fn in_repo(path: &str) -> String {
    Path::new(path)
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}
