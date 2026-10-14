//! How a node ends: verifying what it declared, and recording how it
//! finished or why it failed.
//!
//! Every kind closes through [`close_node`], so every kind gets the same
//! after-hooks, the same scope check and the same artifact verification
//! — none of them can forget one. Every node failure the engine records
//! goes through the `fail*` family here, so `node_failed` is written in
//! one place and a new field on the event is a change to one function
//! rather than to every kind of node.

use std::path::{Path, PathBuf};

use yunta_core::events::{
    EngineFact, EventPayload, Failure, NodeFailedPayload, NodeFinishedPayload, NodeOutcome,
    QuestionsAskedPayload, TokenUsage,
};
use yunta_core::protected::{Protection, Trespass};
use yunta_core::{Node, RunId};

use crate::artifacts::close_artifacts;

use super::hooks_exec::after_hooks;
use super::node_artifacts::{acquire_from_child, asked, derive_findings, record_artifacts};
use super::node_exec::{render_artifact_names, NodeEnd};
use super::shared_tree::Landing;
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
    outcome: NodeOutcome,
    tokens: TokenUsage,
    staged: &'a [PathBuf],
    child: Option<ChildRun<'a>>,
}

impl<'a> Close<'a> {
    /// A close that staged nothing in the worktree — every kind but an
    /// agent session, whose adapter writes files of its own that the
    /// scope diff must leave out.
    pub(super) fn new(outcome: NodeOutcome, tokens: TokenUsage) -> Self {
        Close {
            outcome,
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
    let staged = close.staged;
    if let Some(end) = after_hooks(ctx, node, tokens, staged).await? {
        return Ok(end);
    }

    if let Some(end) = super::node_scope::scope_request(ctx, node, staged, tokens).await? {
        return Ok(end);
    }
    if let Some(end) = super::node_scope::scope_violation(ctx, node, staged, tokens).await? {
        return Ok(end);
    }
    if let Some(end) = super::shared_tree::land_unit(ctx, node, staged, tokens).await? {
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
        Err(failure) => return fail_with_staged(ctx, node, failure, tokens, staged).await,
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
    if let Some(end) = derive_findings(ctx, node, ceiling, staged, tokens).await? {
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
            Err(problem) => {
                let failure: Failure = problem.into();
                return fail_with_staged(ctx, node, failure, tokens, staged).await;
            }
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
                    let failure = Failure::artifacts(failures);
                    return fail_with_staged(ctx, node, failure, tokens, staged).await;
                }
            };
            if let Some(failure) = unexplained_plan(ctx, node, &own).await? {
                let failure = Failure::artifacts(vec![failure]);
                return fail_with_staged(ctx, node, failure, tokens, staged).await;
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
        Some(asked) => {
            // The hash the fact names is the one the run's own store
            // answers for, read back off the log rather than taken from
            // the bytes the close happened to hold: the round that
            // follows resolves the same acceptance, and one number that
            // came from two places is one that can disagree with itself.
            let crate::replay::RunView { events, state } = ctx.run_view().await?;
            let questions_hash = held_questions(ctx, &events, &node.id)?;
            // Whether the round waits is the workflow's to say, under the
            // mode the run was created in.
            let waits = asked.waits(ctx.manifest.workflow.ask(node, state.run.mode()));
            match QuestionsAskedPayload::new(questions_hash, asked.ids, tokens) {
                Some(payload) => {
                    ctx.emit(
                        Some(&node.id),
                        EventPayload::Gates(GateEvent::QuestionsAsked(payload)),
                    )
                    .await?;
                    if waits {
                        return Ok(NodeEnd::Asked);
                    }
                    // The workflow lets a round with nothing required go:
                    // each question says what the work assumes without an
                    // answer, and the run goes on with that rather than
                    // waiting for a person.
                    crate::answers::record_assumed(&ctx.log(), ctx.run_dir, &node.id)
                        .await
                        .map_err(|source| RunError::Broken {
                            diagnostic: source.to_string(),
                        })?;
                    let outcome = NodeOutcome::Fact(EngineFact::QuestionsAssumed);
                    let spent = TokenUsage::default();
                    finish_node_staged(ctx, node, outcome, spent, staged).await
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
                    finish_node_staged(ctx, node, close.outcome, tokens, staged).await
                }
            }
        }
        None => finish_node_staged(ctx, node, close.outcome, tokens, staged).await,
    }
}

/// The hash the run holds `node`'s questions document under.
///
/// Read from the log, which is the run's only answer to what it holds:
/// the round that answers these questions resolves the same acceptance
/// and compares the two, so both have to come from there.
fn held_questions(
    ctx: &RunCtx<'_>,
    events: &[yunta_core::events::StoredEvent],
    node: &NodeId,
) -> Result<ContentHash, RunError> {
    let held = crate::artifacts::RunArtifacts::of(ctx.run_dir, events);
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

/// Finishes a node whose close staged nothing in the worktree: see
/// [`finish_node_staged`].
pub(super) async fn finish_node(
    ctx: &RunCtx<'_>,
    node: &Node,
    outcome: NodeOutcome,
    tokens: TokenUsage,
) -> Result<NodeEnd, RunError> {
    finish_node_staged(ctx, node, outcome, tokens, &[]).await
}

/// The one place `node_finished` is written.
///
/// Every way a node reaches its end passes through here — the close that
/// verified what it declared, and the round that recorded the answer a
/// node was waiting on — so a node has exactly one terminal however it
/// got there. `staged` is what the adapter declared it wrote for itself,
/// which the close leaves out of what it judges.
pub(super) async fn finish_node_staged(
    ctx: &RunCtx<'_>,
    node: &Node,
    outcome: NodeOutcome,
    tokens: TokenUsage,
    staged: &[PathBuf],
) -> Result<NodeEnd, RunError> {
    let closed = Box::pin(closed(ctx, node, Landing::Finished, staged)).await?;
    // Work that reached what the project denies is not work that
    // finished: it fails, and nothing denied was committed.
    if !closed.refused.is_empty() {
        let failure = refused_failure(&closed.refused);
        return emit_failed(ctx, node, closed.failed(failure, tokens)).await;
    }
    ctx.emit(
        Some(&node.id),
        EventPayload::Node(NodeEvent::Finished(
            NodeFinishedPayload::leaving(outcome, tokens, closed.tree).committed(closed.commit),
        )),
    )
    .await?;
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
    landing: Landing,
    staged: &[PathBuf],
) -> Result<Closed, RunError> {
    let at_close = super::shared_tree::at_close(ctx, node, landing, staged).await?;
    let (commit, tree) = match at_close.captured {
        Some(captured) => (captured.commit, captured.tree),
        None => (None, super::shared_tree::recorded_tree(ctx, node).await?),
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
    refused: Vec<Trespass>,
}

impl Closed {
    /// The `node_failed` payload this close leaves for `failure`: the tree it
    /// settled, the commit it made, and the paths it refused.
    fn failed(self, failure: Failure, tokens: TokenUsage) -> NodeFailedPayload {
        let refused: Vec<PathBuf> = self
            .refused
            .iter()
            .map(|trespass| trespass.path.clone())
            .collect();
        NodeFailedPayload::new(failure, tokens)
            .leaving(self.tree)
            .committed(self.commit)
            .refusing(refused)
    }
}

/// The failure a close that refused work leaves: each refused path, with
/// the reason it is protected, aligned by index.
fn refused_failure(refused: &[Trespass]) -> Failure {
    let (paths, reasons): (Vec<PathBuf>, Vec<Protection>) = refused
        .iter()
        .map(|trespass| (trespass.path.clone(), trespass.reason))
        .unzip();
    Failure::paths_denied(paths, reasons)
}

/// The run's tree as `node` leaves it when no commit holds its work, with the
/// ignored paths the project protects forced in, through that node's index.
/// A node with a checkout of its own names the tree it landed in, not its
/// checkout. `shared_tree::recorded_tree` chooses between this and a commit's
/// tree, for every record of the node.
pub(super) async fn tree_of(ctx: &RunCtx<'_>, node: &Node) -> Result<yunta_core::TreeId, RunError> {
    let run_tree = ctx.unit.as_ref().map_or(ctx.worktree, |mine| mine.into);
    let index =
        crate::run_dir::index_for(ctx.run_dir, &crate::worktree::UnitId::Node(node.id.clone()));
    let pathspecs = ctx.protected().pathspecs();
    Ok(crate::worktree::capture_tree(run_tree, &index, &pathspecs, ctx.root_supervision()).await?)
}

/// Fails a node with its cause, kept as data. The close stages nothing:
/// see [`fail_with_tokens_staged`].
pub(super) async fn fail(
    ctx: &RunCtx<'_>,
    node: &Node,
    cause: Failure,
) -> Result<NodeEnd, RunError> {
    fail_with_tokens(ctx, node, cause, TokenUsage::default()).await
}

/// Fails a node with its cause and what it spent — a hook's exit code, a
/// budget, a runner that does not resolve. Such a failure is not missing
/// its diagnostics: it simply is not about a document. The close stages
/// nothing: see [`fail_with_tokens_staged`].
pub(super) async fn fail_with_tokens(
    ctx: &RunCtx<'_>,
    node: &Node,
    cause: Failure,
    tokens: TokenUsage,
) -> Result<NodeEnd, RunError> {
    fail_with_tokens_staged(ctx, node, cause, tokens, &[]).await
}

/// [`fail_with_tokens`] for a close that staged `staged` in the worktree,
/// which the close leaves out of what it judges.
pub(super) async fn fail_with_tokens_staged(
    ctx: &RunCtx<'_>,
    node: &Node,
    cause: Failure,
    tokens: TokenUsage,
    staged: &[PathBuf],
) -> Result<NodeEnd, RunError> {
    fail_with_staged(ctx, node, cause, tokens, staged).await
}

/// Fails a node with its typed failure: the cause the engine decided, kept
/// as data, which every surface words from the log. The close stages
/// nothing: see [`fail_with_staged`].
pub(super) async fn fail_with(
    ctx: &RunCtx<'_>,
    node: &Node,
    failure: Failure,
    tokens: TokenUsage,
) -> Result<NodeEnd, RunError> {
    fail_with_staged(ctx, node, failure, tokens, &[]).await
}

/// [`fail_with`] for a close that staged `staged` in the worktree, which
/// the close leaves out of what it judges.
pub(super) async fn fail_with_staged(
    ctx: &RunCtx<'_>,
    node: &Node,
    failure: Failure,
    tokens: TokenUsage,
    staged: &[PathBuf],
) -> Result<NodeEnd, RunError> {
    let closed = Box::pin(closed(ctx, node, Landing::Failed, staged)).await?;
    emit_failed(ctx, node, closed.failed(failure, tokens)).await
}

/// Fails a node whose session the engine asked to stop and could not stop,
/// with that failure and what the session spent. It runs no close: the session
/// may still be writing its checkout, so nothing is captured, committed or
/// judged there, and no git runs. Without git there is nothing a cancelled
/// run's token can cancel, so the failure reaches the log whether or not the
/// run was cancelled.
pub(super) async fn fail_unstopped(
    ctx: &RunCtx<'_>,
    node: &Node,
    failure: Failure,
    tokens: TokenUsage,
) -> Result<NodeEnd, RunError> {
    emit_failed(ctx, node, NodeFailedPayload::new(failure, tokens)).await
}

/// The one place `node_failed` is written. The failure reaches the log as
/// the data it is — a cause, or the declared artifacts that did not close —
/// and every surface produces its own prose from that, so none of them can
/// disagree with the facts behind it.
async fn emit_failed(
    ctx: &RunCtx<'_>,
    node: &Node,
    failed: NodeFailedPayload,
) -> Result<NodeEnd, RunError> {
    ctx.emit(
        Some(&node.id),
        EventPayload::Node(NodeEvent::Failed(failed)),
    )
    .await?;
    Ok(NodeEnd::Failed)
}

/// A tasks document `node` closes with that a gate shows a person, and
/// that does not say what it changes and why. The submission tool
/// refuses one when it is handed over; this holds one that arrived as a
/// file to the same rule.
async fn unexplained_plan(
    ctx: &RunCtx<'_>,
    node: &yunta_core::Node,
    verified: &[crate::artifacts::VerifiedArtifact],
) -> Result<Option<yunta_core::diagnostic::ArtifactFailure>, RunError> {
    let unexplained = verified.iter().find_map(|artifact| {
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
    });
    let Some(unexplained) = unexplained else {
        return Ok(None);
    };
    // Only a document with an unexplained change asks the run's state
    // whether its node is reviewed, so every other close reads no state.
    let state = ctx.run_view().await?.state;
    if !crate::tasks::plan_reviewed(&ctx.manifest.workflow, &state, &node.id) {
        return Ok(None);
    }
    Ok(Some(unexplained))
}
