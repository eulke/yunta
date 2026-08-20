//! Executing one node (T4.1 recorte) — the imperative half. Every
//! outcome, good or bad, lands in the event log; a node that cannot run
//! (undefined template variable, unresolvable runner, unsupported
//! `until`) fails *in the log* with a diagnostic, it does not abort the
//! engine — degradación explícita, jamás silenciosa.

use std::collections::BTreeMap;

use tokio_util::sync::CancellationToken;
use yunta_adapters::{PermissionProfile, SessionRequest};
use yunta_core::events::{
    EventPayload, HookExecutedPayload, HookPhase, NodeFailedPayload, NodeFinishedPayload,
    RunnerResolvedPayload, TaskStatus, TaskStatusChangedPayload, TokenUsage,
};
use yunta_core::{HookFailurePolicy, HookStep, Hooks, JoinPolicy, Node, NodeKind, PromptSource};

use crate::artifacts::close_artifacts;
use crate::replay::{derive, NodeState};
use crate::runner::resolve_runner;
use crate::scope::scope_check;
use crate::task_cycle::{dispatch_session, DispatchOutcome};
use crate::template::render_template;

use super::{RunCtx, RunError};

/// How the node's execution ended, as recorded in the log by the caller.
pub(super) enum NodeEnd {
    Finished,
    Failed,
    /// DI-11: the run's root cancellation cut this node mid-flight — no
    /// terminal event was recorded, on purpose: the node stays orphaned
    /// (`running` in the log) so a later resume re-treats it per its
    /// `on_interrupt` policy, exactly like a crash (§8.1).
    Interrupted,
}

/// DI-11: the shared "my token fired" epilogue — which cancellation was
/// it? A user/root cancel leaves the node orphaned; a `join: any`
/// sibling race records the loss so the group can close over it.
pub(super) fn cancelled_end(ctx: &RunCtx<'_>, node: &Node) -> Result<NodeEnd, RunError> {
    if ctx.root_cancel.is_cancelled() {
        return Ok(NodeEnd::Interrupted);
    }
    fail(
        ctx,
        node,
        "interrupted: a sibling in this join: any group finished first".to_string(),
        false,
    )
}

