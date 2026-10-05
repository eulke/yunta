//! A person's answer to a loop that ended owing scope decisions: what its
//! tasks asked to be allowed to write, which nobody was there to decide
//! while the loop ran.
//!
//! The answer is the failure's: `grant` widens each task's scope by what it
//! asked for, and the task's next attempt picks its work back up with the
//! wider scope. Any other answer denies every request — with the finding a
//! denial becomes — and the tasks go on within what they declared, however
//! the answer runs the loop again.

use yunta_core::events::{
    Decider, EventPayload, Failure, Finding, FindingEvent, FindingPostedPayload, FindingSeverity,
    OwedScope, ScopeEvent, ScopeExpansionDeniedPayload, ScopeExpansionGrantedPayload,
};
use yunta_core::{FindingId, Location, Node, RelativePath, ScopeExpansionMode, ScopeGlob, Seq};

use super::{RunCtx, RunError};
use crate::replay::NodeState;
use crate::reserved::ReservedOption;

/// Answers the scope requests `node`'s loop ended owing, when a person
/// answered that failure: every request, with that answer. Once per
/// decision: a request the log already answered after it is left alone.
pub(super) async fn answer(ctx: &RunCtx<'_>, node: &Node) -> Result<(), RunError> {
    let view = ctx.run_view().await?;
    let Some((decided_at, choice)) = view.state.choice_after_failure(&node.id) else {
        return Ok(());
    };
    let Some(NodeState::Failed {
        failure: Failure::ScopeOwed { owed },
        ..
    }) = view.state.nodes.state(&node.id)
    else {
        return Ok(());
    };
    let grant = ReservedOption::of(&choice.option) == Some(ReservedOption::Grant);
    let denied = super::denied::Denied::of(ctx).await?.every();
    let mut granted = view.state.grants.granted();
    for request in owed {
        if answered_since(&view.events, request, decided_at) {
            continue;
        }
        let decided_by = Decider::Person {
            id: choice.by.clone(),
        };
        let event = if grant {
            granted += 1;
            ScopeEvent::Granted(ScopeExpansionGrantedPayload {
                task_id: Some(request.task_id.clone()),
                decided_by,
                mode: ScopeExpansionMode::Ask,
                count_this_run: granted,
                paths: request
                    .paths
                    .iter()
                    .filter(|glob| !yunta_core::reaches_any(glob, &denied))
                    .cloned()
                    .collect(),
            })
        } else {
            ScopeEvent::Denied(ScopeExpansionDeniedPayload {
                task_id: request.task_id.clone(),
                decided_by,
                mode: ScopeExpansionMode::Ask,
                count_this_run: granted,
                denial_reason: Some(format!("{} chose `{}`", choice.by, choice.option)),
            })
        };
        ctx.emit(Some(&node.id), EventPayload::Scope(event)).await?;
        if !grant {
            post_denial(ctx, node, request, decided_at).await?;
        }
    }
    Ok(())
}

/// Whether the log answered `request` after `decided_at`.
fn answered_since(
    events: &[yunta_core::events::StoredEvent],
    request: &OwedScope,
    decided_at: Seq,
) -> bool {
    events
        .iter()
        .filter(|event| event.seq > decided_at)
        .any(|event| match event.payload() {
            Some(EventPayload::Scope(ScopeEvent::Granted(granted))) => {
                granted.task_id.as_ref() == Some(&request.task_id)
            }
            Some(EventPayload::Scope(ScopeEvent::Denied(denied))) => {
                denied.task_id == request.task_id
            }
            _ => false,
        })
}

/// The finding a denied request becomes, carrying the session's own reason.
async fn post_denial(
    ctx: &RunCtx<'_>,
    node: &Node,
    request: &OwedScope,
    decided_at: Seq,
) -> Result<(), RunError> {
    let finding = Finding {
        id: FindingId::try_from(format!("scope-owed-{}-{decided_at}", request.task_id))?,
        severity: FindingSeverity::Minor,
        title: format!("scope expansion denied for task `{}`", request.task_id),
        location: Location::work(
            RelativePath::of(request.paths.first().map(ScopeGlob::as_str)),
            None,
        ),
        detail: format!(
            "denied by a person at the gate — paths asked for: {}; agent's stated reason: {}",
            yunta_core::listed_globs(&request.paths),
            request.reason
        ),
        proposed_criterion: None,
    };
    ctx.emit(
        Some(&node.id),
        EventPayload::Findings(FindingEvent::Posted(FindingPostedPayload { finding })),
    )
    .await?;
    Ok(())
}
