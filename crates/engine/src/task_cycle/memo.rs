//! What a run already knows its criteria answered: a command's exit code
//! on one tree, kept while nothing it could turn on changed — and, for a
//! red one, until a person chooses to run its node again.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use yunta_core::ContentHash;

use super::criteria::{could_not_run, exit_code_of};
use super::tree::{asks_git, Tree};
use super::TaskCycleError;
use crate::process::{spawn_governed, CommandOutput, GovernedCommand, Printed, Supervision};

/// The run's memoization cache: a criterion's result is reused when its
/// command, the working tree's content and the resolved config are all
/// unchanged since it last answered. One `Memo` per `execute_run` call
/// holds what this invocation answered, seeded at a wake with what the
/// log says earlier invocations answered — only for the trees each
/// answer names, and only from an invocation whose commands ran with the
/// environment this one's do.
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
    /// What the lineage's measurement says about the suite every task is
    /// held to, as this invocation learns it.
    suite: super::SuiteGate,
}

/// What a command answered on one tree. A red answer keeps what the
/// command printed, so a check that reuses it still says why it fails —
/// the common case is a session's `yunta_check_task` running the
/// criteria and the close reusing them on the same tree. A green one
/// keeps nothing to explain.
#[derive(Clone)]
pub(super) struct Answer {
    pub(super) exit_code: i32,
    pub(super) output: Option<Printed>,
}

impl Memo {
    pub fn new(config_hash: ContentHash) -> Self {
        Self {
            config_hash,
            cache: Mutex::new(HashMap::new()),
            asking: Mutex::new(HashMap::new()),
            suite: super::SuiteGate::default(),
        }
    }

    /// The suite every task is held to, as the measurement settles it.
    pub fn suite(&self) -> &super::SuiteGate {
        &self.suite
    }

    pub(super) fn key(&self, cmd: &str, tree: &Tree) -> ContentHash {
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
    pub(super) async fn turn(&self, key: &ContentHash) -> tokio::sync::OwnedMutexGuard<()> {
        let slot = {
            let mut asking = self.asking.lock().unwrap_or_else(|e| e.into_inner());
            asking.entry(key.clone()).or_default().clone()
        };
        slot.lock_owned().await
    }

    pub(super) fn get(&self, key: &ContentHash) -> Option<Answer> {
        let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.get(key).cloned()
    }

    /// Remembers what a command answered under `key` — unless it could
    /// not run at all. A command that was not found says nothing about
    /// the tree, and the next check on the same tree must run it again:
    /// the program may be on the `PATH` by then.
    pub(super) fn put(&self, key: ContentHash, exit_code: i32, output: &CommandOutput) {
        if could_not_run(exit_code).is_some() {
            return;
        }
        let answer = Answer {
            exit_code,
            output: (exit_code != 0).then(|| Printed::Ran(output.clone())),
        };
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.insert(key, answer);
    }

    /// What `cmd`'s answer on `cwd` is kept under, read as `cwd` stands
    /// now.
    pub(super) async fn key_on(
        &self,
        cmd: &str,
        cwd: &Path,
        supervision: Supervision<'_>,
    ) -> Result<ContentHash, TaskCycleError> {
        let tree = self.tree_for(cmd, cwd, supervision).await?;
        Ok(self.key(cmd, &tree))
    }

    /// Lets go of every red answer: a person who chose to run something
    /// again after a failure says the world it failed in may have
    /// changed, so what failed is asked again. What passed stays passed.
    pub(crate) fn forget_failures(&self) {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.retain(|_, answer| answer.exit_code == 0);
    }

    /// Takes in what earlier invocations of the run answered — every
    /// answer the log holds that still speaks for this one.
    pub(crate) fn seed(
        &self,
        events: &[yunta_core::events::StoredEvent],
        run: &yunta_core::events::RunLedger,
    ) {
        let answers = super::recorded::answers(events, run);
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        for answer in answers {
            cache.insert(
                self.key(&answer.cmd, &answer.tree),
                Answer {
                    exit_code: answer.exit_code,
                    output: answer.printed,
                },
            );
        }
    }

    /// `cwd` as an answer of `cmd` is kept for it, read as it stands now
    /// — for a caller that runs the command some other way and hands the
    /// answer to [`Memo::passed_on`].
    pub(crate) async fn tree_for(
        &self,
        cmd: &str,
        cwd: &Path,
        supervision: Supervision<'_>,
    ) -> Result<Tree, TaskCycleError> {
        Tree::of(cwd, asks_git(cmd), supervision).await
    }

    /// Remembers that `cmd` passed on `tree`, measured by a caller rather
    /// than a check: the suite a run measures, which every task is then
    /// held to.
    pub(crate) fn passed_on(&self, cmd: &str, tree: &Tree) {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.insert(
            self.key(cmd, tree),
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
            output: Some(Printed::Ran(output)),
        })
    }
}

/// What a memoized command answered, and whether this invocation had to
/// run it to find out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Memoized {
    pub exit_code: i32,
    pub reused: bool,
    /// What the command printed, as [`CriterionRun::output`](super::CriterionRun::output) keeps it.
    pub output: Option<Printed>,
}
