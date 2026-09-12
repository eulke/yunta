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
    EventPayload, Failure, HookPhase, NodeFailedPayload, NodeFinishedPayload, TokenUsage,
};
use yunta_core::{HookFailurePolicy, Node, RunId};

use crate::artifacts::close_artifacts;
use crate::scope::scope_check;

use super::hooks_exec::{effective_hooks, run_hook, HookRun};
use super::node_artifacts::{
    acquire_from_child, derive_findings, pending_questions, record_artifacts,
};
use super::node_exec::{render_artifact_names, NodeEnd};
use super::{RunCtx, RunError};

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
            HookRun::Ran(false) if step.on_failure == HookFailurePolicy::Fail => {
                return fail_with_tokens(
                    ctx,
                    node,
                    format!("after hook `{}` failed", step.run),
                    false,
                    tokens,
                )
                .await;
            }
            HookRun::Ran(_) => {}
        }
    }

    if let Some(end) = scope_violation(ctx, node, close.staged, tokens).await? {
        return Ok(end);
    }

    // An opaque artifact's name can carry a template
    // (`report-{{runner.role}}.md`) — rendered per node so every fan-out
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
    // over from that log instead of asking its own.
    let verified = match close.child {
        Some(child) => match acquire_from_child(ctx, node, child).await? {
            Ok(acquired) => acquired,
            Err(problem) => return fail_with(ctx, node, problem.into(), false, tokens).await,
        },
        None => match close_artifacts(node, ctx.run_dir, &ctx.load_events().await?, ceiling) {
            Ok(verified) => verified,
            Err(failures) => {
                return fail_with(ctx, node, Failure::artifacts(failures), false, tokens).await
            }
        },
    };

    record_artifacts(ctx, node, &verified).await?;
    let pending = pending_questions(&verified);
    if !pending.is_empty() {
        return fail_with_tokens(
            ctx,
            node,
            format!(
                "node `{}` asked {} question(s) awaiting an answer: {}",
                node.id,
                pending.len(),
                pending.join(", ")
            ),
            false,
            tokens,
        )
        .await;
    }

    ctx.emit(
        Some(&node.id),
        EventPayload::NodeFinished(NodeFinishedPayload {
            outcome: close.outcome,
            tokens_used: tokens,
        }),
    )
    .await?;
    write_progress(ctx).await?;
    Ok(NodeEnd::Finished)
}

/// The node's whole diff against its declared `scope:`, audited as
/// `scope_checked` and failing the node when anything falls outside.
/// `staged` is what the adapter declared it wrote for itself, which is
/// not the node's doing and so is not the node's diff.
///
/// `None` when the node owes no audit at all, or when its diff is
/// inside what it may touch. Which scope that is — a declared one, or
/// nothing whatsoever for a `read-only` node — is
/// [`audited_scope`](crate::audited_scope)'s call, not this one's.
async fn scope_violation(
    ctx: &RunCtx<'_>,
    node: &Node,
    staged: &[PathBuf],
    tokens: TokenUsage,
) -> Result<Option<NodeEnd>, RunError> {
    let Some(scope) = crate::audited_scope(node) else {
        return Ok(None);
    };
    let result = scope_check(ctx.worktree, scope, staged).await?;
    ctx.emit(
        Some(&node.id),
        EventPayload::ScopeChecked(yunta_core::events::ScopeCheckedPayload {
            task_id: None,
            diff: result.diff.clone(),
            violations: result.violations.clone(),
        }),
    )
    .await?;
    if result.violations.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        fail_with_tokens(
            ctx,
            node,
            format!(
                "scope violated: {} file(s) outside the declared globs",
                result.violations.len()
            ),
            false,
            tokens,
        )
        .await?,
    ))
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
/// call, right after the `node_finished` that triggers it (`node_failed`
/// is not itself a regeneration point).
pub(super) async fn write_progress(ctx: &RunCtx<'_>) -> Result<(), RunError> {
    let events = ctx.load_events().await?;
    let markdown = crate::progress::render_progress(&ctx.manifest.workflow, &events);
    std::fs::write(ctx.run_dir.join("progress.md"), markdown).map_err(|source| RunError::Io {
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
    ctx.emit(
        Some(&node.id),
        EventPayload::NodeFailed(NodeFailedPayload::new(failure, retryable, tokens)),
    )
    .await?;
    Ok(NodeEnd::Failed)
}
