//! One way to run a subprocess. Every command the engine spawns is born
//! in its own process group, registered for the run so `yunta cancel`
//! can find it, bounded by a timeout and by the run's cancellation, and
//! killed with its whole tree on either. Its pipes are read to the end
//! on every path, so the outcome always carries what the child wrote —
//! whether it exited, timed out or was cancelled.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use yunta_core::Pid;

use crate::process_registry::{self, ProcessRegistry};

/// Who watches a governed subprocess: the run's registry, so `yunta
/// cancel` finds it, and the token whose firing kills it.
#[derive(Clone, Copy, Default)]
pub struct Supervision<'a> {
    pub registry: Option<&'a ProcessRegistry>,
    pub cancel: Option<&'a CancellationToken>,
}

impl Supervision<'_> {
    /// No registry and no cancellation: the child is bounded only by
    /// its own timeout.
    pub fn none() -> Self {
        Supervision::default()
    }
}

impl fmt::Debug for Supervision<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Supervision")
            .field("registered", &self.registry.is_some())
            .field("cancellable", &self.cancel.is_some())
            .finish()
    }
}

/// What to do with one of the child's output streams.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Capture {
    /// The child writes to the engine's own stream.
    #[default]
    Inherit,
    /// The stream is discarded.
    Discard,
    /// The stream is read to the end and returned in the outcome.
    Collect,
}

/// A subprocess to run under the engine's governance. Its stdin is the
/// bytes given, then closed — never the engine's own stdin, which
/// belongs to the person at the console.
#[derive(Debug, Clone)]
pub struct GovernedCommand {
    program: PathBuf,
    args: Vec<String>,
    cwd: PathBuf,
    stdin: Option<Vec<u8>>,
    stdout: Capture,
    stderr: Capture,
    timeout: Option<Duration>,
}

impl GovernedCommand {
    /// `program`, run in `cwd`: no arguments, no stdin, streams
    /// inherited, no timeout.
    pub fn new(program: impl Into<PathBuf>, cwd: &Path) -> Self {
        GovernedCommand {
            program: program.into(),
            args: Vec::new(),
            cwd: cwd.to_path_buf(),
            stdin: None,
            stdout: Capture::Inherit,
            stderr: Capture::Inherit,
            timeout: None,
        }
    }

