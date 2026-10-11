//! A child run as its parent drives it: an unfinished child is re-entered,
//! and a child, its promotion successors included, is driven to its next
//! stop, which is mapped onto the node that holds it.

use std::path::Path;

use tokio_util::sync::CancellationToken;
use yunta_core::events::{
    ChildEvent, ChildRunCreatedPayload, ChildRunFinishedPayload, EventPayload, TerminalState,
};
use yunta_core::{Isolation, Manifest, Node, RunId};

use super::super::node_close::{close_node, fail, ChildRun, Close};
use super::super::node_exec::{cancelled_end, NodeEnd};
use super::super::{RunCtx, RunError, RunTerminal};
use super::{runs_root, worktrees_root};

/// Re-enters an existing, unfinished child run — everything it needs is
/// its own frozen truth (manifest + paths + log), never re-resolved.
pub(super) async fn resume_child(
    ctx: &RunCtx<'_>,
    node: &Node,
    child_id: &RunId,
    cancel: &CancellationToken,
) -> Result<NodeEnd, RunError> {
    let child_run_dir = runs_root(ctx).join(child_id.as_str());
    let manifest_path = crate::run_dir::manifest_path(&child_run_dir);
    let child_manifest: Manifest = match super::super::read_manifest(&manifest_path).await {
        Ok(manifest) => manifest.doc,
        Err(error) => {
            return fail(
                ctx,
                node,
                format!(
                    "child run `{child_id}` cannot resume: {}",
                    yunta_core::describe(&error)
                ),
                false,
            )
            .await;
        }
    };
    let child_tree = match child_manifest.isolation {
        Isolation::None => ctx.worktree.to_path_buf(),
        Isolation::Worktree => {
            // The checkout the child's own log names; a log that names none
            // was written when a child's tree was named after it.
            let child_events = ctx.storage.events_for_run(child_id.clone()).await?;
            let bound = crate::replay::derive(&child_events)
                .run
                .checkout()
                .map(Path::to_path_buf);
            let tree = bound.unwrap_or_else(|| {
                child_manifest
                    .paths
                    .as_ref()
                    .map(|paths| paths.worktrees_root().to_path_buf())
                    .unwrap_or_else(|| worktrees_root(ctx))
                    .join(child_id.as_str())
            });
            // A checkout given back while the child was parked, or taken
            // away, is found again on the child's branch.
            match Box::pin(
                ctx.pool
                    .wake_checkout(child_id, &tree, ctx.root_supervision()),
            )
            .await
            {
                Ok(tree) => tree,
                Err(e) => {
                    let said = format!("child run `{child_id}` cannot resume: {e}");
                    return fail(ctx, node, said, false).await;
                }
            }
        }
    };
    drive_child(
        ctx,
        node,
        Child {
            id: child_id,
            manifest: &child_manifest,
            run_dir: &child_run_dir,
            tree: &child_tree,
        },
        cancel,
    )
    .await
}

/// A child run as its parent holds it: the frozen truth it was created
/// with, carried together because no caller ever has one of these
/// without the other three.
pub(super) struct Child<'a> {
    pub(super) id: &'a RunId,
    pub(super) manifest: &'a Manifest,
    pub(super) run_dir: &'a Path,
    pub(super) tree: &'a Path,
}

