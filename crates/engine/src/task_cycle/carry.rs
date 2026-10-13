//! A cycle that continues from the work a blocked task's last attempt
//! left, when a person chose that over starting again.
//!
//! The work goes back into the fresh unit as uncommitted changes and is
//! judged before any session opens, with the same judgement an
//! attempt's close makes: a task the work already closes is done
//! without spending one, one whose criterion still cannot run stops
//! there, and only a task the work does not close goes on to sessions.
//! The first cycle's pre-check already proved the criteria red; this
//! cycle has no pre-check of its own, because its tree holds the work.

use yunta_core::events::Phase;
use yunta_core::protected::Closing;
use yunta_core::{CommitSha, ScopeGlob};

use super::attempt::AttemptParams;
use super::judge::{judge, Judgement, Work};
use super::record::Recorder;
use super::{BlockedCause, CriterionRun, TaskCycleError, TaskOutcome};
use crate::git::{Cause, GitError};
use crate::worktree::{carry_work, Carried};

/// What judging the carried work settled.
pub(super) enum Carry {
    /// The cycle ends here, before any session.
    Settled {
        outcome: TaskOutcome,
        last_check: Option<yunta_core::Seq>,
    },
    /// The work does not close the task: sessions take it from here.
    Unsettled { last_check: Option<yunta_core::Seq> },
}

/// Puts the work at `left` back into this cycle's unit and judges it.
pub(super) async fn continue_from(
    params: &AttemptParams<'_>,
    recorder: Recorder<'_>,
    left: &CommitSha,
) -> Result<Carry, TaskCycleError> {
    let &AttemptParams {
        task,
        unit,
        memo,
        supervision,
        ..
    } = params;
    let carried = carry_work(unit, left, &memo.pathspecs(), supervision)
        .await
        .map_err(|source| TaskCycleError::Carry {
            task: task.id.clone(),
            source: Box::new(source),
        })?;
    if let Carried::NoLongerApplies { stopped, paths } = carried {
        return Ok(Carry::Settled {
            last_check: recorder
                .criteria(
                    Phase::Post,
                    std::slice::from_ref(&stopped_on(&stopped, &paths)),
                )
                .await?,
            outcome: TaskOutcome::Blocked {
                cause: BlockedCause::CarriedWorkNoLongerApplies { paths },
            },
        });
    }
    // The tests the task is held to, as its spec has them, whatever the
    // carried work held of them.
    super::spec::write(task, unit, params.setup).await?;
    judge_in_place(params, recorder).await
}

/// Judges the work already in this cycle's unit — what a cycle resuming
/// the session that left it finds there — before any session opens. The
/// answer the session waited for may be all the work needed.
pub(super) async fn judge_in_place(
    params: &AttemptParams<'_>,
    recorder: Recorder<'_>,
) -> Result<Carry, TaskCycleError> {
    let judgement = judged(params).await?;
    let last_check = recorder
        .post_check(&judgement.criteria, &judgement.waiting)
        .await?;
    recorder.scope(&judgement.scope).await?;
    Ok(match settled(&judgement) {
        Some(outcome) => Carry::Settled {
            outcome,
            last_check,
        },
        None => Carry::Unsettled { last_check },
    })
}

/// The carried work, judged as an attempt's close judges its own: the
/// criteria on the unit's tree, and its diff against the scope the task
/// declared plus what the log granted it.
async fn judged(params: &AttemptParams<'_>) -> Result<Judgement, TaskCycleError> {
    let &AttemptParams {
        task,
        unit,
        memo,
        already_granted_paths,
        protected,
        supervision,
        ..
    } = params;
    let scope: Vec<ScopeGlob> = task
        .scope
        .iter()
        .chain(already_granted_paths)
        .cloned()
        .collect();
    judge(
        task,
        crate::scope::Ceiling {
            scope: &scope,
            protected,
            closing: Closing::Every,
        },
        Work { unit, staged: &[] },
        memo,
        supervision,
    )
    .await
}

/// What the carried work settles without a session: a criterion that
/// still cannot run blocks the task, work that closes it finishes it,
/// and anything else is for the sessions to take on.
fn settled(judgement: &Judgement) -> Option<TaskOutcome> {
    let unrunnable: Vec<CriterionRun> = judgement
        .criteria
        .iter()
        .filter(|run| run.could_not_run().is_some())
        .cloned()
        .collect();
    if !unrunnable.is_empty() {
        return Some(TaskOutcome::Blocked {
            cause: BlockedCause::Unrunnable { runs: unrunnable },
        });
    }
    judgement.closes().then_some(TaskOutcome::Done)
}

/// The check a pick that stopped on conflicts is recorded as — the way a
/// replay that stopped on conflicts is: the git command that stopped, as it
/// was run, and the paths it stopped on, as a criterion that did not pass.
fn stopped_on(stopped: &GitError, paths: &[std::path::PathBuf]) -> CriterionRun {
    let listed: Vec<String> = paths
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    CriterionRun {
        cmd: format!(
            "git {}: conflicts in {}",
            stopped.invocation.args.join(" "),
            listed.join(", ")
        ),
        output: None,
        // git's own exit when it gave one. A cut drain gives none, and a pick
        // that left paths unmerged exits 1, as a conflict does.
        exit_code: match &stopped.cause {
            Cause::Exit {
                code: Some(code), ..
            } => *code,
            _ => 1,
        },
        is_guard: false,
        reused: false,
        duration_ms: None,
        tree: None,
        head: None,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::stopped_on;
    use crate::git::{Cause, GitError, Invocation};

    #[test]
    fn the_finding_names_the_pick_and_every_path_it_stopped_on() {
        let stopped = GitError {
            invocation: Invocation {
                dir: PathBuf::from("unit"),
                args: ["cherry-pick", "--no-commit", "abc123"]
                    .map(String::from)
                    .to_vec(),
            },
            cause: Cause::Exit {
                code: Some(1),
                stderr: String::new(),
            },
        };

        let run = stopped_on(
            &stopped,
            &[PathBuf::from("a.txt"), PathBuf::from("dir/b.txt")],
        );

        assert_eq!(
            run.cmd,
            "git cherry-pick --no-commit abc123: conflicts in a.txt, dir/b.txt"
        );
        assert_eq!(run.exit_code, 1);
    }

    #[test]
    fn a_cut_drain_records_the_exit_a_conflicted_pick_has() {
        let stopped = GitError {
            invocation: Invocation {
                dir: PathBuf::from("unit"),
                args: ["cherry-pick", "--no-commit", "abc123"]
                    .map(String::from)
                    .to_vec(),
            },
            cause: Cause::Truncated {
                stderr: String::new(),
            },
        };

        let run = stopped_on(&stopped, &[PathBuf::from("a.txt")]);

        assert_eq!(run.exit_code, 1);
    }
}
