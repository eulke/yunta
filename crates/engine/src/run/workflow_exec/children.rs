//! The child a node births: its frozen manifest, the tree it works in, the
//! link that records it on the parent's log, and the run directory it is
//! created in. The link is written ahead of the child, so a crash in
//! between leaves a reference the next resume supersedes.

use std::path::PathBuf;

use tokio_util::sync::CancellationToken;
use yunta_core::events::{ChildEvent, ChildRefusal, ChildRunCreatedPayload, EventPayload, Failure};
use yunta_core::{
    ConfigLayer, ContentHash, InputName, Isolation, Manifest, ModeName, Node, Redactor, RunId,
    Workflow,
};

use crate::frozen::read::ChildWorkflow;
use crate::manifest::FrozenRun;
use crate::replay::RunState;
use crate::run::BirthArtifact;

use super::super::node_close::fail;
use super::super::node_exec::cancelled_end;
use super::super::{CreateRunParams, RunCtx, RunError};
use super::hooks::Admitted;
use super::{child_refused, ended, runs_root, worktrees_root, Step, WorkflowCall};

/// What a node that births its child holds once the child exists and is
/// linked: its identity, its frozen manifest, the run directory it was created
/// in and the tree it works in.
pub(super) struct Born {
    pub(super) id: RunId,
    pub(super) manifest: Manifest,
    pub(super) run_dir: PathBuf,
    pub(super) tree: PathBuf,
}

/// The child's frozen manifest and the artifacts its birth holds: its mounts,
/// then the documents its inputs named, with the blobs they were frozen with.
struct Frozen {
    manifest: Manifest,
    artifacts: Vec<BirthArtifact>,
    blobs: Vec<(ContentHash, Vec<u8>)>,
}

/// What the birth holds once its identity and tree are settled, ahead of its
/// link being written.
struct Ready {
    id: RunId,
    manifest: Manifest,
    tree: PathBuf,
    artifacts: Vec<BirthArtifact>,
    blobs: Vec<(ContentHash, Vec<u8>)>,
    redactor: Redactor,
}

/// Births the child `admitted` describes: freezes its manifest, settles it with
/// the roots and isolation its node declares, links it on the parent's log and
/// creates its run directory. A cut or a refusal ends the node.
pub(super) async fn birth(
    ctx: &RunCtx<'_>,
    node: &Node,
    cancel: &CancellationToken,
    call: &WorkflowCall<'_>,
    admitted: Admitted<'_>,
    state: &RunState,
) -> Result<Step<Born>, RunError> {
    let mode = child_mode(&admitted.workflow);
    let frozen = match freeze(ctx, node, cancel, call.use_name, admitted, state).await? {
        Step::Proceed(frozen) => frozen,
        Step::Ended(end) => return Ok(Step::Ended(end)),
    };
    let ready = match settle(ctx, node, call, frozen).await? {
        Step::Proceed(ready) => ready,
        Step::Ended(end) => return Ok(Step::Ended(end)),
    };
    let run_dir =
        match link_and_create(ctx, node, cancel, state, mode, &ready, call.use_name).await? {
            Step::Proceed(run_dir) => run_dir,
            Step::Ended(end) => return Ok(Step::Ended(end)),
        };
    let Ready {
        id, manifest, tree, ..
    } = ready;
    Ok(Step::Proceed(Born {
        id,
        manifest,
        run_dir,
        tree,
    }))
}

/// Settles the frozen manifest with the state roots and isolation the node
/// declares, mints the child's id and takes the tree it works in. A refusal
/// ends the node.
async fn settle(
    ctx: &RunCtx<'_>,
    node: &Node,
    call: &WorkflowCall<'_>,
    frozen: Frozen,
) -> Result<Step<Ready>, RunError> {
    let Frozen {
        mut manifest,
        artifacts,
        blobs,
    } = frozen;
    // One word all the way down: what the node declares is what the
    // child's manifest freezes, with nothing in between to translate.
    manifest.isolation = call.isolation;
    manifest.paths = match yunta_core::FrozenPaths::new(runs_root(ctx), worktrees_root(ctx)) {
        Ok(paths) => Some(paths),
        Err(e) => {
            let engine_error = format!(
                "child run of `{}` cannot freeze its state roots: {e}",
                call.use_name
            );
            return ended(fail(ctx, node, Failure::EngineFailed { engine_error }).await);
        }
    };

    // A fresh id from the injected source: the link to this parent and
    // node is the `child_run_created` below, never the name.
    let id = ctx.ids.mint_run_id(ctx.clock.now());

    // Resolved before the child's tree is taken or its link is written: a
    // short declared secret refuses the birth with nothing taken or linked.
    let redactor = super::super::run_redactor(&manifest, &*ctx.secrets)?;
    let tree = match open_tree(ctx, node, &id, &manifest, call.isolation).await? {
        Step::Proceed(tree) => tree,
        Step::Ended(end) => return Ok(Step::Ended(end)),
    };
    Ok(Step::Proceed(Ready {
        id,
        manifest,
        tree,
        artifacts,
        blobs,
        redactor,
    }))
}

