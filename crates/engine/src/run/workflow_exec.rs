//! `kind: workflow` (§12, T9.3) — composition as **linked runs**: each
//! sub-workflow is a complete run (own run_id, manifest, event log and
//! run.dir), never an inline expansion. The parent freezes only the
//! child's *name and inputs*; the child resolves and freezes its own
//! workflow file at birth, so history pins the child's manifest through
//! the recorded `child_run_id` — reproducing an old parent never
//! re-resolves `name@current`.
//!
//! Mechanics this module fixes (documented in `docs/m0-status.md`'s
//! T9.3 entry):
//! - `use: <name>` resolves to `.yunta/workflows/<name>.yaml` **in the
//!   parent run's own working tree** — the repo's versioned catalog,
//!   the same one `list_workflows` reads.
//! - The child run id is `<parent>-<node>` (with a `-N` ordinal when a
//!   re-route runs the node again), derived from the parent's log —
//!   deterministic, no entropy in the engine.
//! - `child_run_created` lands on the parent's log *before* the child's
//!   own `run_created`: a crash in between leaves a dangling reference
//!   (a child with no events) that the next resume supersedes with a
//!   fresh ordinal, instead of a half-created run colliding with its
//!   own re-creation.
//! - Budgets cascade at birth: the child's frozen
//!   `limits.max_tokens_per_run` is the parent's *remaining* budget, so
//!   the parent's cap bounds the whole tree; the child's spend
//!   aggregates back up through this node's own close.
//! - `isolation: worktree` (default) branches the child's tree off the
//!   parent's HEAD; `inherit` runs the child directly in the parent's
//!   tree as manifest `isolation: none` — the child never owns (nor
//!   cleans up, nor commits) a tree that isn't its own.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tokio_util::sync::CancellationToken;
use yunta_core::events::{
    ChildRunCreatedPayload, ChildRunFinishedPayload, EventPayload, TerminalState,
};
use yunta_core::{Isolation, Manifest, Node, RunId, Workflow, WorkflowIsolation};

use crate::replay::derive;
use crate::template::render_template;

use super::node_exec::{cancelled_end, close_node, fail, template_vars, NodeEnd};
use super::{RunCtx, RunError, RunTerminal};

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
/// paths (DI-07) when present; otherwise the `runs`-sibling `worktrees`
/// directory the project layout uses — library callers (tests) without
/// frozen paths get a deterministic location next to their runs root.
fn worktrees_root(ctx: &RunCtx<'_>) -> PathBuf {
    if let Some(paths) = &ctx.manifest.paths {
        return paths.worktrees_root.clone();
    }
    let runs = runs_root(ctx);
    runs.parent()
        .map(|parent| parent.join("worktrees"))
        .unwrap_or_else(|| runs.join("worktrees"))
}

