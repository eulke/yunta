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
use tokio::io::AsyncWriteExt;
use tokio::task::{JoinError, JoinSet};
use tokio_util::sync::CancellationToken;
use yunta_core::process::group::{force_kill_group, GroupError};
use yunta_core::process::signal::{signal_group, Signal};
use yunta_core::{Clock, Pid};

use crate::process_registry::{self, ProcessRegistry};
mod pipes;
mod state;
use pipes::{read_to_capture, stdio, Captured, PipeFailure};
use state::{child_has_exited, observation_interval, wait_for_deadline, Waited};

/// The run context a governed subprocess runs under: who watches it — the
/// run's registry, so `yunta cancel` finds it, and the token whose firing
/// kills it — and the variables layered onto its environment (a run's
/// injected `PATH` and the like, empty by default).
///
/// There is no supervision without an owner: a token and a clock are
/// what every caller has, so they are fields and not options, and the
/// registry is optional because only a run has one.
#[derive(Clone, Copy)]
pub struct Supervision<'a> {
    pub registry: Option<&'a ProcessRegistry>,
    /// Whose firing kills the child and its whole tree.
    pub cancel: &'a CancellationToken,
    /// Variables set on the child on top of the inherited environment —
    /// the run's `subprocess_vars`, so a node's `PATH` is injected rather
    /// than read from a mutated process. Empty leaves the child's
    /// environment inherited unchanged.
    pub env: &'a [(String, String)],
    /// What tells the time, for the one thing supervision does with it:
    /// judging whether a lock's holder is still the process that took
    /// it.
    pub clock: &'a dyn Clock,
}

impl<'a> Supervision<'a> {
    /// A supervision outside any run: the caller's token and clock, no
    /// registry and no env overrides — what a CLI command and a test
    /// spawn under.
    pub fn outside_any_run(cancel: &'a CancellationToken, clock: &'a dyn Clock) -> Self {
        Supervision {
            registry: None,
            cancel,
            env: &[],
            clock,
        }
    }

    /// The same supervision, with variables layered onto the child's
    /// environment.
    pub fn with_env(self, env: &'a [(String, String)]) -> Self {
        Supervision { env, ..self }
    }
}

impl fmt::Debug for Supervision<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Supervision")
            .field("registered", &self.registry.is_some())
            .field("cancelled", &self.cancel.is_cancelled())
            .field("env_vars", &self.env.len())
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
        output: Box<CapturedOutput>,
    },
    #[error("failed to observe the child process for `{command}`")]
    Observe {
        command: String,
        #[source]
        source: std::io::Error,
        output: Box<CapturedOutput>,
    },
    #[error("failed to kill the process group of `{command}`: {source}")]
    Kill {
        command: String,
        #[source]
        source: Box<GroupError>,
        output: Box<CapturedOutput>,
    },
    #[error("failed to read {stream:?} for `{command}`")]
    Read {
        command: String,
        stream: PipeKind,
        #[source]
        source: std::io::Error,
        output: Box<CapturedOutput>,
    },
    #[error("the {stream:?} task for `{command}` failed")]
    ReadTask {
        command: String,
        stream: PipeKind,
        #[source]
        source: JoinError,
        output: Box<CapturedOutput>,
    },
}

/// Output already captured when supervision failed. Kept behind a box in
/// [`SpawnError`] so carrying diagnostics does not inflate every run error.
#[derive(Debug)]
pub struct CapturedOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

fn captured_output(stdout: Vec<u8>, stderr: Vec<u8>) -> Box<CapturedOutput> {
    Box::new(CapturedOutput { stdout, stderr })
}

#[derive(Debug, Clone, Copy)]
pub enum PipeKind {
    Stdin,
    Stdout,
    Stderr,
}

