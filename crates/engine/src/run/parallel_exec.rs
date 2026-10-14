//! `kind: parallel` — runs a group's children together and joins them
//! under the group's `join:` policy, interrupting the losers of `any`.

use std::future::Future;

use futures::stream::{FuturesUnordered, StreamExt};
use tokio_util::sync::CancellationToken;
use yunta_core::events::{EngineFact, Failure, NodeOutcome, PauseReason, TokenUsage};
use yunta_core::{JoinPolicy, Node, NodeId};

use crate::replay::{NodeState, RunState};

use super::node_close::{close_node, fail, Close};
use super::node_exec::{execute_node, NodeEnd};
use super::{RunCtx, RunError};

/// Runs `node`'s children: all of them concurrently, joined
/// per `join`. Re-entrant on resume: a child already terminal in the log
/// — `Finished`, or `Failed` — is never re-dispatched, and a group whose
/// winning child already finished (crash between the child's own
/// `node_finished` and the group's) closes immediately without racing
/// anyone else. See the module's resume-safety test.
pub(super) async fn execute_parallel(
    ctx: &RunCtx<'_>,
    node: &Node,
    join: JoinPolicy,
    children: &[Node],
    cancel: &CancellationToken,
) -> Result<NodeEnd, RunError> {
    let group_cancel = cancel.child_token();
    let state = ctx.run_view().await?.state;
    let (already_failed, to_run) = plan_children(ctx, children, &state).await?;
    match join {
        JoinPolicy::All => {
            join_all_children(ctx, node, children, &already_failed, &to_run, &group_cancel).await
        }
        JoinPolicy::Any => {
            join_any_children(
                ctx,
                node,
                children,
                &state,
                &already_failed,
                &to_run,
                &group_cancel,
            )
            .await
        }
    }
}

/// Settles which of a group's children run: the ones an orphan policy fails
/// here, the ones already failed, and the ones to dispatch with the attempt
/// each one is on.
async fn plan_children<'a>(
    ctx: &RunCtx<'_>,
    children: &'a [Node],
    state: &RunState,
) -> Result<(Vec<&'a Node>, Vec<(&'a Node, u32)>), RunError> {
    // An orphan of this group resolves its `on_interrupt` the same way
    // an orphan of the run does, and the one policy that changes what
    // the group may do is `fail_if_uncertain`: its work is not safe to
    // repeat, so the child is recorded failed here rather than run a
    // second time. `resume_session` needs nothing of the group — a
    // restarted child that opened a session continues it when the node
    // itself dispatches.
    let uncertain: Vec<NodeId> = super::schedule::resume_policies(
        children.iter(),
        state,
        ctx.manifest.config.resolved_on_interrupt(),
    )
    .into_iter()
    .filter(|policy| policy.on_interrupt == yunta_core::OnInterrupt::FailIfUncertain)
    .map(|policy| policy.node)
    .collect();
    for child in children
        .iter()
        .filter(|child| uncertain.contains(&child.id))
    {
        let failure = Failure::Uncertain {
            uncertain: child.id.clone(),
        };
        fail(ctx, child, failure).await?;
    }

    let already_failed: Vec<&Node> = children
        .iter()
        .filter(|child| {
            uncertain.contains(&child.id)
                || matches!(state.nodes.state(&child.id), Some(NodeState::Failed { .. }))
        })
        .collect();
    // Fresh children start at attempt 1; a child left `running` with no
    // terminal event (crash, root cancel, or a paused child run under a
    // workflow node) is an orphan the group's own restart re-runs,
    // the same `restart_node` rule applied inside the group — for a
    // workflow child that re-run is what resumes its child run
    // recursively.
    let to_run: Vec<(&Node, u32)> = children
        .iter()
        .filter(|child| !uncertain.contains(&child.id))
        .filter_map(|child| match state.nodes.state(&child.id) {
            None => Some((child, 1)),
            Some(NodeState::Running { attempt }) => Some((child, attempt + 1)),
            _ => None,
        })
        .collect();
    Ok((already_failed, to_run))
}

