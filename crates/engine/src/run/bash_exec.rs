//! `kind: bash` — one governed subprocess whose exit code is the
//! verdict and whose output `node-output:` can read back.

use tokio_util::sync::CancellationToken;
use yunta_core::events::{DeniedRule, EngineFact, Failure, NodeOutcome, TokenUsage};
use yunta_core::{ConfigKey, Node, Resolved, RunCommand};

use crate::process::{spawn_governed, GovernedCommand, Outcome};

use super::node_close::{close_node, fail, fail_with, Close};
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
    run: &RunCommand,
    cancel: &CancellationToken,
) -> Result<NodeEnd, RunError> {
    // A script is the workflow's and is rendered with its templates; a
    // project's command runs as the project wrote it.
    let rendered = match run.resolve(&ctx.manifest.config) {
        Ok(Resolved::Script(script)) => match render_or_fail(ctx, node, script).await? {
            Step::Value(rendered) => rendered,
            Step::Ended(end) => return Ok(end),
        },
        Ok(Resolved::Project { text, .. }) => text.to_string(),
        Err(command) => {
            let key = ConfigKey::Command {
                command: command.clone(),
            };
            return super::check_exec::unset(ctx, node, key).await;
        }
    };

    // The runtime moment: the rendered command against the merged
    // model, right before spawn — a template can assemble what the static
    // scan in `check` never saw.
    if let Some(failure) = denied_by_permissions(ctx, &rendered) {
        return fail(ctx, node, failure).await;
    }

    let command = GovernedCommand::shell(ctx.worktree, &rendered);
    let outcome = spawn_governed(command, ctx.supervision(cancel)).await?;
    let Outcome::Exited {
        status,
        stdout,
        stderr,
        ..
    } = &outcome
    else {
        // A bash node has no timeout of its own: the only way it stops
        // early is the run's cancellation.
        return cancelled_end(ctx, node).await;
    };
    // Captured regardless of exit status — a
    // failing `lint` is exactly the case a corrective node's own
    // `node-output` context wants to read.
    crate::run::context_resolve::write_node_output(ctx.run_dir, &node.id, stdout, stderr).await?;

    if status.success() {
        close_succeeded(ctx, node).await
    } else {
        let code = status.code().unwrap_or(-1);
        let failure = super::command_exited(ctx, &outcome, code, None, "bash node").await?;
        fail_with(ctx, node, failure, TokenUsage::default()).await
    }
}

/// Closes a bash node whose command exited 0: the exit status is the one
/// fact the engine holds about it.
async fn close_succeeded(ctx: &RunCtx<'_>, node: &Node) -> Result<NodeEnd, RunError> {
    let outcome = NodeOutcome::Fact(EngineFact::ExitStatus { code: 0 });
    close_node(ctx, node, Close::new(outcome, TokenUsage::default())).await
}

/// The failure a rendered command leaves on its node when the project's
/// permissions refuse it; `None` when they allow it.
fn denied_by_permissions(ctx: &RunCtx<'_>, rendered: &str) -> Option<Failure> {
    let denied =
        crate::permissions::command_violation(rendered, ctx.manifest.config.permissions.as_ref())?;
    Some(Failure::CommandDenied {
        denied_rule: DeniedRule::Command(denied),
    })
}
