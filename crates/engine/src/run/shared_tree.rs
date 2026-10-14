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
use yunta_core::commit_message::CommitMessage;

use yunta_core::events::{Failure, TokenUsage};
use yunta_core::protected::{Closing, Protected, Trespass};
use yunta_core::{CommitSha, Isolation, Node, NodeId, NodeKind, TreeId};

use super::node_close::{fail_with_tokens_staged, tree_of};
use super::node_exec::NodeEnd;
use super::{RunCtx, RunError};
use crate::replay::{NodeState, RunState};

/// How the attempt whose work is being committed ended — what its commit
/// says about it, and which closing judges it.
#[derive(Clone, Copy)]
pub(super) enum Landing {
    Finished,
    Failed,
}

/// The closing a node's own close judges its diff by. A loop's close leaves
/// approved tests out: each of its tasks judged them when it landed.
pub(super) fn closing_of(node: &Node) -> Closing {
    if matches!(node.kind, NodeKind::Loop { .. }) {
        Closing::LoopClose
    } else {
        Closing::Every
    }
}

/// What a close did to the run's tree: the commit it made and the tree
/// that commit holds, when it made one, and the paths it refused.
#[derive(Default)]
pub(super) struct AtClose {
    /// The run's tree as the close read it to commit it — `None` when it
    /// committed nothing there — and the commit it made, if any.
    pub(super) captured: Option<crate::worktree::Captured>,
    /// What the work left that no session of the run may write: never
    /// committed, and in a run with a worktree of its own put back as the
    /// branch had it.
    pub(super) refused: Vec<Trespass>,
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
/// refused and put back first. `staged` is what the adapter wrote for
/// itself, which is never a trespass.
pub(super) async fn at_close(
    ctx: &RunCtx<'_>,
    node: &Node,
    landing: Landing,
    staged: &[PathBuf],
) -> Result<AtClose, RunError> {
    if !shares_the_tree(ctx, node) {
        return Ok(AtClose::default());
    }
    if !commits_here(ctx) {
        return Box::pin(refused_in_place(ctx, node, staged)).await;
    }
    let _landing = ctx.landing.lock().await;
    let Some(message) = commit_message(ctx, node, landing).await? else {
        return Ok(AtClose::default());
    };
    let index =
        crate::run_dir::index_for(ctx.run_dir, &crate::worktree::UnitId::Node(node.id.clone()));
    // Boxed: a close runs at the bottom of every composed run's stack.
    let refused = Box::pin(refuse_denied(ctx, node, &index, staged)).await?;
    let captured = Box::pin(crate::worktree::commit_tree(
        ctx.worktree,
        &index,
        &message,
        ctx.root_supervision(),
    ))
    .await?;
    Ok(AtClose {
        captured: Some(captured),
        refused,
    })
}

/// What `node`'s close judges its diff by, as the log stands: the set the
/// run protects, with the instruction files a person granted `node` lifted.
async fn node_judged(ctx: &RunCtx<'_>, node: &Node) -> Result<Protected, RunError> {
    let state = ctx.run_view().await?.state;
    Ok(super::node_scope::judged_by(&ctx.protected(), &state, node))
}

/// What the run's tree would commit that `node`'s close judges by protects,
/// put back as `HEAD` has it.
async fn refuse_denied(
    ctx: &RunCtx<'_>,
    node: &Node,
    index: &Path,
    staged: &[PathBuf],
) -> Result<Vec<Trespass>, RunError> {
    let protected = node_judged(ctx, node).await?;
    let pathspecs = ctx.protected().pathspecs();
    let supervision = ctx.root_supervision();
    let tree = crate::worktree::capture_tree(ctx.worktree, index, &pathspecs, supervision).await?;
    let head = crate::worktree::head_commit(ctx.worktree, supervision).await?;
    let added =
        crate::scope::changed_between(ctx.worktree, head.as_str(), tree.as_str(), supervision)
            .await?;
    let refused = protected.trespasses(&added, staged, closing_of(node));
    let paths: Vec<PathBuf> = refused
        .iter()
        .map(|trespass| trespass.path.clone())
        .collect();
    crate::worktree::restore(ctx.worktree, &paths, supervision).await?;
    Ok(refused)
}

/// In a person's own checkout nothing is committed or put back: what
/// `node` wrote there that its close judges by protects is only refused,
/// for the node to fail with.
async fn refused_in_place(
    ctx: &RunCtx<'_>,
    node: &Node,
    staged: &[PathBuf],
) -> Result<AtClose, RunError> {
    let state = ctx.run_view().await?.state;
    let protected = super::node_scope::judged_by(&ctx.protected(), &state, node);
    let index =
        crate::run_dir::index_for(ctx.run_dir, &crate::worktree::UnitId::Node(node.id.clone()));
    let refused = match state.nodes.from_tree(&node.id).cloned() {
        Some(from) => {
            let pathspecs = ctx.protected().pathspecs();
            let diff = crate::scope::changed_since(
                ctx.worktree,
                &from,
                &index,
                &pathspecs,
                ctx.root_supervision(),
            )
            .await?;
            protected.trespasses(&diff, staged, closing_of(node))
        }
        // A log written before a start named its tree is judged from the
        // commit untreed attempts start from, as its close is.
        None => {
            let base = untreed_base(ctx).await?;
            crate::scope::protection_audit(
                ctx.worktree,
                base.as_str(),
                &index,
                &protected,
                closing_of(node),
                staged,
                ctx.root_supervision(),
            )
            .await?
            .trespasses()
        }
    };
    Ok(AtClose {
        captured: None,
        refused,
    })
}

/// The commit a node's diff is judged from when no start recorded the tree it
/// began from: HEAD where the run commits its own tree, else the commit the
/// run began on.
pub(super) async fn untreed_base(ctx: &RunCtx<'_>) -> Result<CommitSha, RunError> {
    match commits_here(ctx) {
        true => Ok(crate::worktree::head_commit(ctx.worktree, ctx.root_supervision()).await?),
        false => Ok(ctx.manifest.base_commit.clone()),
    }
}

/// What `node`'s commit says, read off the run's state — or `None` while
/// another node is still working in the same tree. The state is dropped
/// here, before any git runs.
async fn commit_message(
    ctx: &RunCtx<'_>,
    node: &Node,
    landing: Landing,
) -> Result<Option<String>, RunError> {
    let state = ctx.run_view().await?.state;
    if working_beside(&ctx.manifest.workflow, &state, &node.id) {
        return Ok(None);
    }
    let said = message(&ctx.manifest.workflow, &state, node, landing);
    Ok(Some(
        CommitMessage::new(said, ctx.run_id).node(&node.id).text(),
    ))
}

/// Commits what the run's tree holds that no node committed, as `node`
/// starts from it — a person's edits while the run was parked, or what
/// an interrupted attempt left — and answers with the commit and the
/// tree it holds — the tree whether or not there was anything to commit.
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
) -> Result<Option<crate::worktree::Captured>, RunError> {
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
    Ok(Some(
        Box::pin(crate::worktree::commit_tree(
            tree,
            &index,
            &message,
            ctx.root_supervision(),
        ))
        .await?,
    ))
}

