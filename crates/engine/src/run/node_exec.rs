//! Executing one node — the imperative half. Every
//! outcome, good or bad, lands in the event log; a node that cannot run
//! (undefined template variable, unresolvable runner, unsupported
//! `until`) fails *in the log* with a diagnostic, it does not abort the
//! engine — explicit degradation, never silent.

use std::collections::BTreeMap;

use tokio_util::sync::CancellationToken;
use yunta_adapters::{PermissionProfile, SessionRequest};
use yunta_core::events::{
    EventPayload, HookExecutedPayload, HookPhase, NodeFailedPayload, NodeFinishedPayload,
    RunnerResolvedPayload, TaskStatus, TaskStatusChangedPayload, TokenUsage,
};
use yunta_core::{
    AdapterId, AgentName, HookFailurePolicy, HookStep, Hooks, JoinPolicy, Node, NodeKind,
    PromptSource,
};

use crate::artifacts::close_artifacts;
use crate::process::{spawn_governed, Capture, GovernedCommand, Outcome};
use crate::replay::NodeState;
use crate::runner::resolve_runner;
use crate::scope::scope_check;
use crate::task_cycle::{dispatch_session, DispatchOutcome};
use crate::template::{render_template, TemplateError};

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
        NodeKind::Bash { run } => execute_bash(ctx, node, run, cancel).await?,
        NodeKind::Prompt { prompt } => execute_prompt(ctx, node, prompt, cancel).await?,
        NodeKind::Loop {
            until: yunta_core::LoopUntil::AllTasksComplete,
            prompt,
            ..
        } => super::loop_exec::execute_loop(ctx, node, prompt, cancel).await?,
        NodeKind::Parallel {
            join,
            coordination,
            nodes,
        } => {
            let end = execute_parallel(ctx, node, *join, nodes, cancel).await?;
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
            super::check_exec::execute_check(ctx, node, builtin, cancel).await?
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
                ctx, node, r#use, inputs, *isolation, mounts, cancel,
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
            unreachable!(
                "kind: gate never dispatches through execute_node — see this arm's own comment"
            )
        }
    };
    Ok(end)
}