/// `join: all`: the group closes when every child finishes, and fails
/// naming the children that did not.
async fn join_all_children(
    ctx: &RunCtx<'_>,
    node: &Node,
    children: &[Node],
    already_failed: &[&Node],
    to_run: &[(&Node, u32)],
    group_cancel: &CancellationToken,
) -> Result<NodeEnd, RunError> {
    if !already_failed.is_empty() {
        return fail_group(ctx, node, JoinPolicy::All, node_ids(already_failed)).await;
    }
    let results = futures::future::join_all(
        to_run
            .iter()
            .map(|(child, attempt)| execute_node(ctx, child, *attempt, group_cancel)),
    )
    .await;
    let ends = settle_all(node, to_run, results)?;
    if ends.interrupted {
        return Ok(NodeEnd::Interrupted);
    }
    if let Some(reason) = ends.child_paused {
        return Ok(NodeEnd::Paused { reason });
    }
    if !ends.failed.is_empty() {
        return fail_group(ctx, node, JoinPolicy::All, ends.failed).await;
    }
    close_node(
        ctx,
        node,
        Close::new(
            NodeOutcome::Fact(EngineFact::ChildrenFinished {
                count: children.len(),
            }),
            TokenUsage::default(),
        ),
    )
    .await
}

/// How the children of a `join: all` group ended, as the group's close reads them.
#[derive(Default)]
struct ChildEnds {
    /// The children that failed, in the order they ended.
    failed: Vec<NodeId>,
    /// Whether a child ended with a root cancellation, which closes nothing.
    interrupted: bool,
    /// The first reason a child run paused for.
    child_paused: Option<PauseReason>,
}

/// Settles each child of a `join: all` group from its result, in declaration order.
fn settle_all(
    node: &Node,
    to_run: &[(&Node, u32)],
    results: Vec<Result<NodeEnd, RunError>>,
) -> Result<ChildEnds, RunError> {
    let mut ends = ChildEnds::default();
    for ((child, _), result) in to_run.iter().zip(results) {
        match result? {
            NodeEnd::Failed => ends.failed.push(child.id.clone()),
            // A root cancellation unwound this child —
            // the group closes nothing; the whole run is
            // pausing, and resume re-enters it. Noted for
            // the group's close: every sibling's result was
            // awaited above, and dropping a sibling's recorded
            // failure here would change nothing it wrote.
            NodeEnd::Interrupted => ends.interrupted = true,
            // Same shape — the group stays open and the
            // run pauses on the first paused child's reason.
            NodeEnd::Paused { reason } => {
                ends.child_paused.get_or_insert(reason);
            }
            NodeEnd::Finished => {}
            // `check` refuses a child that produces
            // `questions`, because the scheduler puts questions
            // to a person one top-level node at a time and a
            // group's child never reaches that step. Reaching
            // here means a workflow got past `check` that
            // should not have.
            NodeEnd::Asked => return Err(asked_in_group(node, &child.id)),
        }
    }
    Ok(ends)
}

/// The error of a child that handed questions over: a node inside a group is never asked.
fn asked_in_group(group: &Node, child: &NodeId) -> RunError {
    RunError::Broken {
        diagnostic: format!(
            "child `{child}` of parallel group `{}` handed questions over — a \
             node inside a group is never asked; `check` refuses this \
             workflow",
            group.id
        ),
    }
}

/// `join: any`: the first child to finish wins the group, and the rest are
/// cancelled. A group with no winner pauses on a child run waiting on its own
/// pause, or fails naming the children that failed.
async fn join_any_children(
    ctx: &RunCtx<'_>,
    node: &Node,
    children: &[Node],
    state: &RunState,
    already_failed: &[&Node],
    to_run: &[(&Node, u32)],
    group_cancel: &CancellationToken,
) -> Result<NodeEnd, RunError> {
    if let Some(already_won) = children.iter().find(|child| {
        matches!(
            state.nodes.state(&child.id),
            Some(NodeState::Finished { .. })
        )
    }) {
        return close_succeeded_first(ctx, node, already_won.id.clone()).await;
    }

    let race = race_children(ctx, node, already_failed, to_run, group_cancel).await?;
    if race.interrupted {
        return Ok(NodeEnd::Interrupted);
    }
    match race.winner {
        Some(id) => {
            // A sibling won while a workflow child's run sits
            // paused: the group closes (that's `join: any`'s
            // contract) and the child run stays paused on its
            // own log — a complete run, individually resumable
            // (`yunta resume <child>`), never silently killed.
            close_succeeded_first(ctx, node, id).await
        }
        None => {
            if let Some(reason) = race.child_paused {
                // No winner and a child run waiting on its own
                // pause: the group can't close over an open
                // child — the run pauses and resume
                // re-enters the race.
                return Ok(NodeEnd::Paused { reason });
            }
            fail_group(ctx, node, JoinPolicy::Any, race.failures).await
        }
    }
}

