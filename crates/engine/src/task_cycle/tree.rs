//! What a criterion's answer on a checkout is kept for: the tree the
//! checkout holds, and the commit it stands on when the command reads
//! git's history.

use std::path::Path;

use yunta_core::protected::Pathspecs;
use yunta_core::{Criterion, TreeId};

use super::TaskCycleError;
use crate::process::Supervision;

/// What a criterion's answer on a checkout turns on: the git tree of
/// everything the checkout holds, and — read only when a command that
/// runs `git` asks — the commit it stands on.
pub(crate) struct Tree {
    pub(super) content: TreeId,
    pub(super) head: Option<String>,
}

impl Tree {
    /// The git tree of what the checkout holds.
    pub(crate) fn content(&self) -> &TreeId {
        &self.content
    }

    /// `cwd` as it stands now. The content is every file a checkout
    /// shows — untracked ones included, ignored ones not unless they lie
    /// under `covered` — staged through an index of this call's own,
    /// which starts as a copy of the checkout's so only what changed is
    /// hashed again, and which no other capture of the same checkout
    /// shares.
    pub(super) async fn of(
        cwd: &Path,
        with_head: bool,
        covered: &Pathspecs,
        supervision: Supervision<'_>,
    ) -> Result<Self, TaskCycleError> {
        let own = crate::git::read(
            cwd,
            &["rev-parse", "--path-format=absolute", "--git-path", "index"],
            supervision,
        )
        .await?;
        let staging = tempfile::tempdir().map_err(TaskCycleError::TreeIndex)?;
        let index = staging.path().join("index");
        if let Err(source) = tokio::fs::copy(own.trim(), &index).await {
            // A checkout with no index yet stages from nothing.
            if source.kind() != std::io::ErrorKind::NotFound {
                return Err(TaskCycleError::TreeIndex(source));
            }
        }
        let content = crate::worktree::capture_tree(cwd, &index, covered, supervision)
            .await
            .map_err(|source| TaskCycleError::TreeContent(Box::new(source)))?;
        let head = match with_head {
            true => {
                let head = crate::git::read(cwd, &["rev-parse", "HEAD"], supervision).await?;
                Some(head.trim().to_string())
            }
            false => None,
        };
        Ok(Tree { content, head })
    }

    /// `cwd` as `criteria` need it read: with the commit it stands on
    /// when one of them runs `git`.
    pub(super) async fn for_criteria(
        criteria: &[Criterion],
        cwd: &Path,
        covered: &Pathspecs,
        supervision: Supervision<'_>,
    ) -> Result<Self, TaskCycleError> {
        let with_head = criteria.iter().any(|criterion| asks_git(&criterion.cmd));
        Self::of(cwd, with_head, covered, supervision).await
    }
}

/// What `cwd` holds, as the tree a criterion's answer is kept for.
pub(crate) async fn content_of(
    cwd: &Path,
    covered: &Pathspecs,
    supervision: Supervision<'_>,
) -> Result<TreeId, TaskCycleError> {
    Ok(Tree::of(cwd, false, covered, supervision).await?.content)
}

/// Whether `cmd` runs `git`, which can answer from history as well as
/// from the files a checkout holds.
pub(super) fn asks_git(cmd: &str) -> bool {
    crate::check::leading_programs(cmd)
        .iter()
        .any(|program| program == "git")
}