/// Runs `node`'s children: all of them concurrently, joined
/// per `join`. Re-entrant on resume: a child already terminal in the log
/// — `Finished`, or `Failed` — is never re-dispatched, and a group whose
/// winning child already finished (crash between the child's own
/// `node_finished` and the group's) closes immediately without racing
/// anyone else. See the module's resume-safety test.
async fn execute_parallel(
    ctx: &RunCtx<'_>,
    node: &Node,
    join: JoinPolicy,
    children: &[Node],
    cancel: &CancellationToken,
) -> Result<NodeEnd, RunError> {
    let group_cancel = cancel.child_token();
    let state = ctx.run_view().await?.state;

    let already_failed: Vec<&Node> = children
        .iter()
        .filter(|child| matches!(state.nodes.get(&child.id), Some(NodeState::Failed { .. })))
        .collect();
    // Fresh children start at attempt 1; a child left `running` with no
    // terminal event (crash, root cancel, or a paused child run under a
    // workflow node) is an orphan the group's own restart re-runs,
    // the same `restart_node` rule applied inside the group — for a
    // workflow child that re-run is what resumes its child run
    // recursively.
    let to_run: Vec<(&Node, u32)> = children
        .iter()
        .filter_map(|child| match state.nodes.get(&child.id) {
            None => Some((child, 1)),
            Some(NodeState::Running { attempt }) => Some((child, attempt + 1)),
            _ => None,
        })
        .collect();

    match join {
        JoinPolicy::All => {
            if let Some(first) = already_failed.first() {
                return fail(
                    ctx,
                    node,
                    format!("child `{}` failed under join: all", first.id),
                    false,
                )
                .await;
            }
            let results = futures::future::join_all(
                to_run
                    .iter()
                    .map(|(child, attempt)| execute_node(ctx, child, *attempt, &group_cancel)),
            )
            .await;

            let mut failed_child = None;
            let mut interrupted = false;
            let mut child_paused: Option<String> = None;
            for ((child, _), result) in to_run.iter().zip(results) {
                match result? {
                    NodeEnd::Failed => {
                        failed_child.get_or_insert(&child.id);
                    }
                    // A root cancellation unwound this child —
                    // the group closes nothing; the whole run is
                    // pausing, and resume re-enters it. Noted, not
                    // returned yet: every sibling's result was already
                    // awaited above, and dropping a sibling's recorded
                    // failure here would change nothing it wrote.
                    NodeEnd::Interrupted => interrupted = true,
                    // Same shape — the group stays open and the
                    // run pauses naming the paused child run.
                    NodeEnd::ChildPaused { reason } => {
                        child_paused.get_or_insert(reason);
                    }
                    NodeEnd::Finished => {}
                }
            }
            if interrupted {
                return Ok(NodeEnd::Interrupted);
            }
            if let Some(reason) = child_paused {
                return Ok(NodeEnd::ChildPaused { reason });
            }
            if let Some(id) = failed_child {
                return fail(
                    ctx,
                    node,
                    format!("child `{id}` failed under join: all"),
                    false,
                )
                .await;
            }
            close_node(
                ctx,
                node,
                format!("{} child(ren) finished", children.len()),
                TokenUsage::default(),
            )
            .await
        }
        JoinPolicy::Any => {
            use futures::stream::{FuturesUnordered, StreamExt};

            if let Some(already_won) = children.iter().find(|child| {
                matches!(state.nodes.get(&child.id), Some(NodeState::Finished { .. }))
            }) {
                return close_node(
                    ctx,
                    node,
                    format!("`{}` succeeded first", already_won.id),
                    TokenUsage::default(),
                )
                .await;
            }

            let mut failures: Vec<&yunta_core::NodeId> =
                already_failed.iter().map(|child| &child.id).collect();
            let mut running: FuturesUnordered<_> = to_run
                .iter()
                .map(|(child, attempt)| {
                    let cancel = group_cancel.clone();
                    async move { (&child.id, execute_node(ctx, child, *attempt, &cancel).await) }
                })
                .collect();

            let mut winner = None;
            let mut child_paused: Option<String> = None;
            while winner.is_none() {
                let Some((child_id, result)) = running.next().await else {
                    break;
                };
                match result? {
                    NodeEnd::Finished => {
                        winner = Some(child_id);
                        group_cancel.cancel();
                    }
                    NodeEnd::Failed => failures.push(child_id),
                    // Root cancellation, not a sibling race —
                    // drain the rest and unwind without a terminal. A
                    // storage error a drained sibling hits propagates
                    // (`result?`) rather than vanishing into the drain,
                    // exactly as the no-winner drain below already does.
                    NodeEnd::Interrupted => {
                        while let Some((_, result)) = running.next().await {
                            result?;
                        }
                        return Ok(NodeEnd::Interrupted);
                    }
                    // A paused child run is neither a win nor a
                    // loss — the race stays live: a sibling can still
                    // win the group. Recorded for the no-winner ending.
                    NodeEnd::ChildPaused { reason } => {
                        child_paused.get_or_insert(reason);
                    }
                }
            }
            // Drain the rest: the cancelled losers finishing their own
            // interrupt→kill sequence (each records its own failure).
            while let Some((_, result)) = running.next().await {
                let _ = result?;
            }

            match winner {
                Some(id) => {
                    // A sibling won while a workflow child's run sits
                    // paused: the group closes (that's `join: any`'s
                    // contract) and the child run stays paused on its
                    // own log — a complete run, individually resumable
                    // (`yunta resume <child>`), never silently killed.
                    close_node(
                        ctx,
                        node,
                        format!("`{id}` succeeded first"),
                        TokenUsage::default(),
                    )
                    .await
                }
                None => {
                    if let Some(reason) = child_paused {
                        // No winner and a child run waiting on its own
                        // pause: the group can't close over an open
                        // child — the run pauses and resume
                        // re-enters the race.
                        return Ok(NodeEnd::ChildPaused { reason });
                    }
                    fail(
                        ctx,
                        node,
                        format!("join: any — no child succeeded ({} failed)", failures.len()),
                        false,
                    )
                    .await
                }
            }
        }
    }
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
async fn run_hook(
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

/// A node's hooks with `node_defaults.hooks` filled in per phase:
/// a phase the node itself leaves empty inherits the workflow-level
/// default's list for that phase; a phase the node declares replaces the
/// default wholesale, the same "arrays replace" rule config layers use
/// rather than concatenating the two.
fn effective_hooks(ctx: &RunCtx<'_>, node: &Node) -> Hooks {
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

/// The declared artifact names re-render with the node's own
/// template vars (`{{runner.role}}` above all), so each fan-out sibling
/// declares — and verifies — its own file. Nodes without templates in
/// their names come back unchanged.
fn render_artifact_names(ctx: &RunCtx<'_>, node: &Node) -> Result<Node, TemplateError> {
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

/// Runs after-hooks, then verifies scope and artifacts — the close
/// sequence every successful node body goes through (session → after →
/// verification).
/// Closes a node whose session staged nothing in the worktree — every
/// kind but an agent session.
pub(super) async fn close_node(
    ctx: &RunCtx<'_>,
    node: &Node,
    outcome: String,
    tokens: TokenUsage,
) -> Result<NodeEnd, RunError> {
    close_node_staged(ctx, node, outcome, tokens, &[]).await
}

/// Closes a node: its `after` hooks run, its scope is checked over the
/// whole diff — hook edits included, `staged` paths (what the adapter
/// declared it wrote for itself) left out — and it finishes.
pub(super) async fn close_node_staged(
    ctx: &RunCtx<'_>,
    node: &Node,
    outcome: String,
    tokens: TokenUsage,
    staged: &[std::path::PathBuf],
) -> Result<NodeEnd, RunError> {
    for step in &effective_hooks(ctx, node).after {
        match run_hook(ctx, node, HookPhase::After, step).await? {
            HookRun::Violation(rule) => {
                return fail_with_tokens(ctx, node, rule, false, tokens).await
            }
            HookRun::Ran(false) if step.on_failure == HookFailurePolicy::Fail => {
                return fail_with_tokens(
                    ctx,
                    node,
                    format!("after hook `{}` failed", step.run),
                    false,
                    tokens,
                )
                .await;
            }
            HookRun::Ran(_) => {}
        }
    }

    if !node.scope.is_empty() {
        let result = scope_check(ctx.worktree, &node.scope, staged).await?;
        ctx.emit(
            Some(&node.id),
            EventPayload::ScopeChecked(yunta_core::events::ScopeCheckedPayload {
                task_id: None,
                diff: result.diff.clone(),
                violations: result.violations.clone(),
            }),
        )
        .await?;
        if !result.violations.is_empty() {
            return fail_with_tokens(
                ctx,
                node,
                format!(
                    "scope violated: {} file(s) outside the declared globs",
                    result.violations.len()
                ),
                false,
                tokens,
            )
            .await;
        }
    }

    let max_artifact_bytes = ctx
        .manifest
        .config
        .limits
        .as_ref()
        .and_then(|limits| limits.max_artifact_bytes);
    // Artifact names are templates too (`findings-{{runner.role}}`
    // in the reference workflow) — rendered per node so every fan-out
    // sibling verifies its own file.
    let node_rendered = match render_artifact_names(ctx, node) {
        Ok(rendered) => rendered,
        Err(error) => return fail_with_tokens(ctx, node, error.to_string(), false, tokens).await,
    };
    let node = &node_rendered;
    match close_artifacts(node, ctx.run_dir, max_artifact_bytes) {
        Ok(verified) => {
            // A `kind: questions` artifact's own
            // session has already closed by this point (the same
            // "artifact read only at node close" ordering
            // `task-ledger`/`findings` already rely on) — nothing
            // renders mid-session. With a live surface the questions
            // are put to the human right here (after the artifacts are
            // recorded, below); without one — or on an invalid reply —
            // the run pauses citing exactly what's unanswered, never
            // hangs, never silently proceeds as if nothing were asked.

            // A re-plan — this same node producing a task
            // ledger a second time, whether via a reroute back to it or a
            // resumed run — must not silently keep a task `done` whose
            // identity actually changed. Identity is exactly the
            // Contrato's own wording: same `id`, same `criteria`, same
            // `scope` — `depends_on` is deliberately not part of it, the
            // spec never mentions it. The most recent prior registration
            // per task id is all that's needed; `TaskRegistered`'s own
            // replay handling (`or_insert`, never overwrites an existing
            // status) already makes an identical re-registration a no-op,
            // so only a genuine mismatch needs an explicit event here.
            let previous_registrations: BTreeMap<
                yunta_core::TaskId,
                (Vec<yunta_core::events::Criterion>, Vec<String>),
            > = ctx
                .load_events()
                .await?
                .into_iter()
                .filter_map(|event| match event.payload() {
                    Some(EventPayload::TaskRegistered(p)) => {
                        Some((p.task_id.clone(), (p.criteria.clone(), p.scope.clone())))
                    }
                    _ => None,
                })
                .collect();

            for artifact in &verified {
                ctx.emit(
                    Some(&node.id),
                    EventPayload::ArtifactWritten(yunta_core::events::ArtifactWrittenPayload {
                        path: artifact.path.clone(),
                        content_hash: artifact.content_hash.clone(),
                        artifact_kind: artifact.kind.clone(),
                    }),
                )
                .await?;
                if let Some(ledger) = &artifact.ledger {
                    for task in &ledger.tasks {
                        let criteria: Vec<yunta_core::events::Criterion> =
                            task.criteria.iter().map(Into::into).collect();
                        let registered_seq = ctx
                            .emit(
                                Some(&node.id),
                                EventPayload::TaskRegistered(
                                    yunta_core::events::TaskRegisteredPayload {
                                        task_id: task.id.clone(),
                                        criteria: criteria.clone(),
                                        scope: task.scope.clone(),
                                        depends_on: task.depends_on.clone(),
                                    },
                                ),
                            )
                            .await?;
                        let changed_identity = previous_registrations.get(&task.id).is_some_and(
                            |(previous, scope)| *previous != criteria || *scope != task.scope,
                        );
                        if changed_identity {
                            ctx.emit(
                                Some(&node.id),
                                EventPayload::TaskStatusChanged(TaskStatusChangedPayload {
                                    task_id: task.id.clone(),
                                    new_status: TaskStatus::Pending,
                                    caused_by: registered_seq,
                                }),
                            )
                            .await?;
                        }
                    }
                }
                if let Some(findings) = &artifact.findings {
                    for finding in findings {
                        ctx.emit(
                            Some(&node.id),
                            EventPayload::FindingPosted(yunta_core::events::FindingPostedPayload {
                                finding: finding.clone(),
                            }),
                        )
                        .await?;
                    }
                }
            }
            // Unanswered questions close the node as
            // waiting-shaped (`node_failed` here, derived `Waiting` by
            // replay via the typed `kind: questions` on the artifact
            // event above) — the actual asking happens in ONE place, the
            // scheduler's own `AskQuestions` step (`questions_exec`),
            // which serves the first invocation and every resume through
            // the identical path.
            let pending: Vec<String> = verified
                .iter()
                .filter_map(|artifact| artifact.questions.as_deref())
                .flatten()
                .map(|q| q.id.to_string())
                .collect();
            if !pending.is_empty() {
                return fail_with_tokens(
                    ctx,
                    node,
                    format!(
                        "node `{}` asked {} question(s) awaiting an answer: {}",
                        node.id,
                        pending.len(),
                        pending.join(", ")
                    ),
                    false,
                    tokens,
                )
                .await;
            }

            ctx.emit(
                Some(&node.id),
                EventPayload::NodeFinished(NodeFinishedPayload {
                    outcome,
                    tokens_used: tokens,
                }),
            )
            .await?;
            write_progress(ctx).await?;
            Ok(NodeEnd::Finished)
        }
        Err(errors) => {
            let listed = errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; ");
            fail_with_tokens(ctx, node, listed, false, tokens).await
        }
    }
}

pub(super) async fn fail(
    ctx: &RunCtx<'_>,
    node: &Node,
    outcome: String,
    retryable: bool,
) -> Result<NodeEnd, RunError> {
    fail_with_tokens(ctx, node, outcome, retryable, TokenUsage::default()).await
}

/// Regenerates `progress.md` at `run.dir`'s root — the
/// engine's own call, right after the `node_finished` that triggers it
/// (the Contrato's literal text names only `node_finished`, not
/// `node_failed`, as the regeneration point).
pub(super) async fn write_progress(ctx: &RunCtx<'_>) -> Result<(), RunError> {
    let events = ctx.load_events().await?;
    let markdown = crate::progress::render_progress(&ctx.manifest.workflow, &events);
    std::fs::write(ctx.run_dir.join("progress.md"), markdown).map_err(|source| RunError::Io {
        context: "write progress.md".to_string(),
        source,
    })
}

pub(super) async fn fail_with_tokens(
    ctx: &RunCtx<'_>,
    node: &Node,
    outcome: String,
    retryable: bool,
    tokens: TokenUsage,
) -> Result<NodeEnd, RunError> {
    ctx.emit(
        Some(&node.id),
        EventPayload::NodeFailed(NodeFailedPayload {
            outcome,
            tokens_used: tokens,
            retryable,
        }),
    )
    .await?;
    Ok(NodeEnd::Failed)
}

/// Runs the bash command, cancellable: a `join: any` sibling
/// winning sends `SIGKILL` to this whole process group and fails the
/// node rather than waiting for `sh` to exit on its own. Stdout/stderr
/// are drained concurrently with `wait()` by owned reader tasks — reading
/// them only after `wait()` (like a naive `child.wait()` + read) risks
/// the child blocking forever on a full pipe for any command chatty
/// enough to fill one before exiting.
async fn execute_bash(
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
        close_node(ctx, node, "exit 0".to_string(), TokenUsage::default()).await
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

/// The node's prompt text: frozen file content from the manifest when the
/// workflow declared `{file: ...}`, the inline string otherwise — never a
/// re-read from disk.
pub(super) fn prompt_text<'a>(
    ctx: &'a RunCtx<'_>,
    node: &'a Node,
    prompt: &'a PromptSource,
) -> &'a str {
    match prompt {
        PromptSource::Inline(text) => text,
        PromptSource::File(_) => ctx
            .manifest
            .prompts
            .get(&node.id)
            .map(String::as_str)
            // A file prompt missing from the manifest cannot happen for a
            // manifest built by `build_manifest` (it reads every file
            // prompt or errors); an empty prompt for a hand-edited
            // manifest fails the session visibly downstream.
            .unwrap_or(""),
    }
}

/// Resolves the node's runner or fails the node; on success emits
/// `runner_resolved` and hands back the request pieces.
pub(super) async fn resolve_node_runner(
    ctx: &RunCtx<'_>,
    node: &Node,
) -> Result<Step<yunta_core::RunnerCandidate>, RunError> {
    // A node without `runner:` falls back to `defaults.runner`.
    let default_runner = ctx
        .manifest
        .config
        .defaults
        .as_ref()
        .and_then(|defaults| defaults.runner.as_ref());
    let Some(role) = node.runner.as_ref().or(default_runner) else {
        let end = fail(
            ctx,
            node,
            format!(
                "node `{}` has no `runner:` and the config declares no `defaults.runner` — \
                 declare one",
                node.id
            ),
            false,
        )
        .await?;
        return Ok(Step::Ended(end));
    };

    match resolve_runner(
        role,
        &ctx.manifest.config,
        &|adapter| ctx.adapters.contains_key(adapter),
        ctx.adapter_override,
    ) {
        Ok(resolved) => {
            let mut chosen = resolved.chosen.clone();
            // The node's own `agent:` wins over the
            // candidate's.
            if let Some(agent) = &node.agent {
                chosen.agent = Some(agent.clone());
            }
            // An adapter without `custom_agents` fails the node
            // rather than silently dropping the requested agent.
            if chosen.agent.is_some() {
                let has_custom_agents = ctx
                    .adapters
                    .get(&chosen.adapter)
                    .is_some_and(|adapter| adapter.capabilities().custom_agents);
                if !has_custom_agents {
                    let end = fail(
                        ctx,
                        node,
                        format!(
                            "node `{}` requests agent `{}` but adapter `{}` does not declare \
                             `custom_agents` — pick a candidate on an adapter that does, or \
                             drop the agent",
                            node.id,
                            chosen.agent.as_ref().map_or("", AgentName::as_str),
                            chosen.adapter
                        ),
                        false,
                    )
                    .await?;
                    return Ok(Step::Ended(end));
                }
            }
            ctx.emit(
                Some(&node.id),
                EventPayload::RunnerResolved(RunnerResolvedPayload {
                    runner: resolved.runner.clone(),
                    chosen: chosen.clone(),
                    discarded: resolved.discarded.clone(),
                }),
            )
            .await?;
            Ok(Step::Value(chosen))
        }
        Err(e) => Ok(Step::Ended(fail(ctx, node, e.to_string(), false).await?)),
    }
}

/// A blackboard group's session that cannot reach the per-run MCP
/// endpoint — the node fails with it, never emulates.
#[derive(Debug, thiserror::Error)]
pub(super) enum RunToolsSetupError {
    #[error(
        "node `{node}` is in a `coordination: blackboard` group but adapter `{adapter}` declares \
         no `run_tools` capability — the blackboard cannot be mounted; pick a runner on an \
         adapter that can be a client of the per-run MCP endpoint"
    )]
    NoRunToolsCapability {
        node: yunta_core::NodeId,
        adapter: AdapterId,
    },
    #[error(
        "node `{node}` is in a `coordination: blackboard` group but its per-run MCP listener \
         failed to start: {source}"
    )]
    ListenerFailed {
        node: yunta_core::NodeId,
        #[source]
        source: std::io::Error,
    },
}

