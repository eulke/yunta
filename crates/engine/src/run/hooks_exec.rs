//! `hooks:` around a node — the `before`/`after` commands, the node's
//! own list merged over `node_defaults`, and how a hook failure lands.

use std::path::PathBuf;

use yunta_core::events::{
    CommandExit, CommandOrigin, DeniedCommand, DeniedRule, EventPayload, Failure,
    HookExecutedPayload, HookPhase, TokenUsage,
};
use yunta_core::{ConfigKey, HookFailurePolicy, HookStep, Hooks, Node, Resolved};

use crate::process::{spawn_governed, CommandOutput, GovernedCommand};
use yunta_core::template::render_template;

use super::node_close::{fail_with_staged, fail_with_tokens_staged};
use super::node_exec::{template_vars, NodeEnd};
use super::{RunCtx, RunError};
use yunta_core::events::NodeEvent;

/// How one hook step went: it passed, it failed (with what it left
/// behind, before the caller applies `on_failure`), or the permissions
/// model refused it outright. The distinction matters because
/// `on_failure: warn` downgrades a hook's own failure, never a
/// governance violation — otherwise any hook could opt out of the model
/// by declaring itself warn-only.
pub(super) enum HookRun {
    Passed,
    Failed(CommandExit),
    Violation(DeniedCommand),
    /// It names a command the run's frozen config does not declare.
    Unset(ConfigKey),
}

/// What a hook that failed left behind, named as the workflow wrote it.
fn hook_exit(
    phase: HookPhase,
    step: &HookStep,
    code: i32,
    tail: Vec<String>,
    output: Option<yunta_core::ContentHash>,
) -> CommandExit {
    CommandExit {
        origin: Some(CommandOrigin::Hook {
            phase,
            command: step.run.to_string(),
        }),
        code,
        tail,
        output,
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
    let script = match step.run.resolve(&ctx.manifest.config) {
        Ok(Resolved::Script(script)) => script,
        Ok(Resolved::Project { text, .. }) => text,
        Err(command) => {
            return Ok(HookRun::Unset(ConfigKey::Command {
                command: command.clone(),
            }))
        }
    };
    // A project's command runs as the project wrote it; only a script is
    // the workflow's to template.
    let rendered = match step.run.project() {
        Some(_) => Ok(script.to_string()),
        None => render_template(script, &template_vars(ctx, node)),
    };
    let rendered = match rendered {
        Ok(rendered) => rendered,
        Err(_) => {
            // An unrenderable hook is a failed hook — the
            // `hook_executed { exit_code: -1 }` emitted here is the log's
            // account of it; no warning duplicates that event.
            ctx.emit(
                Some(&node.id),
                EventPayload::Node(NodeEvent::HookExecuted(HookExecutedPayload {
                    phase,
                    command: script.to_string(),
                    exit_code: -1,
                    output: None,
                    tail: Vec::new(),
                })),
            )
            .await?;
            return Ok(HookRun::Failed(hook_exit(
                phase,
                step,
                -1,
                Vec::new(),
                None,
            )));
        }
    };

    // The runtime moment: the *rendered* command, right before it runs
    // — a template can assemble what the YAML never showed.
    if let Some(denied) =
        crate::permissions::command_violation(&rendered, ctx.manifest.config.permissions.as_ref())
    {
        return Ok(HookRun::Violation(denied));
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
    let exit_code = outcome.exit_code();
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
            let exit = hook_exit(phase, step, exit_code, tail.clone(), Some(output.clone()));
            (tail, HookRun::Failed(exit))
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

/// Runs the node's after-hooks in order. The first that fails the node ends
/// its close with that failure; a hook that passes lets the close go on.
pub(super) async fn after_hooks(
    ctx: &RunCtx<'_>,
    node: &Node,
    tokens: TokenUsage,
    staged: &[PathBuf],
) -> Result<Option<NodeEnd>, RunError> {
    for step in &effective_hooks(ctx, node).after {
        let end = match run_hook(ctx, node, HookPhase::After, step).await? {
            HookRun::Violation(denied) => {
                let failure = Failure::CommandDenied {
                    denied_rule: DeniedRule::Command(denied),
                };
                fail_with_tokens_staged(ctx, node, failure, tokens, staged).await?
            }
            HookRun::Unset(key) => {
                fail_with_staged(ctx, node, Failure::unset(key), tokens, staged).await?
            }
            HookRun::Failed(exit) if step.on_failure == HookFailurePolicy::Fail => {
                let failure = Failure::exited(exit);
                fail_with_staged(ctx, node, failure, tokens, staged).await?
            }
            HookRun::Passed | HookRun::Failed(_) => continue,
        };
        return Ok(Some(end));
    }
    Ok(None)
}
