//! Running a task's criteria: each command once per tree it ran
//! against, memoized, before an attempt and after it.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use yunta_core::events::CriterionType;
use yunta_core::Criterion;
use yunta_core::{ContentHash, Task, TaskId};

use super::{CriterionRun, PreCheckOutcome, TaskCycleError};
use crate::process::{spawn_governed, Capture, GovernedCommand, Outcome, Supervision};

/// Per-run memoization cache: a criterion's result is reused when
/// its command, the working tree's content, and the resolved config are
/// all unchanged since the last time it ran *in this run*. Never
/// cross-run — a fresh `Memo` per `execute_run` call is correct, not a
/// gap: a resumed run simply starts with a cold cache and re-verifies
/// once more than strictly necessary, which is safe (over-verifying),
/// unlike a stale cross-run cache (which would risk under-verifying).
///
/// The full key is `cmd + tree_hash + declared env +
/// resolved config` — `declared env` drops out here because criteria
/// have no `env:` field in the schema yet (nothing to declare yet).
pub struct Memo {
    config_hash: ContentHash,
    cache: Mutex<HashMap<ContentHash, i32>>,
    /// Observed wall-clock durations per criterion command, this
    /// invocation only — the same lifetime discipline as the result
    /// cache above (a resume starts cold and re-learns, which only
    /// costs one declared-order pass). Keyed by the bare command, not
    /// the memo key: a criterion's cost profile survives tree changes,
    /// which is exactly when the ordering matters (a memo hit never
    /// re-runs anything, so there is nothing to reorder).
    durations: Mutex<HashMap<String, Vec<u64>>>,
}

impl Memo {
    pub fn new(config_hash: ContentHash) -> Self {
        Self {
            config_hash,
            cache: Mutex::new(HashMap::new()),
            durations: Mutex::new(HashMap::new()),
        }
    }

    fn record_duration(&self, cmd: &str, duration_ms: u64) {
        let mut durations = self.durations.lock().unwrap_or_else(|e| e.into_inner());
        durations
            .entry(cmd.to_string())
            .or_default()
            .push(duration_ms);
    }

    /// The median of this command's observed durations as a cheapest-first
    /// sort key, `None` with no history yet — the workspace's one
    /// [`crate::stats::median`], truncated back to whole milliseconds.
    fn median_duration(&self, cmd: &str) -> Option<u64> {
        let durations = self.durations.lock().unwrap_or_else(|e| e.into_inner());
        let samples = durations.get(cmd)?;
        let mut sorted: Vec<f64> = samples.iter().map(|&ms| ms as f64).collect();
        sorted.sort_by(|a, b| a.total_cmp(b));
        crate::stats::median(&sorted).map(|ms| ms as u64)
    }

    fn key(&self, cmd: &str, tree_hash: &ContentHash) -> ContentHash {
        yunta_core::sha256_hex(format!("{cmd}\x00{tree_hash}\x00{}", self.config_hash).as_bytes())
    }

    fn get(&self, cmd: &str, tree_hash: &ContentHash) -> Option<i32> {
        let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.get(&self.key(cmd, tree_hash)).copied()
    }

    fn put(&self, cmd: &str, tree_hash: &ContentHash, exit_code: i32) {
        let key = self.key(cmd, tree_hash);
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.insert(key, exit_code);
    }
}

/// A fingerprint of `cwd`'s current content: the commit it's on,
/// its full diff against that commit (tracked changes), and every
/// untracked file's own content hash — conservative on purpose. Missing
/// an untracked file's content from the fingerprint would let two
/// genuinely different trees hash the same and wrongly reuse a stale
/// result; a bare filename list (from `git status`) isn't enough since a
/// file can change content without its name changing.
async fn tree_hash(cwd: &Path) -> Result<ContentHash, TaskCycleError> {
    let run_git = |args: &'static [&'static str]| async move {
        crate::git::output(cwd, args).await.map_err(|e| {
            let detail = e.detail();
            TaskCycleError::TreeHash {
                args: e.args,
                cwd: e.cwd,
                detail,
            }
        })
    };

    let head = run_git(&["rev-parse", "HEAD"]).await?;
    let diff = run_git(&["diff", "HEAD"]).await?;
    let untracked = run_git(&["ls-files", "--others", "--exclude-standard"]).await?;

    let mut untracked_fingerprint = String::new();
    for path in untracked.lines() {
        let bytes = std::fs::read(cwd.join(path)).unwrap_or_default();
        untracked_fingerprint.push_str(path);
        untracked_fingerprint.push(':');
        untracked_fingerprint.push_str(yunta_core::sha256_hex(&bytes).as_str());
        untracked_fingerprint.push('\n');
    }

    Ok(yunta_core::sha256_hex(
        format!("{head}\n{diff}\n{untracked_fingerprint}").as_bytes(),
    ))
}