/// What [`open_run_tools`] resolved. `session` is the listener when one
/// opened; `degraded` carries the reason to record when the session
/// proceeds without run tools — the caller emits that
/// `capability_degraded` on the run's log, since this function has no
/// fallible emit of its own.
pub(super) struct RunToolsResolution {
    pub session: Option<crate::run_tools::RunToolsSession>,
    pub degraded: Option<String>,
}

/// Opens this session attempt's per-run MCP listener, or decides
/// it must not exist. A resolution with no session and no degradation —
/// no `run_tools` capability outside a blackboard group — is the resting
/// state. A resolution carrying `degraded` is the recorded fallback: the
/// listener could not bind but the node can proceed without it.
/// `Err(diagnostic)` is the fatal case: the node's group declared
/// `coordination: blackboard` and this session cannot carry it
/// (capability missing, or the listener failed to bind) — the caller
/// fails the node with it, never emulates.
pub(super) async fn open_run_tools(
    ctx: &RunCtx<'_>,
    node: &Node,
    adapter: &dyn yunta_adapters::Adapter,
    adapter_id: &AdapterId,
    task: Option<&yunta_core::TaskId>,
) -> Result<RunToolsResolution, RunToolsSetupError> {
    let host = &ctx.run_tools_host;
    let needs_blackboard = host.is_blackboard_member(&node.id);
    if !adapter.capabilities().run_tools {
        if needs_blackboard {
            return Err(RunToolsSetupError::NoRunToolsCapability {
                node: node.id.clone(),
                adapter: adapter_id.clone(),
            });
        }
        return Ok(RunToolsResolution {
            session: None,
            degraded: None,
        });
    }
    match crate::run_tools::open_session_listener(
        host.clone(),
        node.id.clone(),
        task.cloned(),
        ctx.worktree.to_path_buf(),
    )
    .await
    {
        Ok(session) => Ok(RunToolsResolution {
            session: Some(session),
            degraded: None,
        }),
        Err(e) => {
            if needs_blackboard {
                return Err(RunToolsSetupError::ListenerFailed {
                    node: node.id.clone(),
                    source: e,
                });
            }
            Ok(RunToolsResolution {
                session: None,
                degraded: Some(format!("the session runs without run tools: {e}")),
            })
        }
    }
}