/// The tree a record of `node` names: its start, its close, and the wake that
/// finds it failed. A node that works in the run's own tree, in a run that
/// commits there, names the tree its commit would hold, whether or not its
/// close commits: the run's tree and its `HEAD` hold the same content, less
/// what git ignores. Any other node names the tree `tree_of` reads.
///
/// Read without committing anything, so a record and a wake that come
/// later name the same kind of tree for the same node.
pub(super) async fn recorded_tree(ctx: &RunCtx<'_>, node: &Node) -> Result<TreeId, RunError> {
    if !commits_here(ctx) || !shares_the_tree(ctx, node) {
        return tree_of(ctx, node).await;
    }
    let index =
        crate::run_dir::index_for(ctx.run_dir, &crate::worktree::UnitId::Node(node.id.clone()));
    Ok(crate::worktree::capture_for_commit(ctx.worktree, &index, ctx.root_supervision()).await?)
}

/// Lands what a node did in its own checkout onto the run's tree, and says
/// nothing for a node that never had one.
///
/// A unit lands the moment its work passes its audit: from there what it
/// wrote is the run's, and a later failure about what the node
/// *declared* — a missing artifact — is about the node, not about its
/// edits, exactly as it is for a node that worked in the run's own tree
/// all along. A node that failed its audit never reaches here, so a
/// write outside every glob never becomes the run's.
///
/// A replay git cannot finish fails the node naming the paths: two nodes
/// that can be open at once had their scopes proved disjoint by `check`,
/// so a conflict here is a workflow that got past it.
pub(super) async fn land_unit(
    ctx: &RunCtx<'_>,
    node: &Node,
    staged: &[PathBuf],
    tokens: TokenUsage,
) -> Result<Option<NodeEnd>, RunError> {
    let Some(mine) = ctx.unit else {
        return Ok(None);
    };
    let supervision = ctx.root_supervision();
    let said = CommitMessage::new(
        yunta_core::text::detailed(format!("node {}", node.id), &close_title(node)),
        ctx.run_id,
    )
    .node(&node.id)
    .text();
    crate::worktree::commit_work(mine.unit, &said, supervision).await?;
    let landing = ctx.landing.lock().await;
    match crate::worktree::rebase_onto(mine.unit, mine.into, supervision).await? {
        crate::worktree::Rebase::Onto(_) => {}
        crate::worktree::Rebase::Conflicts(paths) => {
            // Released before the node fails: a close takes the same lock
            // to decide what it commits.
            drop(landing);
            let failure = Failure::EngineFailed {
                engine_error: format!(
                    "landing `{}` on the run's tree: git could not replay it over {}",
                    node.id,
                    yunta_core::text::counted(paths.len(), "path"),
                ),
            };
            return fail_with_tokens_staged(ctx, node, failure, tokens, staged)
                .await
                .map(Some);
        }
    }
    crate::worktree::land(mine.unit, mine.into, supervision).await?;
    // Landed, its refs go; the checkout stays the node's, so a person who
    // sends the run back to it finds its session where it saw it.
    crate::worktree::forget_attempts(mine.unit, supervision).await?;
    Ok(None)
}

