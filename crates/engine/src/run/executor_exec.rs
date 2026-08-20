//! `kind: executor` (D47/D87, T5.6) — the extension point when neither
//! `bash` nor `check`'s closed builtin list is enough: external code, a
//! JSON contract over stdio. D47/D87 fix the high-level shape (JSON in,
//! JSON out, exit code is the verdict) but stop short of naming fields —
//! this module's own doc comments carry the concrete contract this
//! recorte adds on top, written up in `docs/m0-status.md`'s T5.6 entry
//! pending a real ADR revision.

use std::path::Path;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;
use yunta_core::events::TokenUsage;
use yunta_core::{ExecutorKind, ExecutorRegistration, Node};

use super::node_exec::{close_node, fail, kill_process_group, NodeEnd};
use super::{RunCtx, RunError};

/// The stdin JSON shape (D47: "`with:`, paths del run, env declarado").
/// `run` mirrors the `{{run.dir}}`/`{{run.worktree}}` template variables
/// every other node already exposes (`node_exec::template_vars`), rather
/// than inventing different names for the same two paths. `env` is
/// always present but empty in this recorte — no per-node `env:`
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

/// The stdout JSON shape this recorte defines: an optional `summary`
/// string, used as the node's outcome text when present. Exit code, not
/// this payload, is the pass/fail verdict (D47's own text) — stdout that
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

enum WaitOutcome {
    Done(std::process::ExitStatus),
    TimedOut,
    Cancelled,
}

pub(super) async fn execute_executor(
    ctx: &RunCtx<'_>,
    node: &Node,
    executor: &str,
    with: &serde_json::Map<String, serde_json::Value>,
    timeout_seconds: Option<u64>,
    cancel: &CancellationToken,
) -> Result<NodeEnd, RunError> {
    let Some(registration) = ctx
        .manifest
        .config
        .skills
        .as_ref()
        .and_then(|skills| skills.executors.iter().find(|e| e.name == executor))
    else {
        return fail(
            ctx,
            node,
            format!("executor `{executor}` needs a matching entry under `skills.executors`"),
            false,
        );
    };
    // The only variant today (`ExecutorKind::Binary`) — matched
    // explicitly rather than ignored so a future `wasm` variant (D47)
    // breaks this call site at compile time instead of silently running
    // it as a binary.
    let ExecutorKind::Binary = registration.kind;
    let path = resolve_path(ctx.worktree, registration);

    // §6.1 names executors among the four command surfaces the model
    // covers; an executor's "command" is its resolved binary path.
    if let Some(rule) = crate::permissions::command_violation(
        &path.display().to_string(),
        ctx.manifest.config.permissions.as_ref(),
    ) {
        return fail(ctx, node, rule, false);
    }

    let stdin_bytes = match serde_json::to_vec(&build_stdin(ctx, with)) {
        Ok(bytes) => bytes,
        Err(source) => {
            return fail(
                ctx,
                node,
                format!("failed to serialize executor `{executor}`'s stdin: {source}"),
                false,
            );
        }
    };

    let mut std_cmd = std::process::Command::new(&path);
    std_cmd
        .current_dir(ctx.worktree)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    // A4: an executor's whole process tree must die together on timeout
    // or cancellation, the same process-group pattern every other
    // engine-spawned command in this codebase uses.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        std_cmd.process_group(0);
    }
    let mut child = tokio::process::Command::from(std_cmd)
        .spawn()
        .map_err(|source| RunError::Io {
            context: format!("spawn executor `{executor}` for node `{}`", node.id),
            source,
        })?;
    let _pgid_registration =
        crate::process_registry::register(ctx.process_registry.as_ref(), child.id());

    let Some(mut stdin) = child.stdin.take() else {
        // Unreachable given `Stdio::piped()` above, but a typed error
        // beats a panic (CLAUDE.md: no unwrap/expect outside tests) —
        // this crate never trusts "can't happen" enough to crash on it.
        return Err(RunError::Io {
            context: format!("executor `{executor}` for node `{}`", node.id),
            source: std::io::Error::other("spawned child has no stdin pipe"),
        });
    };
    let write_task = tokio::spawn(async move {
        let _ = stdin.write_all(&stdin_bytes).await;
        // Dropping `stdin` here closes the pipe, signaling EOF.
    });
    let stdout_task = child.stdout.take().map(|mut pipe| {
        tokio::spawn(async move {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf).await;
            buf
        })
    });
    let stderr_task = child.stderr.take().map(|mut pipe| {
        tokio::spawn(async move {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf).await;
            buf
        })
    });

    let deadline = timeout_seconds.map(|s| tokio::time::Instant::now() + Duration::from_secs(s));
    let outcome = if let Some(deadline_at) = deadline {
        tokio::select! {
            _ = cancel.cancelled() => WaitOutcome::Cancelled,
            result = tokio::time::timeout_at(deadline_at, child.wait()) => {
                match result {
                    Ok(status) => WaitOutcome::Done(status.map_err(|source| RunError::Io {
                        context: format!("wait for executor `{executor}`"),
                        source,
                    })?),
                    Err(_elapsed) => WaitOutcome::TimedOut,
                }
            }
        }
    } else {
        tokio::select! {
            _ = cancel.cancelled() => WaitOutcome::Cancelled,
            status = child.wait() => WaitOutcome::Done(status.map_err(|source| RunError::Io {
                context: format!("wait for executor `{executor}`"),
                source,
            })?),
        }
    };

    let _ = write_task.await;

    let status = match outcome {
        WaitOutcome::Cancelled => {
            if let Some(pid) = child.id() {
                kill_process_group(pid).await;
            }
            let _ = child.wait().await;
            return fail(
                ctx,
                node,
                "interrupted: cancelled — a `join: any` sibling won, or the run itself was \
                  cancelled"
                    .to_string(),
                false,
            );
        }
        WaitOutcome::TimedOut => {
            if let Some(pid) = child.id() {
                kill_process_group(pid).await;
            }
            let _ = child.wait().await;
            return fail(
                ctx,
                node,
                format!(
                    "executor `{executor}` exceeded its {}s timeout",
                    timeout_seconds.unwrap_or_default()
                ),
                false,
            );
        }
        WaitOutcome::Done(status) => status,
    };

    let stdout_bytes = match stdout_task {
        Some(task) => task.await.unwrap_or_default(),
        None => Vec::new(),
    };
    let stderr_bytes = match stderr_task {
        Some(task) => task.await.unwrap_or_default(),
        None => Vec::new(),
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
    }
}
