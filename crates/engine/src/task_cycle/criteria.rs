//! Running a task's criteria: each command once per tree it ran
//! against, memoized, before an attempt and after it — the pre-check in
//! the order the log priced those commands, cheapest first.

use std::path::Path;

use yunta_core::events::{CriterionType, TaskLedger};
use yunta_core::Criterion;
use yunta_core::{Task, TaskId};

use super::memo::Memo;
use super::tree::{asks_git, Tree};
use super::{CriterionRun, TaskCycleError};
use crate::process::{
    spawn_governed, CommandOutput, GovernedCommand, Outcome, Printed, Supervision,
};

/// The exit code the log records for how a criterion's command ended.
/// A command the engine stopped before it answered — a timeout, or a
/// cancellation — has no exit code of its own, so it is recorded as
/// `-2`, which no process exits with; one killed by a signal as `-1`.
pub(crate) fn exit_code_of(outcome: Outcome) -> i32 {
    match outcome {
        Outcome::Exited { status, .. } => status.code().unwrap_or(-1),
        Outcome::TimedOut { .. } | Outcome::Cancelled { .. } => -2,
    }
}

/// Why a criterion never answered, when its exit code says it did not:
/// the shell could not find its command (127), found it but could not
/// execute it (126), or the engine stopped it first (`-2`, see
/// [`exit_code_of`]). `None` for every other exit code, which is the
/// command's own answer.
///
/// A criterion that could not run is neither red nor green: no work on
/// the tree changes it, so it is never taken for a verdict about one.
pub fn could_not_run(exit_code: i32) -> Option<&'static str> {
    match exit_code {
        127 => Some("command not found"),
        126 => Some("command not executable"),
        -2 => Some("stopped before it answered"),
        _ => None,
    }
}

/// Runs one criterion command, measuring its wall-clock cost —
/// an observed fact about an external process, same standing as its
/// exit code, and recorded like one: the order a later check runs its
/// commands in is derived from the value the log carries, never
/// measured again. The injected `Clock` governs when an event happened,
/// which is not what this measures.
async fn run_criterion(
    task_id: &TaskId,
    cwd: &Path,
    cmd: &str,
    supervision: Supervision<'_>,
) -> Result<(i32, u64, CommandOutput), TaskCycleError> {
    let started = std::time::Instant::now();
    // What a criterion prints is the run's, never the terminal's: kept
    // with its check, so the person watching, the one deciding and the
    // session after this one read why it did not pass — and the view
    // the run draws is the only thing on the screen.
    let outcome = spawn_governed(GovernedCommand::shell(cwd, cmd), supervision)
        .await
        .map_err(|source| TaskCycleError::Criterion {
            task: task_id.clone(),
            cmd: cmd.to_string(),
            source,
        })?;
    let output = CommandOutput::of(&outcome);
    let exit_code = exit_code_of(outcome);
    let duration_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    Ok((exit_code, duration_ms, output))
}

/// The tree read once per call and shared across every criterion in it
/// — criteria are read-only, so the tree can't change between them, and
/// one capture beats N. `criteria` arrives already in the order the
/// caller wants executed (declared, or the learned order) — this
/// function only runs and records.
async fn run_all_criteria(
    task_id: &TaskId,
    criteria: &[Criterion],
    cwd: &Path,
    memo: &Memo,
    supervision: Supervision<'_>,
) -> Result<Vec<CriterionRun>, TaskCycleError> {
    let tree = Tree::for_criteria(criteria, cwd, supervision).await?;
    run_on(task_id, criteria, &tree, cwd, memo, supervision).await
}

