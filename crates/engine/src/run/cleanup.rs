//! What a run gives back as it ends. Every run, whatever its end, gives its
//! units' checkouts back to the project's pool, and the branches whose work
//! its tree holds go with them. A run that finished asking to be cleaned up
//! (`on_finish.cleanup: worktree`) also takes away its own linked worktree.
//! Neither ever un-finishes the run the log already closed.

use yunta_core::events::FindingSeverity;
use yunta_core::{Location, RelativePath};

use super::{RunCtx, RunError};

/// Gives the checkouts the run's units hold back to the project's pool.
/// One that cannot be given back stays its unit's until `gc` collects the
/// run: a warning, never an un-finished run.
pub(super) async fn units(ctx: &RunCtx<'_>) {
    let released = crate::worktree::release_unit_checkouts(
        &ctx.pool,
        ctx.worktree,
        ctx.run_dir,
        ctx.run_id,
        ctx.root_supervision(),
    );
    if let Err(error) = released.await {
        tracing::warn!(%error, "the checkouts the run's units held were not all given back");
    }
}

/// Takes away the run's own worktree when its workflow asks for it and
/// the run worked in a tree of its own.
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
