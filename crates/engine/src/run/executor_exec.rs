//! `kind: executor` — the extension point when neither
//! `bash` nor `check`'s closed builtin list is enough: external code, a
//! JSON contract over stdio. The spec fixes the high-level shape (JSON
//! in, JSON out, exit code is the verdict); this module's own doc
//! comments carry the concrete contract it adds on top, and naming those
//! fields in the spec is registered debt A-12 in
//! `docs/design/deuda-consciente.md`.

use std::path::Path;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use yunta_core::events::TokenUsage;
use yunta_core::{ExecutorKind, ExecutorName, ExecutorRegistration, Node};

use super::node_exec::{close_node, fail, NodeEnd};
use super::{RunCtx, RunError};
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

fn resolve_path(worktree: &Path, registration: &ExecutorRegistration) -> std::path::PathBuf {
    if registration.path.is_absolute() {
        registration.path.clone()
    } else {
        worktree.join(&registration.path)
    }
}

pub(super) async fn execute_executor(
    ctx: &RunCtx<'_>,
    node: &Node,
    executor: &ExecutorName,
    with: &serde_json::Map<String, serde_json::Value>,
    timeout_seconds: Option<u64>,
    cancel: &CancellationToken,
) -> Result<NodeEnd, RunError> {
    let Some(registration) = ctx
        .manifest
        .config
        .skills
        .as_ref()
        .and_then(|skills| skills.executors.iter().find(|e| e.name == *executor))
    else {
        return fail(
            ctx,
            node,
            format!("executor `{executor}` needs a matching entry under `skills.executors`"),
            false,
        )
        .await;
    };
    // The only variant today (`ExecutorKind::Binary`) — matched
    // explicitly rather than ignored so a future `wasm` variant
    // breaks this call site at compile time instead of silently running
    // it as a binary.
    let ExecutorKind::Binary = registration.kind;
    let path = resolve_path(ctx.worktree, registration);

    // Executors are among the four command surfaces the permission model
    // covers; an executor's "command" is its resolved binary path.
    if let Some(rule) = crate::permissions::command_violation(
        &path.display().to_string(),
        ctx.manifest.config.permissions.as_ref(),
    ) {
        return fail(ctx, node, rule, false).await;
    }

    let stdin_bytes = match serde_json::to_vec(&build_stdin(ctx, with)) {
        Ok(bytes) => bytes,
        Err(source) => {
            return fail(
                ctx,
                node,
                format!("failed to serialize executor `{executor}`'s stdin: {source}"),
                false,
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
    let (status, stdout_bytes, stderr_bytes) =
        match spawn_governed(command, ctx.supervision(cancel)).await? {
            Outcome::Exited {
                status,
                stdout,
                stderr,
            } => (status, stdout, stderr),
            Outcome::Cancelled { .. } => return super::node_exec::cancelled_end(ctx, node).await,
            Outcome::TimedOut { .. } => {
                return fail(
                    ctx,
                    node,
                    format!(
                        "executor `{executor}` exceeded its {}s timeout",
                        timeout_seconds.unwrap_or_default()
                    ),
                    false,
                )
                .await;
            }
        };

    let exit_code = status.code().unwrap_or(-1);
    if exit_code == 0 {
        let summary = serde_json::from_slice::<ExecutorOutput>(&stdout_bytes)
            .ok()
            .and_then(|output| output.summary)
            .unwrap_or_else(|| format!("executor `{executor}` exited 0"));
        close_node(ctx, node, summary, TokenUsage::default()).await
    } else {
        let stderr_tail: String = String::from_utf8_lossy(&stderr_bytes)
            .lines()
            .rev()
            .take(20)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        fail(
            ctx,
            node,
            format!("executor `{executor}` exited {exit_code}: {stderr_tail}"),
            false,
        )
        .await
    }
}