/// Runs one criterion command, measuring its wall-clock cost —
/// an observed fact about an external process, same standing as its
/// exit code; the injected `Clock` governs event timestamps and derived
/// state, neither of which this feeds.
async fn run_criterion(
    task_id: &TaskId,
    cwd: &Path,
    cmd: &str,
    supervision: Supervision<'_>,
) -> Result<(i32, u64), TaskCycleError> {
    let started = std::time::Instant::now();
    // A criterion shares the engine's streams: its output is the
    // person's to read, its exit code the engine's to record.
    let command = GovernedCommand::shell(cwd, cmd)
        .stdout(Capture::Inherit)
        .stderr(Capture::Inherit);
    let exit_code = match spawn_governed(command, supervision)
        .await
        .map_err(|source| TaskCycleError::Criterion {
            task: task_id.clone(),
            cmd: cmd.to_string(),
            source,
        })? {
        Outcome::Exited { status, .. } => status.code().unwrap_or(-1),
        // Stopped by the engine before it could answer — never a real
        // exit code, so the record says so.
        Outcome::TimedOut { .. } | Outcome::Cancelled { .. } => -2,
    };
    let duration_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    Ok((exit_code, duration_ms))
}

/// Tree hash computed once per call and shared across every criterion in
/// it — criteria are read-only, so the tree can't change between
/// them, and one `git` round-trip beats N. `criteria` arrives already in
/// the order the caller wants executed (declared, or the learned
/// order) — this function only runs and records.
async fn run_all_criteria(
    task_id: &TaskId,
    criteria: &[Criterion],
    cwd: &Path,
    memo: &Memo,
    supervision: Supervision<'_>,
) -> Result<Vec<CriterionRun>, TaskCycleError> {
    let tree_hash = tree_hash(cwd).await?;
    let mut runs = Vec::with_capacity(criteria.len());
    for criterion in criteria {
        let (exit_code, reused, duration_ms) = match memo.get(&criterion.cmd, &tree_hash) {
            Some(exit_code) => (exit_code, true, None),
            None => {
                let (exit_code, duration_ms) =
                    run_criterion(task_id, cwd, &criterion.cmd, supervision).await?;
                memo.put(&criterion.cmd, &tree_hash, exit_code);
                memo.record_duration(&criterion.cmd, duration_ms);
                (exit_code, false, Some(duration_ms))
            }
        };
        runs.push(CriterionRun {
            cmd: criterion.cmd.clone(),
            exit_code,
            is_guard: criterion.r#type == Some(CriterionType::Guard),
            reused,
            duration_ms,
        });
    }
    Ok(runs)
}

/// Pre-check in rojo: every non-`guard` criterion must
/// fail, every `guard` must pass. Runs every criterion regardless — the
/// report should show all of them, not stop at the first surprise.
/// Execution order is the learned one: ascending historical
/// median duration, criteria without history last in declared order —
/// the fast, likely-to-fail evidence lands first while the verdict
/// (computed over the complete set) stays order-independent by
/// construction.
pub async fn pre_check(
    task: &Task,
    cwd: &Path,
    memo: &Memo,
    supervision: Supervision<'_>,
) -> Result<(Vec<CriterionRun>, PreCheckOutcome), TaskCycleError> {
    let mut ordered: Vec<&Criterion> = task.criteria.iter().collect();
    // Stable sort: no-history criteria (u64::MAX key) keep declared
    // order among themselves.
    ordered.sort_by_key(|criterion| memo.median_duration(&criterion.cmd).unwrap_or(u64::MAX));
    let ordered: Vec<Criterion> = ordered.into_iter().cloned().collect();
    let runs = run_all_criteria(&task.id, &ordered, cwd, memo, supervision).await?;

    let mut outcome = PreCheckOutcome::Red;
    for run in &runs {
        if matches!(outcome, PreCheckOutcome::Red) {
            if run.is_guard && run.exit_code != 0 {
                outcome = PreCheckOutcome::BrokenGuard {
                    cmd: run.cmd.clone(),
                };
            } else if !run.is_guard && run.exit_code == 0 {
                outcome = PreCheckOutcome::TrivialCriterion {
                    cmd: run.cmd.clone(),
                };
            }
        }
    }

    Ok((runs, outcome))
}

/// Post-check: every criterion, guard or not, must now
/// pass.
pub async fn post_check(
    task: &Task,
    cwd: &Path,
    memo: &Memo,
    supervision: Supervision<'_>,
) -> Result<Vec<CriterionRun>, TaskCycleError> {
    run_all_criteria(&task.id, &task.criteria, cwd, memo, supervision).await
}