/// Runs `criteria` in order on `tree`, which `cwd` holds, each through
/// the memo.
async fn run_on(
    task_id: &TaskId,
    criteria: &[Criterion],
    tree: &Tree,
    cwd: &Path,
    memo: &Memo,
    supervision: Supervision<'_>,
) -> Result<Vec<CriterionRun>, TaskCycleError> {
    let mut runs = Vec::with_capacity(criteria.len());
    for criterion in criteria {
        let key = memo.key(&criterion.cmd, tree);
        let _turn = memo.turn(&key).await;
        let (exit_code, reused, duration_ms, output) = match memo.get(&key) {
            Some(answer) => (answer.exit_code, true, None, answer.output),
            None => {
                let (exit_code, duration_ms, output) =
                    run_criterion(task_id, cwd, &criterion.cmd, supervision).await?;
                memo.put(key, exit_code, &output);
                (
                    exit_code,
                    false,
                    Some(duration_ms),
                    Some(Printed::Ran(output)),
                )
            }
        };
        runs.push(CriterionRun {
            cmd: criterion.cmd.clone(),
            exit_code,
            is_guard: criterion.r#type == Some(CriterionType::Guard),
            reused,
            duration_ms,
            output,
            tree: Some(tree.content.clone()),
            head: tree.head.clone().filter(|_| asks_git(&criterion.cmd)),
        });
    }
    Ok(runs)
}

/// The median of what the log says `cmd` cost, as a cheapest-first sort
/// key; `None` for a command the log never priced. The workspace's one
/// [`crate::stats::median`], over samples the log holds in the order
/// they were measured, truncated back to whole milliseconds.
fn median_duration(history: &TaskLedger, cmd: &str) -> Option<u64> {
    let mut sorted: Vec<f64> = history
        .criterion_durations(cmd)
        .iter()
        .map(|&ms| ms as f64)
        .collect();
    sorted.sort_by(|a, b| a.total_cmp(b));
    crate::stats::median(&sorted).map(|ms| ms as u64)
}

/// Runs every criterion of `task` on `cwd` as its pre-check would —
/// through the same cache, which keeps what they answered for that
/// tree — so what a command that could not run said can be handed back
/// to whoever wrote it.
/// What one command answers in `cwd`, through the memo: a criterion no
/// task declares, such as the one a finding proposes to prove its fix.
pub(crate) async fn probe_command(
    cmd: &str,
    cwd: &Path,
    memo: &Memo,
    supervision: Supervision<'_>,
) -> Result<CriterionRun, TaskCycleError> {
    let criterion = Criterion {
        cmd: cmd.to_string(),
        r#type: None,
        proves: None,
    };
    let task = TaskId::from_static("proposed-criterion");
    let runs = run_all_criteria(
        &task,
        std::slice::from_ref(&criterion),
        cwd,
        memo,
        supervision,
    );
    Ok(runs.await?.remove(0))
}

pub(crate) async fn probe(
    task: &Task,
    cwd: &Path,
    memo: &Memo,
    supervision: Supervision<'_>,
) -> Result<Vec<CriterionRun>, TaskCycleError> {
    run_all_criteria(&task.id, &task.criteria, cwd, memo, supervision).await
}

/// Pre-check in rojo: every non-`guard` criterion must
/// fail, every `guard` must pass. Runs every criterion regardless — the
/// report should show all of them, not stop at the first surprise.
/// Execution order is the learned one, and `history` is where it is
/// learned from: ascending median of what the log records each command
/// costing, criteria the log never priced last in declared order — the
/// fast, likely-to-fail evidence lands first while the verdict
/// (computed over the complete set) stays order-independent by
/// construction.
pub async fn pre_check(
    task: &Task,
    cwd: &Path,
    memo: &Memo,
    history: &TaskLedger,
    supervision: Supervision<'_>,
) -> Result<Vec<CriterionRun>, TaskCycleError> {
    // The suite still being measured is left out: the measurement is the
    // answer on the tree the task starts from.
    let mut ordered: Vec<&Criterion> = task
        .criteria
        .iter()
        .filter(|criterion| !memo.held().pending(criterion))
        .collect();
    // Stable sort: criteria the log never priced (u64::MAX key) keep
    // declared order among themselves.
    ordered.sort_by_key(|criterion| median_duration(history, &criterion.cmd).unwrap_or(u64::MAX));
    let ordered: Vec<Criterion> = ordered.into_iter().cloned().collect();
    run_all_criteria(&task.id, &ordered, cwd, memo, supervision).await
}

