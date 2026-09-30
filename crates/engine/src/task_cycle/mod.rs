//! The task cycle — the part of the tasks cycle
//! that actually runs a task through pre-check, dispatch, post-check and
//! scope check. `yunta_engine::register` validates a tasks document before
//! any of this; this module is what happens once a task is `ready`.
//!
//! The engine, never the agent, decides `done`: [`run_task`] always
//! re-runs every criterion after dispatch, regardless of what the
//! session reported — an agent that claims success with red criteria
//! still leaves the task not-done.

mod attempt;
mod carry;
mod criteria;
mod error;
mod expansion;
mod judge;
mod outcome;
mod record;
mod session;
mod spec;
mod stream;

use std::path::PathBuf;
use yunta_core::ScopeGlob;

use tokio_util::sync::CancellationToken;
use yunta_core::events::{Phase, TaskLedger};
use yunta_core::port::{Adapter, Budget, PermissionProfile};
use yunta_core::Task;

pub use outcome::{
    surprises, AttemptRecord, BlockedCause, DispatchOutcome, Surprise, TaskCycleReport, TaskOutcome,
};

use crate::process::Supervision;
use attempt::{run_one_attempt, AttemptParams, AttemptStep};
pub(crate) use record::to_results;
use record::Recorder;

pub(crate) use criteria::{content_of, could_not_run, pre_check_unless_cut, probe};
pub use criteria::{post_check, pre_check, Memo, Memoized};
pub(crate) use judge::{judge, Judgement, Work};
pub(crate) use session::dispatch_session;
pub use session::{DispatchError, RunToolsNeed, SessionObserver, SessionSetup};
pub(crate) use session::{Dispatched, Opening, Resume};

pub use error::TaskCycleError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriterionRun {
    pub cmd: String,
    pub exit_code: i32,
    pub is_guard: bool,
    /// Whether this result came from the memoization cache instead of
    /// an actual execution — `criteria_checked` records it so recibo/replay
    /// show what ran versus what was reused, nothing verified in silence.
    pub reused: bool,
    /// Wall-clock milliseconds the execution took — what the
    /// learned ordering feeds on. `None` when `reused` (nothing ran).
    pub duration_ms: Option<u64>,
    /// What the command printed on this tree: in this run of it, or, for
    /// a red answer the cache reused, in the run that gave it. `None` for
    /// a green answer reused, and for a check the engine states rather
    /// than runs.
    pub output: Option<crate::process::CommandOutput>,
}

impl CriterionRun {
    /// Why this criterion never answered, if it did not.
    pub fn could_not_run(&self) -> Option<&'static str> {
        criteria::could_not_run(self.exit_code)
    }

    /// The last line it printed, if it ran and printed anything.
    pub fn said(&self) -> Option<String> {
        self.output.as_ref().and_then(|output| output.last_words())
    }

    /// How its exit code reads to a person: the code, and what it means
    /// when the command never answered.
    pub fn exit_described(&self) -> String {
        match self.could_not_run() {
            Some(why) => format!("{} ({why})", self.exit_code),
            None => self.exit_code.to_string(),
        }
    }
}

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
/// [`crate::scope_expansion::GrantLedger`] — `max_per_run` is
/// run-scoped, not task-scoped, and the ledger's atomic cap window is
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
    pub grants: &'a crate::scope_expansion::GrantLedger,
    pub already_granted_paths: &'a [ScopeGlob],
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

/// A session a cycle picks back up, and the answer it is told: what
/// changed since it stopped, which is the one reason it is continued
/// rather than started over.
#[derive(Debug, Clone, PartialEq)]
pub struct Continuing {
    pub session: yunta_core::SessionId,
    pub answer: Answer,
}

/// What a person answered a session, the one thing that changed for it.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    /// The answer to the scope it asked for.
    Scope(yunta_core::events::ScopeAnswer),
    /// A person's review of what it handed over.
    Review(Review),
    /// A person's answer to the departure from the plan it declared.
    Deviation(yunta_core::events::DeviationResolvedPayload),
}

