//! When what a node did in the run's own tree becomes a commit.
//!
//! A node with a checkout of its own lands its work as commits. A node
//! that works in the run's tree leaves its work there, and it becomes a
//! commit when the node closes — finished or failed — so the run's branch
//! holds every node's work: what a later node pushes is what the run did,
//! a checkout opened from the branch sees it, and removing the worktree
//! loses nothing. The run's tree and its `HEAD` are then the same
//! content, less what git ignores.

use std::path::{Path, PathBuf};

use yunta_core::{CommitSha, Isolation, Node, NodeId, NodeKind, TreeId};

use super::{RunCtx, RunError};
use crate::replay::{NodeState, RunState};

/// How the attempt whose work is being committed closed — what its
/// commit says about it.
#[derive(Clone, Copy)]
pub(super) enum Closing {
    Finished,
    Failed,
}

/// What a close did to the run's tree: the commit it made and the tree
/// that commit holds, when it made one, and the paths it refused.
#[derive(Default)]
pub(super) struct AtClose {
    pub(super) committed: Option<(CommitSha, TreeId)>,
    /// What the work left that no session of the run may write: never
    /// committed, and in a run with a worktree of its own put back as the
    /// branch had it.
    pub(super) refused: Vec<PathBuf>,
}

/// Commits what `node` left in the run's tree as it closes, and answers
/// with the commit and the tree it holds, and what it refused.
///
/// Nothing is committed for a node with a checkout of its own (it lands
/// its work), for a gate (it writes nothing), in a run that works in a
/// person's own checkout (their branch is left as it was found), or while
/// another node is still working in the same tree: the tree holds both
/// nodes' work at once, so it is committed by whichever of them closes
/// last, naming the others. The close that commits answers for the whole
/// commit: what it would add that no session of the run may write is
/// refused and put back first.
pub(super) async fn at_close(
    ctx: &RunCtx<'_>,
    node: &Node,
    closing: Closing,
) -> Result<AtClose, RunError> {
    if !shares_the_tree(ctx, node) {
        return Ok(AtClose::default());
    }
    if !commits_here(ctx) {
        return Box::pin(refused_in_place(ctx, node)).await;
    }
    let _landing = ctx.landing.lock().await;
    let Some(message) = commit_message(ctx, node, closing).await? else {
        return Ok(AtClose::default());
    };
    let index =
        crate::run_dir::index_for(ctx.run_dir, &crate::worktree::UnitId::Node(node.id.clone()));
    // Boxed: a close runs at the bottom of every composed run's stack.
    let refused = Box::pin(refuse_denied(ctx, node, &index)).await?;
    let committed = Box::pin(crate::worktree::commit_tree(
        ctx.worktree,
        &index,
        &message,
        ctx.root_supervision(),
    ))
    .await?;
    Ok(AtClose { committed, refused })
}

/// What the run's tree would commit that the run denies `node`, put back
/// as `HEAD` has it.
async fn refuse_denied(
    ctx: &RunCtx<'_>,
    node: &Node,
    index: &Path,
) -> Result<Vec<PathBuf>, RunError> {
    let deny = super::denied::Denied::of(ctx).await?.closing(node);
    if deny.is_empty() {
        return Ok(Vec::new());
    }
    let supervision = ctx.root_supervision();
    let tree = crate::worktree::capture_tree(ctx.worktree, index, supervision).await?;
    let head = crate::worktree::head_commit(ctx.worktree, supervision).await?;
    let added = crate::scope::changed_between(ctx.worktree, &head, &tree, supervision).await?;
    let refused = crate::scope::denied(&added, &deny, &[])?;
    crate::worktree::restore(ctx.worktree, &refused, supervision).await?;
    Ok(refused)
}

/// In a person's own checkout nothing is committed or put back: what
/// `node` wrote there that the run denies it is only refused, for the
/// node to fail with.
async fn refused_in_place(ctx: &RunCtx<'_>, node: &Node) -> Result<AtClose, RunError> {
    let deny = super::denied::Denied::of(ctx).await?.closing(node);
    let state = ctx.run_view().await?.state;
    let from = state.nodes.from_tree(&node.id).cloned();
    let (false, Some(from)) = (deny.is_empty(), from) else {
        return Ok(AtClose::default());
    };
    let index =
        crate::run_dir::index_for(ctx.run_dir, &crate::worktree::UnitId::Node(node.id.clone()));
    let diff =
        crate::scope::changed_since(ctx.worktree, &from, &index, ctx.root_supervision()).await?;
    Ok(AtClose {
        committed: None,
        refused: crate::scope::denied(&diff, &deny, &[])?,
    })
}

/// What `node`'s commit says, read off the run's state — or `None` while
/// another node is still working in the same tree. The state is dropped
/// here, before any git runs.
async fn commit_message(
    ctx: &RunCtx<'_>,
    node: &Node,
    closing: Closing,
) -> Result<Option<String>, RunError> {
    let state = ctx.run_view().await?.state;
    if working_beside(&ctx.manifest.workflow, &state, &node.id) {
        return Ok(None);
    }
    Ok(Some(message(&ctx.manifest.workflow, &state, node, closing)))
}

