//! What a git invocation that did not answer was: what ran, and why it did
//! not. The text a reader sees is composed once, by [`GitError`]'s
//! `Display`.

use std::path::PathBuf;

use crate::lock::LockError;

/// A git invocation that did not answer: what ran, and why it did not.
///
/// Its `Display` says what was attempted and then the cause's text. A
/// caller that carries the error reads that sentence; a caller that
/// prints the cause alone reads [`GitError::detail`].
#[derive(Debug, thiserror::Error)]
#[error("{}", yunta_core::text::detailed(headline(&self.invocation), &self.detail()))]
pub struct GitError {
    pub invocation: Invocation,
    pub cause: Cause,
}

/// The git a failure concerns: the directory it ran in, and its arguments.
#[derive(Debug, Clone)]
pub struct Invocation {
    pub dir: PathBuf,
    pub args: Vec<String>,
}

/// Why a git invocation did not answer.
#[derive(Debug)]
pub enum Cause {
    /// The token fires or the timeout runs out: no answer exists.
    Stopped(Stopped),
    /// git could not be started, or supervising it failed, or what it
    /// printed could not be read as its answer. The `io::Error` is what
    /// [`GitError::detail`] renders.
    Spawn(std::io::Error),
    /// git exited without an answer: with a non-zero status, or with none
    /// when a signal ended it. `stderr` is git's own words.
    Exit { code: Option<i32>, stderr: String },
    /// The drain stops reading: what git wrote may not be all there.
    Truncated { stderr: String },
    /// The mutation lock that a metadata change takes was not got. A
    /// timeout names the holder's pid; a cancelled wait is a stop the
    /// token made. Boxed because it is the largest cause by far, and a git
    /// error is carried up through every error that a git call can fail.
    Lock(Box<LockError>),
}

/// What stopped a git before it could answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stopped {
    /// The supervision's token fired: whoever asked for this git no
    /// longer wants it.
    Cancelled,
    /// The command's own timeout elapsed.
    TimedOut,
}

impl GitError {
    /// Whether the token stopped the call: a git it stopped, or a wait
    /// for the mutation lock that it stopped. No verdict exists then.
    pub fn cancelled(&self) -> bool {
        match &self.cause {
            Cause::Stopped(Stopped::Cancelled) => true,
            Cause::Lock(lock) => matches!(**lock, LockError::Cancelled { .. }),
            _ => false,
        }
    }

    /// The cause's text, rendered once: the spawn error's text, git's own
    /// stderr trimmed (empty when git said nothing), what stopped git, or
    /// the lock error's text.
    pub fn detail(&self) -> String {
        match &self.cause {
            Cause::Stopped(Stopped::Cancelled) => {
                "git was cancelled and was killed with its whole process tree".to_string()
            }
            Cause::Stopped(Stopped::TimedOut) => {
                "git timed out and was killed with its whole process tree".to_string()
            }
            Cause::Spawn(source) => source.to_string(),
            Cause::Exit { stderr, .. } | Cause::Truncated { stderr } => stderr.trim().to_string(),
            Cause::Lock(error) => error.to_string(),
        }
    }
}

/// What was attempted, which is all a `GitError` says on its own.
fn headline(invocation: &Invocation) -> String {
    format!(
        "git {} in `{}` failed",
        invocation.args.join(" "),
        invocation.dir.display()
    )
}
