//! The one place the engine and CLI run `git`. Every invocation goes
//! through here, so a git failure always carries the same shape: the
//! [`Invocation`] it ran as, and the [`Cause`] it did not answer. Callers
//! map [`GitError`] into their own module's error at the edge.
//!
//! A git a run spawns is a subprocess the run owns: it is born in its own
//! process group, registered, and killed with its whole tree when the run
//! is cancelled — which is what the [`Supervision`] every async call
//! takes carries. A `git fetch` against an unreachable remote is the case
//! that makes it matter: without it, Ctrl-C leaves it running and the run
//! it belonged to is already gone.
//!
//! What a call is for decides how it reads git's answer: [`read`] for
//! output, [`ask`] and [`attempt`] for an exit that means yes or no, and
//! [`change`] for a mutation. An [`Effect`] says whether a mutation takes
//! the mutation lock, which only worktree metadata needs.

mod error;
mod mutation;
mod program;
mod repo;
mod status;

pub use error::{Cause, GitError, Invocation, Stopped};
pub use mutation::{mutation_lock, MutationLock};
pub use repo::{committer, common_dir, files_naming, push_branch, remote_url, run_branches};
pub use status::{tree_state, Status};

use std::path::Path;
use std::process::Output;

use yunta_core::CommitSha;

use crate::process::{
    spawn_governed, Capture, Drain, GovernedCommand, Outcome, SpawnError, Supervision,
};
use mutation::held_for;

/// What a mutation changes, which says whether it takes the mutation lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Objects written to the object database, such as a commit made with
    /// a private index.
    Objects,
    /// Refs and branch configuration: `update-ref`, a push.
    Refs,
    /// The checkout: its index and its working tree.
    Checkout,
    /// Worktree metadata under the common git directory, which every other
    /// `git worktree` call reads: adding, removing and pruning checkouts,
    /// and deleting branches. Takes `yunta-worktree.lock`.
    Metadata,
}

/// The exit codes that answer a git call with yes and with no. Any other
/// exit is an error.
#[derive(Debug, Clone, Copy)]
pub struct Exits {
    pub yes: i32,
    pub no: i32,
}

/// `merge-base --is-ancestor` exits 0 for an ancestor and 1 for any other
/// commit.
pub const IS_ANCESTOR: Exits = Exits { yes: 0, no: 1 };
/// `check-ignore -q` exits 0 for an ignored path and 1 for a path git keeps.
pub const CHECK_IGNORE: Exits = Exits { yes: 0, no: 1 };
/// `branch -d` exits 0 when it deletes the branch and 1 when git keeps it.
pub const BRANCH_DELETED: Exits = Exits { yes: 0, no: 1 };

/// git's stdout on exit 0, decoded lossily. Any other exit is an error. A
/// caller that needs the raw bytes, such as a `-z` listing whose paths may
/// not be UTF-8, uses [`read_bytes`].
pub async fn read(dir: &Path, args: &[&str], sup: Supervision<'_>) -> Result<String, GitError> {
    Ok(lossy(&read_bytes(dir, args, sup).await?))
}

/// Raw stdout bytes on exit 0, so a `-z` listing's NUL-joined paths survive
/// byte for byte.
pub async fn read_bytes(
    dir: &Path,
    args: &[&str],
    sup: Supervision<'_>,
) -> Result<Vec<u8>, GitError> {
    let output = run(dir, args, sup).await?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(exit_error(dir, args, &output))
    }
}

/// A question git answers by its exit. `exits` names the exit that means yes
/// and the one that means no; any other exit is an error.
pub async fn ask(
    dir: &Path,
    args: &[&str],
    exits: Exits,
    sup: Supervision<'_>,
) -> Result<bool, GitError> {
    yes_or_no(dir, args, exits, sup).await
}

/// The commit `rev` names in `dir`, peeled to a commit. `Ok(None)` is a
/// commit the repository lacks, which git answers with exit 1; any other
/// failure is an error, and so is a stdout that is no commit id.
pub async fn verify(
    dir: &Path,
    rev: &str,
    sup: Supervision<'_>,
) -> Result<Option<CommitSha>, GitError> {
    let commit = format!("{rev}^{{commit}}");
    let args = ["rev-parse", "--verify", "--quiet", commit.as_str()];
    let output = run(dir, &args, sup).await?;
    match output.status.code() {
        Some(0) => {
            let said = lossy(&output.stdout);
            said.trim()
                .parse::<CommitSha>()
                .map(Some)
                .map_err(|_| unreadable(dir, &args, said.trim()))
        }
        Some(1) => Ok(None),
        _ => Err(exit_error(dir, &args, &output)),
    }
}

/// A mutation that takes the lock its `effect` asks for, and returns its
/// stdout on exit 0. An [`Effect::Metadata`] call takes `yunta-worktree.lock`
/// before git runs and holds it until git returns. A caller that holds the
/// lock already uses [`change_held`].
pub async fn change(
    dir: &Path,
    effect: Effect,
    args: &[&str],
    sup: Supervision<'_>,
) -> Result<String, GitError> {
    match effect {
        Effect::Metadata => {
            let held = held_for(dir, args, sup).await?;
            change_held(&held, dir, args, sup).await
        }
        Effect::Objects | Effect::Refs | Effect::Checkout => read(dir, args, sup).await,
    }
}

/// A mutation git may decline, answered by its exit: `exits` names the exit
/// that means done and the one that means declined, and any other exit is an
/// error. Takes the lock its `effect` asks for, as [`change`] does.
pub async fn attempt(
    dir: &Path,
    effect: Effect,
    args: &[&str],
    exits: Exits,
    sup: Supervision<'_>,
) -> Result<bool, GitError> {
    match effect {
        Effect::Metadata => {
            let held = held_for(dir, args, sup).await?;
            attempt_held(&held, dir, args, exits, sup).await
        }
        Effect::Objects | Effect::Refs | Effect::Checkout => yes_or_no(dir, args, exits, sup).await,
    }
}