/// `cancel` only ever fires for a child of a `join: any` parallel group
/// once a sibling has won (T4.6) — every other call site passes a token
/// nothing ever cancels, so this is a no-op parameter for them.
pub(super) async fn execute_node(
    ctx: &RunCtx<'_>,
    node: &Node,
    attempt: u32,
    cancel: &CancellationToken,
) -> Result<NodeEnd, RunError> {
    ctx.emit(
        Some(&node.id),
        EventPayload::NodeStarted(yunta_core::events::NodeStartedPayload { attempt }),
    )?;

    // hooks.before (§11.1): a failing before aborts without spending a
    // token; a failing after fails the node before verification. Either
    // phase's step can opt into `on_failure: warn` instead of the default
    // `fail`, in which case a non-zero exit is recorded but doesn't stop
    // the node.
    let hooks = effective_hooks(ctx, node);
    for step in &hooks.before {
        match run_hook(ctx, node, HookPhase::Before, step).await? {
            HookRun::Violation(rule) => return fail(ctx, node, rule, false),
            HookRun::Ran(false) if step.on_failure == HookFailurePolicy::Fail => {
                return fail(
                    ctx,
                    node,
                    format!("before hook `{}` failed", step.run),
                    false,
                );
            }
            HookRun::Ran(_) => {}
        }
    }

    let end = match &node.kind {
        NodeKind::Bash { run } => execute_bash(ctx, node, run, cancel).await?,
        NodeKind::Prompt { prompt } => execute_prompt(ctx, node, prompt, cancel).await?,
        NodeKind::Loop { until, prompt, .. } => {
            super::loop_exec::execute_loop(ctx, node, until, prompt, cancel).await?
        }
        NodeKind::Parallel { join, nodes } => {
            execute_parallel(ctx, node, *join, nodes, cancel).await?
        }
        NodeKind::Check { builtin } => {
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
        // §5.6/T7.7: a gate's resolution is a forge round-trip, not a
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

/// Runs `node`'s children (§5.8, T4.6): all of them concurrently, joined
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
    let state = derive(&ctx.load_events()?);

    let already_failed: Vec<&Node> = children
        .iter()
        .filter(|child| matches!(state.nodes.get(&child.id), Some(NodeState::Failed { .. })))
        .collect();
    let to_run: Vec<&Node> = children
        .iter()
        .filter(|child| !state.nodes.contains_key(&child.id))
        .collect();

    match join {
        JoinPolicy::All => {
            if let Some(first) = already_failed.first() {
                return fail(
                    ctx,
                    node,
                    format!("child `{}` failed under join: all", first.id),
                    false,
                );
            }
            let results = futures::future::join_all(
                to_run
                    .iter()
                    .map(|child| execute_node(ctx, child, 1, &group_cancel)),
            )
            .await;

            let mut failed_child = None;
            for (child, result) in to_run.iter().zip(results) {
                match result? {
                    NodeEnd::Failed => {
                        failed_child.get_or_insert(&child.id);
                    }
                    // DI-11: a root cancellation unwound this child —
                    // the group closes nothing; the whole run is
                    // pausing, and resume re-enters it.
                    NodeEnd::Interrupted => return Ok(NodeEnd::Interrupted),
                    NodeEnd::Finished => {}
                }
            }
            if let Some(id) = failed_child {
                return fail(
                    ctx,
                    node,
                    format!("child `{id}` failed under join: all"),
                    false,
                );
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
                .map(|child| {
                    let cancel = group_cancel.clone();
                    async move { (&child.id, execute_node(ctx, child, 1, &cancel).await) }
                })
                .collect();

            let mut winner = None;
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
                    // DI-11: root cancellation, not a sibling race —
                    // drain the rest and unwind without a terminal.
                    NodeEnd::Interrupted => {
                        while running.next().await.is_some() {}
                        return Ok(NodeEnd::Interrupted);
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
                    close_node(
                        ctx,
                        node,
                        format!("`{id}` succeeded first"),
                        TokenUsage::default(),
                    )
                    .await
                }
                None => fail(
                    ctx,
                    node,
                    format!("join: any — no child succeeded ({} failed)", failures.len()),
                    false,
                ),
            }
        }
    }
}

/// Template variables for one node's own rendering (§9.3/T6.3): `run.*`
/// is always present; `runner.role` is the node's own declared `runner:`
/// (the role name itself, known statically from the workflow — never the
/// adapter/model a later resolution step picks, so no ordering
/// dependency on `resolve_node_runner`); `project.*` mirrors whatever
/// the merged config's `project:` group declares; `inputs.*` (T1.5) is
/// every declared input's already-resolved-and-validated value, read
/// straight from the frozen manifest — never re-resolved per node, since
/// that would make a `default` non-deterministic across nodes (D82).
pub(super) fn template_vars(ctx: &RunCtx<'_>, node: &Node) -> BTreeMap<String, String> {
    let mut vars = BTreeMap::from([
        ("run.dir".to_string(), ctx.run_dir.display().to_string()),
        (
            "run.worktree".to_string(),
            ctx.worktree.display().to_string(),
        ),
        // §5.6/T7.7's own example (`external.branch: "{{run.branch}}"`)
        // — a fresh push target derived from the run id, not
        // necessarily the worktree's own local checkout branch (which
        // `isolation: none` never creates one of at all, `worktree.rs`'s
        // own doc comment).
        ("run.branch".to_string(), format!("yunta/{}", ctx.run_id)),
    ]);
    if let Some(role) = &node.runner {
        vars.insert("runner.role".to_string(), role.clone());
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
pub(super) fn render_or_fail(
    ctx: &RunCtx<'_>,
    node: &Node,
    input: &str,
) -> Result<Result<String, NodeEnd>, RunError> {
    match render_template(input, &template_vars(ctx, node)) {
        Ok(rendered) => Ok(Ok(rendered)),
        Err(e) => {
            let end = fail(ctx, node, e.to_string(), false)?;
            Ok(Err(end))
        }
    }
}

/// How one hook step went: it ran (with its own success bool, before the
/// caller applies `on_failure`), or the permissions model refused it
/// outright. The distinction matters because `on_failure: warn` downgrades
/// a hook's own failure, never a governance violation (§6.1) — otherwise
/// any hook could opt out of the model by declaring itself warn-only.
pub(super) enum HookRun {
    Ran(bool),
    Violation(String),
}

/// A hook only ever fails or warns (§11.1) — unless the permissions model
/// (§6.1, T5.7) refuses its rendered command before it ever spawns.
async fn run_hook(
    ctx: &RunCtx<'_>,
    node: &Node,
    phase: HookPhase,
    step: &HookStep,
) -> Result<HookRun, RunError> {
    let rendered = match render_template(&step.run, &template_vars(ctx, node)) {
        Ok(rendered) => rendered,
        Err(e) => {
            // An unrenderable hook is a failed hook — recorded as such.
            ctx.emit(
                Some(&node.id),
                EventPayload::HookExecuted(HookExecutedPayload {
                    phase,
                    command: step.run.clone(),
                    exit_code: -1,
                }),
            )?;
            tracing::warn!(node_id = %node.id, error = %e, "hook template failed to render");
            return Ok(HookRun::Ran(false));
        }
    };

    // §6.1's runtime moment: the *rendered* command, right before it runs
    // — a template can assemble what the YAML never showed.
    if let Some(rule) =
        crate::permissions::command_violation(&rendered, ctx.manifest.config.permissions.as_ref())
    {
        return Ok(HookRun::Violation(rule));
    }

    let mut std_cmd = std::process::Command::new("sh");
    std_cmd.arg("-c").arg(&rendered).current_dir(ctx.worktree);
    // A4: a timed-out hook's whole process tree must die together, not
    // just the `sh` that ran it — same reasoning as the adapter session's
    // own process group (yunta-adapters::claude_code).
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        std_cmd.process_group(0);
    }
    let mut child = tokio::process::Command::from(std_cmd)
        .spawn()
        .map_err(|source| RunError::Io {
            context: format!("spawn hook `{rendered}`"),
            source,
        })?;
    let _pgid_registration =
        crate::process_registry::register(ctx.process_registry.as_ref(), child.id());

    let exit_code = match step.timeout_seconds.map(std::time::Duration::from_secs) {
        None => child
            .wait()
            .await
            .map_err(|source| RunError::Io {
                context: format!("run hook `{rendered}`"),
                source,
            })?
            .code()
            .unwrap_or(-1),
        Some(timeout) => match tokio::time::timeout(timeout, child.wait()).await {
            Ok(status) => status
                .map_err(|source| RunError::Io {
                    context: format!("run hook `{rendered}`"),
                    source,
                })?
                .code()
                .unwrap_or(-1),
            Err(_elapsed) => {
                if let Some(pid) = child.id() {
                    kill_process_group(pid).await;
                }
                let _ = child.wait().await;
                // Never a real process exit code (those are 0..=255) —
                // distinct from -1's "couldn't even render/run" so a
                // timeout is diagnosable from the event alone.
                -2
            }
        },
    };

    ctx.emit(
        Some(&node.id),
        EventPayload::HookExecuted(HookExecutedPayload {
            phase,
            command: rendered,
            exit_code,
        }),
    )?;
    Ok(HookRun::Ran(exit_code == 0))
}

/// The node's rung on the permissions ladder (§6.1, T5.7) mapped onto the
/// adapter's session profile — absent means the engine's long-standing
/// default, `edit`.
pub(super) fn session_profile(node: &Node) -> PermissionProfile {
    match node.permissions {
        Some(yunta_core::NodePermissions::ReadOnly) => PermissionProfile::ReadOnly,
        Some(yunta_core::NodePermissions::Full) => PermissionProfile::Full,
        Some(yunta_core::NodePermissions::Edit) | None => PermissionProfile::Edit,
    }
}

/// Sends `SIGKILL` to `pid`'s whole process group (A4) — the `--` before
/// the negative pid is load-bearing, see `claude_code::signal_group`'s
/// doc comment for the procps-ng behavior this avoids.
pub(super) async fn kill_process_group(pid: u32) {
    let _ = tokio::process::Command::new("kill")
        .arg("-KILL")
        .arg("--")
        .arg(format!("-{pid}"))
        .status()
        .await;
}

/// A node's hooks with `node_defaults.hooks` filled in per phase (§11.1):
/// a phase the node itself leaves empty inherits the workflow-level
/// default's list for that phase; a phase the node declares replaces the
/// default wholesale, the same "arrays reemplazan" rule config layers use
/// (§2.2/D52) rather than concatenating the two.
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

/// T9.4: the declared artifact names re-render with the node's own
/// template vars (`{{runner.role}}` above all), so each fan-out sibling
/// declares — and verifies — its own file. Nodes without templates in
/// their names come back unchanged.
fn render_artifact_names(ctx: &RunCtx<'_>, node: &Node) -> Result<Node, String> {
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
            *name = render_template(name, &vars).map_err(|e| e.to_string())?;
        }
    }
    Ok(rendered)
}

/// Runs after-hooks, then verifies scope and artifacts — the close
/// sequence every successful node body goes through (§11.1's order:
/// session → after → verificación).
pub(super) async fn close_node(
    ctx: &RunCtx<'_>,
    node: &Node,
    outcome: String,
    tokens: TokenUsage,
) -> Result<NodeEnd, RunError> {
    for step in &effective_hooks(ctx, node).after {
        match run_hook(ctx, node, HookPhase::After, step).await? {
            HookRun::Violation(rule) => return fail_with_tokens(ctx, node, rule, false, tokens),
            HookRun::Ran(false) if step.on_failure == HookFailurePolicy::Fail => {
                return fail_with_tokens(
                    ctx,
                    node,
                    format!("after hook `{}` failed", step.run),
                    false,
                    tokens,
                );
            }
            HookRun::Ran(_) => {}
        }
    }

    // Scope check over the node's whole diff (§6) — hook edits included.
    if !node.scope.is_empty() {
        let result = scope_check(ctx.worktree, &node.scope).await?;
        ctx.emit(
            Some(&node.id),
            EventPayload::ScopeChecked(yunta_core::events::ScopeCheckedPayload {
                task_id: None,
                diff: result.diff.clone(),
                violations: result.violations.clone(),
            }),
        )?;
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
            );
        }
    }

    let max_artifact_bytes = ctx
        .manifest
        .config
        .limits
        .as_ref()
        .and_then(|limits| limits.max_artifact_bytes);
    // T9.4: artifact names are templates too (`findings-{{runner.role}}`
    // in the reference workflow) — rendered per node so every fan-out
    // sibling verifies its own file.
    let node_rendered = match render_artifact_names(ctx, node) {
        Ok(rendered) => rendered,
        Err(detail) => return fail_with_tokens(ctx, node, detail, false, tokens),
    };
    let node = &node_rendered;
    match close_artifacts(node, ctx.run_dir, max_artifact_bytes) {
        Ok(verified) => {
            // §4.1/T5.14/DI-02: a `kind: questions` artifact's own
            // session has already closed by this point (the same
            // "artifact read only at node close" ordering
            // `task-ledger`/`findings` already rely on) — nothing
            // renders mid-session. With a live surface the questions
            // are put to the human right here (after the artifacts are
            // recorded, below); without one — or on an invalid reply —
            // the run pauses citing exactly what's unanswered, never
            // hangs, never silently proceeds as if nothing were asked
            // (A6).

            // §5.7/T5.13: a re-plan — this same node producing a task
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
                .load_events()?
                .into_iter()
                .filter_map(|event| match event.payload {
                    EventPayload::TaskRegistered(p) => Some((p.task_id, (p.criteria, p.scope))),
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
                )?;
                if let Some(ledger) = &artifact.ledger {
                    for task in &ledger.tasks {
                        let registered_seq = ctx.emit(
                            Some(&node.id),
                            EventPayload::TaskRegistered(
                                yunta_core::events::TaskRegisteredPayload {
                                    task_id: task.id.clone(),
                                    criteria: task.criteria.clone(),
                                    scope: task.scope.clone(),
                                    depends_on: task.depends_on.clone(),
                                },
                            ),
                        )?;
                        let changed_identity = previous_registrations.get(&task.id).is_some_and(
                            |(criteria, scope)| *criteria != task.criteria || *scope != task.scope,
                        );
                        if changed_identity {
                            ctx.emit(
                                Some(&node.id),
                                EventPayload::TaskStatusChanged(TaskStatusChangedPayload {
                                    task_id: task.id.clone(),
                                    new_status: TaskStatus::Pending,
                                    caused_by: registered_seq,
                                }),
                            )?;
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
                        )?;
                    }
                }
            }
            // DI-02/DI-03: unanswered questions close the node as
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
                .map(|q| q.id.clone())
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
                );
            }

            ctx.emit(
                Some(&node.id),
                EventPayload::NodeFinished(NodeFinishedPayload {
                    outcome,
                    tokens_used: tokens,
                }),
            )?;
            write_progress(ctx)?;
            Ok(NodeEnd::Finished)
        }
        Err(errors) => {
            let listed = errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; ");
            fail_with_tokens(ctx, node, listed, false, tokens)
        }
    }
}