/// The child's frozen manifest, on the adapter its parent runs on, with the
/// artifacts its birth holds. A cut ends the node as a stop, and a birth the
/// engine refuses fails it.
async fn freeze(
    ctx: &RunCtx<'_>,
    node: &Node,
    cancel: &CancellationToken,
    use_name: &str,
    admitted: Admitted<'_>,
    state: &RunState,
) -> Result<Step<Frozen>, RunError> {
    let Admitted {
        child,
        workflow,
        provided,
        mounted,
    } = admitted;
    let config = child_config(ctx, state);
    let birth = birth_child(ctx, cancel, child, &workflow, &config, &provided, use_name).await;
    let FrozenRun {
        manifest,
        documents,
        blobs,
    } = match birth {
        // A child runs on the adapter its parent was asked to run on.
        Ok(frozen) => frozen.with_adapter(ctx.manifest.adapter.clone()),
        // A freeze the token cut ends the node the way a birth cut does
        // below: a stop is not a failed child.
        Err(e) if e.cancelled() => return ended(cancelled_end(ctx, node).await),
        // The inputs the node hands the child do not fit what it declares.
        Err(crate::manifest::ManifestError::Inputs(error)) => {
            let refusal = ChildRefusal::Inputs { problem: error };
            return ended(fail(ctx, node, child_refused(use_name, refusal)).await);
        }
        Err(e) => {
            let engine_error =
                format!("child workflow `{use_name}` cannot freeze its manifest: {e}");
            return ended(fail(ctx, node, Failure::EngineFailed { engine_error }).await);
        }
    };
    // The mounts are the child's birth artifacts, and the documents its own
    // `inputs:` named join them: both are artifacts the child holds ahead of any
    // of its nodes running.
    let mut artifacts = mounted;
    artifacts.extend(documents);
    Ok(Step::Proceed(Frozen {
        manifest,
        artifacts,
        blobs,
    }))
}

/// The configuration the child is born with: the parent's, with its frozen
/// token cap set to what the parent has left, so the parent's ceiling bounds
/// the whole tree.
fn child_config(ctx: &RunCtx<'_>, state: &RunState) -> ConfigLayer {
    // Budgets cascade: the child's frozen cap is what the parent
    // has left — auditable in the child's own manifest, and the child's
    // ordinary budget machinery enforces the parent's ceiling over the
    // whole subtree. A human's `continue` on the parent (budget already
    // lifted) leaves the child's own configured cap untouched instead
    // of freezing an unlimited child forever.
    let mut config = ctx.manifest.config.clone();
    if !ctx.budget_lifted.load(std::sync::atomic::Ordering::Relaxed) {
        if let Some(limits) = config.limits.as_mut() {
            if let Some(cap) = limits.max_tokens_per_run {
                let spent = state.spend().so_far().total();
                limits.max_tokens_per_run = Some(cap.saturating_sub(spent));
            }
        }
    }
    config
}

/// The tree a child works in: the one its node works in under `none`, or a
/// checkout of the project's pool under `worktree`. A cut ends the node, and a
/// tree that cannot be prepared fails it.
async fn open_tree(
    ctx: &RunCtx<'_>,
    node: &Node,
    id: &RunId,
    manifest: &Manifest,
    isolation: Isolation,
) -> Result<Step<PathBuf>, RunError> {
    match isolation {
        // `none` shares the tree this node works in: the engine never
        // commits, cleans up or locks a tree this run does not own.
        Isolation::None => Ok(Step::Proceed(ctx.worktree.to_path_buf())),
        // A tree of its own is a checkout of the project's pool, like the
        // parent's.
        Isolation::Worktree => {
            // Boxed: a child's tree is taken at the bottom of every composed
            // run's stack.
            let opened = Box::pin(ctx.pool.open_run(
                id,
                &manifest.base_commit,
                crate::worktree::Birth::of(&manifest.workflow),
                ctx.root_supervision(),
            ))
            .await;
            match opened {
                Ok((tree, _held)) => Ok(Step::Proceed(tree)),
                // Somebody stopped the run while its child's tree was
                // being made: that is not the node failing, it is the
                // node being cut.
                Err(e) if e.cancelled() => ended(cancelled_end(ctx, node).await),
                Err(e) => {
                    let engine_error = format!("child run `{id}` cannot prepare its worktree: {e}");
                    ended(fail(ctx, node, Failure::EngineFailed { engine_error }).await)
                }
            }
        }
    }
}

