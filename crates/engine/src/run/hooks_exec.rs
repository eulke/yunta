//! `hooks:` around a node — the `before`/`after` commands, the node's
//! own list merged over `node_defaults`, and how a hook failure lands.

use yunta_core::events::{EventPayload, HookExecutedPayload, HookPhase};
use yunta_core::{HookStep, Hooks, Node};

use crate::process::{spawn_governed, CommandOutput, GovernedCommand, Outcome};
use yunta_core::template::render_template;

use super::node_exec::template_vars;
use super::{RunCtx, RunError};
use yunta_core::events::NodeEvent;

/// How one hook step went: it passed, it failed (with the last lines it
/// printed, before the caller applies `on_failure`), or the permissions
/// model refused it outright. The distinction matters because
/// `on_failure: warn` downgrades a hook's own failure, never a
/// governance violation — otherwise any hook could opt out of the model
/// by declaring itself warn-only.
pub(super) enum HookRun {
    Passed,
    Failed { said: String },
    Violation(String),
}

impl HookRun {
    /// Why a node whose hook failed fails: the hook, and what it printed
    /// last, the way a `bash` node's failure quotes its own.
    pub(super) fn failure(phase: HookPhase, step: &HookStep, said: &str) -> String {
        let phase = match phase {
            HookPhase::Before => "before",
            HookPhase::After => "after",
        };
        yunta_core::text::detailed(format!("{phase} hook `{}` failed", step.run), said)
    }
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
                EventPayload::Node(NodeEvent::HookExecuted(HookExecutedPayload {
                    phase,
                    command: step.run.clone(),
                    exit_code: -1,
                    output: None,
                    tail: Vec::new(),
                })),
            )
            .await?;
            return Ok(HookRun::Failed {
                said: String::new(),
            });
        }
    };

    // The runtime moment: the *rendered* command, right before it runs
    // — a template can assemble what the YAML never showed.
    if let Some(rule) =
        crate::permissions::command_violation(&rendered, ctx.manifest.config.permissions.as_ref())
    {
        return Ok(HookRun::Violation(rule));
    }

    // What a hook prints is the run's, never the terminal's: it lands in
    // the run's objects, and the tail of a failing hook on its event. A
    // hook is bounded by its own timeout and by the run's cancellation;
    // either kills its whole process tree.
    let mut command = GovernedCommand::shell(ctx.worktree, &rendered);
    if let Some(seconds) = step.timeout_seconds {
        command = command.timeout(std::time::Duration::from_secs(seconds));
    }
    let outcome = spawn_governed(command, ctx.supervision(&ctx.root_cancel)).await?;
    let exit_code = match &outcome {
        Outcome::Exited { status, .. } => status.code().unwrap_or(-1),
        // Never a real process exit code (those are 0..=255) — distinct
        // from -1's "couldn't even render/run", so a hook the engine
        // stopped is diagnosable from the event alone.
        Outcome::TimedOut { .. } | Outcome::Cancelled { .. } => -2,
    };
    let printed = CommandOutput::of(&outcome);
    let output = crate::artifacts::store::ObjectStore::at(ctx.run_dir)
        .put_redacted(printed.bytes(), &ctx.redactor)
        .await
        .map_err(|source| RunError::Io {
            context: format!("keep what hook `{rendered}` printed"),
            source,
        })?;
    let (tail, run) = match exit_code {
        0 => (Vec::new(), HookRun::Passed),
        _ => {
            let tail = printed.tail();
            let said = tail.join("\n");
            (tail, HookRun::Failed { said })
        }
    };

    ctx.emit(
        Some(&node.id),
        EventPayload::Node(NodeEvent::HookExecuted(HookExecutedPayload {
            phase,
            command: rendered,
            exit_code,
            output: Some(output),
            tail,
        })),
    )
    .await?;
    Ok(run)
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