/// Executes the child run to its next stop and maps that onto this
/// node — chasing a promotion chain to its end: a chain
/// member that closes `promoted` gets its successor created (fresh
/// worktree off the parent's tree, artifacts inherited, `promoted_from`
/// audited) and recorded as a NEW linked child of this same node, then
/// driven in turn. A terminal child closes the node; each member's
/// whole spend rides its own `child_run_finished.tokens` (replay
/// aggregates it into the parent's total exactly once — the node's own
/// close deliberately carries none); a paused child keeps the node open
/// and pauses the parent.
pub(super) async fn drive_child(
    ctx: &RunCtx<'_>,
    node: &Node,
    child: Child<'_>,
    cancel: &CancellationToken,
) -> Result<NodeEnd, RunError> {
    let Child {
        id: child_id,
        manifest: child_manifest,
        run_dir: child_run_dir,
        tree: child_tree,
    } = child;
    let mut current_id = child_id.clone();
    let mut current_manifest = child_manifest.clone();
    let mut current_run_dir = child_run_dir.to_path_buf();
    let mut current_tree = child_tree.to_path_buf();
    loop {
        // Boxed for the indirect recursion `execute_run_at_depth` →
        // `execute_node` → here → `execute_run_at_depth`.
        let report = {
            let future: std::pin::Pin<
                Box<
                    dyn std::future::Future<Output = Result<super::super::RunReport, RunError>>
                        + '_,
                >,
            > = Box::pin(super::super::execute_run_at_depth(
                super::super::RunEnv {
                    run_id: &current_id,
                    manifest: &current_manifest,
                    run_dir: &current_run_dir,
                    worktree: &current_tree,
                    adapters: ctx.adapters,
                    storage: ctx.storage,
                    clock: ctx.clock.clone(),
                    ids: ctx.ids,
                    human_interaction: ctx.human_interaction,
                    forge: ctx.forge,
                    cancel,
                    ambient: ctx.ambient,
                    secrets: ctx.secrets.clone(),
                    // A child run's sessions ask the same judge the
                    // parent's do: the hook is the binary, not the run.
                    rail_command: ctx.rail_command.clone(),
                    // One observer serves the whole invocation, this
                    // child included: its frames name the child's own
                    // run_id, because the child emits through its own
                    // `RunCtx`.
                    observer: ctx.observer.clone(),
                },
                ctx.depth + 1,
            ));
            future.await?
        };
        // Where the member works as it stopped, which a move may have
        // taken away from where it began: a successor carries on there.
        let worked_in = report.tree(&current_tree);

        match report.terminal {
            RunTerminal::Finished => {
                ctx.emit(
                    Some(&node.id),
                    EventPayload::Children(ChildEvent::Finished(ChildRunFinishedPayload::new(
                        current_id.clone(),
                        current_manifest.workflow_hash.clone(),
                        TerminalState::Done,
                        report.state.total_tokens(),
                    ))),
                )
                .await?;
                // Only a child that reached `Done` hands anything over.
                // A promoted member is not the end of the composition —
                // its successor inherits everything it held and the loop
                // goes on — and a failed child fails this node, which
                // never closes: a run that abandoned its work states no
                // artifacts for a parent to take over.
                return close_node(
                    ctx,
                    node,
                    Close::new(
                        format!("child run `{current_id}` finished"),
                        yunta_core::events::TokenUsage::default(),
                    )
                    .child(ChildRun {
                        id: &current_id,
                        run_dir: &current_run_dir,
                    }),
                )
                .await;
            }
            RunTerminal::Promoted { suggested_mode } => {
                // The chain member's log is closed for good
                // (`run_finished: promoted` — nothing reopens it) — the
                // link records it with its spend, and the successor
                // becomes the node's next linked child.
                ctx.emit(
                    Some(&node.id),
                    EventPayload::Children(ChildEvent::Finished(ChildRunFinishedPayload::new(
                        current_id.clone(),
                        current_manifest.workflow_hash.clone(),
                        TerminalState::Promoted,
                        report.state.total_tokens(),
                    ))),
                )
                .await?;
                let successor = match super::super::promote::create_promotion_successor(
                    super::super::Predecessor {
                        id: &current_id,
                        manifest: &current_manifest,
                        worktree: &worked_in,
                        run_dir: &current_run_dir,
                    },
                    &suggested_mode,
                    super::super::RunRoots {
                        runs: &runs_root(ctx),
                        worktrees: &worktrees_root(ctx),
                    },
                    super::super::promote::CallerInfra {
                        storage: ctx.storage,
                        ids: ctx.ids,
                        supervision: ctx.root_supervision(),
                        environment: crate::process::execution_environment(ctx.ambient),
                        secrets: &*ctx.secrets,
                    },
                )
                .await
                {
                    Ok(successor) => successor,
                    Err(e) => {
                        return fail(
                            ctx,
                            node,
                            format!(
                                "child run `{current_id}` promoted toward `{suggested_mode}` \
                                 but its successor could not be created: {e}"
                            ),
                            false,
                        )
                        .await;
                    }
                };
                ctx.emit(
                    Some(&node.id),
                    EventPayload::Children(ChildEvent::Created(ChildRunCreatedPayload {
                        child_run_id: successor.run_id.clone(),
                        child_workflow_hash: successor.manifest.workflow_hash.clone(),
                    })),
                )
                .await?;
                current_id = successor.run_id;
                current_manifest = successor.manifest;
                current_run_dir = successor.run_dir;
                current_tree = successor.worktree;
            }
            RunTerminal::Failed { reason } => {
                // A child that closed itself as failed (`defaults.on_failure`)
                // fails the workflow node — the link records the failure with
                // its spend, and the diagnostic names the child.
                ctx.emit(
                    Some(&node.id),
                    EventPayload::Children(ChildEvent::Finished(ChildRunFinishedPayload::new(
                        current_id.clone(),
                        current_manifest.workflow_hash.clone(),
                        TerminalState::Failed,
                        report.state.total_tokens(),
                    ))),
                )
                .await?;
                // The child's reason keeps its own lines under this
                // one. Its diagnostics are not copied up: the child is a
                // run of its own, and its log is where they are
                // recorded. Flattening one run's evidence into another's
                // outcome is what turns three levels of composition into
                // a single unreadable line.
                return fail(
                    ctx,
                    node,
                    format!(
                        "child run `{current_id}` failed:\n  {}",
                        yunta_core::text::hanging(&reason, "  ")
                    ),
                    false,
                )
                .await;
            }
            RunTerminal::Paused { reason } => {
                if ctx.root_cancel.is_cancelled() || cancel.is_cancelled() {
                    // The child paused because a cancellation reached
                    // it, not on its own account — the shared epilogue
                    // decides orphan vs. join:any loss.
                    return cancelled_end(ctx, node).await;
                }
                return Ok(NodeEnd::ChildPaused {
                    node: node.id.clone(),
                    reason: format!(
                        "child run `{current_id}` paused, and resuming this run resumes \
                         it:\n  {}",
                        yunta_core::text::hanging(&reason, "  ")
                    ),
                });
            }
        }
    }
}
