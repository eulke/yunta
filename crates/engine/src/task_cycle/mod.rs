//! The task cycle — the part of the tasks cycle
//! that actually runs a task through pre-check, dispatch, post-check and
//! scope check. `yunta_engine::register` validates a tasks document before
//! any of this; this module is what happens once a task is `ready`.
//!
//! The engine, never the agent, decides `done`: [`run_task`] always
//! re-runs every criterion after dispatch, regardless of what the
//! session reported — an agent that claims success with red criteria
//! still leaves the task not-done.

mod answer;
mod attempt;
mod carry;
mod criteria;
mod dispatch;
mod error;
mod evidence;
mod held;
mod judge;
mod memo;
mod open;
mod outcome;
mod record;
mod recorded;
mod session;
mod spec;
mod stream;
mod tree;

use std::path::PathBuf;
use std::sync::Arc;
use yunta_core::ScopeGlob;

use tokio_util::sync::CancellationToken;
use yunta_core::events::{Phase, TaskLedger, TokenUsage};
use yunta_core::expansion::Decision;
use yunta_core::port::{Adapter, Budget, PermissionProfile};
use yunta_core::protected::Protected;
use yunta_core::Task;

pub use outcome::{
    surprises, AttemptRecord, CriterionRun, DispatchOutcome, TaskCycleReport, TaskOutcome,
};
pub use yunta_core::events::{Surprise, TaskCause};

use crate::process::Supervision;
use attempt::{run_one_attempt, AttemptParams, AttemptStep};
pub(crate) use record::recorded;
use record::Recorder;

pub(crate) use criteria::{
    could_not_run, exit_code_of, pre_check_unless_cut, probe, probe_command, unanswered_words,
};
pub use criteria::{post_check, pre_check, PostCheck};
pub(crate) use dispatch::not_stopped_failure;
pub use dispatch::DispatchError;
pub(crate) use dispatch::{dispatch_session, Dispatched, Opening, Resume};
pub use held::HeldGate;
pub(crate) use judge::{
    judge, scope_in_force, Documents, JudgeEnv, Judgement, Refusals, Settle, Sitting, Work,
};
pub use memo::{Memo, Memoized};
pub use session::{KeepsOutput, RunToolsNeed, SessionObserver, SessionSetup};
pub(crate) use stream::record_unlisted_tools;
pub(crate) use tree::content_of;

pub use answer::{Answer, Continuing, RespecifiedTask, Respecify, Review};
pub use error::TaskCycleError;
pub(crate) use spec::files as spec_files;

/// The permission/scope policy a task cycle enforces:
/// `permissions` is the merged model every criterion command is checked
/// against before anything runs — a violating criterion blocks the whole
/// task citing the rule (a policy outcome in the report, never an engine
/// abort), scanned here at the cycle's single entry point so the
/// standalone [`pre_check`]/[`post_check`] helpers stay pure building
/// blocks. `profile` is the node's own rung of the same permission
/// ladder, forwarded to every session this cycle opens. `scope_expansion`
/// carries the loop node's own settings (absent means the schema's
/// own default, `deny`); `grants` is the batch's shared
/// [`crate::scope_expansion::GrantWindow`] — `max_per_run` is
/// run-scoped, not task-scoped, and the window's atomic cap check is
/// what makes the count exact when several batch members request at
/// once. `already_granted_paths` are the paths every *prior*
/// `scope_expansion_granted` on the log authorized for this task — a
/// human grant lands between attempts, so the retry's effective scope
/// must include them from the very first diff it evaluates.
pub struct ScopeGovernance<'a> {
    pub permissions: Option<&'a yunta_core::PermissionsConfig>,
    pub profile: PermissionProfile,
    pub scope_expansion: Option<&'a yunta_core::ScopeExpansion>,
    /// The run's `limits.max_expansion_files` ceiling, resolved once by
    /// the caller — a `rules`-mode request touching more files than this
    /// is denied.
    pub max_expansion_files: usize,
    pub grants: &'a crate::scope_expansion::GrantWindow,
    pub already_granted_paths: &'a [ScopeGlob],
    /// What the other tasks of the batch may write: a path the evidence
    /// locates there is a person's to grant, never the engine's.
    pub beside: &'a [ScopeGlob],
    /// What no session may write, lifted by what a person granted this task.
    pub protected: Arc<Protected>,
}