/// Runs `command` to its end under the engine's governance: in its own
/// process group, registered while it lives, killed with its whole
/// tree when its timeout elapses or the supervision's token fires.
/// The leader stays waitable until the process group has been closed and
/// its pipes have drained. This keeps its PID from being reused while any
/// signal can still target the group.
pub async fn spawn_governed(
    command: GovernedCommand,
    supervision: Supervision<'_>,
) -> Result<Outcome, SpawnError> {
    let described = command.describe();
    let mut std_cmd = std::process::Command::new(&command.program);
    std_cmd
        .args(&command.args)
        .current_dir(&command.cwd)
        .envs(supervision.env.iter().map(|(k, v)| (k, v)))
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
        .kill_on_drop(true)
        .spawn()
        .map_err(|source| SpawnError::Spawn {
            command: described.clone(),
            source,
        })?;
    let pgid = process_registry::child_pid(&child).ok_or_else(|| SpawnError::NoPid {
        command: described.clone(),
    })?;
    let _registration = process_registry::register(supervision.registry, Some(pgid));

    let timeout_at = command
        .timeout
        .map(|timeout| tokio::time::Instant::now() + timeout);
    let stdout = Captured::default();
    let stderr = Captured::default();
    let mut pipes = JoinSet::new();

    if let Some((bytes, mut pipe)) = command.stdin.zip(child.stdin.take()) {
        pipes.spawn(async move {
            let result = match pipe.write_all(&bytes).await {
                Ok(()) => match pipe.shutdown().await {
                    Ok(()) => Ok(()),
                    Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
                    Err(error) => Err(error),
                },
                Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
                Err(error) => Err(error),
            };
            (PipeKind::Stdin, result)
        });
    }
    if let Some(pipe) = child.stdout.take() {
        let capture = stdout.clone();
        pipes.spawn(async move { (PipeKind::Stdout, read_to_capture(pipe, capture).await) });
    }
    if let Some(pipe) = child.stderr.take() {
        let capture = stderr.clone();
        pipes.spawn(async move { (PipeKind::Stderr, read_to_capture(pipe, capture).await) });
    }

    let mut observe_error = None;
    let mut child_observation = observation_interval();
    child_observation.tick().await;
    let mut waited = loop {
        tokio::select! {
            biased;
            _ = supervision.cancel.cancelled() => break Waited::Cancelled,
            _ = wait_for_deadline(timeout_at) => break Waited::TimedOut,
            _ = child_observation.tick() => {
                match child_has_exited(pgid) {
                    Ok(true) => break Waited::Exited,
                    Ok(false) => {}
                    Err(error) => {
                        observe_error = Some(error);
                        break Waited::Failed;
                    }
                }
            }
        }
    };

    // Keep this close operation pinned while watching the token and deadline. If
    // either fires after the shell exits but while descendants still own
    // a pipe, the operation is still cancelled/timed out and the same
    // shutdown continues to completion.
    let cleanup = force_kill_group(pgid);
    tokio::pin!(cleanup);
    let cleanup_result = loop {
        tokio::select! {
            biased;
            result = &mut cleanup => break result,
            _ = supervision.cancel.cancelled(), if waited == Waited::Exited => {
                waited = Waited::Cancelled;
            }
            _ = wait_for_deadline(timeout_at), if waited == Waited::Exited => {
                waited = Waited::TimedOut;
            }
        }
    };
    if cleanup_result.is_err() {
        // The shared helper reports the inspection/signal error and makes
        // its own emergency SIGKILL attempt. Keep one final direct attempt
        // here before waiting on our owned leader.
        if let Err(error) = signal_group(pgid, Signal::SIGKILL) {
            tracing::warn!(pgid = %pgid, error = %error, "final process-group kill attempt failed");
        }
        if let Err(error) = child.start_kill() {
            tracing::warn!(pgid = %pgid, error = %error, "final process-leader kill attempt failed");
        }
    }

    let mut pipe_failure = None;
    let mut abort_pipes = cleanup_result.is_err();
    while !pipes.is_empty() && !abort_pipes {
        tokio::select! {
            biased;
            joined = pipes.join_next() => match joined {
                Some(Ok((stream, Err(error)))) => {
                    pipe_failure.get_or_insert(PipeFailure::Io(stream, error));
                    abort_pipes = true;
                }
                Some(Err(error)) => {
                    pipe_failure.get_or_insert(PipeFailure::Join(PipeKind::Stdout, error));
                    abort_pipes = true;
                }
                _ => {}
            },
            _ = supervision.cancel.cancelled(), if waited == Waited::Exited => {
                waited = Waited::Cancelled;
                abort_pipes = true;
            }
            _ = wait_for_deadline(timeout_at), if waited == Waited::Exited => {
                waited = Waited::TimedOut;
                abort_pipes = true;
            }
        }
    }
    if abort_pipes {
        // The leader is intentionally still unreaped, so the process group
        // id is still its identity while cleanup is repeated here.
        let second_cleanup = force_kill_group(pgid).await;
        if cleanup_result.is_ok() {
            if let Err(error) = second_cleanup {
                pipe_failure.get_or_insert(PipeFailure::Group(error));
            }
        }
        pipes.abort_all();
        while pipes.join_next().await.is_some() {}
    }

    // The group has been signalled and its members stopped or killed. The
    // leader is now collected, which is the first point at which its pid
    // may safely be reused.
    let status = loop {
        tokio::select! {
            biased;
            status = child.wait() => break status,
            _ = supervision.cancel.cancelled(), if waited == Waited::Exited => {
                waited = Waited::Cancelled;
            }
            _ = wait_for_deadline(timeout_at), if waited == Waited::Exited => {
                waited = Waited::TimedOut;
            }
        }
    };
    let captured_stdout = stdout.snapshot();
    let captured_stderr = stderr.snapshot();

    if let Some(error) = observe_error {
        return Err(SpawnError::Observe {
            command: described,
            source: error,
            output: captured_output(captured_stdout, captured_stderr),
        });
    }
    if let Err(source) = cleanup_result {
        return Err(SpawnError::Kill {
            command: described,
            source: Box::new(source),
            output: captured_output(captured_stdout, captured_stderr),
        });
    }
    if let Some(failure) = pipe_failure {
        return Err(match failure {
            PipeFailure::Io(stream, source) => SpawnError::Read {
                command: described,
                stream,
                source,
                output: captured_output(captured_stdout, captured_stderr),
            },
            PipeFailure::Join(stream, source) => SpawnError::ReadTask {
                command: described,
                stream,
                source,
                output: captured_output(captured_stdout, captured_stderr),
            },
            PipeFailure::Group(source) => SpawnError::Kill {
                command: described,
                source: Box::new(source),
                output: captured_output(captured_stdout, captured_stderr),
            },
        });
    }
    let status = status.map_err(|source| SpawnError::Wait {
        command: described,
        source,
        output: captured_output(captured_stdout.clone(), captured_stderr.clone()),
    })?;
    Ok(match waited {
        Waited::Exited => Outcome::Exited {
            status,
            stdout: captured_stdout,
            stderr: captured_stderr,
        },
        Waited::TimedOut | Waited::Failed => Outcome::TimedOut {
            pgid,
            stdout: captured_stdout,
            stderr: captured_stderr,
        },
        Waited::Cancelled => Outcome::Cancelled {
            pgid,
            stdout: captured_stdout,
            stderr: captured_stderr,
        },
    })
}