pub(super) fn fail(
    ctx: &RunCtx<'_>,
    node: &Node,
    outcome: String,
    retryable: bool,
) -> Result<NodeEnd, RunError> {
    fail_with_tokens(ctx, node, outcome, retryable, TokenUsage::default())
}

/// Regenerates `progress.md` at `run.dir`'s root (§2, §8.2, T5.5) — the
/// engine's own call, right after the `node_finished` that triggers it
/// (§8.2's literal text names only `node_finished`, not `node_failed`, as
/// the regeneration point).
pub(super) fn write_progress(ctx: &RunCtx<'_>) -> Result<(), RunError> {
    let events = ctx.load_events()?;
    let markdown = crate::progress::render_progress(&ctx.manifest.workflow, &events);
    std::fs::write(ctx.run_dir.join("progress.md"), markdown).map_err(|source| RunError::Io {
        context: "write progress.md".to_string(),
        source,
    })
}

pub(super) fn fail_with_tokens(
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
    )?;
    Ok(NodeEnd::Failed)
}

/// Runs the bash command, cancellable (T4.6): a `join: any` sibling
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
    let rendered = match render_or_fail(ctx, node, run)? {
        Ok(rendered) => rendered,
        Err(end) => return Ok(end),
    };

    // §6.1's runtime moment: the rendered command against the merged
    // model, right before spawn — a template can assemble what the static
    // scan in `check` never saw.
    if let Some(rule) =
        crate::permissions::command_violation(&rendered, ctx.manifest.config.permissions.as_ref())
    {
        return fail(ctx, node, rule, false);
    }

    let mut std_cmd = std::process::Command::new("sh");
    std_cmd
        .arg("-c")
        .arg(&rendered)
        .current_dir(ctx.worktree)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        std_cmd.process_group(0);
    }
    let mut child = tokio::process::Command::from(std_cmd)
        .spawn()
        .map_err(|source| RunError::Io {
            context: format!("spawn bash node `{}`", node.id),
            source,
        })?;
    let _pgid_registration =
        crate::process_registry::register(ctx.process_registry.as_ref(), child.id());

    let stderr_task = child.stderr.take().map(|mut pipe| {
        tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf).await;
            buf
        })
    });
    let stdout_task = child.stdout.take().map(|mut pipe| {
        tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf).await;
            buf
        })
    });

    tokio::select! {
        _ = cancel.cancelled() => {
            if let Some(pid) = child.id() {
                kill_process_group(pid).await;
            }
            let _ = child.wait().await;
            if let Some(task) = stderr_task {
                let _ = task.await;
            }
            if let Some(task) = stdout_task {
                let _ = task.await;
            }
            cancelled_end(ctx, node)
        }
        status = child.wait() => {
            let status = status.map_err(|source| RunError::Io {
                context: format!("run bash node `{}`", node.id),
                source,
            })?;
            let stderr_bytes = match stderr_task {
                Some(task) => task.await.unwrap_or_default(),
                None => Vec::new(),
            };
            let stdout_bytes = match stdout_task {
                Some(task) => task.await.unwrap_or_default(),
                None => Vec::new(),
            };
            // §9/§11.2, T6.1: captured regardless of exit status — a
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
            }
        }
    }
}

