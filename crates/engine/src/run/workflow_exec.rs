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
use yunta_core::{
    Isolation, Manifest, MountSpec, Node, NodeKind, RunId, Workflow, WorkflowIsolation,
};

use crate::replay::derive;
use crate::template::render_template;

use super::node_exec::{cancelled_end, close_node, fail, template_vars, NodeEnd};
use super::CreateRunParams;
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
/// paths when present; otherwise the `runs`-sibling `worktrees`
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

/// Resolves every declared mount to bytes, in memory, *before*
/// the child is linked or born — a missing source fails the parent's
/// node with nothing dangling. A `kind: workflow` source resolves
/// through the recorded link (its last `child_run_finished` on this
/// log) to that child run's `artifacts/`; any other node is the
/// parent's own `run.dir/artifacts/`. Returns `(dest_name, bytes)`
/// pairs, or the diagnostic to fail the node with.
fn resolve_mounts(
    ctx: &RunCtx<'_>,
    events: &[yunta_core::events::Event],
    mounts: &[MountSpec],
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut resolved = Vec::new();
    for mount in mounts {
        let m = &mount.artifact;
        let target = ctx
            .manifest
            .workflow
            .nodes
            .iter()
            .find(|candidate| candidate.id == m.node);
        let source_dir = match target.map(|candidate| &candidate.kind) {
            Some(NodeKind::Workflow { .. }) => {
                let child = events.iter().rev().find_map(|e| match &e.payload {
                    EventPayload::ChildRunFinished(p) if e.node_id.as_ref() == Some(&m.node) => {
                        Some(p.child_run_id.clone())
                    }
                    _ => None,
                });
                match child {
                    Some(child_id) => runs_root(ctx).join(child_id.as_str()).join("artifacts"),
                    None => {
                        return Err(format!(
                            "mount `{}` from node `{}`: no linked child run of `{}` reached a \
                             terminal state in this run — did this run's mode exclude it?",
                            m.name, m.node, m.node
                        ));
                    }
                }
            }
            _ => ctx.run_dir.join("artifacts"),
        };
        let path = source_dir.join(&m.name);
        match std::fs::read(&path) {
            Ok(bytes) => {
                resolved.push((m.rename.clone().unwrap_or_else(|| m.name.clone()), bytes));
            }
            Err(e) => {
                return Err(format!(
                    "mount `{}` from node `{}`: `{}` cannot be read: {e} — the source node \
                     never produced it",
                    m.name,
                    m.node,
                    path.display()
                ));
            }
        }
    }
    Ok(resolved)
}

pub(super) async fn execute_workflow(
    ctx: &RunCtx<'_>,
    node: &Node,
    use_name: &str,
    inputs: &BTreeMap<String, String>,
    isolation: WorkflowIsolation,
    mounts: &[MountSpec],
    cancel: &CancellationToken,
) -> Result<NodeEnd, RunError> {
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
        );
    }

    // Resume before create (a parent's resume recursively resumes
    // orphaned children): the last child this node created that
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
            );
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
            );
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

    // Mounts resolve to bytes here, before anything is linked or
    // born — a missing source is this node's failure, with no dangling
    // child left behind.
    let mounted = match resolve_mounts(ctx, &events, mounts) {
        Ok(mounted) => mounted,
        Err(diagnostic) => return fail(ctx, node, diagnostic, false),
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
                let spent = super::budget::tokens_spent(derive(&events).total_tokens);
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

    // The copies land before the link — a crash here re-derives
    // the same ordinal (nothing was linked) and simply rewrites them.
    // The promotion inheritance mechanism generalized: files into the
    // child's own `artifacts/`, where its ordinary machinery (context
    // `artifact: {name}`, `{{run.dir}}` templates) already looks.
    if !mounted.is_empty() {
        let child_artifacts = runs.join(child_id.as_str()).join("artifacts");
        std::fs::create_dir_all(&child_artifacts).map_err(|source| RunError::Io {
            context: format!("create `{}`", child_artifacts.display()),
            source,
        })?;
        for (dest_name, bytes) in &mounted {
            let dest = child_artifacts.join(dest_name);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|source| RunError::Io {
                    context: format!("create `{}`", parent.display()),
                    source,
                })?;
            }
            std::fs::write(&dest, bytes).map_err(|source| RunError::Io {
                context: format!("write mounted artifact `{}`", dest.display()),
                source,
            })?;
        }
    }

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
        CreateRunParams {
            run_id: &child_id,
            manifest: &child_manifest,
            runs_root: &runs,
            mode: &child_mode,
            promoted_from: None,
        },
        ctx.storage,
        ctx.clock,
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
        .and_then(|text| yunta_core::yaml::parse(&text).map_err(|e| e.to_string()))
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
    child_id: &RunId,
    child_manifest: &Manifest,
    child_run_dir: &Path,
    child_tree: &Path,
    cancel: &CancellationToken,
) -> Result<NodeEnd, RunError> {
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
                    clock: ctx.clock,
                    max_task_retries: ctx.max_task_retries,
                    human_interaction: ctx.human_interaction,
                    forge: ctx.forge,
                    cancel: Some(cancel),
                },
                ctx.depth + 1,
            ));
            future.await?
        };

        match report.terminal {
            RunTerminal::Finished => {
                ctx.emit(
                    Some(&node.id),
                    EventPayload::ChildRunFinished(ChildRunFinishedPayload {
                        child_run_id: current_id.clone(),
                        child_workflow_hash: current_manifest.workflow_hash.clone(),
                        terminal_state: TerminalState::Done,
                        tokens: report.state.total_tokens,
                    }),
                )?;
                return close_node(
                    ctx,
                    node,
                    format!("child run `{current_id}` finished"),
                    yunta_core::events::TokenUsage::default(),
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
                    EventPayload::ChildRunFinished(ChildRunFinishedPayload {
                        child_run_id: current_id.clone(),
                        child_workflow_hash: current_manifest.workflow_hash.clone(),
                        terminal_state: TerminalState::Promoted,
                        tokens: report.state.total_tokens,
                    }),
                )?;
                let successor = match super::promote::create_promotion_successor(
                    super::Predecessor {
                        id: &current_id,
                        manifest: &current_manifest,
                        worktree: &current_tree,
                        run_dir: &current_run_dir,
                    },
                    ctx.worktree,
                    &suggested_mode,
                    &runs_root(ctx),
                    &worktrees_root(ctx),
                    ctx.storage,
                    ctx.clock,
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
                        );
                    }
                };
                ctx.emit(
                    Some(&node.id),
                    EventPayload::ChildRunCreated(ChildRunCreatedPayload {
                        child_run_id: successor.run_id.clone(),
                        child_workflow_hash: successor.manifest.workflow_hash.clone(),
                    }),
                )?;
                current_id = successor.run_id;
                current_manifest = successor.manifest;
                current_run_dir = successor.run_dir;
                current_tree = successor.worktree;
            }
            RunTerminal::Paused { reason } => {
                if ctx.root_cancel.is_cancelled() || cancel.is_cancelled() {
                    // The child paused because a cancellation reached
                    // it, not on its own account — the shared epilogue
                    // decides orphan vs. join:any loss.
                    return cancelled_end(ctx, node);
                }
                return Ok(NodeEnd::ChildPaused {
                    reason: format!(
                        "child run `{current_id}` paused: {reason} — resuming this run \
                         resumes it"
                    ),
                });
            }
        }
    }
}
