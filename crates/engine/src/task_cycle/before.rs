//! What a task cycle settles ahead of its first attempt: the gates that refuse
//! or cut it, the work it carries or the session it resumes, and its pre-check.

use yunta_core::events::{Phase, TaskLedger};
use yunta_core::Task;

use super::attempt::AttemptParams;
use super::criteria::pre_check_unless_cut;
use super::record::Recorder;
use super::{carry, spec};
use super::{
    surprises, Answer, Continuing, CriterionRun, TaskCause, TaskCycleError, TaskCycleReport,
    TaskOutcome,
};

/// Where a cycle stands once what it knows ahead of its first attempt is
/// settled: the attempts start from the runs the pre-check left, or the
/// cycle has already ended.
pub(super) enum Begin {
    Attempts {
        pre_runs: Vec<CriterionRun>,
        last_check: Option<yunta_core::Seq>,
    },
    Ended(TaskCycleReport),
}

/// The report of a cycle whose task holds a criterion the permission policy
/// denies: the rule blocks the task, and no criterion runs. `None` when
/// every criterion may run.
pub(super) async fn denied(
    task: &Task,
    permissions: Option<&yunta_core::PermissionsConfig>,
    recorder: Recorder<'_>,
) -> Result<Option<TaskCycleReport>, TaskCycleError> {
    for criterion in &task.criteria {
        if let Some(rule) = crate::permissions::command_violation(&criterion.cmd, permissions) {
            return Ok(Some(TaskCycleReport {
                task_id: task.id.clone(),
                staged: Vec::new(),
                pre_check: Vec::new(),
                attempts: Vec::new(),
                outcome: TaskOutcome::Blocked {
                    cause: TaskCause::CommandDenied { rule },
                },
                needs_human_decision: false,
                last_check: recorder.criteria(Phase::Pre, &[]).await?.seq,
            }));
        }
    }
    Ok(None)
}

/// The report of a cycle a cancellation cut with no criterion answered: the
/// empty pre-check it recorded, which the status change closing it cites.
pub(super) async fn cut_report(
    task: &Task,
    recorder: Recorder<'_>,
) -> Result<TaskCycleReport, TaskCycleError> {
    let last_check = recorder.criteria(Phase::Pre, &[]).await?.seq;
    Ok(TaskCycleReport::cut(task.id.clone(), last_check))
}

/// Settles what a cycle knows ahead of its first attempt: the work it carries,
/// and the session it resumes, are judged in place; otherwise the task's
/// criteria are pre-checked.
pub(super) async fn before_attempts(
    task: &Task,
    params: &AttemptParams<'_>,
    recorder: Recorder<'_>,
    history: &TaskLedger,
    guarded: Option<Vec<CriterionRun>>,
    carry: Option<&yunta_core::CommitSha>,
    resume: &Option<Continuing>,
) -> Result<Begin, TaskCycleError> {
    // A cycle a person had continue from the work its task's last
    // attempt left judges that work before anything else: the first
    // cycle's pre-check already proved the criteria red, and a session
    // opens only when the work does not close the task. A cycle resuming
    // the session that left the work finds it already in its unit, and
    // judges it where it is.
    // A departure a person sent back is the one resume the work cannot
    // settle: what the session left is what they did not accept, however
    // its criteria read, so the session goes on.
    let sent_back = matches!(
        resume,
        Some(Continuing {
            answer: Answer::Deviation(yunta_core::events::DeviationResolvedPayload {
                accepted: false,
                ..
            }),
            ..
        })
    );
    let carried = match (carry, resume) {
        (Some(left), _) => Some(carry::continue_from(params, recorder, left).await?),
        (None, Some(_)) if sent_back => Some(carry::Carry::Unsettled { last_check: None }),
        (None, Some(_)) => Some(carry::judge_in_place(params, recorder).await?),
        (None, None) => None,
    };
    match carried {
        Some(carry::Carry::Settled {
            outcome,
            last_check,
        }) => Ok(Begin::Ended(TaskCycleReport {
            task_id: task.id.clone(),
            staged: Vec::new(),
            pre_check: Vec::new(),
            attempts: Vec::new(),
            outcome,
            needs_human_decision: false,
            last_check,
        })),
        Some(carry::Carry::Unsettled { last_check }) => Ok(Begin::Attempts {
            pre_runs: Vec::new(),
            last_check,
        }),
        None => pre_check_runs(task, params, recorder, history, guarded).await,
    }
}

/// The pre-check of a cycle's criteria on its unit's tree, with the guards that
/// answered ahead of the tests being laid in. A cut or a settled pre-check ends the
/// cycle here, with no session opened.
async fn pre_check_runs(
    task: &Task,
    params: &AttemptParams<'_>,
    recorder: Recorder<'_>,
    history: &TaskLedger,
    guarded: Option<Vec<CriterionRun>>,
) -> Result<Begin, TaskCycleError> {
    let rest = match &guarded {
        Some(_) => spec::only(task, false),
        None => task.clone(),
    };
    let Some(mut pre_runs) = pre_check_unless_cut(
        &rest,
        &params.unit.worktree,
        params.memo,
        history,
        params.supervision,
    )
    .await?
    else {
        return Ok(Begin::Ended(cut_report(task, recorder).await?));
    };
    if let Some(guards) = guarded {
        pre_runs.splice(0..0, guards);
    }
    let last_check = recorder.criteria(Phase::Pre, &pre_runs).await?.seq;
    // A token that fired during the pre-check stopped its commands
    // before they answered: the task was cut, not found wanting.
    // A non-guard that already passes, or a guard already red,
    // means the criteria are wrong, not the task.
    let outcome = if params.supervision.cancel.is_cancelled() {
        Some(TaskOutcome::Interrupted)
    } else {
        yunta_core::NonEmpty::new(surprises(task, &pre_runs)).map(|found| TaskOutcome::Blocked {
            cause: TaskCause::PreCheck { found },
        })
    };
    match outcome {
        Some(outcome) => Ok(Begin::Ended(TaskCycleReport {
            task_id: task.id.clone(),
            staged: Vec::new(),
            pre_check: pre_runs,
            attempts: Vec::new(),
            outcome,
            needs_human_decision: false,
            last_check,
        })),
        None => Ok(Begin::Attempts {
            pre_runs,
            last_check,
        }),
    }
}
