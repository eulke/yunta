//! What a person's choice to widen a failed node's scope does: the grant
//! goes on the log, once, before the attempt it widens.

use yunta_core::events::{Decider, EventPayload, ScopeEvent, ScopeExpansionGrantedPayload};
use yunta_core::{Node, ScopeExpansionMode};

use super::{RunCtx, RunError};
use crate::replay::NodeState;
use crate::reserved::ReservedOption;

/// A person chose to widen this node's scope after it failed on it: the
/// grant goes on the log once, before the attempt it widens starts, so
/// that attempt's fence and its close both read it from there.
///
/// Once per decision. A restart finds the grant already after the
/// decision and leaves it alone, and any other choice — or none — grants
/// nothing: widening a scope is a person's call, never the engine's.
pub(super) async fn grant_chosen_scope(ctx: &RunCtx<'_>, node: &Node) -> Result<(), RunError> {
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
    let paths = failure.scope_wanted().map_err(|error| RunError::Broken {
        diagnostic: format!(
            "node `{}`'s grant: {}",
            node.id,
            yunta_core::describe(&error)
        ),
    })?;
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
