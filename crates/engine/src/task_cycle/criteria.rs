//! Running a task's criteria: each command once per tree it ran
//! against, memoized, before an attempt and after it — the pre-check in
//! the order the log priced those commands, cheapest first.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use yunta_core::events::{CriterionType, TaskLedger};
use yunta_core::Criterion;
use yunta_core::{ContentHash, Task, TaskId, TreeId};

use super::{CriterionRun, TaskCycleError};
use crate::process::{spawn_governed, CommandOutput, GovernedCommand, Outcome, Supervision};

/// Per-invocation memoization cache: a criterion's result is reused
/// when its command, the working tree's content and the resolved config
/// are all unchanged since the last time it ran *in this invocation*.
/// One `Memo` per `execute_run` call is what the cache is for: a
/// resumed run starts cold and verifies once more than strictly
/// necessary, which is safe (over-verifying), unlike a result carried
/// across invocations onto a tree no event in between speaks for (which
/// would risk under-verifying).
///
/// The key is `sha256(cmd \0 content \0 head \0 config_hash)`: the
/// command as written, the git tree of what the checkout holds, and the
/// hash of the resolved config it runs under — the things its exit code
/// can turn on, since a criterion declares no `env:` of its own. `head`
/// is the commit the checkout stands on, and counts only for a command
/// that runs `git`: that one can read history, where any other reads
/// files, so the same content committed, or replayed onto a base that
/// did not move, keeps its answer.
pub struct Memo {
    config_hash: ContentHash,
    cache: Mutex<HashMap<ContentHash, Answer>>,
    /// One slot per key a check is asking about: two checks of one
    /// command on one tree at the same moment — the tasks of a batch,
    /// each pre-checking the suite on the commit they all start from —
    /// take turns, and the second reuses what the first answered instead
    /// of running the command alongside it.
    asking: Mutex<HashMap<ContentHash, std::sync::Arc<tokio::sync::Mutex<()>>>>,
}

/// What a command answered on one tree. A red answer keeps what the
/// command printed, so a check that reuses it still says why it fails —
/// the common case is a session's `yunta_check_task` running the
/// criteria and the close reusing them on the same tree. A green one
/// keeps nothing to explain.
#[derive(Clone)]
struct Answer {
    exit_code: i32,
    output: Option<CommandOutput>,
}

impl Memo {
    pub fn new(config_hash: ContentHash) -> Self {
        Self {
            config_hash,
            cache: Mutex::new(HashMap::new()),
            asking: Mutex::new(HashMap::new()),
        }
    }

    fn key(&self, cmd: &str, tree: &Tree) -> ContentHash {
        let head = if asks_git(cmd) {
            tree.head.as_deref().unwrap_or_default()
        } else {
            ""
        };
        yunta_core::sha256_hex(
            format!(
                "{cmd}\x00{}\x00{head}\x00{}",
                tree.content, self.config_hash
            )
            .as_bytes(),
        )
    }

    /// Takes the turn to ask about `key`: held while the caller reads
    /// the cache and, on a miss, runs the command and records what it
    /// answered.
    async fn turn(&self, key: &ContentHash) -> tokio::sync::OwnedMutexGuard<()> {
        let slot = {
            let mut asking = self.asking.lock().unwrap_or_else(|e| e.into_inner());
            asking.entry(key.clone()).or_default().clone()
        };
        slot.lock_owned().await
    }

    fn get(&self, key: &ContentHash) -> Option<Answer> {
        let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.get(key).cloned()
    }

    /// Remembers what a command answered under `key` — unless it could
    /// not run at all. A command that was not found says nothing about
    /// the tree, and the next check on the same tree must run it again:
    /// the program may be on the `PATH` by then.
    fn put(&self, key: ContentHash, exit_code: i32, output: &CommandOutput) {
        if could_not_run(exit_code).is_some() {
            return;
        }
        let answer = Answer {
            exit_code,
            output: (exit_code != 0).then(|| output.clone()),
        };
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.insert(key, answer);
    }

    /// What `cmd`'s answer on `cwd` is kept under, read as `cwd` stands
    /// now — for a caller that runs the command some other way and
    /// hands the answer to [`Memo::passed`].
    pub(crate) async fn key_on(
        &self,
        cmd: &str,
        cwd: &Path,
        supervision: Supervision<'_>,
    ) -> Result<ContentHash, TaskCycleError> {
        let tree = Tree::of(cwd, asks_git(cmd), supervision).await?;
        Ok(self.key(cmd, &tree))
    }

