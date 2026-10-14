//! A node's own scope over one attempt: widened, before the attempt
//! starts, by what a person granted after its last failure; held by its
//! session's fence and tools while it runs; and at its close, the
//! request its session left and the audit of its diff — each of which
//! can fail the node with what a person needs to decide.

use std::path::PathBuf;
use std::sync::Arc;

use yunta_core::events::{Decider, EventPayload, Failure, NodeEvent, TokenUsage};
use yunta_core::expansion::{node_offer, Decision, Waiting};
use yunta_core::protected::{Closing, Protected};
use yunta_core::reserved::ReservedOption;
use yunta_core::{Node, ScopeGlob, TreeId};

use super::node_close::{fail_with_staged, fail_with_tokens_staged};
use super::node_exec::NodeEnd;
use super::scope_record::{record, Recording, Subject};
use super::shared_tree::closing_of;
use super::{RunCtx, RunError};
use crate::replay::{NodeState, RunState};
use crate::run_tools::NodeScopeAccess;
use crate::scope_expansion::{consume_request, read_request, ScopeExpansionRequest};
use crate::task_cycle::{judge, JudgeEnv, Refusals, Settle, Sitting, Work};
use crate::worktree::UnitId;

/// What a node's session works to, when the node declares a scope and is
/// not read-only: its effective scope as the log stands when the attempt
/// begins, where a check keeps its index, whether a person may widen it on
/// this run, and what the run protects from it.
pub(super) async fn session_access(
    ctx: &RunCtx<'_>,
    node: &Node,
) -> Result<Option<Arc<NodeScopeAccess>>, RunError> {
    if !node.scope.is_declared() || !node.writes_worktree() {
        return Ok(None);
    }
    let state = ctx.run_view().await?.state;
    let protected = judged_by(&ctx.protected(), &state, node);
    Ok(crate::node_scope_in_force(node, &state).map(|scope| {
        Arc::new(NodeScopeAccess {
            scope,
            index: crate::run_dir::index_for(ctx.run_dir, &UnitId::Node(node.id.clone()))
                .with_extension("check"),
            may_ask: super::schedule::person_may_grant_scope(&ctx.manifest.config),
            protected: Arc::new(protected),
            staged: Default::default(),
        })
    }))
}

