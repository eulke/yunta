//! How a node ends: verifying what it declared, recording a failure, and
//! refreshing `progress.md` either way.
//!
//! Every kind closes through [`close_node`], so every kind gets the same
//! after-hooks, the same scope check and the same artifact verification
//! — none of them can forget one. Every node failure the engine records
//! goes through the `fail*` family here, so `node_failed` is written in
//! one place and a new field on the event is a change to one function
//! rather than to every kind of node.

use std::path::{Path, PathBuf};

use yunta_core::events::{
    EventPayload, Failure, HookPhase, NodeFailedPayload, NodeFinishedPayload,
    QuestionsAskedPayload, TokenUsage,
};
use yunta_core::{HookFailurePolicy, Node, RunId};

use crate::artifacts::close_artifacts;

use super::hooks_exec::{effective_hooks, run_hook, HookRun};
use super::node_artifacts::{acquire_from_child, asked, derive_findings, record_artifacts};
use super::node_exec::{render_artifact_names, NodeEnd};
use super::{RunCtx, RunError};
use yunta_core::events::ArtifactId;
use yunta_core::events::{GateEvent, NodeEvent};
use yunta_core::{ArtifactKind, ContentHash, NodeId};

/// The child run a `kind: workflow` node closes on: what it produced is
/// what that node produced, and the run's log is where that is stated.
#[derive(Clone, Copy)]
pub(super) struct ChildRun<'a> {
    pub(super) id: &'a RunId,
    pub(super) run_dir: &'a Path,
}

/// What a node's close needs beyond the node itself.
///
/// Grouped rather than passed loose because every field answers a
/// question only the caller can: how the node ended, what it spent, what
/// the adapter declared it wrote for itself, and — for a composition —
/// which run actually did the work.
pub(super) struct Close<'a> {
    outcome: String,
    tokens: TokenUsage,
    staged: &'a [PathBuf],
    child: Option<ChildRun<'a>>,
}

impl<'a> Close<'a> {
    /// A close that staged nothing in the worktree — every kind but an
    /// agent session, whose adapter writes files of its own that the
    /// scope diff must leave out.
    pub(super) fn new(outcome: impl Into<String>, tokens: TokenUsage) -> Self {
        Close {
            outcome: outcome.into(),
            tokens,
            staged: &[],
            child: None,
        }
    }

    /// The paths the adapter declared it wrote for itself, left out of
    /// the scope diff.
    pub(super) fn staged(mut self, staged: &'a [PathBuf]) -> Self {
        self.staged = staged;
        self
    }

    /// The child run this node's declared artifacts come from, for a
    /// `kind: workflow` node whose child reached its end.
    pub(super) fn child(mut self, child: ChildRun<'a>) -> Self {
        self.child = Some(child);
        self
    }
}