/// [`pre_check`], unless a cancellation cut it while it still read the
/// tree: then `None`. The token that fired stopped git itself, so its
/// failure says nothing about the task, which was cut before any
/// criterion ran.
pub(crate) async fn pre_check_unless_cut(
    task: &Task,
    cwd: &Path,
    memo: &Memo,
    history: &TaskLedger,
    supervision: Supervision<'_>,
) -> Result<Option<Vec<CriterionRun>>, TaskCycleError> {
    match pre_check(task, cwd, memo, history, supervision).await {
        Err(_) if supervision.cancel.is_cancelled() => Ok(None),
        ran => ran.map(Some),
    }
}

/// What a post-check answered: every criterion it ran, and the guards
/// it left waiting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostCheck {
    pub runs: Vec<CriterionRun>,
    /// The guards that did not run, by command, because a criterion the
    /// check ran before them is red.
    pub waiting: Vec<String>,
    /// The tree the checkout held, which every answer is for.
    pub tree: yunta_core::TreeId,
}

impl PostCheck {
    /// Whether the check passes: every criterion ran, and each exits 0.
    pub fn passes(&self) -> bool {
        self.waiting.is_empty() && self.runs.iter().all(|run| run.exit_code == 0)
    }
}

/// Post-check: every criterion, guard or not, must now pass. The task's
/// own criteria run first, in declared order, and its guards only once
/// every one of those passes, in their order until the first red one.
///
/// While a criterion is red the check cannot close the task whatever a
/// later guard answers, and a guard that is the project's whole suite
/// holds that same red test: run then, it costs the suite and says
/// nothing the red criterion did not. The suite still being measured goes
/// after every guard that can run now, and is waited for only once those
/// pass. A guard left waiting is in no run, so nothing reads it as passed.
pub async fn post_check(
    task: &Task,
    cwd: &Path,
    memo: &Memo,
    supervision: Supervision<'_>,
) -> Result<PostCheck, TaskCycleError> {
    let (guards, own): (Vec<Criterion>, Vec<Criterion>) =
        task.criteria.iter().cloned().partition(Criterion::is_guard);
    let tree = Tree::for_criteria(&task.criteria, cwd, supervision).await?;
    let mut runs = run_on(&task.id, &own, &tree, cwd, memo, supervision).await?;
    if !runs.iter().all(|run| run.exit_code == 0) {
        let waiting = guards.into_iter().map(|guard| guard.cmd).collect();
        return Ok(PostCheck {
            runs,
            waiting,
            tree: tree.content,
        });
    }
    let (ready, pending) = memo.held().split(guards);
    let (ran, mut waiting) = run_until_red(&task.id, &ready, &tree, cwd, memo, supervision).await?;
    let red = ran.iter().any(|run| run.exit_code != 0);
    runs.extend(ran);
    if red {
        waiting.extend(pending.into_iter().map(|guard| guard.cmd));
        return Ok(PostCheck {
            runs,
            waiting,
            tree: tree.content,
        });
    }
    // The suite still being measured is waited for here, where its answer
    // is what decides whether it holds the task at all.
    let pending = memo.held().judging(pending, supervision.cancel).await;
    let (ran, waiting) = run_until_red(&task.id, &pending, &tree, cwd, memo, supervision).await?;
    runs.extend(ran);
    Ok(PostCheck {
        runs,
        waiting,
        tree: tree.content,
    })
}

/// Runs `guards` in order on `tree` until one is red: what ran, and the
/// commands after the red one, left waiting.
async fn run_until_red(
    task_id: &TaskId,
    guards: &[Criterion],
    tree: &Tree,
    cwd: &Path,
    memo: &Memo,
    supervision: Supervision<'_>,
) -> Result<(Vec<CriterionRun>, Vec<String>), TaskCycleError> {
    let mut runs = Vec::with_capacity(guards.len());
    for (at, guard) in guards.iter().enumerate() {
        let ran = run_on(
            task_id,
            std::slice::from_ref(guard),
            tree,
            cwd,
            memo,
            supervision,
        )
        .await?;
        let red = ran.iter().any(|run| run.exit_code != 0);
        runs.extend(ran);
        if red {
            let waiting = guards.iter().skip(at + 1).map(|guard| guard.cmd.clone());
            return Ok((runs, waiting.collect()));
        }
    }
    Ok((runs, Vec::new()))
}
