//! The loop node's task cycle (T4.1 + T5.2 + T5.10 wiring, Contrato
//! §5.2/§5.5): each iteration forms a batch of up to `concurrency` `ready`
//! tasks (ledger declaration order), dispatches every member in its own
//! isolated worktree concurrently, then integrates them **serially, in
//! that same declaration order** — rebase onto the current tree,
//! re-verify criteria and scope there, and only then fast-forward the
//! run's shared worktree. `concurrency: 1` (the default) walks the exact
//! same path with a batch of one; §5.5/D65 are explicit that this needs
//! no special case.

use std::path::{Path, PathBuf};

use yunta_adapters::Budget;
use yunta_core::events::{
    CriteriaCheckedPayload, CriterionResult, CriterionType, Event, EventPayload,
    LoopIterationPayload, Phase, ScopeCheckedPayload, TaskStatus, TaskStatusChangedPayload,
    TokenUsage,
};
use yunta_core::{Isolation, Ledger, Node, NodeKind, PromptSource, Task};

use crate::replay::{derive, RunState};
use crate::scope::scope_check;
use crate::task_cycle::{post_check, run_task, CriterionRun, Memo, TaskCycleReport, TaskOutcome};
use crate::worktree::prepare_worktree;

use super::node_exec::{
    close_node, fail, fail_with_tokens, prompt_text, render_or_fail, resolve_node_runner, NodeEnd,
};
use super::{RunCtx, RunError};

