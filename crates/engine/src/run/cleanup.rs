//! `on_finish.cleanup: worktree`: what a run that finished asking to be
//! cleaned up takes away — the checkouts its units worked in, the branches
//! whose work its tree holds, and its own linked worktree. A cleanup that
//! fails warns and never un-finishes the run the log already closed.

use yunta_core::events::FindingSeverity;
use yunta_core::{Location, RelativePath};

use super::{RunCtx, RunError};

/// Cleans up the run's checkouts when its workflow asks for it and the run
/// worked in a tree of its own.
pub(super) async fn worktrees(ctx: &RunCtx<'_>) -> Result<(), RunError> {
    let wants_cleanup = ctx.manifest.workflow.on_finish.iter().any(|step| {
        matches!(
            step,
            yunta_core::OnFinishStep::Cleanup {
                cleanup: yunta_core::CleanupTarget::Worktree
            }
        )
    });
    if wants_cleanup && ctx.manifest.isolation == yunta_core::Isolation::Worktree {
        // The units' checkouts first, while the run's tree still names
        // what landed: a branch whose work it holds goes with them.
        let released = crate::worktree::release_unit_checkouts(
            ctx.worktree,
            ctx.run_dir,
            ctx.run_id,
            ctx.root_supervision(),
        );
        if let Err(e) = released.await {
            ctx.engine_finding(
                None,
                "cleanup-failed",
                FindingSeverity::Minor,
                "on_finish.cleanup: worktree failed".to_string(),
                Location::work(RelativePath::here(), None),
                format!("the checkouts the run's units worked in could not be removed: {e}"),
            )
            .await?;
        }
        match crate::worktree::cleanup_worktree(
            ctx.worktree,
            &crate::worktree::run_branch(ctx.run_id),
            ctx.root_supervision(),
        )
        .await
        {
            Ok(crate::worktree::WorktreeCleanup::Removed) => {}
            Ok(crate::worktree::WorktreeCleanup::NotALinkedWorktree) => {
                ctx.engine_finding(
                    None,
                    "cleanup-not-a-worktree",
                    FindingSeverity::Minor,
                    "on_finish.cleanup: worktree skipped".to_string(),
                    Location::work(RelativePath::here(), None),
                    "the run's tree is not a linked git worktree, so removing it would delete a \
                     primary checkout — nothing was touched"
                        .to_string(),
                )
                .await?;
            }
            Err(e) => {
                ctx.engine_finding(
                    None,
                    "cleanup-failed",
                    FindingSeverity::Minor,
                    "on_finish.cleanup: worktree failed".to_string(),
                    Location::work(RelativePath::here(), None),
                    format!("the run's linked worktree could not be removed: {e}"),
                )
                .await?;
            }
        }
    }
    Ok(())
}