/// A person chose to widen this node's scope after it failed on it: the
/// choice is decided as the menu offered it, and the outcome goes on the
/// log once, before the attempt it widens starts, so that attempt's fence
/// and its close both read it from there.
///
/// Once per decision. A restart finds the grant already after the decision,
/// or an attempt begun after it, and leaves it alone. Any other choice grants
/// nothing: widening a scope is a person's call, never the engine's.
pub(super) async fn grant_chosen_scope(ctx: &RunCtx<'_>, node: &Node) -> Result<(), RunError> {
    // A loop's owed requests are answered task by task.
    super::owed_scope::answer(ctx, node).await?;
    let view = ctx.run_view().await?;
    let state = &view.state;
    let Some((decided_at, choice)) = state.choice_after_failure(&node.id) else {
        return Ok(());
    };
    let widened = state
        .grants
        .last_granted_to_node(&node.id)
        .is_some_and(|at| at > decided_at);
    let begun = state
        .nodes
        .get(&node.id)
        .and_then(|record| record.last_started)
        .is_some_and(|at| at > decided_at);
    if ReservedOption::of(&choice.option) != Some(ReservedOption::Grant) || widened || begun {
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
    // A failure that widens nothing of the node (an owed request, answered
    // task by task above) has no node decision to record.
    if wanted.is_empty() {
        return Ok(());
    }
    let decision = match node_offer(&wanted, &ctx.protected(), widenable(ctx, node)) {
        Ok(paths) => Decision::Grant {
            paths,
            by: Decider::Person {
                id: choice.by.clone(),
            },
        },
        Err(denial) => Decision::Deny(denial),
    };
    record_node_decision(ctx, node, state, None, &decision).await
}

/// What the node changed and what of it falls outside its declared
/// `scope:`, recorded as `scope_checked`. `None` when the node owes no
/// audit — it constrains nothing, or nothing named the tree it began
/// from.
///
/// Which scope is audited — a declared one with whatever a person
/// granted it, or nothing whatsoever for a `read-only` node — is
/// [`node_scope_in_force`](crate::node_scope_in_force)'s call, not this one's.
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
    let Some(scope) = crate::node_scope_in_force(node, &view.state) else {
        return Ok(None);
    };
    let judged = judged_by(&ctx.protected(), &view.state, node);
    let closing = closing_of(node);
    let Some(from) = view.state.nodes.from_tree(&node.id).cloned() else {
        let index = crate::run_dir::index_for(ctx.run_dir, &UnitId::Node(node.id.clone()));
        return untreed_protection(ctx, &index, &judged, closing, staged)
            .await
            .map(Some);
    };
    let result = node_audit(ctx, &from, &scope, &judged, closing, staged).await?;
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

/// The node's diff since `from`, judged against `scope` as its close judges it.
/// A node has no criteria and no request here, so the judgement is the audit.
async fn node_audit(
    ctx: &RunCtx<'_>,
    from: &TreeId,
    scope: &[ScopeGlob],
    judged: &Protected,
    closing: Closing,
    staged: &[PathBuf],
) -> Result<crate::ScopeCheckResult, RunError> {
    let sitting = Sitting {
        work: Work {
            checkout: ctx.worktree,
            from,
            staged,
        },
        task: None,
        scope,
        request: None,
        refused: Refusals::None,
        window: None,
        bounds: None,
        declared: None,
        documents: None,
        env: JudgeEnv {
            memo: &ctx.memo,
            protected: judged,
            closing,
            supervision: ctx.root_supervision(),
        },
    };
    Ok(judge(sitting, Settle::Commit).await?.scope)
}

/// The judgement of an attempt whose start recorded no tree, as a log written
/// before starts recorded one does. Its scope has no starting point to be
/// audited from, so the scope is not audited and no scope check is recorded;
/// what the run protects is still judged on the diff from the commit
/// [`untreed_base`](super::shared_tree::untreed_base) names.
async fn untreed_protection(
    ctx: &RunCtx<'_>,
    index: &std::path::Path,
    judged: &Protected,
    closing: yunta_core::protected::Closing,
    staged: &[PathBuf],
) -> Result<crate::ScopeCheckResult, RunError> {
    let base = super::shared_tree::untreed_base(ctx).await?;
    Ok(crate::scope::protection_audit(
        ctx.worktree,
        base.as_str(),
        index,
        judged,
        closing,
        staged,
        ctx.root_supervision(),
    )
    .await?)
}

/// A request the node's session made for more scope, taken out of its
/// checkout before the audit — a control file is not the node's work — and
/// recorded as the session wrote it.
///
/// Where the offer holds, a person may answer it: the node fails on the
/// request, so the failure's menu puts it to them with the session's own
/// reason, and its work is not done until they answer. Where the offer
/// refuses it, the refusal is recorded with its finding, and the close goes
/// on under the scope the node has.
pub(super) async fn scope_request(
    ctx: &RunCtx<'_>,
    node: &Node,
    staged: &[PathBuf],
    tokens: TokenUsage,
) -> Result<Option<NodeEnd>, RunError> {
    if !crate::audits(node) {
        return Ok(None);
    }
    let taken = match read_request(ctx.worktree).await {
        Ok(Some(request)) => consume_request(ctx.worktree).await.map(|()| Some(request)),
        other => other,
    };
    let request = match taken {
        Ok(Some(request)) => request,
        Ok(None) => return Ok(None),
        Err(error) => {
            let failure = Failure::EngineFailed {
                engine_error: yunta_core::describe(&error),
            };
            return fail_with_tokens_staged(ctx, node, failure, tokens, staged)
                .await
                .map(Some);
        }
    };
    let decision = match node_offer(&request.paths, &ctx.protected(), widenable(ctx, node)) {
        Ok(paths) => Decision::Escalate {
            paths,
            because: Waiting::Person,
        },
        Err(denial) => Decision::Deny(denial),
    };
    let state = ctx.run_view().await?.state;
    record_node_decision(ctx, node, &state, Some(&request), &decision).await?;
    match decision {
        Decision::Escalate { paths, .. } => {
            let failure = Failure::scope_requested(paths, request.reason);
            fail_with_staged(ctx, node, failure, tokens, staged)
                .await
                .map(Some)
        }
        _ => Ok(None),
    }
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
        let failure = Failure::paths_denied(result.denied, result.denied_by);
        return Ok(Some(
            fail_with_staged(ctx, node, failure, tokens, staged).await?,
        ));
    }
    if result.violations.is_empty() {
        return Ok(None);
    }
    // A write that reached the diff where nothing but the fence could have
    // stopped it: the adapter answers for it, beside the failure the
    // violation causes either way.
    let (coverage, confinement) = ctx.last_fence(&node.id).await?;
    if let Some(breach) =
        crate::scope::fence_breach(coverage.as_ref(), confinement.as_ref(), &result)
    {
        ctx.record_breach(&node.id, &breach).await?;
    }
    let failure = Failure::scope_violated(result.violations);
    Ok(Some(
        fail_with_staged(ctx, node, failure, tokens, staged).await?,
    ))
}

/// What the run protects from `node`'s own work, as the log stands:
/// `protected`, with the instruction files a person granted the node lifted.
///
/// A grant lifts only while it still reaches nothing a person may not grant,
/// so a path the run has protected since the grant stays protected.
pub(super) fn judged_by(protected: &Protected, state: &RunState, node: &Node) -> Protected {
    let granted: Vec<ScopeGlob> = state
        .grants
        .paths_for_node(&node.id)
        .iter()
        .filter(|glob| {
            protected
                .reached_by(glob)
                .is_none_or(|reason| reason.person_may_grant())
        })
        .cloned()
        .collect();
    protected.lifting(&granted)
}

/// Whether a person may widen `node`'s scope on this run: the run's ceiling
/// lets one grant, and the node writes the worktree. The menu asks the same
/// question.
fn widenable(ctx: &RunCtx<'_>, node: &Node) -> bool {
    node.writes_worktree() && super::schedule::person_may_grant_scope(&ctx.manifest.config)
}

/// Records the engine's decision about one of the node's own scope
/// choices: the session's request when it made one, and the decision.
async fn record_node_decision(
    ctx: &RunCtx<'_>,
    node: &Node,
    state: &RunState,
    asked: Option<&ScopeExpansionRequest>,
    decision: &Decision,
) -> Result<(), RunError> {
    let attempt = state
        .nodes
        .get(&node.id)
        .map(|record| record.attempts)
        .unwrap_or_default();
    record(
        &ctx.log(),
        &node.id,
        Recording {
            subject: Subject::Node,
            mode: None,
            by: Decider::Rule,
            asked,
            decision,
            count: state.grants.granted(),
            located: false,
            precheck: None,
            attempt,
            new_request: true,
        },
    )
    .await
}