pub(super) async fn execute_loop(
    ctx: &RunCtx<'_>,
    node: &Node,
    until: &str,
    prompt: &PromptSource,
) -> Result<NodeEnd, RunError> {
    if until != "all_tasks_complete" {
        return fail(
            ctx,
            node,
            format!("loop until `{until}` is not supported — M-0 only has `all_tasks_complete`"),
            false,
        );
    }
    let instruction = match render_or_fail(ctx, node, prompt_text(ctx, node, prompt))? {
        Ok(rendered) => rendered,
        Err(end) => return Ok(end),
    };
    let chosen = match resolve_node_runner(ctx, node)? {
        Ok(chosen) => chosen,
        Err(end) => return Ok(end),
    };
    let adapter = &ctx.adapters[&chosen.adapter];

    let Some(ledger) = load_registered_ledger(ctx)? else {
        return fail(
            ctx,
            node,
            "no task ledger has been registered before this loop — a previous node must \
             produce an artifact with `kind: task-ledger`"
                .to_string(),
            false,
        );
    };

    // Absent means the engine's own default, 1 — sequential, deliberately
    // not config-overridable (§5.5/D65: token spend multiplies with it,
    // so it's declared per-workflow, never inherited silently).
    let concurrency = match &node.kind {
        NodeKind::Loop { concurrency, .. } => concurrency.unwrap_or(1).max(1),
        _ => 1,
    };

    let mut tokens = TokenUsage::default();
    let mut iteration: u32 = 0;
    // Blocked reasons gathered this invocation, so the loop's own failure
    // can cite them (§6.1 for permission blocks; useful for every block).
    // A resume starts empty — the log carries each task's *status*, and
    // the generic tail below still names which tasks are blocked.
    let mut blocked_reasons: Vec<String> = Vec::new();

    loop {
        iteration += 1;
        let events = ctx.load_events()?;
        let state = derive(&events);

        let batch = select_batch(&ledger, &state, concurrency);

        if batch.is_empty() {
            let all_done = ledger
                .tasks
                .iter()
                .all(|task| state.tasks.get(&task.id) == Some(&TaskStatus::Done));
            ctx.emit(
                Some(&node.id),
                EventPayload::LoopIteration(LoopIterationPayload {
                    iteration,
                    until_result: all_done,
                }),
            )?;
            if all_done {
                return close_node(
                    ctx,
                    node,
                    format!("{} task(s) done", ledger.tasks.len()),
                    tokens,
                )
                .await;
            }
            let mut diagnostic = "no task is ready and not all are done — blocked or failed \
                                  tasks need a decision"
                .to_string();
            for reason in &blocked_reasons {
                diagnostic.push_str("; ");
                diagnostic.push_str(reason);
            }
            return fail_with_tokens(ctx, node, diagnostic, false, tokens);
        };

        // Every batch member's worktree branches from the same starting
        // point (§5.5: "worktree por tarea desde el commit base actual"),
        // captured once so all N tasks work from an identical snapshot.
        let base_commit = head_commit(ctx.worktree).await?;

        let dispatches = futures::future::join_all(batch.iter().map(|task| {
            dispatch_task_in_isolation(
                ctx,
                node,
                task,
                &events,
                &base_commit,
                &instruction,
                adapter.as_ref(),
            )
        }))
        .await;

        // Integration is serial and follows the batch's own order, which
        // is ledger declaration order (§5.5: "en orden de declaración del
        // ledger — no en orden de finalización") — never the order
        // dispatch happened to finish in.
        for dispatch in dispatches {
            let (task, task_worktree, mut report) = dispatch?;

            let mut last_check_seq = ctx.emit(
                Some(&node.id),
                EventPayload::CriteriaChecked(CriteriaCheckedPayload {
                    task_id: task.id.clone(),
                    phase: Phase::Pre,
                    results: to_results(&report.pre_check),
                }),
            )?;

            for attempt in report.attempts.drain(..) {
                tokens = sum_tokens(tokens, attempt.tokens);
                last_check_seq = ctx.emit(
                    Some(&node.id),
                    EventPayload::CriteriaChecked(CriteriaCheckedPayload {
                        task_id: task.id.clone(),
                        phase: Phase::Post,
                        results: to_results(&attempt.post_check),
                    }),
                )?;
                ctx.emit(
                    Some(&node.id),
                    EventPayload::ScopeChecked(ScopeCheckedPayload {
                        task_id: Some(task.id.clone()),
                        diff: attempt.scope.diff,
                        violations: attempt.scope.violations,
                    }),
                )?;
            }

            let blocked_reason = match report.outcome {
                TaskOutcome::Blocked { reason } => {
                    ctx.emit(
                        Some(&node.id),
                        EventPayload::TaskStatusChanged(TaskStatusChangedPayload {
                            task_id: task.id.clone(),
                            new_status: TaskStatus::Blocked,
                            caused_by: last_check_seq,
                        }),
                    )?;
                    Some(reason)
                }
                TaskOutcome::Done => {
                    let outcome = integrate_task(
                        ctx,
                        node,
                        task,
                        &task_worktree,
                        &ctx.memo,
                        &mut last_check_seq,
                    )
                    .await?;
                    // §5.5's own words: a rejected integration "vuelve a
                    // ready sobre el árbol nuevo" — back to `Pending`, so
                    // a future batch retries it automatically. This is
                    // deliberately NOT `Blocked`: green in isolation but
                    // broken by a sibling's integration is a timing
                    // artifact of concurrency, not evidence the task
                    // itself can't succeed — that verdict only comes from
                    // `run_task`'s own retry exhaustion, the branch above.
                    let new_status = match &outcome {
                        IntegrationOutcome::Integrated => TaskStatus::Done,
                        IntegrationOutcome::Rejected(reason) => {
                            tracing::warn!(
                                task_id = %task.id,
                                %reason,
                                "task integration rejected, returning to ready"
                            );
                            TaskStatus::Pending
                        }
                    };
                    ctx.emit(
                        Some(&node.id),
                        EventPayload::TaskStatusChanged(TaskStatusChangedPayload {
                            task_id: task.id.clone(),
                            new_status,
                            caused_by: last_check_seq,
                        }),
                    )?;
                    // Never counted toward the loop's own "no task ready"
                    // diagnostic — a `Pending` task is retriable, not
                    // stuck, so there's nothing to cite a human decision
                    // for yet.
                    None
                }
            };
            if let Some(reason) = blocked_reason {
                blocked_reasons.push(format!("task `{}` blocked: {reason}", task.id));
            }
        }
    }
}