/// The node's prompt text: frozen file content from the manifest when the
/// workflow declared `{file: ...}`, the inline string otherwise — never a
/// re-read from disk (§2.1).
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
pub(super) fn resolve_node_runner(
    ctx: &RunCtx<'_>,
    node: &Node,
) -> Result<Result<yunta_core::RunnerCandidate, NodeEnd>, RunError> {
    // DI-13: a node without `runner:` falls back to `defaults.runner`.
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
        )?;
        return Ok(Err(end));
    };

    match resolve_runner(role, &ctx.manifest.config, &|adapter| {
        ctx.adapters.contains_key(adapter)
    }) {
        Ok(resolved) => {
            let mut chosen = resolved.chosen.clone();
            // §13.3/T9.4: the node's own `agent:` wins over the
            // candidate's.
            if let Some(agent) = &node.agent {
                chosen.agent = Some(agent.clone());
            }
            // A6: an adapter without `custom_agents` fails the node
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
                            chosen.agent.as_deref().unwrap_or(""),
                            chosen.adapter
                        ),
                        false,
                    )?;
                    return Ok(Err(end));
                }
            }
            ctx.emit(
                Some(&node.id),
                EventPayload::RunnerResolved(RunnerResolvedPayload {
                    role: resolved.role.clone(),
                    chosen: chosen.clone(),
                    discarded: resolved.discarded.clone(),
                }),
            )?;
            Ok(Ok(chosen))
        }
        Err(e) => {
            let end = fail(ctx, node, e.to_string(), false)?;
            Ok(Err(end))
        }
    }
}

