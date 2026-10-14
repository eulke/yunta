//! `kind: executor` — the extension point when neither
//! `bash` nor `check`'s closed builtin list is enough: external code, a
//! JSON contract over stdio. The high-level shape is fixed (JSON
//! in, JSON out, exit code is the verdict); this module's own doc
//! comments carry the concrete contract it adds on top, and naming those
//! fields is open work: their names are not fixed.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use yunta_core::events::{NodeOutcome, TokenUsage};
use yunta_core::{ExecutorKind, ExecutorName, Node};

use super::node_close::{close_node, fail, fail_with, Close};
use super::node_exec::NodeEnd;
use super::{RunCtx, RunError};
use crate::frozen::read;
use crate::frozen::Source;
use crate::process::{spawn_governed, Capture, GovernedCommand, Outcome};

/// The stdin JSON shape (`with:`, the run's own paths, declared env).
/// `run` mirrors the `{{run.dir}}`/`{{run.worktree}}` template variables
/// every other node already exposes (`node_exec::template_vars`), rather
/// than inventing different names for the same two paths. `env` is
/// always present but empty for now — no per-node `env:`
/// declaration exists on `kind: executor` yet (same "not designed yet"
/// treatment `isolation: container` gets elsewhere), so there is nothing
/// to declare, but the key stays so an executor never has to branch on
/// its absence.
fn build_stdin(
    ctx: &RunCtx<'_>,
    with: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Value {
    serde_json::json!({
        "with": with,
        "run": {
            "dir": ctx.run_dir.display().to_string(),
            "worktree": ctx.worktree.display().to_string(),
        },
        "env": {},
    })
}

/// The stdout JSON shape this module defines: an optional `summary`
/// string, used as the node's outcome text when present. Exit code, not
/// this payload, is the pass/fail verdict — stdout that
/// is empty, not JSON, or JSON without `summary` never fails the node by
/// itself; it just falls back to a generic outcome string.
#[derive(serde::Deserialize)]
struct ExecutorOutput {
    summary: Option<String>,
}

/// The path `executor` runs from: the copy the run froze at birth, verified
/// against its hash before it runs. A refusal closes the node, and so does a
/// command the permission rules deny. The rules judge the path the config
/// declares, which is the name the person wrote them against; the copy is
/// what runs.
async fn executor_path(
    ctx: &RunCtx<'_>,
    node: &Node,
    executor: &ExecutorName,
    declared: &Path,
) -> Result<Result<PathBuf, NodeEnd>, RunError> {
    let path = match Source::of(ctx.manifest, ctx.run_dir, ctx.worktree, ctx.ambient)
        .executor(executor)
        .await
    {
        Ok(path) => path,
        Err(error) => {
            return Ok(Err(
                super::session_policy::close_frozen(ctx, node, error).await?
            ))
        }
    };
    // Executors are among the four command surfaces the permission model
    // covers; an executor's "command" is the binary path its config declares.
    if let Some(rule) = crate::permissions::command_violation(
        &declared.display().to_string(),
        ctx.manifest.config.permissions.as_ref(),
    ) {
        return Ok(Err(fail(ctx, node, rule).await?));
    }
    Ok(Ok(path))
}

pub(super) async fn execute_executor(
    ctx: &RunCtx<'_>,
    node: &Node,
    executor: &ExecutorName,
    with: &serde_json::Map<String, serde_json::Value>,
    timeout_seconds: Option<u64>,
    cancel: &CancellationToken,
) -> Result<NodeEnd, RunError> {
    let Some(registration) = read::registration(&ctx.manifest.config, executor) else {
        return super::check_exec::unset(
            ctx,
            node,
            yunta_core::ConfigKey::Executor {
                executor: executor.clone(),
            },
        )
        .await;
    };
    // The only variant today (`ExecutorKind::Binary`) — matched
    // explicitly rather than ignored so a future `wasm` variant
    // breaks this call site at compile time instead of silently running
    // it as a binary.
    let ExecutorKind::Binary = registration.kind;
    let declared = read::executor_path(ctx.worktree, registration);
    let path = match executor_path(ctx, node, executor, &declared).await? {
        Ok(path) => path,
        Err(end) => return Ok(end),
    };

    let stdin_bytes = match serde_json::to_vec(&build_stdin(ctx, with)) {
        Ok(bytes) => bytes,
        Err(source) => {
            return fail(
                ctx,
                node,
                format!("failed to serialize executor `{executor}`'s stdin: {source}"),
            )
            .await;
        }
    };

    let mut command = GovernedCommand::new(&path, ctx.worktree)
        .stdin(stdin_bytes)
        .stdout(Capture::Collect)
        .stderr(Capture::Collect);
    if let Some(seconds) = timeout_seconds {
        command = command.timeout(Duration::from_secs(seconds));
    }
    let outcome = spawn_governed(command, ctx.supervision(cancel)).await?;
    let (status, stdout_bytes) = match &outcome {
        Outcome::Exited { status, stdout, .. } => (status, stdout),
        Outcome::Cancelled { .. } => return super::node_exec::cancelled_end(ctx, node).await,
        Outcome::TimedOut { .. } => {
            return fail(
                ctx,
                node,
                format!(
                    "executor `{executor}` exceeded its {}s timeout",
                    timeout_seconds.unwrap_or_default()
                ),
            )
            .await;
        }
    };

    let exit_code = status.code().unwrap_or(-1);
    if exit_code == 0 {
        let summary = serde_json::from_slice::<ExecutorOutput>(stdout_bytes)
            .ok()
            .and_then(|output| output.summary)
            .unwrap_or_else(|| format!("executor `{executor}` exited 0"));
        let node_outcome = NodeOutcome::Summary(summary);
        close_node(ctx, node, Close::new(node_outcome, TokenUsage::default())).await
    } else {
        // An executor that signals only through its exit code — a probe
        // that writes nothing — is reported as that code alone.
        let origin = yunta_core::events::CommandOrigin::Executor {
            executor: executor.to_string(),
        };
        let failure = super::command_exited(
            ctx,
            &outcome,
            exit_code,
            Some(origin),
            &format!("executor `{executor}`"),
        )
        .await?;
        fail_with(ctx, node, failure, TokenUsage::default()).await
    }
}