    /// Remembers that a command passed under `key`, measured by a
    /// caller rather than a check: the suite a run measures before its
    /// first node, which every task is then held to.
    pub(crate) fn passed(&self, key: ContentHash) {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.insert(
            key,
            Answer {
                exit_code: 0,
                output: None,
            },
        );
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
        let key = self.key_on(cmd, cwd, supervision).await?;
        let _turn = self.turn(&key).await;
        if let Some(answer) = self.get(&key) {
            return Ok(Memoized {
                exit_code: answer.exit_code,
                reused: true,
                output: answer.output,
            });
        }
        // What the command prints is the run's to keep, never the
        // terminal's: the person watching reads the run's own view.
        let outcome = spawn_governed(GovernedCommand::shell(cwd, cmd), supervision)
            .await
            .map_err(|source| TaskCycleError::MemoizedCommand {
                cmd: cmd.to_string(),
                source,
            })?;
        let output = CommandOutput::of(&outcome);
        let exit_code = exit_code_of(outcome);
        self.put(key, exit_code, &output);
        Ok(Memoized {
            exit_code,
            reused: false,
            output: Some(output),
        })
    }
}

/// The exit code the log records for how a criterion's command ended.
/// A command the engine stopped before it answered — a timeout, or a
/// cancellation — has no exit code of its own, so it is recorded as
/// `-2`, which no process exits with; one killed by a signal as `-1`.
fn exit_code_of(outcome: Outcome) -> i32 {
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

/// What a memoized command answered, and whether this invocation had to
/// run it to find out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Memoized {
    pub exit_code: i32,
    pub reused: bool,
    /// What the command printed, as [`CriterionRun::output`] keeps it.
    pub output: Option<CommandOutput>,
}

/// What a criterion's answer on a checkout turns on: the git tree of
/// everything the checkout holds, and — read only when a command that
/// runs `git` asks — the commit it stands on.
struct Tree {
    content: TreeId,
    head: Option<String>,
}

impl Tree {
    /// `cwd` as it stands now. The content is every file a checkout
    /// shows — untracked ones included, ignored ones not — staged
    /// through an index of this call's own, which starts as a copy of
    /// the checkout's so only what changed is hashed again, and which
    /// no other capture of the same checkout shares.
    async fn of(
        cwd: &Path,
        with_head: bool,
        supervision: Supervision<'_>,
    ) -> Result<Self, TaskCycleError> {
        let git = |args: &'static [&'static str]| async move {
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
        let own = git(&["rev-parse", "--path-format=absolute", "--git-path", "index"]).await?;
        let staging = tempfile::tempdir().map_err(TaskCycleError::TreeIndex)?;
        let index = staging.path().join("index");
        if let Err(source) = tokio::fs::copy(own.trim(), &index).await {
            // A checkout with no index yet stages from nothing.
            if source.kind() != std::io::ErrorKind::NotFound {
                return Err(TaskCycleError::TreeIndex(source));
            }
        }
        let content = crate::worktree::capture_tree(cwd, &index, supervision)
            .await
            .map_err(|source| TaskCycleError::TreeContent(Box::new(source)))?;
        let head = match with_head {
            true => Some(git(&["rev-parse", "HEAD"]).await?.trim().to_string()),
            false => None,
        };
        Ok(Tree { content, head })
    }
}

/// What `cwd` holds, as the tree a criterion's answer is kept for.
pub(crate) async fn content_of(
    cwd: &Path,
    supervision: Supervision<'_>,
) -> Result<TreeId, TaskCycleError> {
    Ok(Tree::of(cwd, false, supervision).await?.content)
}

/// Whether `cmd` runs `git`, which can answer from history as well as
/// from the files a checkout holds.
fn asks_git(cmd: &str) -> bool {
    crate::check::leading_programs(cmd)
        .iter()
        .any(|program| program == "git")
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
    let with_head = criteria.iter().any(|criterion| asks_git(&criterion.cmd));
    let tree = Tree::of(cwd, with_head, supervision).await?;
    let mut runs = Vec::with_capacity(criteria.len());
    for criterion in criteria {
        let key = memo.key(&criterion.cmd, &tree);
        let _turn = memo.turn(&key).await;
        let (exit_code, reused, duration_ms, output) = match memo.get(&key) {
            Some(answer) => (answer.exit_code, true, None, answer.output),
            None => {
                let (exit_code, duration_ms, output) =
                    run_criterion(task_id, cwd, &criterion.cmd, supervision).await?;
                memo.put(key, exit_code, &output);
                (exit_code, false, Some(duration_ms), Some(output))
            }
        };
        runs.push(CriterionRun {
            cmd: criterion.cmd.clone(),
            exit_code,
            is_guard: criterion.r#type == Some(CriterionType::Guard),
            reused,
            duration_ms,
            output,
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
    let mut ordered: Vec<&Criterion> = task.criteria.iter().collect();
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
