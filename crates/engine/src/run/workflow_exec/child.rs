//! A child run as its parent drives it: an unfinished child is re-entered,
//! and a child, its promotion successors included, is driven to its next
//! stop, which is mapped onto the node that holds it.

use std::path::{Path, PathBuf};

use tokio_util::sync::CancellationToken;
use yunta_core::events::{
    ChildEvent, ChildRunCreatedPayload, ChildRunFinishedPayload, EngineError, EngineFact,
    EngineStep, EventPayload, Failure, NodeOutcome, PauseReason, TerminalState, TokenUsage,
};
use yunta_core::{Isolation, Manifest, ModeName, Node, RunId};

use super::super::node_close::{close_node, fail, fail_with, ChildRun, Close};
use super::super::node_exec::{cancelled_end, NodeEnd};
use super::super::promote::PromotionSuccessor;
use super::super::{RunCtx, RunError, RunTerminal};
use super::{ended, runs_root, worktrees_root, Step};
use crate::replay::{RunState, RunView};

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
                resume_failure(child_id, yunta_core::describe(&error)),
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
            let child = RunView::of(child_events).state;
            let bound = child.run.checkout().map(Path::to_path_buf);
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
                Err(e) => return fail(ctx, node, resume_failure(child_id, e.to_string())).await,
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