/// [`change`] for a caller that holds the mutation lock already. The call
/// takes no lock of its own: a nested wait on the lock held would never end.
pub async fn change_held(
    _held: &MutationLock,
    dir: &Path,
    args: &[&str],
    sup: Supervision<'_>,
) -> Result<String, GitError> {
    read(dir, args, sup).await
}

/// [`attempt`] for a caller that holds the mutation lock already.
pub async fn attempt_held(
    _held: &MutationLock,
    dir: &Path,
    args: &[&str],
    exits: Exits,
    sup: Supervision<'_>,
) -> Result<bool, GitError> {
    yes_or_no(dir, args, exits, sup).await
}

/// One governed `git`, run to its end: its exit status and both streams.
///
/// A git the token or the timeout stopped is an error and not an exit
/// status: it never answered, and a caller that read "non-zero" out of a
/// killed child would read a verdict into a run somebody stopped. A drain
/// that stopped reading is an error too, since what git wrote may not be
/// all there.
async fn run(dir: &Path, args: &[&str], sup: Supervision<'_>) -> Result<Output, GitError> {
    let fail = |cause| error_for(dir, args, cause);
    let program = program::program(sup.env, sup).await.map_err(&fail)?;
    // The repository's upkeep is left to the person's own git. An automatic
    // `gc` or `maintenance` a commit starts detaches into the background, out
    // of the process group the engine supervises, and works on in the run's
    // repository beside its commits and suites, holding the pipes the engine
    // reads git through.
    let mut command = GovernedCommand::new(program, dir)
        .arg("-c")
        .arg("gc.auto=0")
        .arg("-c")
        .arg("maintenance.auto=false")
        .stdout(Capture::Collect)
        .stderr(Capture::Collect);
    for arg in args {
        command = command.arg(*arg);
    }
    let ran = match spawn_governed(command, sup).await {
        Ok(Outcome::Exited {
            status,
            stdout,
            stderr,
            drain: Drain::Complete,
        }) => Output {
            status,
            stdout,
            stderr,
        },
        Ok(Outcome::Exited {
            stderr,
            drain: Drain::Truncated,
            ..
        }) => {
            return Err(fail(Cause::Truncated {
                stderr: lossy(&stderr),
            }))
        }
        Ok(Outcome::TimedOut { .. }) => return Err(fail(Cause::Stopped(Stopped::TimedOut))),
        Ok(Outcome::Cancelled { .. }) => return Err(fail(Cause::Stopped(Stopped::Cancelled))),
        Err(error) => return Err(fail(Cause::Spawn(spawn_io(error)))),
    };
    Ok(ran)
}

/// The yes-or-no an exit gives: `exits.yes` is true and `exits.no` is false.
/// Any other exit is an error.
async fn yes_or_no(
    dir: &Path,
    args: &[&str],
    exits: Exits,
    sup: Supervision<'_>,
) -> Result<bool, GitError> {
    let output = run(dir, args, sup).await?;
    match output.status.code() {
        Some(code) if code == exits.yes => Ok(true),
        Some(code) if code == exits.no => Ok(false),
        _ => Err(exit_error(dir, args, &output)),
    }
}

/// The error for a git invocation of `args` in `dir` that did not answer.
fn error_for(dir: &Path, args: &[&str], cause: Cause) -> GitError {
    GitError {
        invocation: Invocation {
            dir: dir.to_path_buf(),
            args: args.iter().map(|arg| arg.to_string()).collect(),
        },
        cause,
    }
}

/// The error for a git that ran to its end with an exit that did not answer.
fn exit_error(dir: &Path, args: &[&str], output: &Output) -> GitError {
    error_for(
        dir,
        args,
        Cause::Exit {
            code: output.status.code(),
            stderr: lossy(&output.stderr),
        },
    )
}

/// The error for a git that exited zero with an answer that is no commit id.
fn unreadable(dir: &Path, args: &[&str], said: &str) -> GitError {
    error_for(
        dir,
        args,
        Cause::Spawn(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("git printed `{said}`, which is no commit id"),
        )),
    )
}

/// The `io::Error` a spawn that did not run to its end stands for: the OS's
/// own error when the spawn itself failed, and the spawn error's text
/// otherwise.
fn spawn_io(error: SpawnError) -> std::io::Error {
    match error {
        SpawnError::Spawn { source, .. } => source,
        other => std::io::Error::other(other.to_string()),
    }
}

/// Bytes git wrote, read as text: a byte that is not UTF-8 is replaced.
fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The engine's git starts no upkeep of its own: whatever the
    /// repository configures, the commands it runs read `gc.auto` as 0
    /// and `maintenance.auto` as off.
    #[tokio::test]
    async fn the_engine_s_git_starts_no_upkeep_of_its_own() {
        let cancel = tokio_util::sync::CancellationToken::new();
        let supervision = Supervision::outside_any_run(&cancel, &yunta_testkit_core::FixedClock);
        let repo = tempfile::tempdir().unwrap();
        yunta_testkit::init_repo(repo.path());
        yunta_testkit::git(repo.path(), &["config", "gc.auto", "6700"]);

        let gc = read(repo.path(), &["config", "--get", "gc.auto"], supervision).await;
        let maintenance = read(
            repo.path(),
            &["config", "--get", "maintenance.auto"],
            supervision,
        )
        .await;

        assert_eq!(gc.unwrap().trim(), "0");
        assert_eq!(maintenance.unwrap().trim(), "false");
    }
}