/// Up to `concurrency` tasks this iteration may work on, in ledger
/// declaration order: a task whose dependencies are all `Done` and is
/// itself still `Pending`, or an orphaned `Running` task with no
/// terminal event after it (a crash mid-batch, §5.5's own resume
/// guarantee — "las que quedaron `running` huérfanas se reejecutan").
/// Scope disjointness between independent tasks is **not** re-checked
/// here: `ledger::register` (T5.1) already refuses two tasks without a
/// `depends_on` edge declaring overlapping scope, so any two tasks that
/// can both be `ready` at once are disjoint by construction.
fn select_batch<'a>(ledger: &'a Ledger, state: &RunState, concurrency: u32) -> Vec<&'a Task> {
    ledger
        .tasks
        .iter()
        .filter(|task| match state.tasks.get(&task.id) {
            Some(TaskStatus::Pending) => task
                .depends_on
                .iter()
                .all(|dep| state.tasks.get(dep) == Some(&TaskStatus::Done)),
            Some(TaskStatus::Running) => true,
            _ => false,
        })
        .take(concurrency as usize)
        .collect()
}

/// How many times this task has already been dispatched `Running` in the
/// log — 1-indexed, so the first dispatch is attempt 1. Used only to keep
/// worktree/branch names unique across a resumed orphan's fresh attempt;
/// never fed into retry-limit logic (that's `run_task`'s own
/// `max_retries`, scoped to one dispatch).
fn attempt_number(events: &[Event], task_id: &yunta_core::TaskId) -> u32 {
    events
        .iter()
        .filter(|event| {
            matches!(
                &event.payload,
                EventPayload::TaskStatusChanged(p)
                    if p.task_id == *task_id && p.new_status == TaskStatus::Running
            )
        })
        .count() as u32
        + 1
}

/// Isolates one batch member in its own worktree (§5.5: "cada tarea del
/// lote recibe su propio worktree derivado del commit base actual") and
/// runs it through the ordinary task cycle there — pre-check, dispatch,
/// post-check, scope-check, retried up to `max_task_retries` exactly as
/// the sequential path always has. Never commits or marks the task
/// `done`/`blocked` in the log itself; that's the caller's job once every
/// batch member's dispatch has settled, so integration can stay strictly
/// serial and in declaration order.
#[allow(clippy::too_many_arguments)]
async fn dispatch_task_in_isolation<'a>(
    ctx: &RunCtx<'_>,
    node: &Node,
    task: &'a Task,
    events: &[Event],
    base_commit: &str,
    instruction: &str,
    adapter: &dyn yunta_adapters::Adapter,
) -> Result<(&'a Task, PathBuf, TaskCycleReport), RunError> {
    let attempt = attempt_number(events, &task.id);
    let task_worktree = ctx
        .run_dir
        .join("task-worktrees")
        .join(format!("{}-{attempt}", task.id));
    let branch = format!("yunta/task/{}/{attempt}", task.id);
    prepare_worktree(
        ctx.worktree,
        &task_worktree,
        base_commit,
        &branch,
        Isolation::Worktree,
    )
    .await?;

    let registered_seq = events
        .iter()
        .find(|event| {
            matches!(&event.payload, EventPayload::TaskRegistered(p) if p.task_id == task.id)
        })
        .map(|event| event.seq)
        .unwrap_or(0);
    ctx.emit(
        Some(&node.id),
        EventPayload::TaskStatusChanged(TaskStatusChangedPayload {
            task_id: task.id.clone(),
            new_status: TaskStatus::Running,
            caused_by: registered_seq,
        }),
    )?;

    let report = run_task(
        task,
        instruction,
        adapter,
        &task_worktree,
        ctx.max_task_retries,
        Budget::default(),
        &ctx.memo,
        ctx.manifest.config.permissions.as_ref(),
        super::node_exec::session_profile(node),
    )
    .await?;

    Ok((task, task_worktree, report))
}

