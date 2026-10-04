//! What a run gives back as it ends. Every run, whatever its end, gives its
//! checkouts back to the project's pool — its units', with the branches
//! whose work its tree holds, and its own, unless a successor carries on
//! in it. A run that finished asking to be cleaned up
//! (`on_finish.cleanup: worktree`) also lets its own branch go. None of it
//! ever un-finishes the run the log already closed.

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

/// Gives the checkout the run worked in back to the project's pool, when
/// it worked in one of its own. One that cannot be given back stays the
/// run's until `gc` collects it: a warning, never an un-finished run.
pub(super) async fn run(ctx: &RunCtx<'_>) {
    if ctx.manifest.isolation != yunta_core::Isolation::Worktree {
        return;
    }
    let released = crate::worktree::release_run_checkout(
        &ctx.pool,
        ctx.worktree,
        ctx.run_id,
        ctx.root_supervision(),
    );
    if let Err(error) = released.await {
        tracing::warn!(%error, "the checkout the run worked in was not given back");
    }
}

/// Lets the run's own branch go when its workflow asks for it and the run
/// worked in a tree of its own: a checkout of the project's pool goes back
/// to it, and one a run made before its project kept checkouts is taken
/// away.
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
        if ctx.pool.keeps(ctx.worktree, ctx.root_supervision()).await {
            return let_branch_go(ctx).await;
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

/// Deletes the run's branch once its checkout went back to the pool with
/// no branch: deleted only when its work is in what that checkout holds.
async fn let_branch_go(ctx: &RunCtx<'_>) -> Result<(), RunError> {
    let branch = crate::worktree::run_branch(ctx.run_id);
    let args = ["branch", "-d", branch.as_str()];
    let deleted = crate::git::output(ctx.worktree, &args, ctx.root_supervision()).await;
    if let Err(e) = deleted {
        ctx.engine_finding(
            None,
            "cleanup-failed",
            FindingSeverity::Minor,
            "on_finish.cleanup: worktree failed".to_string(),
            Location::work(RelativePath::here(), None),
            format!("the run's branch could not be deleted: {e}"),
        )
        .await?;
    }
    Ok(())
}