pub(super) async fn execute_workflow(
    ctx: &RunCtx<'_>,
    node: &Node,
    use_name: &str,
    inputs: &BTreeMap<String, String>,
    isolation: WorkflowIsolation,
    cancel: &CancellationToken,
) -> Result<NodeEnd, RunError> {
    // §12's "profundidad máxima configurable", enforced where the depth
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
        );
    }

    // Resume before create (§12: "yunta resume del padre retoma hijos
    // huérfanos recursivamente"): the last child this node created that
    // never reached child_run_finished — and actually exists (a
    // dangling reference from a crash between the parent's event and
    // the child's run_created has no events, and is superseded below).
    let events = ctx.load_events()?;
    let created: Vec<RunId> = events
        .iter()
        .filter(|e| e.node_id.as_ref() == Some(&node.id))
        .filter_map(|e| match &e.payload {
            EventPayload::ChildRunCreated(p) => Some(p.child_run_id.clone()),
            _ => None,
        })
        .collect();
    let finished: Vec<RunId> = events
        .iter()
        .filter(|e| e.node_id.as_ref() == Some(&node.id))
        .filter_map(|e| match &e.payload {
            EventPayload::ChildRunFinished(p) => Some(p.child_run_id.clone()),
            _ => None,
        })
        .collect();
    if let Some(open_child) = created.iter().rev().find(|child| !finished.contains(child)) {
        if !ctx.storage.events_for_run(open_child)?.is_empty() {
            return resume_child(ctx, node, open_child, cancel).await;
        }
    }

    // Fresh birth: resolve the CURRENT catalog file from the parent's
    // own tree (§12: "cada hijo resuelve y congela su propio workflow
    // al nacer").
    let catalog_path = ctx
        .worktree
        .join(".yunta/workflows")
        .join(format!("{use_name}.yaml"));
    let text = match std::fs::read_to_string(&catalog_path) {
        Ok(text) => text,
        Err(e) => {
            return fail(
                ctx,
                node,
                format!(
                    "child workflow `use: {use_name}` cannot be read from the repo catalog \
                     `{}`: {e} — add the workflow file there (versioned) or fix the name",
                    catalog_path.display()
                ),
                false,
            );
        }
    };
    let child_workflow: Workflow = match serde_yaml::from_str(&text) {
        Ok(workflow) => workflow,
        Err(e) => {
            return fail(
                ctx,
                node,
                format!(
                    "child workflow `{}` does not parse: {e}",
                    catalog_path.display()
                ),
                false,
            );
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
        );
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
                );
            }
        }
    }

    // Budgets cascade (§12): the child's frozen cap is what the parent
    // has left — auditable in the child's own manifest, and the child's
    // ordinary budget machinery enforces the parent's ceiling over the
    // whole subtree. A human's `continue` on the parent (budget already
    // lifted) leaves the child's own configured cap untouched instead
    // of freezing an unlimited child forever.
    let mut child_config = ctx.manifest.config.clone();
    if !ctx.budget_lifted.load(std::sync::atomic::Ordering::Relaxed) {
        if let Some(limits) = child_config.limits.as_mut() {
            if let Some(cap) = limits.max_tokens_per_run {
                let spent = super::budget::tokens_spent(derive(&events).total_tokens);
                limits.max_tokens_per_run = Some(cap.saturating_sub(spent));
            }
        }
    }

    let workflows_dir = ctx.worktree.join(".yunta/workflows");
    let mut child_manifest = match crate::manifest::build_manifest(
        &child_workflow,
        &child_config,
        &workflows_dir,
        ctx.worktree,
        &provided,
    ) {
        Ok(manifest) => manifest,
        Err(e) => {
            return fail(
                ctx,
                node,
                format!("child workflow `{use_name}` cannot freeze its manifest: {e}"),
                false,
            );
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
    child_manifest.paths = Some(yunta_core::FrozenPaths {
        runs_root: std::path::absolute(&runs).unwrap_or_else(|_| runs.clone()),
        worktrees_root: std::path::absolute(&trees).unwrap_or_else(|_| trees.clone()),
    });

    // Deterministic child id from the log alone: first birth is
    // `<parent>-<node>`; a re-route running the node again (or a
    // superseded dangling reference) counts up.
    let ordinal = created.len() + 1;
    let child_id = if ordinal == 1 {
        RunId::from(format!("{}-{}", ctx.run_id, node.id))
    } else {
        RunId::from(format!("{}-{}-{ordinal}", ctx.run_id, node.id))
    };

    let child_tree = match isolation {
        WorkflowIsolation::Inherit => ctx.worktree.to_path_buf(),
        WorkflowIsolation::Worktree => {
            let tree = trees.join(child_id.as_str());
            match crate::worktree::prepare_worktree(
                ctx.worktree,
                &tree,
                &child_manifest.base_commit,
                &format!("yunta/{child_id}"),
                Isolation::Worktree,
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
                    );
                }
            }
        }
    };

    // Link first, then create: the identity pair (`child_run_id` +
    // `child_workflow_hash`) is on the parent's log before the child
    // exists, so no crash window can orphan a child the parent never
    // heard of.
    ctx.emit(
        Some(&node.id),
        EventPayload::ChildRunCreated(ChildRunCreatedPayload {
            child_run_id: child_id.clone(),
            child_workflow_hash: child_manifest.workflow_hash.clone(),
        }),
    )?;

    // Same mode convention as the CLI: a child declaring `modes:`
    // starts at the floor (first declared — promotion only ever
    // escalates forward); one without runs everything.
    let child_mode = child_workflow
        .modes
        .as_ref()
        .and_then(|modes| modes.keys().next().cloned())
        .unwrap_or_else(|| "default".to_string());
    let child_run_dir = super::create_run(
        &child_id,
        &child_manifest,
        &runs,
        ctx.storage,
        ctx.clock,
        &child_mode,
        None,
    )?;

    drive_child(
        ctx,
        node,
        &child_id,
        &child_manifest,
        &child_run_dir,
        &child_tree,
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
    let child_manifest: Manifest = match std::fs::read_to_string(&manifest_path)
        .map_err(|e| e.to_string())
        .and_then(|text| serde_yaml::from_str(&text).map_err(|e| e.to_string()))
    {
        Ok(manifest) => manifest,
        Err(detail) => {
            return fail(
                ctx,
                node,
                format!(
                    "child run `{child_id}` cannot resume: its manifest `{}` cannot be \
                     read: {detail}",
                    manifest_path.display()
                ),
                false,
            );
        }
    };
    let child_tree = match child_manifest.isolation {
        Isolation::None => ctx.worktree.to_path_buf(),
        Isolation::Worktree => {
            let root = child_manifest
                .paths
                .as_ref()
                .map(|paths| paths.worktrees_root.clone())
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
                );
            }
            tree
        }
    };
    drive_child(
        ctx,
        node,
        child_id,
        &child_manifest,
        &child_run_dir,
        &child_tree,
        cancel,
    )
    .await
}