/// Writes the child's link on the parent's log, then creates its run directory.
/// The link comes first, so a crash between the two leaves a reference the next
/// resume supersedes. A cut ends the node as a stop, and a child whose frozen
/// inputs cannot be written fails it.
async fn link_and_create(
    ctx: &RunCtx<'_>,
    node: &Node,
    cancel: &CancellationToken,
    state: &RunState,
    mode: ModeName,
    ready: &Ready,
    use_name: &str,
) -> Result<Step<PathBuf>, RunError> {
    // Link first, then create: the identity pair (`child_run_id` +
    // `child_workflow_hash`) is on the parent's log before the child
    // exists, so no crash window can orphan a child the parent never
    // heard of — a link whose birth never completed has no run_created
    // and is superseded on resume.
    ctx.emit(
        Some(&node.id),
        EventPayload::Children(ChildEvent::Created(ChildRunCreatedPayload {
            child_run_id: ready.id.clone(),
            child_workflow_hash: ready.manifest.workflow_hash.clone(),
        })),
    )
    .await?;
    let runs = runs_root(ctx);
    let created = super::super::create_run(
        CreateRunParams {
            run_id: &ready.id,
            manifest: &ready.manifest,
            runs_root: &runs,
            mode: &mode,
            worktree: &ready.tree,
            promoted_from: None,
            artifacts: &ready.artifacts,
            // The lineage measures once: the child is born holding what
            // its root measured, so its comparisons see what its parent
            // did to the tree.
            baseline: crate::run::baseline::inherited(ctx.run_id, state).as_ref(),
            environment: crate::process::execution_environment(ctx.ambient),
            redactor: &ready.redactor,
            blobs: &ready.blobs,
        },
        ctx.storage,
        ctx.supervision(cancel),
    )
    .await;
    match created {
        Ok(run_dir) => Ok(Step::Proceed(run_dir)),
        // A birth the cancellation cut has no log of its own to pause:
        // it says so to the node that asked for it, which ends cut
        // like any other, and the loop's next turn pauses the run.
        Err(RunError::Cancelled) => ended(cancelled_end(ctx, node).await),
        // The child's frozen inputs could not be written: the node fails with
        // that, as a birth that cannot read its workflow does.
        Err(RunError::Frozen(e)) => {
            let engine_error = format!("child workflow `{use_name}` cannot be born: {e}");
            ended(fail(ctx, node, Failure::EngineFailed { engine_error }).await)
        }
        Err(other) => Err(other),
    }
}

/// The mode a child starts at. Same mode convention as the CLI: a child
/// declaring `modes:` starts at the floor (first declared — promotion only ever
/// escalates forward); one without runs everything.
fn child_mode(workflow: &Workflow) -> ModeName {
    workflow
        .modes
        .as_ref()
        .and_then(|modes| modes.keys().next().cloned())
        .unwrap_or_default()
}

/// Births the child a fresh node starts. A child of a run that froze its
/// inputs is born from that run's frozen set. A run that predates frozen
/// inputs has none to hand on: its child is born from the checkout as it
/// stands, and freezes what it reads as any run born now does.
async fn birth_child(
    ctx: &RunCtx<'_>,
    cancel: &CancellationToken,
    child: ChildWorkflow<'_>,
    workflow: &yunta_core::Workflow,
    config: &yunta_core::ConfigLayer,
    provided: &std::collections::HashMap<InputName, String>,
    use_name: &str,
) -> Result<crate::manifest::FrozenRun, crate::manifest::ManifestError> {
    match child {
        ChildWorkflow::Frozen { parent, .. } => {
            crate::manifest::child_run(
                workflow,
                config,
                ctx.worktree,
                provided,
                ctx.supervision(cancel),
                crate::manifest::ParentRun {
                    use_name,
                    frozen: parent,
                    run_dir: ctx.run_dir,
                },
            )
            .await
        }
        ChildWorkflow::Checkout { dir, .. } => {
            let user_knowledge = ctx.ambient.and_then(yunta_core::user_knowledge_dir);
            crate::manifest::build_manifest(
                workflow,
                config,
                &dir,
                ctx.worktree,
                provided,
                ctx.supervision(cancel),
                user_knowledge.as_deref(),
            )
            .await
        }
    }
}