enum IntegrationOutcome {
    Integrated,
    Rejected(String),
}

/// Integrates one task verified `Done` in isolation (§5.5, D65): rebase
/// its branch onto the run's *current* integration HEAD (which may have
/// moved since this batch started, if an earlier-declared sibling
/// integrated first), re-run criteria and scope right there — green in
/// isolation is necessary, never sufficient, since a sibling's
/// integration can change the ground a task's work sits on — and only on
/// a clean re-verification does the shared worktree fast-forward onto the
/// rebased result. A rebase conflict or a post-integration criteria/scope
/// failure returns the task to `ready` on the new tree; nothing else in
/// the batch is touched.
async fn integrate_task(
    ctx: &RunCtx<'_>,
    node: &Node,
    task: &Task,
    task_worktree: &Path,
    memo: &Memo,
    last_check_seq: &mut u64,
) -> Result<IntegrationOutcome, RunError> {
    commit_task_work(task_worktree, task).await?;

    let integration_head = head_commit(ctx.worktree).await?;
    if !run_git_ok(task_worktree, &["rebase", &integration_head]).await? {
        let _ = run_git(task_worktree, &["rebase", "--abort"]).await;
        // No `post_check` ran — nothing to attach the reason to but the
        // rebase itself, so it's recorded the same way any other command
        // outcome is: a synthetic `CriterionRun` naming the git command
        // and its (failing) exit code, through the existing
        // `CriteriaChecked` vocabulary rather than a new event kind.
        *last_check_seq = ctx.emit(
            Some(&node.id),
            EventPayload::CriteriaChecked(CriteriaCheckedPayload {
                task_id: task.id.clone(),
                phase: Phase::Post,
                results: vec![CriterionResult {
                    cmd: format!("git rebase {integration_head}"),
                    exit_code: 1,
                    r#type: None,
                    reused: false,
                }],
            }),
        )?;
        return Ok(IntegrationOutcome::Rejected(format!(
            "rebase onto the integrated tree conflicted for task `{}`",
            task.id
        )));
    }

    let post_runs = post_check(task, task_worktree, memo).await?;
    *last_check_seq = ctx.emit(
        Some(&node.id),
        EventPayload::CriteriaChecked(CriteriaCheckedPayload {
            task_id: task.id.clone(),
            phase: Phase::Post,
            results: to_results(&post_runs),
        }),
    )?;
    let scope = scope_check(task_worktree, &task.scope).await?;
    ctx.emit(
        Some(&node.id),
        EventPayload::ScopeChecked(ScopeCheckedPayload {
            task_id: Some(task.id.clone()),
            diff: scope.diff.clone(),
            violations: scope.violations.clone(),
        }),
    )?;

    let criteria_green = post_runs.iter().all(|r| r.exit_code == 0);
    if !criteria_green || !scope.violations.is_empty() {
        return Ok(IntegrationOutcome::Rejected(format!(
            "criteria or scope failed after integration for task `{}`",
            task.id
        )));
    }

    let task_head = head_commit(task_worktree).await?;
    if !run_git_ok(ctx.worktree, &["merge", "--ff-only", &task_head]).await? {
        // §5.5's integration is strictly serial (the engine itself, not
        // an external actor, is the only writer to `ctx.worktree` between
        // reading `integration_head` above and this merge) — a non-fast-
        // forward here means that invariant broke, not a legitimate task
        // outcome, so it surfaces as an engine error rather than a
        // `ready` retry.
        return Err(RunError::Git {
            context: format!("fast-forward integration of task `{}`", task.id),
            detail: "expected a clean fast-forward after rebase but git refused it".to_string(),
        });
    }
    Ok(IntegrationOutcome::Integrated)
}

