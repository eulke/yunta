//! Dispatching one task of a batch in its own worktree, with the
//! attempt number and the expansions already granted to it.

use yunta_core::ScopeGlob;

use yunta_core::events::{EventPayload, StoredEvent, TaskStatus, TaskStatusChangedPayload};
use yunta_core::{CommitSha, Node, Task};

use crate::task_cycle::{run_task, AttemptEnv, Continuing, ScopeGovernance, TaskCycleReport};
use crate::worktree::{open_unit, reopen_unit, Unit, UnitHome, UnitId};

use crate::replay::RunState;
use crate::run::{RunCtx, RunError};
use yunta_core::events::TaskEvent;

/// How many times this task has already been dispatched `Running` in the
/// log — 1-indexed, so the first dispatch is attempt 1. Used only to keep
/// worktree/branch names unique across a resumed orphan's fresh attempt;
/// never fed into the cycle's own decision whether to dispatch again.
pub(super) fn attempt_number(state: &RunState, task_id: &yunta_core::TaskId) -> u32 {
    state.tasks.get(task_id).map_or(0, |record| record.attempts) + 1
}

/// The checkout and the session a task reopened to resume picks back
/// up: the unit holding the work its session left, and that session with
/// the answer it waited for. `None` for a task reopened any other way —
/// and for one whose checkout is gone, which then continues from the work
/// in a fresh unit, with the log saying the session was not resumed.
async fn continuation(
    ctx: &RunCtx<'_>,
    node: &Node,
    task: &Task,
    state: &RunState,
    adapter: &dyn yunta_core::port::Adapter,
) -> Result<Option<(Unit, Continuing)>, RunError> {
    let Some(record) = state.tasks.get(&task.id) else {
        return Ok(None);
    };
    let (Some(session), Some((_, work)), Some(answer)) = (
        record.resumes.clone(),
        record.left_work.as_ref(),
        state.grants.answer_for(&task.id).cloned(),
    ) else {
        return Ok(None);
    };
    let reopened = reopen_unit(
        ctx.run_dir,
        UnitId::Task(task.id.clone()),
        Some(work),
        ctx.root_supervision(),
    )
    .await?;
    let Some(unit) = reopened else {
        ctx.emit(
            Some(&node.id),
            EventPayload::Session(yunta_core::events::SessionEvent::CapabilityDegraded(
                yunta_core::events::CapabilityDegradedPayload::new(
                    yunta_core::Capability::ResumeSession,
                    adapter.id().clone(),
                    yunta_core::events::Policy::FreshSession,
                ),
            )),
        )
        .await?;
        return Ok(None);
    };
    Ok(Some((unit, Continuing { session, answer })))
}

/// Every path a prior `scope_expansion_granted` on the log authorized
/// for `task_id` — the retry after a human grant derives its
/// widened scope from here, never from in-memory state.
fn granted_paths_for(state: &RunState, task_id: &yunta_core::TaskId) -> Vec<ScopeGlob> {
    state.grants.paths_for(task_id).to_vec()
}

/// Isolates one batch member in its own worktree — each task in the
/// batch gets its own worktree derived from the current base commit —
/// and runs it through the ordinary task cycle there — pre-check,
/// dispatch, post-check, scope-check, exactly as the sequential path
/// always has. Never commits or marks
/// the task `done`/`blocked` in the log itself; that's the caller's job
/// once every batch member's dispatch has settled, so integration can
/// stay strictly serial and in declaration order.
/// Everything every member of one batch dispatches with — identical
/// across the whole `batch.iter().zip(&briefs).map(...)` fan-out that
/// calls [`dispatch_task_in_isolation`]; only `task`/`instruction` vary
/// per member, so those stay their own arguments instead of living here.
#[derive(Clone, Copy)]
pub(super) struct BatchDispatchEnv<'a> {
    pub(super) events: &'a [StoredEvent],
    pub(super) base_commit: &'a CommitSha,
    pub(super) adapter: &'a dyn yunta_core::port::Adapter,
    pub(super) scope_expansion: Option<&'a yunta_core::ScopeExpansion>,
    pub(super) grants: &'a crate::scope_expansion::GrantLedger,
    pub(super) cancel: &'a tokio_util::sync::CancellationToken,
    pub(super) setup: &'a crate::task_cycle::SessionSetup,
}

pub(super) async fn dispatch_task_in_isolation<'a>(
    ctx: &RunCtx<'_>,
    node: &Node,
    env: &BatchDispatchEnv<'_>,
    task: &'a Task,
    instruction: &str,
) -> Result<(&'a Task, Unit, TaskCycleReport), RunError> {
    let BatchDispatchEnv {
        events,
        base_commit,
        adapter,
        scope_expansion,
        grants,
        cancel,
        setup,
    } = *env;
    let state = crate::replay::derive(events);
    let attempt = attempt_number(&state, &task.id);
    let continuation = continuation(ctx, node, task, &state, adapter).await?;
    let (unit, resume) = match continuation {
        Some((unit, continuing)) => (unit, Some(continuing)),
        None => (
            open_unit(
                UnitHome {
                    repo: ctx.worktree,
                    run_dir: ctx.run_dir,
                    run_id: ctx.run_id,
                    base: base_commit,
                },
                UnitId::Task(task.id.clone()),
                attempt,
                ctx.root_supervision(),
            )
            .await?,
            None,
        ),
    };

    let registered_seq = events
        .iter()
        .find(|event| {
            matches!(
                event.payload(),
                Some(EventPayload::Tasks(TaskEvent::Registered(p))) if p.task_id == task.id
            )
        })
        .map(|event| event.seq)
        .ok_or_else(|| RunError::Broken {
            diagnostic: format!(
                "task `{}` is dispatched but the log has no task_registered for it",
                task.id
            ),
        })?;
    ctx.emit(
        Some(&node.id),
        EventPayload::Tasks(TaskEvent::StatusChanged(TaskStatusChangedPayload::to(
            task.id.clone(),
            TaskStatus::Running,
            registered_seq,
        ))),
    )
    .await?;

    let report = run_task(
        task,
        instruction,
        AttemptEnv {
            adapter,
            node,
            unit: &unit,
            budget: ctx.session_budget().await?,
            memo: &ctx.memo,
            history: &state.tasks,
            supervision: ctx.supervision(cancel),
            // A task a person reopened to continue carries the work it
            // continues from on the reopening itself — unless the session
            // that left it picks it back up in the unit that holds it.
            carry: state
                .tasks
                .get(&task.id)
                .and_then(|record| record.left_work.as_ref())
                .map(|(_, work)| work)
                .filter(|_| resume.is_none()),
            resume,
        },
        ScopeGovernance {
            permissions: ctx.manifest.config.permissions.as_ref(),
            profile: crate::run::node_exec::session_profile(node),
            scope_expansion,
            max_expansion_files: ctx.manifest.config.resolved_max_expansion_files(),
            grants,
            already_granted_paths: &granted_paths_for(&state, &task.id),
        },
        Some((ctx as &dyn crate::task_cycle::SessionObserver, &node.id)),
        cancel,
        setup,
    )
    .await?;

    Ok((task, unit, report))
}
