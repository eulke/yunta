//! `kind: workflow` — composition as **linked runs**: each
//! sub-workflow is a complete run (own run_id, manifest, event log and
//! run.dir), never an inline expansion. The parent freezes only the
//! child's *name and inputs*; the child resolves and freezes its own
//! workflow file at birth, so history pins the child's manifest through
//! the recorded `child_run_id` — reproducing an old parent never
//! re-resolves `name@current`.
//!
//! Mechanics this module fixes:
//! - `use: <name>` resolves via `crate::catalog::resolve_workflow`
//!   **in the parent run's own working tree**: the repo's versioned
//!   `.yunta/workflows/<name>.yaml` catalog first, a publisher's
//!   vendored packs second — the same resolver `list_workflows` and
//!   `check_workflow_refs` share.
//! - The child run id comes from the run's injected `IdSource`, like
//!   every run id; the link to this parent and node is the
//!   `child_run_created` on the parent's log, never the name.
//! - `child_run_created` lands on the parent's log *before* the child's
//!   own `run_created`: a crash in between leaves a dangling reference
//!   (a child with no events) that the next resume supersedes with a
//!   fresh birth under a fresh id, instead of a half-created run
//!   colliding with its own re-creation.
//! - Budgets cascade at birth: the child's frozen
//!   `limits.max_tokens_per_run` is the parent's *remaining* budget, so
//!   the parent's cap bounds the whole tree; the child's spend
//!   aggregates back up through this node's own close.
//! - `isolation: worktree` (default) branches the child's tree off the
//!   HEAD of the tree this node works in; `none` runs the child
//!   directly in that tree — the child never owns (nor cleans up, nor
//!   commits) a tree that isn't its own. One word at every level, so
//!   what the node declares is what the child's manifest freezes.
//!
//! Artifacts cross that boundary in both directions, and by the log at
//! each end. [`mounts`] is the way in: what the child is born holding,
//! read out of the log of whichever run holds it. The way back is this
//! node's own close — a child that reaches `Done` hands over every
//! artifact this node declares, taken from the child's log under
//! `origin: inherited`, so a composition produces what it says it
//! produces without the parent ever reading the child's directory.

mod child;
mod mounts;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tokio_util::sync::CancellationToken;
use yunta_core::events::{ChildRunCreatedPayload, EventPayload};
use yunta_core::{InputName, Isolation, MountSpec, Node};

use yunta_core::template::render_template;

use child::{drive_child, resume_child, Child};
use mounts::resolve_mounts;

use super::node_close::{fail, fail_with};
use super::node_exec::{template_vars, NodeEnd};
use super::CreateRunParams;
use super::{RunCtx, RunError};
use yunta_core::events::ChildEvent;

/// Where this parent's runs live — the parent's own run.dir sits inside
/// it, so no configuration lookup can ever disagree with where the
/// parent actually is.
fn runs_root(ctx: &RunCtx<'_>) -> PathBuf {
    ctx.run_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| ctx.run_dir.to_path_buf())
}

/// The worktrees root for child trees: the parent's own.
fn worktrees_root(ctx: &RunCtx<'_>) -> PathBuf {
    crate::run_dir::worktrees_root(ctx.manifest, ctx.run_dir)
}

/// What a `kind: workflow` node declares: which workflow to run, what
/// to hand it, and how much of the parent's tree it sees. One value
/// because it is one clause of the node, read together and never apart.
pub(super) struct WorkflowCall<'a> {
    pub use_name: &'a str,
    pub inputs: &'a BTreeMap<InputName, String>,
    pub isolation: Isolation,
    pub mounts: &'a [MountSpec],
}

