//! The run's change, counted when a person is about to judge it, a gate
//! asking or a pull request opening, and when the engine finishes the run
//! (`done`, `failed` or `promoted`): git's count from the run's base commit
//! to its newest tree, recorded as `changes_counted`. A count that already
//! stands on the newest tree is not taken again, and a count git cannot give
//! is logged and not recorded while the run carries on.

use yunta_core::events::{ChangesCountedPayload, EventPayload, RunEvent};
use yunta_core::NodeId;
use yunta_replay::RunState;

use super::{RunCtx, RunError};

/// Counts the run's change and records it, unless `state`, the run as it
/// stands, already holds a count on its newest tree, or no node left a
/// tree. `node` is the gate or the pull request node the count is for, and
/// `None` at the run's end.
pub(super) async fn record(
    ctx: &RunCtx<'_>,
    state: &RunState,
    node: Option<&NodeId>,
) -> Result<(), RunError> {
    let Some((_, tree)) = state.newest_tree(|_| true) else {
        return Ok(());
    };
    if state.current_change().is_some() {
        return Ok(());
    }
    let counted = crate::git::numstat(
        ctx.worktree,
        ctx.manifest.base_commit.as_str(),
        tree.as_str(),
        ctx.root_supervision(),
    )
    .await;
    let stat = match counted {
        Ok(stat) => stat,
        Err(error) if error.cancelled() => return Err(error.into()),
        Err(error) => {
            // A count git cannot give is logged and not recorded, and the
            // run goes on.
            tracing::warn!(error = %error, "the run's change was not counted");
            return Ok(());
        }
    };
    let counted = ChangesCountedPayload {
        tree: tree.clone(),
        files: stat.files,
        added: stat.added,
        removed: stat.removed,
        largest: stat.largest,
    };
    ctx.emit(node, EventPayload::Run(RunEvent::ChangesCounted(counted)))
        .await?;
    Ok(())
}
