//! The one `node_started` this engine writes, and the tree the attempt
//! begins from.

use yunta_core::events::{EventPayload, NodeEvent, NodeStartedPayload};
use yunta_core::{CommitSha, Node, NodeScope, ScopeGlob, TreeId};

use super::{RunCtx, RunError};

/// Writes the one `node_started` this engine ever writes, with the tree
/// the attempt begins from.
///
/// The starting point is recorded here and nowhere else, because a fact
/// derived from the log has to be written at exactly one moment: the
/// audit that reads it back at close is only as true as the single
/// instant this call names. A node given a checkout of its own already
/// has that instant — the one its unit was opened at; a node working in
/// the run's tree starts from what it finds there. A node scoped to the
/// run is given, at the same instant, what the run had changed.
pub(super) async fn emit_started(
    ctx: &RunCtx<'_>,
    node: &Node,
    attempt: u32,
) -> Result<(), RunError> {
    let Some(mine) = ctx.unit else {
        // Finding, committing and starting are one step: no landing moves
        // the run's tree in between.
        let _landing = ctx.landing.lock().await;
        let (found, from) = Box::pin(in_the_runs_tree(ctx, node)).await?;
        return started(ctx, node, attempt, from, found, None).await;
    };
    let _landing = ctx.landing.lock().await;
    let found = Box::pin(super::shared_tree::found(ctx, node)).await?;
    let from = mine.unit.from.clone();
    let run_scope = match node.scope {
        NodeScope::Run => Some(changed_by_the_run(ctx, &from).await?),
        NodeScope::Unscoped | NodeScope::Globs(_) => None,
    };
    started(
        ctx,
        node,
        attempt,
        from,
        found.map(|(commit, _)| commit),
        run_scope,
    )
    .await
}

/// What the run had changed since its base, as the tree `from` an
/// attempt starts from holds it: the paths a node scoped to the run may
/// change, each as the glob that selects exactly it.
async fn changed_by_the_run(ctx: &RunCtx<'_>, from: &TreeId) -> Result<Vec<ScopeGlob>, RunError> {
    let paths = crate::scope::changed_between(
        ctx.worktree,
        &ctx.manifest.base_commit,
        from,
        ctx.root_supervision(),
    )
    .await?;
    // Escaping a path always yields a glob that parses.
    Ok(paths
        .iter()
        .filter_map(|path| ScopeGlob::exact(path).ok())
        .collect())
}

/// What a node working in the run's tree starts from. What the tree
/// holds that no node committed — a person's edits while the run was
/// parked, what an interrupted attempt left — is committed first, as
/// found, so the attempt starts from a branch that holds it.
async fn in_the_runs_tree(
    ctx: &RunCtx<'_>,
    node: &Node,
) -> Result<(Option<CommitSha>, TreeId), RunError> {
    if let Some((commit, tree)) = super::shared_tree::found(ctx, node).await? {
        return Ok((Some(commit), tree));
    }
    let index =
        crate::run_dir::index_for(ctx.run_dir, &crate::worktree::UnitId::Node(node.id.clone()));
    let tree = crate::worktree::capture_tree(ctx.worktree, &index, ctx.root_supervision()).await?;
    Ok((None, tree))
}

async fn started(
    ctx: &RunCtx<'_>,
    node: &Node,
    attempt: u32,
    from: TreeId,
    found: Option<CommitSha>,
    run_scope: Option<Vec<ScopeGlob>>,
) -> Result<(), RunError> {
    ctx.emit(
        Some(&node.id),
        EventPayload::Node(NodeEvent::Started(
            NodeStartedPayload::attempt_from(attempt, from)
                .found(found)
                .run_scope(run_scope),
        )),
    )
    .await?;
    Ok(())
}