/// What a node's landing commit is about, for a person reading the
/// branch: what the node declares it does, or its id when it says
/// nothing about itself.
fn close_title(node: &Node) -> String {
    node.description
        .clone()
        .unwrap_or_else(|| node.id.to_string())
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
    let said = match interrupted {
        Some(attempt) => {
            format!("{subject}\n\nHolds what attempt {attempt} left when it was interrupted.")
        }
        None => subject,
    };
    Ok(Some(
        CommitMessage::new(said, ctx.run_id).node(&node.id).text(),
    ))
}

/// Whether this run commits what its nodes leave in its tree: only a run
/// with a worktree of its own.
fn commits_here(ctx: &RunCtx<'_>) -> bool {
    ctx.manifest.isolation == Isolation::Worktree
}

/// Whether `node` works in the run's own tree: it has no checkout of its
/// own, nor one its group opened, and the workflow does not declare it one.
/// A wake holds no checkout for any node, so the declaration decides there,
/// as it decides for a running node that has none.
fn shares_the_tree(ctx: &RunCtx<'_>, node: &Node) -> bool {
    ctx.unit.is_none() && declared_to_share(node)
}

fn is_gate(node: &Node) -> bool {
    matches!(node.kind, NodeKind::Gate { .. })
}

/// Whether the workflow declares `node` to work in the run's own tree: it is
/// not audited, which is what gives a node a checkout of its own, and it is
/// not a gate.
fn declared_to_share(node: &Node) -> bool {
    !crate::scope::audits(node) && !is_gate(node)
}

/// The nodes that work in the run's own tree, as the workflow declares
/// them: no checkout of their own, and not gates.
fn tree_sharers(workflow: &yunta_core::Workflow) -> impl Iterator<Item = &Node> {
    workflow.iter_nodes().filter(|node| declared_to_share(node))
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
    landing: Landing,
) -> String {
    let record = state.nodes.get(&node.id);
    let title = match landing {
        Landing::Finished => node
            .description
            .clone()
            .unwrap_or_else(|| node.id.to_string()),
        Landing::Failed => format!(
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