/// The resources and retry policy one task's attempts run under —
/// distinct from [`ScopeGovernance`] (what the session may touch) and
/// from the cross-cutting `audit`/`cancel`/`setup` surfaces (who's
/// watching and how it stops).
pub struct AttemptEnv<'a> {
    pub adapter: &'a dyn Adapter,
    /// The loop node these task sessions belong to. A task session is
    /// the node's session: it runs on the node's runner and writes the
    /// node's declared files.
    pub node: &'a yunta_core::Node,
    /// The tree this task works in and the tree it started from. Every
    /// attempt runs in the same checkout and is judged against the same
    /// starting point, which is what makes one attempt answerable for
    /// what an earlier one of its own left behind.
    pub unit: &'a crate::worktree::Unit,
    pub budget: Budget,
    pub memo: &'a Memo,
    /// The run's tasks, as its log leaves them — what the pre-check
    /// reads to run the cheap criteria before the expensive ones,
    /// derived from the same log every wake derives its state from.
    pub history: &'a TaskLedger,
    /// What every subprocess of the cycle is born under: the run's
    /// registry, the node's token, the run's `subprocess_vars` and the
    /// run's clock. It reaches the spawn by parameter, so a criterion
    /// runs under the same governance as the session before it.
    pub supervision: Supervision<'a>,
    /// The work a person chose to have this cycle continue from: what
    /// the task's last attempt left, put back into this unit and judged
    /// before any session opens. `None` for a cycle that starts from the
    /// unit's own tree.
    pub carry: Option<&'a yunta_core::CommitSha>,
    /// The session this cycle picks back up after the answer to the scope
    /// it asked for, instead of opening its first session fresh. `None`
    /// for a cycle that starts a conversation of its own.
    pub resume: Option<Continuing>,
}

/// Runs a task through the full cycle: pre-check once, then dispatch →
/// post-check → scope-check. An attempt that leaves the task unsettled
/// starts another session only when the engine granted the scope the session
/// asked for, or when the session left a request pending under a red criterion
/// on the first attempt or on a tree its previous attempt did not leave. Any
/// other unsettled end is `Blocked`: a session on the same tree would have
/// nothing new to act on. A criterion the session declared impossible ends
/// the cycle at once, before any retry.
///
/// Never trusts the session's own outcome: `succeeded` on each
/// attempt is decided entirely by re-running criteria and the scope
/// diff, regardless of whether the session reported `Completed`.
#[tracing::instrument(
    skip_all,
    fields(
        task = %task.id,
        node_id = audit.map(|(_, node)| node.as_str()).unwrap_or_default(),
    )
)]
pub async fn run_task(
    task: &Task,
    instruction: &str,
    env: AttemptEnv<'_>,
    governance: ScopeGovernance<'_>,
    audit: Option<(&dyn SessionObserver, &yunta_core::NodeId)>,
    cancel: &CancellationToken,
    setup: &SessionSetup,
) -> Result<TaskCycleReport, TaskCycleError> {
    let mut report = cycle(task, instruction, env, governance, audit, cancel, setup).await?;
    // The files its tests live in are in its unit's tree and are not its
    // work, so integration leaves them out of its audit.
    report.staged.extend(spec::paths(spec::files(setup, task)));
    Ok(report)
}

