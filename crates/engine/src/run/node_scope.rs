//! A node's own scope over one attempt: widened, before the attempt
//! starts, by what a person granted after its last failure; held by its
//! session's fence and tools while it runs; and at its close, the
//! request its session left and the audit of its diff — each of which
//! can fail the node with what a person needs to decide.

use std::path::PathBuf;
use std::sync::Arc;

use yunta_core::events::{
    Decider, EventPayload, Failure, NodeEvent, ScopeEvent, ScopeExpansionGrantedPayload,
    ScopeExpansionRequestedPayload, TokenUsage,
};
use yunta_core::{Node, ScopeExpansionMode};

use super::node_close::{fail_with, fail_with_tokens};
use super::node_exec::NodeEnd;
use super::{RunCtx, RunError};
use crate::replay::NodeState;
use crate::reserved::ReservedOption;
use crate::run_tools::NodeScopeAccess;
use crate::scope::audit;
use crate::worktree::UnitId;

/// What a node's own session works to, when the node declares a scope
/// and is not read-only: its effective scope as the log stands when the
/// attempt begins, where a check keeps its index, and whether a person
/// may widen it on this run.
pub(super) async fn session_access(
    ctx: &RunCtx<'_>,
    node: &Node,
) -> Result<Option<Arc<NodeScopeAccess>>, RunError> {
    if !node.scope.is_declared() || node.permissions == Some(yunta_core::NodePermissions::ReadOnly)
    {
        return Ok(None);
    }
    let state = ctx.run_view().await?.state;
    let denied = super::denied::Denied::of(ctx).await?.every();
    Ok(crate::effective_scope(node, &state).map(|scope| {
        Arc::new(NodeScopeAccess {
            scope,
            index: crate::run_dir::index_for(ctx.run_dir, &UnitId::Node(node.id.clone()))
                .with_extension("check"),
            may_ask: super::schedule::person_may_grant_scope(&ctx.manifest.config),
            denied,
            staged: Default::default(),
        })
    }))
}

/// A person chose to widen this node's scope after it failed on it: the
/// grant goes on the log once, before the attempt it widens starts, so
/// that attempt's fence and its close both read it from there.
///
/// Once per decision. A restart finds the grant already after the
/// decision and leaves it alone, and any other choice — or none — grants
/// nothing: widening a scope is a person's call, never the engine's.
pub(super) async fn grant_chosen_scope(ctx: &RunCtx<'_>, node: &Node) -> Result<(), RunError> {
    // A loop's owed requests are answered task by task.
    super::owed_scope::answer(ctx, node).await?;
    let view = ctx.run_view().await?;
    let state = &view.state;
    let Some((decided_at, choice)) = state.choice_after_failure(&node.id) else {
        return Ok(());
    };

    let granted_since = state
        .grants
        .last_granted_to_node(&node.id)
        .is_some_and(|at| at > decided_at);
    if ReservedOption::of(&choice.option) != Some(ReservedOption::Grant) || granted_since {
        return Ok(());
    }
    let Some(NodeState::Failed { failure, .. }) = state.nodes.state(&node.id) else {
        return Ok(());
    };
    let wanted = failure.scope_wanted().map_err(|error| RunError::Broken {
        diagnostic: format!(
            "node `{}`'s grant: {}",
            node.id,
            yunta_core::describe(&error)
        ),
    })?;
    // The menu never offers a grant that reaches a denied path; a choice
    // recorded anyway widens nothing the run denies.
    let denied = super::denied::Denied::of(ctx).await?.every();
    let paths: Vec<yunta_core::ScopeGlob> = wanted
        .into_iter()
        .filter(|glob| !yunta_core::reaches_any(glob, &denied))
        .collect();
    if paths.is_empty() {
        return Ok(());
    }
    ctx.emit(
        Some(&node.id),
        EventPayload::Scope(ScopeEvent::Granted(ScopeExpansionGrantedPayload {
            task_id: None,
            decided_by: Decider::Person {
                id: choice.by.clone(),
            },
            mode: ScopeExpansionMode::Ask,
            count_this_run: state.grants.granted() + 1,
            paths,
        })),
    )
    .await?;
    Ok(())
}

