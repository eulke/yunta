//! `hooks:` around a node — the `before`/`after` commands, the node's
//! own list merged over `node_defaults`, and how a hook failure lands.

use yunta_core::events::{EventPayload, HookExecutedPayload, HookPhase};
use yunta_core::{HookStep, Hooks, Node};

use crate::process::{spawn_governed, Capture, GovernedCommand, Outcome};
use crate::template::render_template;

use super::node_exec::template_vars;
use super::{RunCtx, RunError};

/// How one hook step went: it ran (with its own success bool, before the
/// caller applies `on_failure`), or the permissions model refused it
/// outright. The distinction matters because `on_failure: warn` downgrades
/// a hook's own failure, never a governance violation — otherwise
/// any hook could opt out of the model by declaring itself warn-only.
pub(super) enum HookRun {
    Ran(bool),
    Violation(String),
}

/// A hook only ever fails or warns — unless the permissions model
/// refuses its rendered command before it ever spawns.
pub(super) async fn run_hook(
    ctx: &RunCtx<'_>,
    node: &Node,
    phase: HookPhase,
    step: &HookStep,
) -> Result<HookRun, RunError> {
    let rendered = match render_template(&step.run, &template_vars(ctx, node)) {
        Ok(rendered) => rendered,
        Err(_) => {
            // An unrenderable hook is a failed hook — the
            // `hook_executed { exit_code: -1 }` emitted here is the log's
            // account of it; no warning duplicates that event.
            ctx.emit(
                Some(&node.id),
                EventPayload::HookExecuted(HookExecutedPayload {
                    phase,
                    command: step.run.clone(),
                    exit_code: -1,
                }),
            )
            .await?;
            return Ok(HookRun::Ran(false));
        }
    };

    // The runtime moment: the *rendered* command, right before it runs
    // — a template can assemble what the YAML never showed.
    if let Some(rule) =
        crate::permissions::command_violation(&rendered, ctx.manifest.config.permissions.as_ref())
    {
        return Ok(HookRun::Violation(rule));
    }

    // A hook shares the engine's streams and is bounded by its own
    // timeout and by the run's cancellation; either kills its whole
    // process tree.
    let mut command = GovernedCommand::shell(ctx.worktree, &rendered)
        .stdout(Capture::Inherit)
        .stderr(Capture::Inherit);
    if let Some(seconds) = step.timeout_seconds {
        command = command.timeout(std::time::Duration::from_secs(seconds));
    }
    let exit_code = match spawn_governed(command, ctx.supervision(&ctx.root_cancel)).await? {
        Outcome::Exited { status, .. } => status.code().unwrap_or(-1),
        // Never a real process exit code (those are 0..=255) — distinct
        // from -1's "couldn't even render/run", so a hook the engine
        // stopped is diagnosable from the event alone.
        Outcome::TimedOut { .. } | Outcome::Cancelled { .. } => -2,
    };

    ctx.emit(
        Some(&node.id),
        EventPayload::HookExecuted(HookExecutedPayload {
            phase,
            command: rendered,
            exit_code,
        }),
    )
    .await?;
    Ok(HookRun::Ran(exit_code == 0))
}

/// A node's hooks with `node_defaults.hooks` filled in per phase:
/// a phase the node itself leaves empty inherits the workflow-level
/// default's list for that phase; a phase the node declares replaces the
/// default wholesale, the same "arrays replace" rule config layers use
/// rather than concatenating the two.
pub(super) fn effective_hooks(ctx: &RunCtx<'_>, node: &Node) -> Hooks {
    let defaults = ctx
        .manifest
        .workflow
        .node_defaults
        .as_ref()
        .and_then(|defaults| defaults.hooks.as_ref());
    let own = node.hooks.as_ref();

    let before = own
        .filter(|hooks| !hooks.before.is_empty())
        .map(|hooks| hooks.before.clone())
        .or_else(|| defaults.map(|hooks| hooks.before.clone()))
        .unwrap_or_default();
    let after = own
        .filter(|hooks| !hooks.after.is_empty())
        .map(|hooks| hooks.after.clone())
        .or_else(|| defaults.map(|hooks| hooks.after.clone()))
        .unwrap_or_default();

    Hooks { before, after }
}
