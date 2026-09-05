//! `kind: parallel` — runs a group's children together and joins them
//! under the group's `join:` policy, interrupting the losers of `any`.

use tokio_util::sync::CancellationToken;
use yunta_core::events::TokenUsage;
use yunta_core::{JoinPolicy, Node};

use crate::replay::NodeState;

use super::node_close::{close_node, fail};
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

    let already_failed: Vec<&Node> = children
        .iter()
        .filter(|child| matches!(state.nodes.get(&child.id), Some(NodeState::Failed { .. })))
        .collect();
    // Fresh children start at attempt 1; a child left `running` with no
    // terminal event (crash, root cancel, or a paused child run under a
    // workflow node) is an orphan the group's own restart re-runs,
    // the same `restart_node` rule applied inside the group — for a
    // workflow child that re-run is what resumes its child run
    // recursively.
    let to_run: Vec<(&Node, u32)> = children
        .iter()
        .filter_map(|child| match state.nodes.get(&child.id) {
            None => Some((child, 1)),
            Some(NodeState::Running { attempt }) => Some((child, attempt + 1)),
            _ => None,
        })
        .collect();

    match join {
        JoinPolicy::All => {
            if let Some(first) = already_failed.first() {
                return fail(
                    ctx,
                    node,
                    format!("child `{}` failed under join: all", first.id),
                    false,
                )
                .await;
            }
            let results = futures::future::join_all(
                to_run
                    .iter()
                    .map(|(child, attempt)| execute_node(ctx, child, *attempt, &group_cancel)),
            )
            .await;

            let mut failed_child = None;
            let mut interrupted = false;
            let mut child_paused: Option<String> = None;
            for ((child, _), result) in to_run.iter().zip(results) {
                match result? {
                    NodeEnd::Failed => {
                        failed_child.get_or_insert(&child.id);
                    }
                    // A root cancellation unwound this child —
                    // the group closes nothing; the whole run is
                    // pausing, and resume re-enters it. Noted, not
                    // returned yet: every sibling's result was already
                    // awaited above, and dropping a sibling's recorded
                    // failure here would change nothing it wrote.
                    NodeEnd::Interrupted => interrupted = true,
                    // Same shape — the group stays open and the
                    // run pauses naming the paused child run.
                    NodeEnd::ChildPaused { reason } => {
                        child_paused.get_or_insert(reason);
                    }
                    NodeEnd::Finished => {}
                }
            }
            if interrupted {
                return Ok(NodeEnd::Interrupted);
            }
            if let Some(reason) = child_paused {
                return Ok(NodeEnd::ChildPaused { reason });
            }
            if let Some(id) = failed_child {
                return fail(
                    ctx,
                    node,
                    format!("child `{id}` failed under join: all"),
                    false,
                )
                .await;
            }
            close_node(
                ctx,
                node,
                format!("{} child(ren) finished", children.len()),
                TokenUsage::default(),
            )
            .await
        }
        JoinPolicy::Any => {
            use futures::stream::{FuturesUnordered, StreamExt};

            if let Some(already_won) = children.iter().find(|child| {
                matches!(state.nodes.get(&child.id), Some(NodeState::Finished { .. }))
            }) {
                return close_node(
                    ctx,
                    node,
                    format!("`{}` succeeded first", already_won.id),
                    TokenUsage::default(),
                )
                .await;
            }

            let mut failures: Vec<&yunta_core::NodeId> =
                already_failed.iter().map(|child| &child.id).collect();
            let mut running: FuturesUnordered<_> = to_run
                .iter()
                .map(|(child, attempt)| {
                    let cancel = group_cancel.clone();
                    async move { (&child.id, execute_node(ctx, child, *attempt, &cancel).await) }
                })
                .collect();

            let mut winner = None;
            let mut child_paused: Option<String> = None;
            while winner.is_none() {
                let Some((child_id, result)) = running.next().await else {
                    break;
                };
                match result? {
                    NodeEnd::Finished => {
                        winner = Some(child_id);
                        group_cancel.cancel();
                    }
                    NodeEnd::Failed => failures.push(child_id),
                    // Root cancellation, not a sibling race —
                    // drain the rest and unwind without a terminal. A
                    // storage error a drained sibling hits propagates
                    // (`result?`) rather than vanishing into the drain,
                    // exactly as the no-winner drain below already does.
                    NodeEnd::Interrupted => {
                        while let Some((_, result)) = running.next().await {
                            result?;
                        }
                        return Ok(NodeEnd::Interrupted);
                    }
                    // A paused child run is neither a win nor a
                    // loss — the race stays live: a sibling can still
                    // win the group. Recorded for the no-winner ending.
                    NodeEnd::ChildPaused { reason } => {
                        child_paused.get_or_insert(reason);
                    }
                }
            }
            // Drain the rest: the cancelled losers finishing their own
            // interrupt→kill sequence (each records its own failure).
            while let Some((_, result)) = running.next().await {
                let _ = result?;
            }

            match winner {
                Some(id) => {
                    // A sibling won while a workflow child's run sits
                    // paused: the group closes (that's `join: any`'s
                    // contract) and the child run stays paused on its
                    // own log — a complete run, individually resumable
                    // (`yunta resume <child>`), never silently killed.
                    close_node(
                        ctx,
                        node,
                        format!("`{id}` succeeded first"),
                        TokenUsage::default(),
                    )
                    .await
                }
                None => {
                    if let Some(reason) = child_paused {
                        // No winner and a child run waiting on its own
                        // pause: the group can't close over an open
                        // child — the run pauses and resume
                        // re-enters the race.
                        return Ok(NodeEnd::ChildPaused { reason });
                    }
                    fail(
                        ctx,
                        node,
                        format!("join: any — no child succeeded ({} failed)", failures.len()),
                        false,
                    )
                    .await
                }
            }
        }
    }
}