/// The failure of a child run that could not be resumed: the step names the
/// child, and what the failing call reported is its report.
fn resume_failure(child: &RunId, report: String) -> Failure {
    Failure::EngineFailed {
        engine_error: EngineError::Step {
            step: EngineStep::ChildResume {
                child: child.clone(),
            },
            report: Some(report),
        },
    }
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

/// Whether a child's own log closed `failed`.
fn closed_failed(state: &RunState) -> bool {
    state
        .run
        .finished()
        .is_some_and(|finished| matches!(finished.terminal, TerminalState::Failed))
}

/// The node a child that closed `failed` fails: the link records the child's
/// failure with its spend, and the node's failure names the child it came
/// from. `child_failure` is the failure the child's own run holds, when it
/// holds one.
async fn child_failed(
    ctx: &RunCtx<'_>,
    node: &Node,
    (child_id, child_manifest): (&RunId, &Manifest),
    spent: TokenUsage,
    child_failure: Option<Failure>,
) -> Result<NodeEnd, RunError> {
    ctx.emit(
        Some(&node.id),
        EventPayload::Children(ChildEvent::Finished(ChildRunFinishedPayload::new(
            child_id.clone(),
            child_manifest.workflow_hash.clone(),
            TerminalState::Failed,
            spent,
        ))),
    )
    .await?;
    // The child's failure travels whole, as data, under this node's. Its
    // diagnostics are not copied up: the child is a run of its own, and its
    // log is where they are recorded. Flattening one run's evidence into
    // another's outcome is what turns three levels of composition into a
    // single unreadable line.
    let failure = Failure::ChildFailed {
        child_run: child_id.clone(),
        child_failure: child_failure.map(Box::new),
    };
    fail_with(ctx, node, failure, TokenUsage::default()).await
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
    let mut member = Member {
        id: child.id.clone(),
        manifest: child.manifest.clone(),
        run_dir: child.run_dir.to_path_buf(),
        tree: child.tree.to_path_buf(),
    };
    loop {
        let report = run_member(ctx, cancel, &member).await?;
        // Where the member works as it stopped, which a move may have
        // taken away from where it began: a successor carries on there.
        let worked_in = report.tree(&member.tree);

        match report.terminal {
            // A child re-entered after its own log closed `failed` answers as
            // finished. The failure that log holds is the one the node fails
            // with, as it would have from the child's report.
            RunTerminal::Finished if closed_failed(&report.state) => {
                let spent = report.state.metrics().tokens;
                let child_failure = report.state.last_failure();
                let child = (&member.id, &member.manifest);
                return child_failed(ctx, node, child, spent, child_failure).await;
            }
            RunTerminal::Finished => {
                let spent = report.state.metrics().tokens;
                return finished(ctx, node, &member, spent).await;
            }
            RunTerminal::Promoted { suggested_mode } => {
                let spent = report.state.metrics().tokens;
                let successor =
                    promoted(ctx, node, &member, worked_in, &suggested_mode, spent).await?;
                member = match successor {
                    Step::Proceed(successor) => successor,
                    Step::Ended(end) => return Ok(end),
                };
            }
            RunTerminal::Failed { failure, .. } => {
                // A child that closed itself as failed (`defaults.on_failure`)
                // fails the workflow node — the link records the failure with
                // its spend, and the failure names the child it came from.
                let spent = report.state.metrics().tokens;
                let child = (&member.id, &member.manifest);
                return child_failed(ctx, node, child, spent, Some(failure)).await;
            }
            RunTerminal::Paused { reason } => {
                return paused(ctx, node, cancel, &member.id, reason).await;
            }
        }
    }
}

/// The child run the loop of `drive_child` is driving: the run it is at, and
/// the frozen truth it was created with.
struct Member {
    id: RunId,
    manifest: Manifest,
    run_dir: PathBuf,
    tree: PathBuf,
}

/// Executes `member` to its next stop, one level deeper than this node's run.
async fn run_member(
    ctx: &RunCtx<'_>,
    cancel: &CancellationToken,
    member: &Member,
) -> Result<super::super::RunReport, RunError> {
    // Boxed for the indirect recursion `execute_run_at_depth` →
    // `execute_node` → here → `execute_run_at_depth`.
    let future: std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<super::super::RunReport, RunError>> + '_>,
    > = Box::pin(super::super::execute_run_at_depth(
        super::super::RunEnv {
            run_id: &member.id,
            manifest: &member.manifest,
            run_dir: &member.run_dir,
            worktree: &member.tree,
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
    future.await
}

/// The member closed `done`: its spend rides its link, and the node closes on
/// the child, which hands over every artifact the node declares.
async fn finished(
    ctx: &RunCtx<'_>,
    node: &Node,
    member: &Member,
    spent: TokenUsage,
) -> Result<NodeEnd, RunError> {
    link_finished(ctx, node, member, TerminalState::Done, spent).await?;
    // Only a child that reached `Done` hands anything over.
    // A promoted member is not the end of the composition —
    // its successor inherits everything it held and the loop
    // goes on — and a failed child fails this node, which
    // never closes: a run that abandoned its work states no
    // artifacts for a parent to take over.
    close_node(
        ctx,
        node,
        Close::new(
            NodeOutcome::Fact(EngineFact::ChildRunFinished {
                child: member.id.clone(),
            }),
            yunta_core::events::TokenUsage::default(),
        )
        .child(ChildRun {
            id: &member.id,
            run_dir: &member.run_dir,
        }),
    )
    .await
}

/// The member closed `promoted`: its link records the spend, a successor is
/// created in the tree the member worked in, and the successor becomes the
/// node's next linked child, to be driven in turn.
async fn promoted(
    ctx: &RunCtx<'_>,
    node: &Node,
    member: &Member,
    worked_in: PathBuf,
    suggested_mode: &ModeName,
    spent: TokenUsage,
) -> Result<Step<Member>, RunError> {
    // The chain member's log is closed for good
    // (`run_finished: promoted` — nothing reopens it) — the
    // link records it with its spend, and the successor
    // becomes the node's next linked child.
    link_finished(ctx, node, member, TerminalState::Promoted, spent).await?;
    let successor = match create_successor(ctx, member, &worked_in, suggested_mode).await {
        Ok(successor) => successor,
        Err(e) => {
            let engine_error = EngineError::Step {
                step: EngineStep::ChildSuccessor {
                    child: member.id.clone(),
                    mode: suggested_mode.clone(),
                },
                report: Some(e.to_string()),
            };
            return ended(fail(ctx, node, Failure::EngineFailed { engine_error }).await);
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
    Ok(Step::Proceed(Member {
        id: successor.run_id,
        manifest: successor.manifest,
        run_dir: successor.run_dir,
        tree: successor.worktree,
    }))
}

/// Creates the successor a promoted member hands over to, in the tree it worked in.
async fn create_successor(
    ctx: &RunCtx<'_>,
    member: &Member,
    worked_in: &Path,
    suggested_mode: &ModeName,
) -> Result<PromotionSuccessor, RunError> {
    super::super::promote::create_promotion_successor(
        super::super::Predecessor {
            id: &member.id,
            manifest: &member.manifest,
            worktree: worked_in,
            run_dir: &member.run_dir,
        },
        suggested_mode,
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
}

/// Records how the member ended on the node's link, with the spend it carried.
async fn link_finished(
    ctx: &RunCtx<'_>,
    node: &Node,
    member: &Member,
    terminal: TerminalState,
    spent: TokenUsage,
) -> Result<(), RunError> {
    ctx.emit(
        Some(&node.id),
        EventPayload::Children(ChildEvent::Finished(ChildRunFinishedPayload::new(
            member.id.clone(),
            member.manifest.workflow_hash.clone(),
            terminal,
            spent,
        ))),
    )
    .await?;
    Ok(())
}

/// The member paused. A pause a cancellation reached ends the node as cut; any
/// other pause keeps the node open, and pauses the parent with the child's
/// reason.
async fn paused(
    ctx: &RunCtx<'_>,
    node: &Node,
    cancel: &CancellationToken,
    child_id: &RunId,
    reason: PauseReason,
) -> Result<NodeEnd, RunError> {
    if ctx.root_cancel.is_cancelled() || cancel.is_cancelled() {
        // The child paused because a cancellation reached
        // it, not on its own account — the shared epilogue
        // decides orphan vs. join:any loss.
        return cancelled_end(ctx, node).await;
    }
    Ok(NodeEnd::Paused {
        reason: PauseReason::ChildPaused {
            node: node.id.clone(),
            run: child_id.clone(),
            child: Box::new(reason),
        },
    })
}