/// Closes a node: its `after` hooks run, its scope is checked over the
/// whole diff — hook edits included, staged paths left out — its
/// declared artifacts are verified, and it finishes.
///
/// Where those artifacts come from is the one thing that differs by
/// kind: a `kind: workflow` node's are acquired from the log of the
/// child run named in [`Close::child`], and every other node's are what
/// this run answers for — a document already accepted under that node,
/// or a file it wrote in its staging.
pub(super) async fn close_node(
    ctx: &RunCtx<'_>,
    node: &Node,
    close: Close<'_>,
) -> Result<NodeEnd, RunError> {
    let tokens = close.tokens;
    for step in &effective_hooks(ctx, node).after {
        match run_hook(ctx, node, HookPhase::After, step).await? {
            HookRun::Violation(rule) => {
                return fail_with_tokens(ctx, node, rule, false, tokens).await
            }
            HookRun::Unset(key) => {
                return fail_with(ctx, node, Failure::unset(key), false, tokens).await
            }
            HookRun::Failed(exit) if step.on_failure == HookFailurePolicy::Fail => {
                return fail_with(ctx, node, Failure::exited(exit), false, tokens).await;
            }
            HookRun::Passed | HookRun::Failed(_) => {}
        }
    }

    if let Some(end) = super::node_scope::scope_request(ctx, node, tokens).await? {
        return Ok(end);
    }
    if let Some(end) = super::node_scope::scope_violation(ctx, node, close.staged, tokens).await? {
        return Ok(end);
    }
    if let Some(end) = land_unit(ctx, node, tokens).await? {
        return Ok(end);
    }
    // The tree this node leaves is the one its answers are proved on.
    super::finding_proofs::prove(ctx, node).await?;

    // An opaque artifact's name can carry a template
    // (`report-{{runner.name}}.md`) — rendered per node so every fan-out
    // sibling verifies its own file. A document the engine reads has no
    // name to render: its kind is its identity, in every sibling.
    let node_rendered = match render_artifact_names(ctx, node) {
        Ok(rendered) => rendered,
        Err(error) => return fail_with_tokens(ctx, node, error.to_string(), false, tokens).await,
    };
    let node = &node_rendered;

    let ceiling = ctx
        .manifest
        .config
        .limits
        .as_ref()
        .and_then(|limits| limits.max_artifact_bytes);
    // A session node's findings artifact is what that node reported, so
    // the run derives and accepts it here — before the close asks what
    // this node produced, because the acceptance is what the answer to
    // that question is made of.
    if let Some(end) = derive_findings(ctx, node, ceiling, tokens).await? {
        return Ok(end);
    }
    // A `kind: workflow` node writes no file of its own: what it
    // declares is what its child run produced, so the run takes those
    // over from that log instead of asking its own. Held by value here
    // because the close below borrows what that child's log leaves
    // standing.
    let acquired = match close.child {
        Some(child) => match acquire_from_child(ctx, node, child).await? {
            Ok(acquired) => Some(acquired),
            Err(problem) => return fail_with(ctx, node, problem.into(), false, tokens).await,
        },
        None => None,
    };
    let own;
    let (verified, standing) = match &acquired {
        Some(acquired) => (acquired.verified.as_slice(), Some(&acquired.standing)),
        None => {
            let events = ctx.load_events().await?;
            own = match close_artifacts(node, ctx.run_dir, &events, ceiling).await {
                Ok(verified) => verified,
                Err(failures) => {
                    return fail_with(ctx, node, Failure::artifacts(failures), false, tokens).await
                }
            };
            if let Some(failure) = unexplained_plan(ctx, node, &events, &own) {
                return fail_with(ctx, node, Failure::artifacts(vec![failure]), false, tokens)
                    .await;
            }
            (own.as_slice(), None)
        }
    };

    record_artifacts(ctx, node, verified, standing).await?;
    // A node that asked has done its work: what is left is an answer,
    // and that is a person's to give. It closed here like every other
    // node — hooks, scope, artifacts — and the fact it records instead
    // of a terminal is what makes the wait a wait rather than a failure
    // read as one.
    match asked(verified) {
        Some(questions) => {
            // The hash the fact names is the one the run's own store
            // answers for, read back off the log rather than taken from
            // the bytes the close happened to hold: the round that
            // follows resolves the same acceptance, and one number that
            // came from two places is one that can disagree with itself.
            let questions_hash = held_questions(ctx, &node.id).await?;
            match QuestionsAskedPayload::new(questions_hash, questions, tokens) {
                Some(payload) => {
                    ctx.emit(
                        Some(&node.id),
                        EventPayload::Gates(GateEvent::QuestionsAsked(payload)),
                    )
                    .await?;
                    write_progress(ctx).await?;
                    Ok(NodeEnd::Asked)
                }
                // A questions document with no questions asked nothing.
                // Its answers still exist, empty and the engine's own,
                // so the node after it mounts what it declared to mount.
                None => {
                    crate::answers::record_nothing_asked(&ctx.log(), ctx.run_dir, &node.id)
                        .await
                        .map_err(|source| RunError::Broken {
                            diagnostic: source.to_string(),
                        })?;
                    finish_node(ctx, node, close.outcome, tokens).await
                }
            }
        }
        None => finish_node(ctx, node, close.outcome, tokens).await,
    }
}

