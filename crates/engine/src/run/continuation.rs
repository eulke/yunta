//! What a node's next attempt picks back up, and what it is told.
//!
//! A person answered the node: they granted the scope its last attempt
//! failed on, or reviewed what it handed over and sent the run back to
//! it with what should change. That answer is the one thing that changed
//! for the session that did the work, so the attempt picks that session
//! back up, told the answer, rather than starting it over on the brief.

use std::path::PathBuf;

use yunta_core::events::{EventPayload, GateResolvedPayload};
use yunta_core::{Node, NodeId, NodeKind, Seq, Workflow};

use super::{RunCtx, RunError};
use crate::replay::RunState;
use crate::reserved::ReservedOption;
use crate::task_cycle::{Answer, Continuing, Review};

/// The session a node's attempt picks back up, and the answer it is
/// told: a person's review of what the node handed over, or a grant of
/// the scope its last attempt failed on — whichever came after the
/// node last stopped. `None` for any other attempt, for a node with no
/// session to pick up, and for a node whose kind opens no session of
/// its own.
pub(super) async fn continuation(
    ctx: &RunCtx<'_>,
    node: &Node,
) -> Result<Option<Continuing>, RunError> {
    if !node.kind.opens_resumable_session() {
        return Ok(None);
    }
    let state = ctx.run_view().await?.state;
    let Some(session) = state
        .nodes
        .get(&node.id)
        .and_then(|record| record.last_session.clone())
    else {
        return Ok(None);
    };
    let answer = match review_of(&ctx.manifest.workflow, &state, &node.id) {
        Some((_, review)) => Answer::Review(review),
        None => match granted(&state, &node.id) {
            Some(answer) => answer,
            None => return Ok(None),
        },
    };
    Ok(Some(Continuing { session, answer }))
}

/// A person's review that sent the run back to `node` since it last
/// stopped, and where the node's views of what it handed over sit —
/// what a fresh session of it reads when the session that did the work
/// cannot be picked back up.
pub(super) async fn review(
    ctx: &RunCtx<'_>,
    node: &NodeId,
) -> Result<Option<(Review, Vec<PathBuf>)>, RunError> {
    let state = ctx.run_view().await?.state;
    let Some((_, review)) = review_of(&ctx.manifest.workflow, &state, node) else {
        return Ok(None);
    };
    let handed = state
        .artifacts
        .by_producer(node)
        .map(|held| {
            ctx.run_dir.join(crate::artifacts::store::view_path(
                held.producer.as_ref(),
                &held.artifact.view_name(),
            ))
        })
        .collect();
    Ok(Some((review, handed)))
}

/// The latest choice, at any gate, of an option whose `on:` sends the
/// run back to `node`, made after `node` last stopped — the review its
/// next attempt answers to. A choice from before the node last stopped
/// was already answered by the attempt that followed it.
fn review_of(workflow: &Workflow, state: &RunState, node: &NodeId) -> Option<(Seq, Review)> {
    let stopped = state
        .nodes
        .get(node)
        .and_then(|record| record.last_terminal);
    workflow
        .iter_nodes()
        .filter_map(|gate| {
            let NodeKind::Gate { on, .. } = &gate.kind else {
                return None;
            };
            let (resolution, at) = state.gates.get(&gate.id)?.resolved.last()?;
            let GateResolvedPayload::Chosen(choice) = resolution else {
                return None;
            };
            let after = stopped.is_none_or(|stopped| *at > stopped);
            (on.get(&choice.option) == Some(node) && after).then(|| {
                let review = Review {
                    gate: gate.id.clone(),
                    option: choice.option.clone(),
                    said: choice.free_text.clone().unwrap_or_default(),
                };
                (*at, review)
            })
        })
        .max_by_key(|(at, _)| *at)
}

/// The scope a person granted `node` after its last attempt failed on
/// it, when nothing has stopped the node since.
fn granted(state: &RunState, node: &NodeId) -> Option<Answer> {
    let (decided_at, choice) = state.choice_after_failure(node)?;
    let stopped = state
        .nodes
        .get(node)
        .and_then(|record| record.last_terminal);
    let grant_after = state
        .grants
        .last_granted_to_node(node)
        .is_some_and(|at| at > decided_at);
    let is_grant = ReservedOption::of(&choice.option) == Some(ReservedOption::Grant);
    let unanswered = stopped.is_none_or(|stopped| decided_at > stopped);
    (is_grant && grant_after && unanswered).then(|| {
        Answer::Scope(yunta_core::events::ScopeAnswer::Granted(
            state.grants.last_granted_to_node_paths(node).to_vec(),
        ))
    })
}

/// Says on the log that the session a node's attempt would have picked
/// back up was not: its checkout is gone, so the attempt opens fresh.
pub(super) async fn not_resumed(ctx: &RunCtx<'_>, node: &Node) -> Result<(), RunError> {
    let view = ctx.run_view().await?;
    let Some(adapter) = view
        .state
        .nodes
        .get(&node.id)
        .and_then(|record| record.runner.as_ref())
        .map(|runner| runner.chosen.adapter.clone())
    else {
        return Ok(());
    };
    ctx.emit(
        Some(&node.id),
        EventPayload::Session(yunta_core::events::SessionEvent::CapabilityDegraded(
            yunta_core::events::CapabilityDegradedPayload::new(
                yunta_core::Capability::ResumeSession,
                adapter,
                yunta_core::events::Policy::FreshSession,
            ),
        )),
    )
    .await?;
    Ok(())
}