pub(super) async fn execute_workflow(
    ctx: &RunCtx<'_>,
    node: &Node,
    call: WorkflowCall<'_>,
    cancel: &CancellationToken,
) -> Result<NodeEnd, RunError> {
    let WorkflowCall {
        use_name,
        inputs,
        isolation,
        mounts,
    } = call;
    // The configurable max depth, enforced where the depth
    // actually grows — `check`'s static walk covers the files as they
    // are at check time; this guard covers what the run really loads.
    let max_depth = ctx.manifest.config.resolved_max_workflow_depth();
    if ctx.depth + 1 > max_depth {
        return fail(
            ctx,
            node,
            format!(
                "creating child workflow `{use_name}` would nest {} deep but \
                 `limits.max_workflow_depth` is {max_depth} — flatten the composition or \
                 raise the limit",
                yunta_core::text::counted(ctx.depth as usize + 1, "level")
            ),
        )
        .await;
    }

    // Resume before create (a parent's resume recursively resumes
    // orphaned children): the last child this node created that
    // never reached child_run_finished — and actually exists (a
    // dangling reference from a crash between the parent's event and
    // the child's run_created has no events, and is superseded below).
    let view = ctx.run_view().await?;
    let state = &view.state;
    if let Some(open_child) = state.children.open_under(&node.id).map(|link| &link.run_id) {
        if !ctx
            .storage
            .events_for_run(open_child.clone())
            .await?
            .is_empty()
        {
            return resume_child(ctx, node, open_child, cancel).await;
        }
    }

    // Fresh birth: resolve the CURRENT catalog file from the parent's
    // own tree (each child resolves and freezes its own workflow at
    // birth) — repo catalog first, a publisher's vendored packs second.
    let resolved = match crate::catalog::resolve_workflow(ctx.worktree, use_name) {
        Ok(resolved) => resolved,
        Err(e) => {
            return fail(
                ctx,
                node,
                format!("child workflow `use: {use_name}` cannot be resolved: {e}"),
            )
            .await;
        }
    };
    let text = match tokio::fs::read_to_string(&resolved.path).await {
        Ok(text) => text,
        Err(e) => {
            return fail(
                ctx,
                node,
                format!(
                    "child workflow `{}` cannot be read: {e}",
                    resolved.path.display()
                ),
            )
            .await;
        }
    };
    let child_workflow = match yunta_core::workflow::read::read(&text, &resolved.path) {
        Ok(workflow) => workflow,
        Err(report) => {
            return fail(ctx, node, report.to_string()).await;
        }
    };
    // The same static gate `yunta run` applies before spending anything
    // — a child born broken is refused at birth, with the check's own
    // diagnostics — asked of the child as it is born: holding what this
    // node mounts into it.
    let check_errors = crate::check::check_mounted(
        &child_workflow,
        &ctx.manifest.config,
        &|adapter| {
            ctx.adapters
                .get(adapter)
                .map(|adapter| adapter.capabilities())
        },
        mounts,
    );
    if !check_errors.is_empty() {
        let listed = check_errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        return fail(
            ctx,
            node,
            format!("child workflow `{use_name}` fails check: {listed}"),
        )
        .await;
    }

    // The parent's frozen contribution: the declared inputs, rendered
    // in the parent's own template scope.
    let vars = template_vars(ctx, node);
    let mut provided: std::collections::HashMap<InputName, String> = Default::default();
    for (name, template) in inputs {
        match render_template(template, &vars) {
            Ok(value) => {
                provided.insert(name.clone(), value);
            }
            Err(e) => {
                return fail(
                    ctx,
                    node,
                    format!("child input `{name}` does not render: {e}"),
                )
                .await;
            }
        }
    }

    // Mounts resolve to bytes here, before anything is linked or
    // born — a missing source is this node's failure, with no dangling
    // child left behind.
    let mounted = match resolve_mounts(ctx, state, &view.events, mounts).await {
        Ok(mounted) => mounted,
        Err(problem) => {
            return fail_with(
                ctx,
                node,
                problem.into(),
                yunta_core::events::TokenUsage::default(),
            )
            .await
        }
    };

    // Budgets cascade: the child's frozen cap is what the parent
    // has left — auditable in the child's own manifest, and the child's
    // ordinary budget machinery enforces the parent's ceiling over the
    // whole subtree. A human's `continue` on the parent (budget already
    // lifted) leaves the child's own configured cap untouched instead
    // of freezing an unlimited child forever.
    let mut child_config = ctx.manifest.config.clone();
    if !ctx.budget_lifted.load(std::sync::atomic::Ordering::Relaxed) {
        if let Some(limits) = child_config.limits.as_mut() {
            if let Some(cap) = limits.max_tokens_per_run {
                let spent = state.spend().so_far().total();
                limits.max_tokens_per_run = Some(cap.saturating_sub(spent));
            }
        }
    }

    // A pack-sourced child's own `prompt: {file: ...}` resolves relative
    // to the pack's own directory, not the repo's `.yunta/workflows/` —
    // whichever directory `resolved.path` actually came from.
    let workflows_dir = resolved
        .path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| ctx.worktree.join(".yunta/workflows"));
    let crate::manifest::FrozenRun {
        manifest: mut child_manifest,
        documents,
    } = match crate::manifest::build_manifest(
        &child_workflow,
        &child_config,
        &workflows_dir,
        ctx.worktree,
        &provided,
        ctx.supervision(cancel),
    )
    .await
    {
        Ok(frozen) => frozen,
        // A freeze the token cut ends the node the way a birth cut does
        // below: a stop is not a failed child.
        Err(e) if e.cancelled() => return crate::run::node_exec::cancelled_end(ctx, node).await,
        Err(e) => {
            return fail(
                ctx,
                node,
                format!("child workflow `{use_name}` cannot freeze its manifest: {e}"),
            )
            .await;
        }
    };
    // One word all the way down: what the node declares is what the
    // child's manifest freezes, with nothing in between to translate.
    child_manifest.isolation = isolation;
    // A child runs on the adapter its parent was asked to run on.
    child_manifest.adapter = ctx.manifest.adapter.clone();
    let runs = runs_root(ctx);
    let trees = worktrees_root(ctx);
    child_manifest.paths = match yunta_core::FrozenPaths::new(runs.clone(), trees.clone()) {
        Ok(frozen) => Some(frozen),
        Err(e) => {
            return fail(
                ctx,
                node,
                format!("child run of `{use_name}` cannot freeze its state roots: {e}"),
            )
            .await;
        }
    };

    // A fresh id from the injected source: the link to this parent and
    // node is the `child_run_created` below, never the name.
    let child_id = ctx.ids.mint_run_id(ctx.clock.now());

    // Resolved before the child's tree is taken or its link is written: a
    // short declared secret refuses the birth with nothing taken or linked.
    let redactor = super::run_redactor(&child_manifest, &*ctx.secrets)?;
    let child_tree = match isolation {
        // `none` shares the tree this node works in: the engine never
        // commits, cleans up or locks a tree this run does not own.
        Isolation::None => ctx.worktree.to_path_buf(),
        // A tree of its own is a checkout of the project's pool, like the
        // parent's.
        Isolation::Worktree => {
            // Boxed: a child's tree is taken at the bottom of every composed
            // run's stack.
            let opened = Box::pin(ctx.pool.open_run(
                &child_id,
                &child_manifest.base_commit,
                crate::worktree::Birth::of(&child_manifest.workflow),
                ctx.root_supervision(),
            ))
            .await;
            match opened {
                Ok((tree, _held)) => tree,
                // Somebody stopped the run while its child's tree was
                // being made: that is not the node failing, it is the
                // node being cut.
                Err(e) if e.cancelled() => {
                    return crate::run::node_exec::cancelled_end(ctx, node).await;
                }
                Err(e) => {
                    return fail(
                        ctx,
                        node,
                        format!("child run `{child_id}` cannot prepare its worktree: {e}"),
                    )
                    .await;
                }
            }
        }
    };

    // Link first, then create: the identity pair (`child_run_id` +
    // `child_workflow_hash`) is on the parent's log before the child
    // exists, so no crash window can orphan a child the parent never
    // heard of — a link whose birth never completed has no run_created
    // and is superseded on resume. The mounts are the child's birth
    // artifacts: the promotion inheritance mechanism generalized, files
    // into the child's own `artifacts/`, where its ordinary machinery
    // (context `artifact: {name}`, `{{run.dir}}` templates) already
    // looks. The documents its own `inputs:` named join them: both are
    // artifacts the child holds before any of its nodes runs.
    let mut born = mounted;
    born.extend(documents);
    ctx.emit(
        Some(&node.id),
        EventPayload::Children(ChildEvent::Created(ChildRunCreatedPayload {
            child_run_id: child_id.clone(),
            child_workflow_hash: child_manifest.workflow_hash.clone(),
        })),
    )
    .await?;

    // Same mode convention as the CLI: a child declaring `modes:`
    // starts at the floor (first declared — promotion only ever
    // escalates forward); one without runs everything.
    let child_mode = child_workflow
        .modes
        .as_ref()
        .and_then(|modes| modes.keys().next().cloned())
        .unwrap_or_default();
    let child_run_dir = match super::create_run(
        CreateRunParams {
            run_id: &child_id,
            manifest: &child_manifest,
            runs_root: &runs,
            mode: &child_mode,
            worktree: &child_tree,
            promoted_from: None,
            artifacts: &born,
            // The lineage measures once: the child is born holding what
            // its root measured, so its comparisons see what its parent
            // did to the tree.
            baseline: crate::run::baseline::inherited(ctx.run_id, state).as_ref(),
            environment: crate::process::execution_environment(ctx.ambient),
            redactor: &redactor,
        },
        ctx.storage,
        ctx.supervision(cancel),
    )
    .await
    {
        Ok(run_dir) => run_dir,
        // A birth the cancellation cut has no log of its own to pause:
        // it says so to the node that asked for it, which ends cut
        // like any other, and the loop's next turn pauses the run.
        Err(RunError::Cancelled) => return crate::run::node_exec::cancelled_end(ctx, node).await,
        Err(other) => return Err(other),
    };

    drive_child(
        ctx,
        node,
        Child {
            id: &child_id,
            manifest: &child_manifest,
            run_dir: &child_run_dir,
            tree: &child_tree,
        },
        cancel,
    )
    .await
}