/// What the node changed and what of it falls outside its declared
/// `scope:`, recorded as `scope_checked`. `None` when the node owes no
/// audit — it constrains nothing, or nothing named the tree it began
/// from.
///
/// Which scope is audited — a declared one with whatever a person
/// granted it, or nothing whatsoever for a `read-only` node — is
/// [`effective_scope`](crate::effective_scope)'s call, not this one's.
async fn audited_diff(
    ctx: &RunCtx<'_>,
    node: &Node,
    staged: &[PathBuf],
) -> Result<Option<crate::ScopeCheckResult>, RunError> {
    // The tree this attempt began from, as its own `node_started`
    // recorded it, and the grants the log holds for the node. Read back
    // from the log rather than remembered across the node's execution: a
    // crash between the start and this close must not change what the
    // node answers for.
    let view = ctx.run_view().await?;
    let Some(scope) = crate::effective_scope(node, &view.state) else {
        return Ok(None);
    };
    let Some(from) = view.state.nodes.from_tree(&node.id).cloned() else {
        // A log written before a start named its tree. Nothing to
        // compare against but the run's own base, which is what that log
        // meant, so the audit it asks for is the one it always got.
        return Ok(None);
    };
    let deny = super::denied::Denied::of(ctx).await?.closing(node);
    let ceiling = crate::scope::Ceiling {
        scope: &scope,
        deny: &deny,
    };
    let result = audit(
        ctx.worktree,
        &from,
        &crate::run_dir::index_for(ctx.run_dir, &UnitId::Node(node.id.clone())),
        ceiling,
        staged,
        ctx.root_supervision(),
    )
    .await?;
    ctx.emit(
        Some(&node.id),
        EventPayload::Node(NodeEvent::ScopeChecked(
            yunta_core::events::ScopeCheckedPayload {
                task_id: None,
                diff: result.diff.clone(),
                violations: result.violations.clone(),
                denied: result.denied.clone(),
            },
        )),
    )
    .await?;
    Ok(Some(result))
}

/// A request the node's session made for more scope, taken out of its
/// checkout before the audit — a control file is not the node's work —
/// and recorded as `scope_expansion_requested`.
///
/// Where a person may grant it, the node fails on the request, so the
/// failure's menu puts it to them with the session's own reason: its
/// work is not done until they answer. Where nobody may, the close goes
/// on under the scope the node has, and the log still says it asked.
pub(super) async fn scope_request(
    ctx: &RunCtx<'_>,
    node: &Node,
    tokens: TokenUsage,
) -> Result<Option<NodeEnd>, RunError> {
    if !crate::audits(node) {
        return Ok(None);
    }
    let request = match crate::scope_expansion::load_request(ctx.worktree).await {
        Ok(Some(request)) => request,
        Ok(None) => return Ok(None),
        Err(error) => {
            return fail_with_tokens(ctx, node, yunta_core::describe(&error), false, tokens)
                .await
                .map(Some)
        }
    };
    ctx.emit(
        Some(&node.id),
        EventPayload::Scope(ScopeEvent::Requested(ScopeExpansionRequestedPayload {
            task_id: None,
            paths: request.paths.clone(),
            reason: request.reason.clone(),
            proposed_criterion: request.proposed_criterion.map(Into::into),
            proposed_criterion_precheck: None,
        })),
    )
    .await?;
    let grantable = node.permissions != Some(yunta_core::NodePermissions::ReadOnly)
        && super::schedule::person_may_grant_scope(&ctx.manifest.config);
    // What the run denies every session is never put to a person: the
    // node answers for what it asked beside it, if anything.
    let denied = super::denied::Denied::of(ctx).await?.every();
    let paths: Vec<yunta_core::ScopeGlob> = request
        .paths
        .into_iter()
        .filter(|glob| !yunta_core::reaches_any(glob, &denied))
        .collect();
    if !grantable || paths.is_empty() {
        return Ok(None);
    }
    fail_with(
        ctx,
        node,
        Failure::scope_requested(paths, request.reason),
        false,
        tokens,
    )
    .await
    .map(Some)
}

/// The node's whole diff against its declared `scope:`, audited as
/// `scope_checked` and failing the node, naming every path that falls
/// outside. `staged` is what the adapter declared it wrote for itself, which is
/// not the node's doing and so is not the node's diff.
///
/// `None` when the node owes no audit at all, or when its diff is
/// inside what it may touch.
pub(super) async fn scope_violation(
    ctx: &RunCtx<'_>,
    node: &Node,
    staged: &[PathBuf],
    tokens: TokenUsage,
) -> Result<Option<NodeEnd>, RunError> {
    let Some(result) = audited_diff(ctx, node, staged).await? else {
        return Ok(None);
    };
    // What no session of the run may write is not the node's scope to
    // widen: the failure offers no grant, and the work never lands.
    if !result.denied.is_empty() {
        let failure = Failure::paths_denied(result.denied);
        return Ok(Some(fail_with(ctx, node, failure, false, tokens).await?));
    }
    if result.violations.is_empty() {
        return Ok(None);
    }
    // A write the adapter said it judged before it happened, and which
    // reached the diff anyway: the adapter answers for it, beside the
    // failure the violation causes either way.
    let coverage = ctx.last_coverage(&node.id).await?;
    if let Some(breach) = crate::scope::fence_breach(coverage.as_ref(), &result) {
        let adapter = ctx.resolved_adapter(&node.id).await?;
        if let Some(adapter) = adapter {
            ctx.record_breach(&node.id, &adapter, &breach).await?;
        }
    }
    Ok(Some(
        fail_with(
            ctx,
            node,
            Failure::scope_violated(result.violations),
            false,
            tokens,
        )
        .await?,
    ))
}