async fn execute_prompt(
    ctx: &RunCtx<'_>,
    node: &Node,
    prompt: &PromptSource,
    cancel: &CancellationToken,
) -> Result<NodeEnd, RunError> {
    let rendered = match render_or_fail(ctx, node, prompt_text(ctx, node, prompt)).await? {
        Step::Value(rendered) => rendered,
        Step::Ended(end) => return Ok(end),
    };
    let context_block =
        match super::context_resolve::resolve_and_assemble(ctx, node, cancel).await? {
            Step::Value(block) => block,
            Step::Ended(end) => return Ok(end),
        };
    let rendered = match context_block {
        Some(block) => format!("{block}\n{rendered}"),
        None => rendered,
    };
    let chosen = match resolve_node_runner(ctx, node).await? {
        Step::Value(chosen) => chosen,
        Step::Ended(end) => return Ok(end),
    };

    let adapter = &ctx.adapters[&chosen.adapter];
    // Names resolved by the engine; mounting is the adapter's —
    // and an adapter without the capability degrades with an event,
    // never a fatal error (a skill is instruction, not correctness).
    let skills = match crate::skills::resolve_skills(
        &ctx.manifest.config,
        &ctx.manifest.workflow,
        node,
        ctx.worktree,
    ) {
        Ok(skills) => skills,
        Err(error) => return fail(ctx, node, error.to_string(), false).await,
    };
    let skills = if !skills.is_empty() && !adapter.capabilities().skills {
        ctx.emit(
            Some(&node.id),
            EventPayload::CapabilityDegraded(yunta_core::events::CapabilityDegradedPayload {
                capability: "skills".to_string(),
                adapter: chosen.adapter.clone(),
                policy_applied: "skills not mounted — the adapter declares no native \
                                 mechanism; the session runs without them"
                    .to_string(),
            }),
        )
        .await?;
        Vec::new()
    } else {
        skills
    };
    // A fresh listener + credential for THIS session attempt
    // when the adapter can be a client of it; `None` without the
    // capability is the resting state, not degradation — unless the
    // node sits in a `coordination: blackboard` group, whose declared
    // semantics the engine never emulates: that's a node failure.
    let run_tools = match open_run_tools(ctx, node, adapter.as_ref(), &chosen.adapter, None).await {
        Ok(resolution) => {
            if let Some(policy_applied) = resolution.degraded {
                ctx.emit(
                    Some(&node.id),
                    EventPayload::CapabilityDegraded(
                        yunta_core::events::CapabilityDegradedPayload {
                            capability: "run_tools".to_string(),
                            adapter: chosen.adapter.clone(),
                            policy_applied,
                        },
                    ),
                )
                .await?;
            }
            resolution.session
        }
        Err(error) => return fail(ctx, node, error.to_string(), false).await,
    };
    let request = SessionRequest {
        prompt: rendered,
        cwd: ctx.worktree.to_path_buf(),
        model: Some(chosen.model),
        agent: chosen.agent,
        permissions: session_profile(node),
        env: crate::task_cycle::SessionSetup::secrets_env(&ctx.manifest.config),
        edit_constraints: (!node.scope.is_empty()).then(|| node.scope.clone()),
        budget: ctx.session_budget().await?,
        adapter_settings: ctx.adapter_settings(&chosen.adapter),
        skills,
        run_tools_endpoint: run_tools.as_ref().map(|session| session.endpoint.clone()),
    };

    // An orphaned node under `resume_session` picks its
    // cut conversation back up instead of opening a new one. Anything
    // less than a clean resume — no capability, no recorded session —
    // degrades to a fresh session WITH an event, never silently.
    let policy = node
        .on_interrupt
        .unwrap_or(ctx.manifest.config.resolved_on_interrupt());
    let mut resume_session: Option<yunta_core::SessionId> = None;
    if policy == yunta_core::OnInterrupt::ResumeSession {
        match orphaned_session(&ctx.load_events().await?, &node.id) {
            OrphanedSession::Open(session_id) => {
                if adapter.capabilities().resume_session {
                    resume_session = Some(session_id);
                } else {
                    ctx.emit(
                        Some(&node.id),
                        EventPayload::CapabilityDegraded(
                            yunta_core::events::CapabilityDegradedPayload {
                                capability: "resume_session".to_string(),
                                adapter: chosen.adapter.clone(),
                                policy_applied: "restart_node — the adapter declares no                                                  session resume; a fresh session replaces                                                  the interrupted one"
                                    .to_string(),
                            },
                        ),
                    ).await?;
                }
            }
            OrphanedSession::NoneRecorded => {
                ctx.emit(
                    Some(&node.id),
                    EventPayload::CapabilityDegraded(
                        yunta_core::events::CapabilityDegradedPayload {
                            capability: "resume_session".to_string(),
                            adapter: chosen.adapter.clone(),
                            policy_applied: "restart_node — no session was recorded before                                              the interruption; started fresh"
                                .to_string(),
                        },
                    ),
                ).await?;
            }
            OrphanedSession::NotAnOrphan => {}
        }
    }

    let staged = adapter.staged_paths(&request);
    let (outcome, tokens) = dispatch_session(
        adapter.as_ref(),
        request,
        cancel,
        Some((ctx as &dyn crate::task_cycle::SessionObserver, &node.id)),
        resume_session.as_ref(),
    )
    .await
    .map_err(|error| match error {
        crate::task_cycle::DispatchError::Adapter(source) => RunError::Spawn {
            node: node.id.clone(),
            source,
        },
        crate::task_cycle::DispatchError::Audit(source) => RunError::Storage(source),
    })?;

    match outcome {
        DispatchOutcome::Completed { summary } => {
            close_node_staged(ctx, node, summary, tokens, &staged).await
        }
        DispatchOutcome::Failed { message, retryable } => {
            fail_with_tokens(ctx, node, message, retryable, tokens).await
        }
        // No terminal event means the engine synthesizes a retryable
        // failure — the adapter never invents one.
        DispatchOutcome::Crashed => {
            fail_with_tokens(
                ctx,
                node,
                "session ended without a terminal event".to_string(),
                true,
                tokens,
            )
            .await
        }
        DispatchOutcome::BudgetExceeded { reason } => {
            fail_with_tokens(ctx, node, reason, false, tokens).await
        }
        DispatchOutcome::Cancelled => cancelled_end(ctx, node).await,
    }
}