/// Executes the child run to its next stop and maps that onto this
/// node: a terminal child closes the node (its whole spend aggregating
/// up, §12); a paused child keeps the node open and pauses the parent.
async fn drive_child(
    ctx: &RunCtx<'_>,
    node: &Node,
    child_id: &RunId,
    child_manifest: &Manifest,
    child_run_dir: &Path,
    child_tree: &Path,
    cancel: &CancellationToken,
) -> Result<NodeEnd, RunError> {
    // Boxed for the indirect recursion `execute_run_at_depth` →
    // `execute_node` → here → `execute_run_at_depth`.
    let report = {
        let future: std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<super::RunReport, RunError>> + '_>,
        > = Box::pin(super::execute_run_at_depth(
            child_id,
            child_manifest,
            child_run_dir,
            child_tree,
            ctx.adapters,
            ctx.storage,
            ctx.clock,
            ctx.max_task_retries,
            ctx.human_interaction,
            ctx.forge,
            Some(cancel),
            ctx.depth + 1,
        ));
        future.await?
    };

    match report.terminal {
        RunTerminal::Finished => {
            ctx.emit(
                Some(&node.id),
                EventPayload::ChildRunFinished(ChildRunFinishedPayload {
                    child_run_id: child_id.clone(),
                    child_workflow_hash: child_manifest.workflow_hash.clone(),
                    terminal_state: TerminalState::Done,
                }),
            )?;
            close_node(
                ctx,
                node,
                format!("child run `{child_id}` finished"),
                report.state.total_tokens,
            )
            .await
        }
        RunTerminal::Promoted { suggested_mode } => {
            // The child's log is closed for good (`run_finished:
            // promoted`, I3) — recorded here so the link graph stays
            // whole, then failed explicitly: driving a child's
            // promotion chain (fresh worktree, artifact inheritance)
            // is registered debt (DI-25), not silently improvised.
            ctx.emit(
                Some(&node.id),
                EventPayload::ChildRunFinished(ChildRunFinishedPayload {
                    child_run_id: child_id.clone(),
                    child_workflow_hash: child_manifest.workflow_hash.clone(),
                    terminal_state: TerminalState::Promoted,
                }),
            )?;
            fail(
                ctx,
                node,
                format!(
                    "child run `{child_id}` closed promoted toward mode `{suggested_mode}` — \
                     the engine does not drive a child's promotion chain yet (DI-25); run the \
                     successor manually and re-route or re-run this node"
                ),
                false,
            )
        }
        RunTerminal::Paused { reason } => {
            if ctx.root_cancel.is_cancelled() || cancel.is_cancelled() {
                // The child paused because a cancellation reached it,
                // not on its own account — the shared epilogue decides
                // orphan vs. join:any loss (DI-11).
                return cancelled_end(ctx, node);
            }
            Ok(NodeEnd::ChildPaused {
                reason: format!(
                    "child run `{child_id}` paused: {reason} — resuming this run resumes it"
                ),
            })
        }
    }
}