/// The hash the run holds `node`'s questions document under.
///
/// Read from the log, which is the run's only answer to what it holds:
/// the round that answers these questions resolves the same acceptance
/// and compares the two, so both have to come from there.
async fn held_questions(ctx: &RunCtx<'_>, node: &NodeId) -> Result<ContentHash, RunError> {
    let events = ctx.load_events().await?;
    let held = crate::artifacts::RunArtifacts::of(ctx.run_dir, &events);
    held.held(
        &ArtifactId::Interpreted {
            kind: ArtifactKind::Questions,
        },
        Some(node),
    )
    .map(|found| found.content_hash.clone())
    .ok_or_else(|| RunError::Broken {
        diagnostic: format!("node `{node}` handed over a questions document the run does not hold"),
    })
}

/// The one place `node_finished` is written.
///
/// Every way a node reaches its end passes through here — the close that
/// verified what it declared, and the round that recorded the answer a
/// node was waiting on — so a node has exactly one terminal however it
/// got there, and `progress.md` is regenerated in one place rather than
/// at each site that could finish something.
pub(super) async fn finish_node(
    ctx: &RunCtx<'_>,
    node: &Node,
    outcome: impl Into<String>,
    tokens: TokenUsage,
) -> Result<NodeEnd, RunError> {
    let closed = Box::pin(closed(ctx, node, super::shared_tree::Closing::Finished)).await?;
    // Work that reached what the project denies is not work that
    // finished: it fails, and nothing denied was committed.
    if !closed.refused.is_empty() {
        let failure = Failure::paths_denied(closed.refused.clone());
        return emit_failed(ctx, node, failure, false, tokens, closed).await;
    }
    ctx.emit(
        Some(&node.id),
        EventPayload::Node(NodeEvent::Finished(
            NodeFinishedPayload::leaving(outcome, tokens, closed.tree).committed(closed.commit),
        )),
    )
    .await?;
    write_progress(ctx).await?;
    Ok(NodeEnd::Finished)
}

/// What a node's close leaves on the run: the commit it made of its work
/// in the run's own tree, when it made one, and the tree the run stands
/// at after it — the committed one, or the tree as it is.
///
/// Its callers box it: every node closes through here, a child run's
/// included, and its git calls would otherwise ride inline in every level
/// of a composed run's recursion.
async fn closed(
    ctx: &RunCtx<'_>,
    node: &Node,
    closing: super::shared_tree::Closing,
) -> Result<Closed, RunError> {
    let at_close = super::shared_tree::at_close(ctx, node, closing).await?;
    let (commit, tree) = match at_close.committed {
        Some((commit, tree)) => (Some(commit), tree),
        None => (None, left_tree(ctx, node).await?),
    };
    Ok(Closed {
        commit,
        tree,
        refused: at_close.refused,
    })
}

/// What a node's close leaves on the run, as its terminal event names it.
struct Closed {
    commit: Option<yunta_core::CommitSha>,
    tree: yunta_core::TreeId,
    refused: Vec<PathBuf>,
}

/// The run's tree as `node` leaves it, after whatever the node landed
/// there: a node with a checkout of its own names the tree it landed in,
/// not its checkout.
async fn left_tree(ctx: &RunCtx<'_>, node: &Node) -> Result<yunta_core::TreeId, RunError> {
    let run_tree = ctx.unit.as_ref().map_or(ctx.worktree, |mine| mine.into);
    let index =
        crate::run_dir::index_for(ctx.run_dir, &crate::worktree::UnitId::Node(node.id.clone()));
    Ok(crate::worktree::capture_tree(run_tree, &index, ctx.root_supervision()).await?)
}