/// What resume finds in the log for `node`: the id of a session
/// cut mid-flight (this dispatch is an orphan restart — a prior
/// `node_started` with no terminal event before the current one, and an
/// `agent_session_opened` inside that window), an orphan restart with no
/// session on record (crash before it opened), or nothing to resume at
/// all (a first attempt, or a retry after a *verdict* — a failed
/// session ended with an answer, only an interrupted one is continued).
enum OrphanedSession {
    Open(yunta_core::SessionId),
    NoneRecorded,
    NotAnOrphan,
}

fn orphaned_session(
    events: &[yunta_core::events::StoredEvent],
    node_id: &yunta_core::NodeId,
) -> OrphanedSession {
    let mine = |event: &&yunta_core::events::StoredEvent| event.node_id.as_ref() == Some(node_id);
    let starts: Vec<usize> = events
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            e.node_id.as_ref() == Some(node_id)
                && matches!(e.payload(), Some(EventPayload::NodeStarted(_)))
        })
        .map(|(i, _)| i)
        .collect();
    // The caller's own `node_started` for this attempt is already on the
    // log — the *previous* start is the one that may have been cut.
    let (Some(&current), Some(&previous)) = (
        starts.last(),
        starts.len().checked_sub(2).and_then(|i| starts.get(i)),
    ) else {
        return OrphanedSession::NotAnOrphan;
    };
    let window = &events[previous..current];
    let had_verdict = window.iter().filter(mine).any(|e| {
        matches!(
            e.payload(),
            Some(EventPayload::NodeFinished(_) | EventPayload::NodeFailed(_))
        )
    });
    if had_verdict {
        return OrphanedSession::NotAnOrphan;
    }
    match window
        .iter()
        .filter(mine)
        .rev()
        .find_map(|e| match e.payload() {
            Some(EventPayload::AgentSessionOpened(p)) => Some(p.session_id.clone()),
            _ => None,
        }) {
        Some(session_id) => OrphanedSession::Open(session_id),
        None => OrphanedSession::NoneRecorded,
    }
}
