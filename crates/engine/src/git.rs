//! The one place the engine and CLI run `git`. Every invocation goes
//! through here so a git failure always carries the same shape — the
//! arguments, the directory, git's own stderr — and paths reach git as
//! `OsStr`, never a lossy `to_string`. Callers map [`GitError`] into
//! their own module's error at the edge.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Output;

use thiserror::Error;

/// A git invocation that could not be spawned, or ran and exited
/// non-zero. `source` is present only in the first case; `stderr` and
/// `code` carry git's own output and exit status in the second (`code`
/// stays `None` when a signal killed git before it could exit).
#[derive(Debug, Error)]
#[error("git {args} in `{cwd}` failed{}", suffix(.stderr, .source))]
pub struct GitError {
    pub args: String,
    pub cwd: PathBuf,
    pub stderr: String,
    pub code: Option<i32>,
    #[source]
    pub source: Option<std::io::Error>,
}

impl GitError {
    /// The human-facing cause: git's own error message when it spawned
    /// and failed, or the spawn error itself when it never ran.
    pub fn detail(&self) -> String {
        match &self.source {
            Some(e) => e.to_string(),
            None => self.stderr.clone(),
        }
    }
}

fn suffix(stderr: &str, source: &Option<std::io::Error>) -> String {
    match source {
        Some(e) => format!(": {e}"),
        None if stderr.is_empty() => String::new(),
        None => format!(": {stderr}"),
    }
}

fn describe<S: AsRef<OsStr>>(args: &[S]) -> String {
    args.iter()
        .map(|arg| arg.as_ref().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

fn spawn_error<S: AsRef<OsStr>>(cwd: &Path, args: &[S], source: std::io::Error) -> GitError {
    GitError {
        args: describe(args),
        cwd: cwd.to_path_buf(),
        stderr: String::new(),
        code: None,
        source: Some(source),
    }
}

/// git's stdout on a zero exit, [`GitError`] otherwise. The stdout is
/// decoded lossily — a caller that needs raw bytes (a `-z` listing whose
/// paths may not be UTF-8) uses [`output_bytes`] instead.
fn interpret<S: AsRef<OsStr>>(cwd: &Path, args: &[S], output: Output) -> Result<String, GitError> {
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(exit_error(cwd, args, &output))
    }
}

/// The error for a git that spawned and exited non-zero.
fn exit_error<S: AsRef<OsStr>>(cwd: &Path, args: &[S], output: &Output) -> GitError {
    GitError {
        args: describe(args),
        cwd: cwd.to_path_buf(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        code: output.status.code(),
        source: None,
    }
}

/// Raw stdout bytes on a zero exit — for a `-z` listing whose NUL-joined
/// paths must survive byte-for-byte.
pub async fn output_bytes<S: AsRef<OsStr>>(cwd: &Path, args: &[S]) -> Result<Vec<u8>, GitError> {
    let output = tokio::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .await
        .map_err(|source| spawn_error(cwd, args, source))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(exit_error(cwd, args, &output))
    }
}

/// git's stdout on a zero exit; a `GitError` on spawn failure or a
/// non-zero exit.
pub async fn output<S: AsRef<OsStr>>(cwd: &Path, args: &[S]) -> Result<String, GitError> {
    let output = tokio::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .await
        .map_err(|source| spawn_error(cwd, args, source))?;
    interpret(cwd, args, output)
}

/// Whether the command exited zero. A `GitError` only when git cannot be
/// spawned — a non-zero exit is a `false`, not an error, for the callers
/// that ask a yes/no question (does this rebase apply cleanly?).
pub async fn success<S: AsRef<OsStr>>(cwd: &Path, args: &[S]) -> Result<bool, GitError> {
    let output = tokio::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .await
        .map_err(|source| spawn_error(cwd, args, source))?;
    Ok(output.status.success())
}

/// [`output`] for a synchronous caller (manifest build, a CLI command
/// with no runtime of its own).
pub fn output_blocking<S: AsRef<OsStr>>(cwd: &Path, args: &[S]) -> Result<String, GitError> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|source| spawn_error(cwd, args, source))?;
    interpret(cwd, args, output)
}

/// [`success`] for a synchronous caller.
pub fn success_blocking<S: AsRef<OsStr>>(cwd: &Path, args: &[S]) -> Result<bool, GitError> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|source| spawn_error(cwd, args, source))?;
    Ok(output.status.success())
}
