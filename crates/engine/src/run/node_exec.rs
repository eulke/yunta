//! Executing one node — the imperative half. Every
//! outcome, good or bad, lands in the event log; a node that cannot run
//! (undefined template variable, unresolvable runner, unsupported
//! `until`) fails *in the log* with a diagnostic, it does not abort the
//! engine — explicit degradation, never silent.

use std::collections::BTreeMap;

use tokio_util::sync::CancellationToken;
use yunta_adapters::PermissionProfile;
use yunta_core::events::{EventPayload, HookPhase};
use yunta_core::{HookFailurePolicy, Node, NodeKind};

use crate::template::{render_template, TemplateError};

use super::bash_exec::execute_bash;
use super::hooks_exec::{effective_hooks, run_hook, HookRun};
use super::node_close::fail;
use super::parallel_exec::execute_parallel;
use super::prompt_exec::execute_prompt;
use super::step::Step;
use super::{RunCtx, RunError};

/// How the node's execution ended, as recorded in the log by the caller.
pub(super) enum NodeEnd {
    Finished,
    Failed,
    /// The run's root cancellation cut this node mid-flight — no
    /// terminal event was recorded, on purpose: the node stays orphaned
    /// (`running` in the log) so a later resume re-treats it per its
    /// `on_interrupt` policy, exactly like a crash.
    Interrupted,
    /// A `kind: workflow` node's child run paused on its own log
    /// (a gate, a failure without re-route, its budget). No terminal
    /// event here either — the node stays open so a later resume
    /// re-enters it and resumes the child recursively — but
    /// unlike `Interrupted` the *parent run* must pause with this
    /// reason rather than fall through to its loop-top cancel check.
    ChildPaused {
        reason: String,
    },
}

/// The shared "my token fired" epilogue — which cancellation was
/// it? A user/root cancel leaves the node orphaned; a `join: any`
/// sibling race records the loss so the group can close over it.
pub(super) async fn cancelled_end(ctx: &RunCtx<'_>, node: &Node) -> Result<NodeEnd, RunError> {
    if ctx.root_cancel.is_cancelled() {
        return Ok(NodeEnd::Interrupted);
    }
    fail(
        ctx,
        node,
        "interrupted: a sibling in this join: any group finished first".to_string(),
        false,
    )
    .await
}

/// `cancel` only ever fires for a child of a `join: any` parallel group
/// once a sibling has won — every other call site passes a token
/// nothing ever cancels, so this is a no-op parameter for them.
#[tracing::instrument(
    skip_all,
    fields(run_id = %ctx.run_id, node_id = %node.id, attempt)
)]
pub(super) async fn execute_node(
    ctx: &RunCtx<'_>,
    node: &Node,
    attempt: u32,
    cancel: &CancellationToken,
) -> Result<NodeEnd, RunError> {
    ctx.emit(
        Some(&node.id),
        EventPayload::NodeStarted(yunta_core::events::NodeStartedPayload { attempt }),
    )
    .await?;

    // hooks.before: a failing before aborts without spending a
    // token; a failing after fails the node before verification. Either
    // phase's step can opt into `on_failure: warn` instead of the default
    // `fail`, in which case a non-zero exit is recorded but doesn't stop
    // the node.
    let hooks = effective_hooks(ctx, node);
    for step in &hooks.before {
        match run_hook(ctx, node, HookPhase::Before, step).await? {
            HookRun::Violation(rule) => return fail(ctx, node, rule, false).await,
            HookRun::Ran(false) if step.on_failure == HookFailurePolicy::Fail => {
                return fail(
                    ctx,
                    node,
                    format!("before hook `{}` failed", step.run),
                    false,
                )
                .await;
            }
            HookRun::Ran(_) => {}
        }
    }

    let end = match &node.kind {
        NodeKind::Bash { run } => execute_bash(ctx, node, run, attempt, cancel).await?,
        NodeKind::Prompt { prompt } => execute_prompt(ctx, node, prompt, attempt, cancel).await?,
        NodeKind::Loop {
            until: yunta_core::LoopUntil::AllTasksComplete,
            prompt,
            ..
        } => super::loop_exec::execute_loop(ctx, node, prompt, attempt, cancel).await?,
        NodeKind::Parallel {
            join,
            coordination,
            nodes,
        } => {
            let end = execute_parallel(ctx, node, *join, nodes, attempt, cancel).await?;
            // The blackboard's consolidation happens exactly once,
            // at the group's own terminal close (success or failure —
            // the posts are findings either way), as the group's
            // node-output: sorted by content, never by arrival order,
            // consumable by a node AFTER the parallel and never between
            // siblings hot. An interrupted/paused group stays open, so
            // nothing is consolidated yet.
            if *coordination == yunta_core::Coordination::Blackboard
                && matches!(end, NodeEnd::Finished | NodeEnd::Failed)
            {
                let members: Vec<yunta_core::NodeId> =
                    nodes.iter().map(|child| child.id.clone()).collect();
                let consolidated =
                    crate::run_tools::consolidate_blackboard(&ctx.load_events().await?, &members);
                super::context_resolve::write_node_output(
                    ctx.run_dir,
                    &node.id,
                    consolidated.as_bytes(),
                    &[],
                )?;
            }
            end
        }
        NodeKind::Check(builtin) => {
            super::check_exec::execute_check(ctx, node, builtin, attempt, cancel).await?
        }
        NodeKind::Executor {
            executor,
            with,
            timeout_seconds,
        } => {
            super::executor_exec::execute_executor(
                ctx,
                node,
                executor,
                with,
                *timeout_seconds,
                attempt,
                cancel,
            )
            .await?
        }
        NodeKind::Workflow {
            r#use,
            inputs,
            isolation,
            mounts,
        } => {
            super::workflow_exec::execute_workflow(
                ctx,
                node,
                super::workflow_exec::WorkflowCall {
                    use_name: r#use,
                    inputs,
                    isolation: *isolation,
                    mounts,
                },
                attempt,
                cancel,
            )
            .await?
        }
        // A gate's resolution is a forge round-trip, not a
        // session — `schedule::next_step` intercepts a ready/orphaned
        // gate before it ever becomes an `Execute` step (its own
        // `ScheduleStep::PublishGate`/`PollGate`, handled in
        // `run/mod.rs`), and `check::check_no_gate_in_parallel` refuses
        // the only other way a node reaches this function without going
        // through the top-level scheduler (`parallel`'s own children).
        NodeKind::Gate { .. } => {
            return Err(RunError::Broken {
                diagnostic: format!(
                    "node `{}` is a `kind: gate` but reached node execution, which only \
                     dispatches sessions and checks — a gate resolves through its own \
                     scheduler step (see this arm's own comment)",
                    node.id
                ),
            });
        }
    };
    Ok(end)
}

