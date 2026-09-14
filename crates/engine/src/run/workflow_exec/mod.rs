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
//!   parent's HEAD; `inherit` runs the child directly in the parent's
//!   tree as manifest `isolation: none` — the child never owns (nor
//!   cleans up, nor commits) a tree that isn't its own.
//!
//! Artifacts cross that boundary in both directions, and by the log at
//! each end. [`mounts`] is the way in: what the child is born holding,
//! read out of the log of whichever run holds it. The way back is this
//! node's own close — a child that reaches `Done` hands over every
//! artifact this node declares, taken from the child's log under
//! `origin: inherited`, so a composition produces what it says it
//! produces without the parent ever reading the child's directory.

mod mounts;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tokio_util::sync::CancellationToken;
use yunta_core::events::{
    ChildRunCreatedPayload, ChildRunFinishedPayload, EventPayload, TerminalState,
};
use yunta_core::{Isolation, Manifest, MountSpec, Node, RunId, Workflow, WorkflowIsolation};

use crate::replay::derive;
use yunta_core::template::render_template;

use mounts::resolve_mounts;

use super::node_close::{close_node, fail, fail_with, ChildRun, Close};
use super::node_exec::{cancelled_end, template_vars, NodeEnd};
use super::CreateRunParams;
use super::{RunCtx, RunError, RunTerminal};
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

/// The worktrees root for child trees: the parent manifest's frozen
/// paths when present; otherwise the `runs`-sibling `worktrees`
/// directory the project layout uses — library callers (tests) without
/// frozen paths get a deterministic location next to their runs root.
fn worktrees_root(ctx: &RunCtx<'_>) -> PathBuf {
    if let Some(paths) = &ctx.manifest.paths {
        return paths.worktrees_root().to_path_buf();
    }
    let runs = runs_root(ctx);
    runs.parent()
        .map(|parent| parent.join("worktrees"))
        .unwrap_or_else(|| runs.join("worktrees"))
}