/// Lands what a node did in its own checkout onto the run's tree, and
/// says nothing for a node that never had one.
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
async fn land_unit(
    ctx: &RunCtx<'_>,
    node: &Node,
    tokens: TokenUsage,
) -> Result<Option<NodeEnd>, RunError> {
    let Some(mine) = ctx.unit else {
        return Ok(None);
    };
    let supervision = ctx.root_supervision();
    crate::worktree::commit_work(
        mine.unit,
        &yunta_core::text::detailed(format!("node {}", node.id), &close_title(node)),
        supervision,
    )
    .await?;
    let landing = ctx.landing.lock().await;
    match crate::worktree::rebase_onto(mine.unit, mine.into, supervision).await? {
        crate::worktree::Rebase::Onto(_) => {}
        crate::worktree::Rebase::Conflicts(paths) => {
            // Released before the node fails: a close takes the same lock
            // to decide what it commits.
            drop(landing);
            return fail_with_tokens(
                ctx,
                node,
                format!(
                    "landing `{}` on the run's tree: git could not replay it over {}",
                    node.id,
                    yunta_core::text::counted(paths.len(), "path"),
                ),
                false,
                tokens,
            )
            .await
            .map(Some);
        }
    }
    crate::worktree::land(mine.unit, mine.into, supervision).await?;
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

pub(super) async fn fail(
    ctx: &RunCtx<'_>,
    node: &Node,
    outcome: String,
    retryable: bool,
) -> Result<NodeEnd, RunError> {
    fail_with_tokens(ctx, node, outcome, retryable, TokenUsage::default()).await
}

/// Regenerates `progress.md` at `run.dir`'s root — the engine's own
/// call, right after the `node_finished` or `node_failed` that ends a
/// node, so the next session to read it (a retry, a corrective node)
/// sees the failure it follows.
pub(super) async fn write_progress(ctx: &RunCtx<'_>) -> Result<(), RunError> {
    let events = ctx.load_events().await?;
    let markdown = crate::progress::render_progress(&ctx.manifest.workflow, &events);
    tokio::fs::write(crate::run_dir::progress_path(ctx.run_dir), markdown)
        .await
        .map_err(|source| RunError::Io {
            context: "write progress.md".to_string(),
            source,
        })
}

/// Fails a node with a failure the engine states in one sentence — a
/// hook's exit code, a budget, a runner that does not resolve. Such a
/// failure is not missing its diagnostics: it simply is not about a
/// document.
pub(super) async fn fail_with_tokens(
    ctx: &RunCtx<'_>,
    node: &Node,
    outcome: String,
    retryable: bool,
    tokens: TokenUsage,
) -> Result<NodeEnd, RunError> {
    fail_with(ctx, node, Failure::message(outcome), retryable, tokens).await
}

/// The one place `node_failed` is written. The failure reaches the log
/// as the data it is — one sentence, or the declared artifacts that did
/// not close — and every surface produces its own prose from that, so
/// none of them can disagree with the facts behind it.
pub(super) async fn fail_with(
    ctx: &RunCtx<'_>,
    node: &Node,
    failure: Failure,
    retryable: bool,
    tokens: TokenUsage,
) -> Result<NodeEnd, RunError> {
    let closed = Box::pin(closed(ctx, node, super::shared_tree::Closing::Failed)).await?;
    emit_failed(ctx, node, failure, retryable, tokens, closed).await
}

async fn emit_failed(
    ctx: &RunCtx<'_>,
    node: &Node,
    failure: Failure,
    retryable: bool,
    tokens: TokenUsage,
    closed: Closed,
) -> Result<NodeEnd, RunError> {
    ctx.emit(
        Some(&node.id),
        EventPayload::Node(NodeEvent::Failed(
            NodeFailedPayload::new(failure, retryable, tokens)
                .leaving(closed.tree)
                .committed(closed.commit)
                .refusing(closed.refused),
        )),
    )
    .await?;
    write_progress(ctx).await?;
    Ok(NodeEnd::Failed)
}

/// A tasks document `node` closes with that a gate shows a person, and
/// that does not say what it changes and why. The submission tool
/// refuses one when it is handed over; this holds one that arrived as a
/// file to the same rule.
fn unexplained_plan(
    ctx: &RunCtx<'_>,
    node: &yunta_core::Node,
    events: &[yunta_core::events::StoredEvent],
    verified: &[crate::artifacts::VerifiedArtifact],
) -> Option<yunta_core::diagnostic::ArtifactFailure> {
    if !crate::tasks::plan_reviewed(&ctx.manifest.workflow, events, &node.id) {
        return None;
    }
    verified.iter().find_map(|artifact| {
        let crate::artifacts::ArtifactContent::Tasks(tasks) = &artifact.content else {
            return None;
        };
        let broken = tasks.unexplained();
        (!broken.is_empty()).then(|| {
            yunta_core::diagnostic::ArtifactFailure::Content(yunta_core::diagnostic::Report::new(
                yunta_core::diagnostic::DocumentRef::new(
                    ArtifactKind::Tasks,
                    artifact.path.display().to_string(),
                ),
                broken,
            ))
        })
    })
}