/// How a `join: any` race ended, as the group closes on it.
#[derive(Default)]
struct Race {
    /// The first child to finish, which closes the group.
    winner: Option<NodeId>,
    /// The children that failed: those that failed ahead of the race, then those
    /// that failed during it, in the order they ended.
    failures: Vec<NodeId>,
    /// The first reason a child run paused for, kept for a group with no winner.
    child_paused: Option<PauseReason>,
    /// Whether a child ended with a root cancellation, which closes nothing.
    interrupted: bool,
}

/// Runs the children to run as a race: a child that finishes wins, and the
/// others are cancelled. Every result is awaited ahead of the race returning, so
/// each child has recorded its own ending by then.
async fn race_children(
    ctx: &RunCtx<'_>,
    node: &Node,
    already_failed: &[&Node],
    to_run: &[(&Node, u32)],
    group_cancel: &CancellationToken,
) -> Result<Race, RunError> {
    let mut race = Race {
        failures: node_ids(already_failed),
        ..Race::default()
    };
    let mut running: FuturesUnordered<_> = to_run
        .iter()
        .map(|(child, attempt)| race_child(ctx, child, *attempt, group_cancel.clone()))
        .collect();

    while race.winner.is_none() {
        let Some((child_id, result)) = running.next().await else {
            break;
        };
        match result? {
            NodeEnd::Finished => {
                race.winner = Some(child_id.clone());
                group_cancel.cancel();
            }
            NodeEnd::Failed => race.failures.push(child_id.clone()),
            // Root cancellation, not a sibling race —
            // drain the rest and unwind without a terminal. A
            // storage error a drained sibling hits propagates
            // (`result?`) rather than vanishing into the drain,
            // exactly as the no-winner drain below already does.
            NodeEnd::Interrupted => {
                race.interrupted = true;
                break;
            }
            // A paused child run is neither a win nor a
            // loss — the race stays live: a sibling can still
            // win the group. Recorded for the no-winner ending.
            NodeEnd::Paused { reason } => {
                race.child_paused.get_or_insert(reason);
            }
            // `check` refuses a child that produces `questions`
            // (see the `join: all` arm): a node inside a group
            // is never asked.
            NodeEnd::Asked => {
                drain_children(&mut running).await?;
                return Err(asked_in_group(node, child_id));
            }
        }
    }
    // Drain the rest: the cancelled losers finishing their own
    // interrupt→kill sequence (each records its own failure).
    drain_children(&mut running).await?;
    Ok(race)
}

/// Runs one child of a race, under its own copy of the race's cancellation token.
async fn race_child<'a>(
    ctx: &RunCtx<'_>,
    child: &'a Node,
    attempt: u32,
    cancel: CancellationToken,
) -> (&'a NodeId, Result<NodeEnd, RunError>) {
    (&child.id, execute_node(ctx, child, attempt, &cancel).await)
}

/// Awaits the children left in a race, so each records its own ending, and fails on an error.
async fn drain_children<F, T>(running: &mut FuturesUnordered<F>) -> Result<(), RunError>
where
    F: Future<Output = (T, Result<NodeEnd, RunError>)>,
{
    while let Some((_, result)) = running.next().await {
        result?;
    }
    Ok(())
}

/// Closes the group on the child that finished first.
async fn close_succeeded_first(
    ctx: &RunCtx<'_>,
    node: &Node,
    winner: NodeId,
) -> Result<NodeEnd, RunError> {
    close_node(
        ctx,
        node,
        Close::new(
            NodeOutcome::Fact(EngineFact::SucceededFirst { node: winner }),
            TokenUsage::default(),
        ),
    )
    .await
}

/// Fails the group, naming the children that failed under its join policy.
async fn fail_group(
    ctx: &RunCtx<'_>,
    node: &Node,
    join: JoinPolicy,
    children_failed: Vec<NodeId>,
) -> Result<NodeEnd, RunError> {
    let failure = Failure::ChildrenFailed {
        join,
        children_failed,
    };
    fail(ctx, node, failure).await
}

/// The ids of the given children, in order.
fn node_ids(children: &[&Node]) -> Vec<NodeId> {
    children.iter().map(|child| child.id.clone()).collect()
}