/// What a `kind: workflow` node declares: which workflow to run, what
/// to hand it, and how much of the parent's tree it sees. One value
/// because it is one clause of the node, read together and never apart.
pub(super) struct WorkflowCall<'a> {
    pub use_name: &'a str,
    pub inputs: &'a BTreeMap<String, String>,
    pub isolation: WorkflowIsolation,
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
                "creating child workflow `{use_name}` would nest {} level(s) deep but \
                 `limits.max_workflow_depth` is {max_depth} — flatten the composition or \
                 raise the limit",
                ctx.depth + 1
            ),
            false,
        )
        .await;
    }

    // Resume before create (a parent's resume recursively resumes
    // orphaned children): the last child this node created that
    // never reached child_run_finished — and actually exists (a
    // dangling reference from a crash between the parent's event and
    // the child's run_created has no events, and is superseded below).
    let events = ctx.load_events().await?;
    let state = crate::replay::derive(&events);
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
                false,
            )
            .await;
        }
    };
    let text = match std::fs::read_to_string(&resolved.path) {
        Ok(text) => text,
        Err(e) => {
            return fail(
                ctx,
                node,
                format!(
                    "child workflow `{}` cannot be read: {e}",
                    resolved.path.display()
                ),
                false,
            )
            .await;
        }
    };
    let child_workflow: Workflow = match yunta_core::yaml::parse(&text) {
        Ok(workflow) => workflow,
        Err(e) => {
            return fail(
                ctx,
                node,
                format!(
                    "child workflow `{}` does not parse: {e}",
                    resolved.path.display()
                ),
                false,
            )
            .await;
        }
    };
    // The same static gate `yunta run` applies before spending anything
    // — a child born broken is refused at birth, with the check's own
    // diagnostics.
    let check_errors = crate::check::check(&child_workflow, &ctx.manifest.config);
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
            false,
        )
        .await;
    }

    // The parent's frozen contribution: the declared inputs, rendered
    // in the parent's own template scope.
    let vars = template_vars(ctx, node);
    let mut provided: std::collections::HashMap<String, String> = Default::default();
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
                    false,
                )
                .await;
            }
        }
    }

    // Mounts resolve to bytes here, before anything is linked or
    // born — a missing source is this node's failure, with no dangling
    // child left behind.
    let mounted = match resolve_mounts(ctx, &events, mounts).await {
        Ok(mounted) => mounted,
        Err(problem) => {
            return fail_with(
                ctx,
                node,
                problem.into(),
                false,
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
                let spent = derive(&events).total_tokens().total();
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
    ) {
        Ok(frozen) => frozen,
        Err(e) => {
            return fail(
                ctx,
                node,
                format!("child workflow `{use_name}` cannot freeze its manifest: {e}"),
                false,
            )
            .await;
        }
    };
    child_manifest.isolation = match isolation {
        WorkflowIsolation::Worktree => Isolation::Worktree,
        // `inherit` shares the parent's tree: manifest `none` is the
        // honest reading — the engine never commits, cleans up or locks
        // a tree this run doesn't own.
        WorkflowIsolation::Inherit => Isolation::None,
    };
    let runs = runs_root(ctx);
    let trees = worktrees_root(ctx);
    child_manifest.paths = match yunta_core::FrozenPaths::new(runs.clone(), trees.clone()) {
        Ok(frozen) => Some(frozen),
        Err(e) => {
            return fail(
                ctx,
                node,
                format!("child run of `{use_name}` cannot freeze its state roots: {e}"),
                false,
            )
            .await;
        }
    };

    // A fresh id from the injected source: the link to this parent and
    // node is the `child_run_created` below, never the name.
    let child_id = ctx.ids.mint_run_id(ctx.clock.now());

    let child_tree = match isolation {
        WorkflowIsolation::Inherit => ctx.worktree.to_path_buf(),
        WorkflowIsolation::Worktree => {
            let tree = trees.join(child_id.as_str());
            match crate::worktree::prepare_worktree(
                ctx.worktree,
                &tree,
                &child_manifest.base_commit,
                &crate::worktree::run_branch(&child_id),
                Isolation::Worktree,
                ctx.root_supervision(),
            )
            .await
            {
                Ok(_) => tree,
                Err(e) => {
                    return fail(
                        ctx,
                        node,
                        format!("child run `{child_id}` cannot prepare its worktree: {e}"),
                        false,
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
    let child_run_dir = super::create_run(
        CreateRunParams {
            run_id: &child_id,
            manifest: &child_manifest,
            runs_root: &runs,
            mode: &child_mode,
            worktree: &child_tree,
            promoted_from: None,
            artifacts: &born,
        },
        ctx.storage,
        ctx.clock.as_ref(),
    )
    .await?;

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

/// Re-enters an existing, unfinished child run — everything it needs is
/// its own frozen truth (manifest + paths + log), never re-resolved.
async fn resume_child(
    ctx: &RunCtx<'_>,
    node: &Node,
    child_id: &RunId,
    cancel: &CancellationToken,
) -> Result<NodeEnd, RunError> {
    let child_run_dir = runs_root(ctx).join(child_id.as_str());
    let manifest_path = child_run_dir.join("manifest.yaml");
    let child_manifest: Manifest = match super::read_manifest(&manifest_path) {
        Ok(manifest) => manifest,
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
            let root = child_manifest
                .paths
                .as_ref()
                .map(|paths| paths.worktrees_root().to_path_buf())
                .unwrap_or_else(|| worktrees_root(ctx));
            let tree = root.join(child_id.as_str());
            if !tree.exists() {
                return fail(
                    ctx,
                    node,
                    format!(
                        "child run `{child_id}` cannot resume: its worktree `{}` is gone — \
                         cancel the child or restore the tree",
                        tree.display()
                    ),
                    false,
                )
                .await;
            }
            tree
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
struct Child<'a> {
    id: &'a RunId,
    manifest: &'a Manifest,
    run_dir: &'a Path,
    tree: &'a Path,
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
async fn drive_child(
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
                Box<dyn std::future::Future<Output = Result<super::RunReport, RunError>> + '_>,
            > = Box::pin(super::execute_run_at_depth(
                super::RunEnv {
                    run_id: &current_id,
                    manifest: &current_manifest,
                    run_dir: &current_run_dir,
                    worktree: &current_tree,
                    adapters: ctx.adapters,
                    storage: ctx.storage,
                    clock: ctx.clock.clone(),
                    ids: ctx.ids,
                    max_task_retries: ctx.max_task_retries,
                    human_interaction: ctx.human_interaction,
                    forge: ctx.forge,
                    cancel: Some(cancel),
                    adapter_override: ctx.adapter_override,
                    ambient: ctx.ambient,
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
                let successor = match super::promote::create_promotion_successor(
                    super::Predecessor {
                        id: &current_id,
                        manifest: &current_manifest,
                        worktree: &current_tree,
                        run_dir: &current_run_dir,
                    },
                    ctx.worktree,
                    &suggested_mode,
                    super::RunRoots {
                        runs: &runs_root(ctx),
                        worktrees: &worktrees_root(ctx),
                    },
                    super::promote::CallerInfra {
                        storage: ctx.storage,
                        clock: ctx.clock.as_ref(),
                        ids: ctx.ids,
                        supervision: ctx.root_supervision(),
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