/// Template variables for one node's own rendering: `run.*`
/// is always present; `runner.role` is the node's own declared `runner:`
/// (the role name itself, known statically from the workflow — never the
/// adapter/model a later resolution step picks, so no ordering
/// dependency on `resolve_node_runner`); `project.*` mirrors whatever
/// the merged config's `project:` group declares; `inputs.*` is
/// every declared input's already-resolved-and-validated value, read
/// straight from the frozen manifest — never re-resolved per node, since
/// that would make a `default` non-deterministic across nodes.
pub(super) fn template_vars(ctx: &RunCtx<'_>, node: &Node) -> BTreeMap<String, String> {
    let mut vars = BTreeMap::from([
        ("run.dir".to_string(), ctx.run_dir.display().to_string()),
        (
            "run.worktree".to_string(),
            ctx.worktree.display().to_string(),
        ),
        // The reference example (`external.branch:
        // "{{run.branch}}"`) — a fresh push target derived from the
        // run id, not necessarily the worktree's own local checkout
        // branch (which `isolation: none` never creates one of at all,
        // `worktree.rs`'s own doc comment).
        ("run.branch".to_string(), format!("yunta/{}", ctx.run_id)),
    ]);
    if let Some(role) = &node.runner {
        vars.insert("runner.role".to_string(), role.to_string());
    }
    if let Some(project) = &ctx.manifest.config.project {
        if let Some(name) = &project.name {
            vars.insert("project.name".to_string(), name.clone());
        }
        if let Some(base_branch) = &project.base_branch {
            vars.insert("project.base_branch".to_string(), base_branch.clone());
        }
        if let Some(branch_prefix) = &project.branch_prefix {
            vars.insert("project.branch_prefix".to_string(), branch_prefix.clone());
        }
    }
    for (name, value) in &ctx.manifest.inputs {
        vars.insert(format!("inputs.{name}"), value.clone());
    }
    vars
}

/// Renders `input` or fails the node with a diagnostic naming the
/// variable — a prompt with `{{run.dir}}` left verbatim must never reach
/// an agent.
pub(super) async fn render_or_fail(
    ctx: &RunCtx<'_>,
    node: &Node,
    input: &str,
) -> Result<Step<String>, RunError> {
    match render_template(input, &template_vars(ctx, node)) {
        Ok(rendered) => Ok(Step::Value(rendered)),
        Err(e) => Ok(Step::Ended(fail(ctx, node, e.to_string(), false).await?)),
    }
}

/// The node's rung on the permissions ladder mapped onto the
/// adapter's session profile — absent means the engine's long-standing
/// default, `edit`.
pub(super) fn session_profile(node: &Node) -> PermissionProfile {
    match node.permissions {
        Some(yunta_core::NodePermissions::ReadOnly) => PermissionProfile::ReadOnly,
        Some(yunta_core::NodePermissions::Full) => PermissionProfile::Full,
        Some(yunta_core::NodePermissions::Edit) | None => PermissionProfile::Edit,
    }
}

/// The declared artifact names re-render with the node's own
/// template vars (`{{runner.role}}` above all), so each fan-out sibling
/// declares — and verifies — its own file. Nodes without templates in
/// their names come back unchanged.
pub(super) fn render_artifact_names(ctx: &RunCtx<'_>, node: &Node) -> Result<Node, TemplateError> {
    if node.artifacts.is_none() {
        return Ok(node.clone());
    }
    let vars = template_vars(ctx, node);
    let mut rendered = node.clone();
    if let Some(artifacts) = &mut rendered.artifacts {
        for spec in &mut artifacts.produces {
            let name = match spec {
                yunta_core::ArtifactSpec::Plain(name) => name,
                yunta_core::ArtifactSpec::Typed { name, .. } => name,
            };
            *name = render_template(name, &vars)?;
        }
    }
    Ok(rendered)
}
