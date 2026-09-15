//! Running a task's criteria: each command once per tree it ran
//! against, memoized, before an attempt and after it — the pre-check in
//! the order the log priced those commands, cheapest first.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use yunta_core::events::{CriterionType, TaskLedger};
use yunta_core::Criterion;
use yunta_core::{ContentHash, Task, TaskId};

use super::{CriterionRun, TaskCycleError};
use crate::process::{spawn_governed, Capture, GovernedCommand, Outcome, Supervision};

/// Per-invocation memoization cache: a criterion's result is reused
/// when its command, the working tree's content and the resolved config
/// are all unchanged since the last time it ran *in this invocation*.
/// One `Memo` per `execute_run` call is what the cache is for: a
/// resumed run starts cold and verifies once more than strictly
/// necessary, which is safe (over-verifying), unlike a result carried
/// across invocations onto a tree no event in between speaks for (which
/// would risk under-verifying).
///
/// The key is `sha256(cmd \0 tree_hash \0 config_hash)`: the command as
/// written, a fingerprint of the tree it would run against, and the hash
/// of the resolved config it runs under — the three things its exit code
/// can turn on, since a criterion declares no `env:` of its own.
pub struct Memo {
    config_hash: ContentHash,
    cache: Mutex<HashMap<ContentHash, i32>>,
}

impl Memo {
    pub fn new(config_hash: ContentHash) -> Self {
        Self {
            config_hash,
            cache: Mutex::new(HashMap::new()),
        }
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

    /// The exit code of `cmd` on `cwd` as this invocation already knows
    /// it, or by running it now: what a criterion and a
    /// `baseline_compare` share, so a suite two comparisons ask about
    /// runs once while the tree stands still.
    pub(crate) async fn exit_code(
        &self,
        cmd: &str,
        cwd: &Path,
        supervision: Supervision<'_>,
    ) -> Result<Memoized, TaskCycleError> {
        let tree_hash = tree_hash(cwd, supervision).await?;
        if let Some(exit_code) = self.get(cmd, &tree_hash) {
            return Ok(Memoized {
                exit_code,
                reused: true,
            });
        }
        let command = GovernedCommand::shell(cwd, cmd)
            .stdout(Capture::Inherit)
            .stderr(Capture::Inherit);
        let exit_code = match spawn_governed(command, supervision)
            .await
            .map_err(|source| TaskCycleError::MemoizedCommand {
                cmd: cmd.to_string(),
                source,
            })? {
            Outcome::Exited { status, .. } => status.code().unwrap_or(-1),
            Outcome::TimedOut { .. } | Outcome::Cancelled { .. } => -2,
        };
        self.put(cmd, &tree_hash, exit_code);
        Ok(Memoized {
            exit_code,
            reused: false,
        })
    }
}

/// What a memoized command answered, and whether this invocation had to
/// run it to find out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Memoized {
    pub exit_code: i32,
    pub reused: bool,
}

/// A fingerprint of `cwd`'s current content: the commit it's on,
/// its full diff against that commit (tracked changes), and every
/// untracked file's own content hash — conservative on purpose. Missing
/// an untracked file's content from the fingerprint would let two
/// genuinely different trees hash the same and wrongly reuse a stale
/// result; a bare filename list (from `git status`) isn't enough since a
/// file can change content without its name changing.
async fn tree_hash(
    cwd: &Path,
    supervision: Supervision<'_>,
) -> Result<ContentHash, TaskCycleError> {
    let run_git = |args: &'static [&'static str]| async move {
        crate::git::output(cwd, args, supervision)
            .await
            .map_err(|e| {
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
        let bytes = tokio::fs::read(cwd.join(path)).await.unwrap_or_default();
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
/// exit code, and recorded like one: the order a later check runs its
/// commands in is derived from the value the log carries, never
/// measured again. The injected `Clock` governs when an event happened,
/// which is not what this measures.
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
    let tree_hash = tree_hash(cwd, supervision).await?;
    let mut runs = Vec::with_capacity(criteria.len());
    for criterion in criteria {
        let (exit_code, reused, duration_ms) = match memo.get(&criterion.cmd, &tree_hash) {
            Some(exit_code) => (exit_code, true, None),
            None => {
                let (exit_code, duration_ms) =
                    run_criterion(task_id, cwd, &criterion.cmd, supervision).await?;
                memo.put(&criterion.cmd, &tree_hash, exit_code);
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
    let mut ordered: Vec<&Criterion> = task.criteria.iter().collect();
    // Stable sort: criteria the log never priced (u64::MAX key) keep
    // declared order among themselves.
    ordered.sort_by_key(|criterion| median_duration(history, &criterion.cmd).unwrap_or(u64::MAX));
    let ordered: Vec<Criterion> = ordered.into_iter().cloned().collect();
    run_all_criteria(&task.id, &ordered, cwd, memo, supervision).await
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