/// Commits what the run's tree holds that no node committed, as `node`
/// starts from it — a person's edits while the run was parked, or what
/// an interrupted attempt left — and answers with the commit and the
/// tree it holds.
///
/// A node that works in the run's tree starts from a branch that holds
/// it. A node given a checkout of its own was opened on everything the
/// tree holds; committing it here too keeps the branch its work lands
/// on holding the same content as the checkout it started from, so an
/// edit to what a person left lands rather than conflicting with a
/// branch that never had it.
///
/// `None` when there is nothing to find, for a gate, for a node that
/// works in a checkout its group opened, in a run that works in a
/// person's own checkout, or while another node works in the run's tree:
/// what the tree holds then may be that node's, and its close commits
/// it. The caller holds the landing lock across this and the start it
/// records, so no close commits in between.
pub(super) async fn found(
    ctx: &RunCtx<'_>,
    node: &Node,
) -> Result<Option<(CommitSha, TreeId)>, RunError> {
    let tree = match ctx.unit {
        None => ctx.worktree,
        Some(mine) if mine.into_the_runs_tree && owns(&mine, node) => mine.into,
        Some(_) => return Ok(None),
    };
    if !commits_here(ctx) || is_gate(node) {
        return Ok(None);
    }
    let Some(message) = found_message(ctx, node).await? else {
        return Ok(None);
    };
    let index =
        crate::run_dir::index_for(ctx.run_dir, &crate::worktree::UnitId::Node(node.id.clone()));
    Ok(Box::pin(crate::worktree::commit_tree(
        tree,
        &index,
        &message,
        ctx.root_supervision(),
    ))
    .await?)
}

/// Whether `mine` is `node`'s own checkout, not one its group opened.
fn owns(mine: &super::ctx::NodeUnit<'_>, node: &Node) -> bool {
    mine.unit.who == crate::worktree::UnitId::Node(node.id.clone())
}

/// What a found commit says — and, when this node's previous attempt
/// never closed, that what it holds is what that attempt left. `None`
/// while another node works in the run's tree.
async fn found_message(ctx: &RunCtx<'_>, node: &Node) -> Result<Option<String>, RunError> {
    let state = ctx.run_view().await?.state;
    if working_beside(&ctx.manifest.workflow, &state, &node.id) {
        return Ok(None);
    }
    let subject = format!("found in the run's tree before node {} started", node.id);
    let interrupted = state
        .nodes
        .get(&node.id)
        .filter(|record| record.open_since.is_some())
        .map(|record| record.attempts);
    Ok(Some(match interrupted {
        Some(attempt) => {
            format!("{subject}\n\nHolds what attempt {attempt} left when it was interrupted.")
        }
        None => subject,
    }))
}

/// Whether this run commits what its nodes leave in its tree: only a run
/// with a worktree of its own.
fn commits_here(ctx: &RunCtx<'_>) -> bool {
    ctx.manifest.isolation == Isolation::Worktree
}

/// Whether `node` works in the run's own tree: it has no checkout of its
/// own, nor one its group opened, and it is not a gate.
fn shares_the_tree(ctx: &RunCtx<'_>, node: &Node) -> bool {
    ctx.unit.is_none() && !is_gate(node)
}

fn is_gate(node: &Node) -> bool {
    matches!(node.kind, NodeKind::Gate { .. })
}

/// The nodes that work in the run's own tree, as the workflow declares
/// them: no checkout of their own, and not gates.
fn tree_sharers(workflow: &yunta_core::Workflow) -> impl Iterator<Item = &Node> {
    workflow
        .iter_nodes()
        .filter(|node| !crate::scope::audits(node) && !is_gate(node))
}

/// Whether a node other than `node` is still working in the run's tree.
/// A `parallel` group is running while its children are, so a child never
/// commits and its group commits once, when it closes.
fn working_beside(workflow: &yunta_core::Workflow, state: &RunState, node: &NodeId) -> bool {
    tree_sharers(workflow).any(|other| {
        &other.id != node
            && matches!(
                state.nodes.state(&other.id),
                Some(NodeState::Running { .. })
            )
    })
}

/// What the commit says: the node and what it declares it does, or which
/// of its attempts failed; and the nodes that closed while it worked,
/// whose work it holds too.
fn message(
    workflow: &yunta_core::Workflow,
    state: &RunState,
    node: &Node,
    closing: Closing,
) -> String {
    let record = state.nodes.get(&node.id);
    let title = match closing {
        Closing::Finished => node
            .description
            .clone()
            .unwrap_or_else(|| node.id.to_string()),
        Closing::Failed => format!(
            "what attempt {} left when it failed",
            record.map_or(1, |record| record.attempts)
        ),
    };
    let started = record.and_then(|record| record.last_started);
    let beside: Vec<String> = tree_sharers(workflow)
        .filter(|other| other.id != node.id)
        .filter(|other| {
            state.nodes.get(&other.id).is_some_and(|other| {
                other
                    .last_terminal
                    .is_some_and(|closed| Some(closed) > started)
            })
        })
        .map(|other| format!("`{}`", other.id))
        .collect();
    let subject = format!("node {}: {title}", node.id);
    match beside.is_empty() {
        true => subject,
        false => format!(
            "{subject}\n\nHolds what {} left in the run's tree while this node worked.",
            beside.join(", ")
        ),
    }
}