/// A person's review of what a node handed over: the gate that asked,
/// the option that sent the run back to the node, and what they said.
#[derive(Debug, Clone, PartialEq)]
pub struct Review {
    pub gate: yunta_core::NodeId,
    pub option: yunta_core::OptionId,
    pub said: String,
}

/// Runs a task through the full cycle: pre-check once, then dispatch →
/// post-check → scope-check. A task its attempt leaves red is `Blocked`:
/// another session on the same task, tree and evidence has nothing the
/// first did not, so the next move is a person's. The one attempt that
/// follows on its own is the one a scope the engine granted during the
/// last makes different.
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
                    cause: BlockedCause::CommandDenied { rule },
                },
                needs_human_decision: false,
                last_check: recorder.criteria(Phase::Pre, &[]).await?,
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
        let last_check = recorder.criteria(Phase::Pre, &[]).await?;
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
        let last_check = recorder.criteria(Phase::Pre, &[]).await?;
        return Ok(TaskCycleReport::cut(task.id.clone(), last_check));
    };
    let spec::Laid {
        unit: overlaid,
        guarded,
    } = laid;
    let unit = &overlaid;
    let denied = spec::denied(
        permissions
            .and_then(|permissions| permissions.paths.as_ref())
            .map_or(&[], |paths| paths.deny.as_slice()),
        spec::files(setup, task),
    );
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
        denied: &denied,
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
                let last_check = recorder.criteria(Phase::Pre, &[]).await?;
                return Ok(TaskCycleReport::cut(task.id.clone(), last_check));
            };
            if let Some(guards) = guarded {
                pre_runs.splice(0..0, guards);
            }
            let last_check = recorder.criteria(Phase::Pre, &pre_runs).await?;
            // A token that fired during the pre-check stopped its commands
            // before they answered: the task was cut, not found wanting.
            // A non-guard that already passes, or a guard already red,
            // means the criteria are wrong, not the task.
            let outcome = if supervision.cancel.is_cancelled() {
                Some(TaskOutcome::Interrupted)
            } else {
                yunta_core::NonEmpty::new(surprises(task, &pre_runs)).map(|found| {
                    TaskOutcome::Blocked {
                        cause: BlockedCause::PreCheck(found),
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

    let mut attempts = Vec::new();
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
        let (staged, step) = run_one_attempt(&attempt_params, recorder, attempt).await?;
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
                let widened: Vec<ScopeGlob> = record
                    .scope_expansion
                    .iter()
                    .flat_map(|outcome| outcome.request.paths.iter().cloned())
                    .collect();
                granted.extend(widened.iter().cloned());
                continuing = record.session.clone().map(|session| Continuing {
                    session,
                    answer: Answer::Scope(yunta_core::events::ScopeAnswer::Granted(widened)),
                });
                attempts.push(record);
            }
            AttemptStep::Unmet(record) => {
                last_check = record.recorded.or(last_check);
                attempts.push(record);
                break;
            }
        }
    }

    // What the last attempt left is what a person deciding about the task
    // needs to read: which criteria still fail, and what strayed.
    let last = attempts.last();
    let cause = BlockedCause::Unmet {
        attempts: attempts.len() as u32,
        red: last
            .map(|attempt| {
                attempt
                    .post_check
                    .iter()
                    .filter(|run| run.exit_code != 0)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default(),
        outside: last
            .map(|attempt| attempt.scope.violations.clone())
            .unwrap_or_default(),
    };
    Ok(TaskCycleReport {
        task_id: task.id.clone(),
        staged: last_staged.clone(),
        pre_check: pre_runs,
        attempts,
        needs_human_decision: false,
        outcome: TaskOutcome::Blocked { cause },
        last_check,
    })
}