async fn execute_prompt(
    ctx: &RunCtx<'_>,
    node: &Node,
    prompt: &PromptSource,
    cancel: &CancellationToken,
) -> Result<NodeEnd, RunError> {
    let rendered = match render_or_fail(ctx, node, prompt_text(ctx, node, prompt))? {
        Ok(rendered) => rendered,
        Err(end) => return Ok(end),
    };
    let context_block = match super::context_resolve::resolve_and_assemble(ctx, node).await? {
        Ok(block) => block,
        Err(end) => return Ok(end),
    };
    let rendered = match context_block {
        Some(block) => format!("{block}\n{rendered}"),
        None => rendered,
    };
    let chosen = match resolve_node_runner(ctx, node)? {
        Ok(chosen) => chosen,
        Err(end) => return Ok(end),
    };

    let adapter = &ctx.adapters[&chosen.adapter];
    // DI-13: names resolved by the engine; mounting is the adapter's —
    // and an adapter without the capability degrades with an event,
    // never a fatal error (a skill is instruction, not correctness).
    let skills = match crate::skills::resolve_skills(
        &ctx.manifest.config,
        &ctx.manifest.workflow,
        node,
        ctx.worktree,
    ) {
        Ok(skills) => skills,
        Err(diagnostic) => return fail(ctx, node, diagnostic, false),
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
        )?;
        Vec::new()
    } else {
        skills
    };
    let request = SessionRequest {
        prompt: rendered,
        cwd: ctx.worktree.to_path_buf(),
        model: Some(chosen.model),
        agent: chosen.agent,
        permissions: session_profile(node),
        env: crate::task_cycle::SessionSetup::secrets_env(&ctx.manifest.config),
        edit_constraints: (!node.scope.is_empty()).then(|| node.scope.clone()),
        budget: ctx.session_budget()?,
        adapter_settings: ctx.adapter_settings(&chosen.adapter),
        skills,
    };

    let (outcome, tokens) = dispatch_session(
        adapter.as_ref(),
        request,
        cancel,
        Some((ctx as &dyn crate::task_cycle::SessionObserver, &node.id)),
    )
    .await
    .map_err(|source| RunError::Spawn {
        node: node.id.clone(),
        source,
    })?;

    match outcome {
        DispatchOutcome::Completed { summary } => close_node(ctx, node, summary, tokens).await,
        DispatchOutcome::Failed { message, retryable } => {
            fail_with_tokens(ctx, node, message, retryable, tokens)
        }
        // O2: no terminal event means the engine synthesizes a retryable
        // failure — the adapter never invents one.
        DispatchOutcome::Crashed => fail_with_tokens(
            ctx,
            node,
            "session ended without a terminal event".to_string(),
            true,
            tokens,
        ),
        DispatchOutcome::BudgetExceeded { reason } => {
            fail_with_tokens(ctx, node, reason, false, tokens)
        }
        DispatchOutcome::Cancelled => cancelled_end(ctx, node),
    }
}