/// The cycle [`run_task`] reports on.
async fn cycle(
    task: &Task,
    instruction: &str,
    env: AttemptEnv<'_>,
    governance: ScopeGovernance<'_>,
    audit: Option<(&dyn SessionObserver, &yunta_core::NodeId)>,
    cancel: &CancellationToken,
    setup: &SessionSetup,
) -> Result<TaskCycleReport, TaskCycleError> {
    // What the adapter declares it stages, per attempt; nothing before
    // a session opens.
    let mut last_staged: Vec<PathBuf> = Vec::new();
    let AttemptEnv {
        adapter,
        node,
        unit,
        budget,
        memo,
        history,
        supervision,
        carry,
        resume,
    } = env;
    let ScopeGovernance {
        permissions,
        profile,
        scope_expansion,
        max_expansion_files,
        grants,
        already_granted_paths,
        beside,
        protected,
    } = governance;
    // Every check reaches the log the moment it runs. A cycle records a
    // pre-check even when it ran no criterion — denied, or cut before it
    // began — because the status change that closes the cycle cites it.
    let recorder = Recorder {
        audit,
        task: &task.id,
    };
    for criterion in &task.criteria {
        if let Some(rule) = crate::permissions::command_violation(&criterion.cmd, permissions) {
            return Ok(TaskCycleReport {
                task_id: task.id.clone(),
                staged: last_staged.clone(),
                pre_check: Vec::new(),
                attempts: Vec::new(),
                outcome: TaskOutcome::Blocked {
                    cause: TaskCause::CommandDenied { rule },
                },
                needs_human_decision: false,
                last_check: recorder.criteria(Phase::Pre, &[]).await?.seq,
            });
        }
    }

    // A cycle whose token already fired has nothing to verify: every
    // subprocess the pre-check would run is governed by that same token,
    // so it would only produce "killed before it could answer" for the
    // caller to read as a verdict. A `join: any` sibling winning between
    // the batch starting and this task's first check is exactly that
    // case: the task was cut, not judged.
    if supervision.cancel.is_cancelled() {
        let last_check = recorder.criteria(Phase::Pre, &[]).await?.seq;
        return Ok(TaskCycleReport::cut(task.id.clone(), last_check));
    }

    let start = spec::Start {
        task,
        unit,
        setup,
        fresh: carry.is_none() && resume.is_none(),
        carried: carry.is_some(),
    };
    let Some(laid) = spec::laid(start, memo, history, supervision).await? else {
        let last_check = recorder.criteria(Phase::Pre, &[]).await?.seq;
        return Ok(TaskCycleReport::cut(task.id.clone(), last_check));
    };
    let spec::Laid {
        unit: overlaid,
        guarded,
    } = laid;
    let unit = &overlaid;
    let params = AttemptParams {
        task,
        instruction,
        adapter,
        node,
        unit,
        budget,
        memo,
        profile,
        scope_expansion,
        max_expansion_files,
        grants,
        already_granted_paths,
        beside,
        protected: &protected,
        resume: None,
        audit,
        cancel,
        setup,
        supervision,
    };

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
    let carried = match (carry, &resume) {
        (Some(left), _) => Some(carry::continue_from(&params, recorder, left).await?),
        (None, Some(_)) if sent_back => Some(carry::Carry::Unsettled { last_check: None }),
        (None, Some(_)) => Some(carry::judge_in_place(&params, recorder).await?),
        (None, None) => None,
    };
    let (pre_runs, mut last_check) = match carried {
        Some(carried) => match carried {
            carry::Carry::Settled {
                outcome,
                last_check,
            } => {
                return Ok(TaskCycleReport {
                    task_id: task.id.clone(),
                    staged: Vec::new(),
                    pre_check: Vec::new(),
                    attempts: Vec::new(),
                    outcome,
                    needs_human_decision: false,
                    last_check,
                })
            }
            carry::Carry::Unsettled { last_check } => (Vec::new(), last_check),
        },
        None => {
            let rest = match &guarded {
                Some(_) => spec::only(task, false),
                None => task.clone(),
            };
            let Some(mut pre_runs) =
                pre_check_unless_cut(&rest, &unit.worktree, memo, history, supervision).await?
            else {
                let last_check = recorder.criteria(Phase::Pre, &[]).await?.seq;
                return Ok(TaskCycleReport::cut(task.id.clone(), last_check));
            };
            if let Some(guards) = guarded {
                pre_runs.splice(0..0, guards);
            }
            let last_check = recorder.criteria(Phase::Pre, &pre_runs).await?.seq;
            // A token that fired during the pre-check stopped its commands
            // before they answered: the task was cut, not found wanting.
            // A non-guard that already passes, or a guard already red,
            // means the criteria are wrong, not the task.
            let outcome = if supervision.cancel.is_cancelled() {
                Some(TaskOutcome::Interrupted)
            } else {
                yunta_core::NonEmpty::new(surprises(task, &pre_runs)).map(|found| {
                    TaskOutcome::Blocked {
                        cause: TaskCause::PreCheck { found },
                    }
                })
            };
            if let Some(outcome) = outcome {
                return Ok(TaskCycleReport {
                    task_id: task.id.clone(),
                    staged: last_staged.clone(),
                    pre_check: pre_runs,
                    attempts: Vec::new(),
                    outcome,
                    needs_human_decision: false,
                    last_check,
                });
            }
            (pre_runs, last_check)
        }
    };

    let mut attempts: Vec<AttemptRecord> = Vec::new();
    // What the engine grants during an attempt holds for every attempt
    // after it: the one it is granted for is the next.
    let mut granted: Vec<ScopeGlob> = params.already_granted_paths.to_vec();
    // The session the next attempt picks back up: the one this cycle was
    // reopened to continue, then the one an engine grant widened.
    let mut continuing = resume;
    for attempt in 1.. {
        let attempt_params = AttemptParams {
            already_granted_paths: &granted,
            resume: continuing.as_ref(),
            ..params
        };
        // An attempt that cannot stop its session reports the spend of the
        // attempts before it too: the node that ran this cycle pays for them.
        let earlier: TokenUsage = attempts.iter().map(|record| record.tokens).sum();
        let (staged, step) = run_one_attempt(&attempt_params, recorder, attempt)
            .await
            .map_err(|error| error.with_earlier_spend(earlier))?;
        last_staged = staged;
        match step {
            AttemptStep::Stop {
                record,
                outcome,
                needs_human_decision,
            } => {
                last_check = record.recorded.or(last_check);
                attempts.push(record);
                return Ok(TaskCycleReport {
                    task_id: task.id.clone(),
                    staged: last_staged,
                    pre_check: pre_runs,
                    attempts,
                    outcome,
                    needs_human_decision,
                    last_check,
                });
            }
            AttemptStep::Again(record) => {
                last_check = record.recorded.or(last_check);
                let widened = match record.scope_expansion.as_ref().map(|o| &o.decision) {
                    Some(Decision::Grant { paths, .. }) => paths.clone(),
                    _ => Vec::new(),
                };
                granted.extend(widened.iter().cloned());
                continuing = record.session.clone().map(|session| Continuing {
                    session,
                    answer: Answer::Scope(yunta_core::events::ScopeAnswer::Granted(widened)),
                });
                attempts.push(record);
            }
            AttemptStep::Unmet(record) => {
                last_check = record.recorded.or(last_check);
                let fresh = record.fresh_session_owed(&attempts);
                attempts.push(record);
                if !fresh {
                    break;
                }
                // The pending request stays on the checkout for the session
                // that follows, which starts fresh and carries no grant.
                continuing = None;
            }
        }
    }

    // What the last attempt left is what a person deciding about the task
    // needs to read: which criteria still fail, and what strayed. A request
    // still owed a person's decision is that decision, and it leaves the
    // checkout with the cycle.
    let owed = attempts.last().is_some_and(AttemptRecord::escalated);
    let cause = match owed {
        true => {
            crate::scope_expansion::consume_request(unit.worktree.as_path())
                .await
                .map_err(|source| TaskCycleError::ScopeExpansion {
                    task: task.id.clone(),
                    source,
                })?;
            TaskCause::ScopeDecisionOwed
        }
        false => unmet_cause(&attempts),
    };
    Ok(TaskCycleReport {
        task_id: task.id.clone(),
        staged: last_staged.clone(),
        pre_check: pre_runs,
        attempts,
        needs_human_decision: owed,
        outcome: TaskOutcome::Blocked { cause },
        last_check,
    })
}

/// What the last attempt left: the criteria still red, and the paths it
/// changed outside the scope.
fn unmet_cause(attempts: &[AttemptRecord]) -> TaskCause {
    let last = attempts.last();
    TaskCause::Unmet {
        attempts: attempts.len() as u32,
        red: last
            .map(|attempt| {
                attempt
                    .post_results
                    .iter()
                    .filter(|result| result.exit_code != 0)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default(),
        outside: last
            .map(|attempt| attempt.scope.violations.clone())
            .unwrap_or_default(),
    }
}
