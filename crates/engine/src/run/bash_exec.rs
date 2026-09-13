//! `kind: bash` — one governed subprocess whose exit code is the
//! verdict and whose output `node-output:` can read back.

use tokio_util::sync::CancellationToken;
use yunta_core::events::TokenUsage;
use yunta_core::Node;

use crate::process::{spawn_governed, GovernedCommand, Outcome};

use super::node_close::{close_node, fail, Close};
use super::node_exec::{cancelled_end, render_or_fail, NodeEnd};
use super::step::Step;
use super::{RunCtx, RunError};

/// Runs the bash command, cancellable: a `join: any` sibling
/// winning sends `SIGKILL` to this whole process group and fails the
/// node rather than waiting for `sh` to exit on its own. Stdout/stderr
/// are drained concurrently with `wait()` by owned reader tasks — reading
/// them only after `wait()` (like a naive `child.wait()` + read) risks
/// the child blocking forever on a full pipe for any command chatty
/// enough to fill one before exiting.
pub(super) async fn execute_bash(
    ctx: &RunCtx<'_>,
    node: &Node,
    run: &str,
    cancel: &CancellationToken,
) -> Result<NodeEnd, RunError> {
    let rendered = match render_or_fail(ctx, node, run).await? {
        Step::Value(rendered) => rendered,
        Step::Ended(end) => return Ok(end),
    };

    // The runtime moment: the rendered command against the merged
    // model, right before spawn — a template can assemble what the static
    // scan in `check` never saw.
    if let Some(rule) =
        crate::permissions::command_violation(&rendered, ctx.manifest.config.permissions.as_ref())
    {
        return fail(ctx, node, rule, false).await;
    }

    let command = GovernedCommand::shell(ctx.worktree, &rendered);
    let (status, stdout_bytes, stderr_bytes) =
        match spawn_governed(command, ctx.supervision(cancel)).await? {
            Outcome::Exited {
                status,
                stdout,
                stderr,
            } => (status, stdout, stderr),
            // A bash node has no timeout of its own: the only way it
            // stops early is the run's cancellation.
            Outcome::TimedOut { .. } | Outcome::Cancelled { .. } => {
                return cancelled_end(ctx, node).await;
            }
        };
    // Captured regardless of exit status — a
    // failing `lint` is exactly the case a corrective node's own
    // `node-output` context wants to read.
    crate::run::context_resolve::write_node_output(
        ctx.run_dir,
        &node.id,
        &stdout_bytes,
        &stderr_bytes,
    )?;

    if status.success() {
        close_node(ctx, node, Close::new("exit 0", TokenUsage::default())).await
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
            format!("exit {}: {stderr_tail}", status.code().unwrap_or(-1)),
            false,
        )
        .await
    }
}