/// Commits a done task's work in `cwd` — the task's own isolated worktree
/// during integration (§5.5), or the run's shared worktree when
/// `concurrency` never applies. A task that changed nothing (its criteria
/// were satisfied by side effects that left no diff) simply produces no
/// commit — never an error.
async fn commit_task_work(cwd: &Path, task: &Task) -> Result<(), RunError> {
    let add = run_git(cwd, &["add", "-A"])
        .await
        .map_err(|source| RunError::Io {
            context: format!("stage task `{}` work", task.id),
            source,
        })?;
    if !add.status.success() {
        return Err(RunError::Git {
            context: format!("stage task `{}` work", task.id),
            detail: String::from_utf8_lossy(&add.stderr).trim().to_string(),
        });
    }

    let staged = run_git(cwd, &["diff", "--cached", "--quiet"])
        .await
        .map_err(|source| RunError::Io {
            context: format!("inspect staged work for task `{}`", task.id),
            source,
        })?;
    if staged.status.success() {
        return Ok(()); // nothing staged — nothing to commit
    }

    let commit = run_git(
        cwd,
        &[
            "commit",
            "-q",
            "-m",
            &format!("task {}: {}", task.id, task.title),
        ],
    )
    .await
    .map_err(|source| RunError::Io {
        context: format!("commit task `{}` work", task.id),
        source,
    })?;
    if !commit.status.success() {
        return Err(RunError::Git {
            context: format!("commit task `{}` work", task.id),
            detail: String::from_utf8_lossy(&commit.stderr).trim().to_string(),
        });
    }
    Ok(())
}

async fn run_git(cwd: &Path, args: &[&str]) -> std::io::Result<std::process::Output> {
    tokio::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .await
}

async fn run_git_ok(cwd: &Path, args: &[&str]) -> Result<bool, RunError> {
    let output = run_git(cwd, args).await.map_err(|source| RunError::Io {
        context: format!("run git {}", args.join(" ")),
        source,
    })?;
    Ok(output.status.success())
}

async fn head_commit(repo: &Path) -> Result<String, RunError> {
    let output = run_git(repo, &["rev-parse", "HEAD"])
        .await
        .map_err(|source| RunError::Io {
            context: "read the integration HEAD commit".to_string(),
            source,
        })?;
    if !output.status.success() {
        return Err(RunError::Git {
            context: "read the integration HEAD commit".to_string(),
            detail: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn to_results(runs: &[CriterionRun]) -> Vec<CriterionResult> {
    runs.iter()
        .map(|run| CriterionResult {
            cmd: run.cmd.clone(),
            exit_code: run.exit_code,
            r#type: run.is_guard.then_some(CriterionType::Guard),
            reused: run.reused,
        })
        .collect()
}

fn sum_tokens(a: TokenUsage, b: TokenUsage) -> TokenUsage {
    TokenUsage {
        input: a.input + b.input,
        output: a.output + b.output,
        cached: match (a.cached, b.cached) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
        },
    }
}

/// Finds the task ledger the run registered: the `kind: task-ledger`
/// artifact of a node that produced it earlier, re-read from the run's
/// frozen `artifacts/` (I3: artifacts are immutable once written).
fn load_registered_ledger(ctx: &RunCtx<'_>) -> Result<Option<Ledger>, RunError> {
    for node in &ctx.manifest.workflow.nodes {
        let Some(artifacts) = &node.artifacts else {
            continue;
        };
        for spec in &artifacts.produces {
            let yunta_core::ArtifactSpec::Typed { name, kind } = spec else {
                continue;
            };
            if !matches!(kind, yunta_core::ArtifactKind::TaskLedger) {
                continue;
            }
            let path = ctx.run_dir.join("artifacts").join(name);
            if !path.exists() {
                continue;
            }
            let bytes = std::fs::read(&path).map_err(|source| RunError::Io {
                context: format!("read task ledger `{}`", path.display()),
                source,
            })?;
            let ledger: Ledger =
                serde_yaml::from_slice(&bytes).map_err(|e| RunError::CorruptLedger {
                    path: path.clone(),
                    detail: e.to_string(),
                })?;
            return Ok(Some(ledger));
        }
    }
    Ok(None)
}