    /// `sh -c <script>` in `cwd`, both streams collected.
    pub fn shell(cwd: &Path, script: &str) -> Self {
        Self::new("sh", cwd)
            .arg("-c")
            .arg(script)
            .stdout(Capture::Collect)
            .stderr(Capture::Collect)
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Bytes written to the child's stdin, which is then closed.
    pub fn stdin(mut self, bytes: Vec<u8>) -> Self {
        self.stdin = Some(bytes);
        self
    }

    pub fn stdout(mut self, capture: Capture) -> Self {
        self.stdout = capture;
        self
    }

    pub fn stderr(mut self, capture: Capture) -> Self {
        self.stderr = capture;
        self
    }

    /// How long the child may run before its whole group is killed.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// The program and its arguments on one line, for diagnostics.
    pub fn describe(&self) -> String {
        std::iter::once(self.program.display().to_string())
            .chain(self.args.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// How a governed subprocess ended. `stdout` and `stderr` hold what the
/// child wrote to a collected stream, and are empty otherwise.
#[derive(Debug)]
pub enum Outcome {
    Exited {
        status: ExitStatus,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    },
    /// The timeout elapsed; the whole process group was killed.
    TimedOut {
        pgid: Pid,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    },
    /// The cancellation fired; the whole process group was killed.
    Cancelled {
        pgid: Pid,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    },
}

#[derive(Debug, Error)]
pub enum SpawnError {
    #[error("failed to spawn `{command}`")]
    Spawn {
        command: String,
        #[source]
        source: std::io::Error,
    },
    #[error("`{command}` spawned without a pid")]
    NoPid { command: String },
    #[error("failed to wait for `{command}`")]
    Wait {
        command: String,
        #[source]
        source: std::io::Error,
    },
}

enum Waited {
    Exited(ExitStatus),
    TimedOut,
    Cancelled,
}

/// Runs `command` to its end under the engine's governance: in its own
/// process group, registered while it lives, killed with its whole
/// tree when its timeout elapses or the supervision's token fires.
/// Returns once the child is reaped and every collected stream is read.
pub async fn spawn_governed(
    command: GovernedCommand,
    supervision: Supervision<'_>,
) -> Result<Outcome, SpawnError> {
    let described = command.describe();
    let mut std_cmd = std::process::Command::new(&command.program);
    std_cmd
        .args(&command.args)
        .current_dir(&command.cwd)
        .stdin(if command.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(stdio(command.stdout))
        .stderr(stdio(command.stderr));
    // The child leads its own group, so a kill reaches every process
    // it spawned in turn, not just the one the engine started.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        std_cmd.process_group(0);
    }
    let mut child = tokio::process::Command::from(std_cmd)
        .spawn()
        .map_err(|source| SpawnError::Spawn {
            command: described.clone(),
            source,
        })?;
    let pgid = process_registry::child_pid(&child).ok_or_else(|| SpawnError::NoPid {
        command: described.clone(),
    })?;
    let _registration = process_registry::register(supervision.registry, Some(pgid));

    let stdin_task = command
        .stdin
        .zip(child.stdin.take())
        .map(|(bytes, mut pipe)| {
            tokio::spawn(async move {
                let _ = pipe.write_all(&bytes).await;
            })
        });
    let stdout_task = child.stdout.take().map(read_to_end);
    let stderr_task = child.stderr.take().map(read_to_end);

    let cancelled = async {
        match supervision.cancel {
            Some(token) => token.cancelled().await,
            None => std::future::pending().await,
        }
    };
    let deadline = async {
        match command.timeout {
            Some(timeout) => tokio::time::sleep(timeout).await,
            None => std::future::pending().await,
        }
    };
    let waited = tokio::select! {
        _ = cancelled => Waited::Cancelled,
        _ = deadline => Waited::TimedOut,
        status = child.wait() => Waited::Exited(status.map_err(|source| SpawnError::Wait {
            command: described.clone(),
            source,
        })?),
    };
    if !matches!(waited, Waited::Exited(_)) {
        kill_process_group(pgid).await;
        let _ = child.wait().await;
    }
    if let Some(task) = stdin_task {
        let _ = task.await;
    }
    let stdout = drain(stdout_task).await;
    let stderr = drain(stderr_task).await;
    Ok(match waited {
        Waited::Exited(status) => Outcome::Exited {
            status,
            stdout,
            stderr,
        },
        Waited::TimedOut => Outcome::TimedOut {
            pgid,
            stdout,
            stderr,
        },
        Waited::Cancelled => Outcome::Cancelled {
            pgid,
            stdout,
            stderr,
        },
    })
}

/// Sends `SIGKILL` to `pgid`'s whole process group — the `--` before
/// the negative pid is load-bearing: procps-ng parses `-KILL -123` as
/// two flags without it.
pub async fn kill_process_group(pgid: Pid) {
    let _ = tokio::process::Command::new("kill")
        .arg("-KILL")
        .arg("--")
        .arg(format!("-{pgid}"))
        .status()
        .await;
}

fn stdio(capture: Capture) -> Stdio {
    match capture {
        Capture::Inherit => Stdio::inherit(),
        Capture::Discard => Stdio::null(),
        Capture::Collect => Stdio::piped(),
    }
}

fn read_to_end<R: AsyncRead + Unpin + Send + 'static>(mut pipe: R) -> JoinHandle<Vec<u8>> {
    tokio::spawn(async move {
        let mut buf = Vec::new();
        let _ = pipe.read_to_end(&mut buf).await;
        buf
    })
}

async fn drain(task: Option<JoinHandle<Vec<u8>>>) -> Vec<u8> {
    match task {
        Some(task) => task.await.unwrap_or_default(),
        None => Vec::new(),
    }
}
