//! One way to run a subprocess. Every command the engine spawns is born
//! in its own process group, registered for the run so `yunta cancel`
//! can find it, bounded by a timeout and by the run's cancellation, and
//! killed with its whole tree on either — or when its caller stops
//! waiting and drops it mid-flight. Its pipes are read to the end
//! on every path, so the outcome carries what the child wrote — whether
//! it exited, timed out or was cancelled. A pipe a process outside the
//! group still holds is not waited for, and an exit says so when that
//! cut what it could read.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::time::Duration;

use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use yunta_core::events::STOPPED_EXIT_CODE;
use yunta_core::process::exit::ExitWatch;
use yunta_core::process::group::{force_kill_group, GroupError};
use yunta_core::{Clock, Pid};

use crate::process_registry::{self, ProcessRegistry};
mod close;
mod environment;
mod error;
mod leader;
pub(crate) mod output;
mod pipes;
mod spawn;
mod state;

use close::{close_group, drain_pipes};
pub use environment::execution_environment;
use environment::SHELL;
use error::captured_output;
pub use error::{CapturedOutput, PipeKind, SpawnError};
use leader::Leader;
pub use output::{CommandOutput, Printed};
use pipes::{Captured, PipeFailure};
use spawn::{spawn_leader, start_pipes};
pub use state::wait_for_deadline;
use state::Waited;

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
    /// The directories a program is looked up in when `env` sets no `PATH`
    /// of its own: the ones the environment reads for this process. Empty
    /// when nothing supplies them, and the program is then left to the
    /// child's own search.
    pub path: &'a [PathBuf],
    /// What tells the time, for the one thing supervision does with it:
    /// judging whether a lock's holder is still the process that took
    /// it.
    pub clock: &'a dyn Clock,
}

impl<'a> Supervision<'a> {
    /// A supervision outside any run: the caller's token and clock, no
    /// registry, no env overrides and no search path — what a CLI command
    /// and a test spawn under.
    pub fn outside_any_run(cancel: &'a CancellationToken, clock: &'a dyn Clock) -> Self {
        Supervision {
            registry: None,
            cancel,
            env: &[],
            path: &[],
            clock,
        }
    }

    /// The same supervision, with variables layered onto the child's
    /// environment.
    pub fn with_env(self, env: &'a [(String, String)]) -> Self {
        Supervision { env, ..self }
    }

    /// The same supervision, searching `path` for a program when its
    /// variables set no `PATH` of their own.
    pub fn with_path(self, path: &'a [PathBuf]) -> Self {
        Supervision { path, ..self }
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

/// What to do with one of the child's output streams. Never the
/// engine's own: the terminal belongs to the run's display, so what a
/// child prints is kept or dropped, never interleaved with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Capture {
    /// The stream is discarded.
    Discard,
    /// The stream is read to the end and returned in the outcome.
    #[default]
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
    /// `program`, run in `cwd`: no arguments, no stdin, both streams
    /// collected, no timeout.
    pub fn new(program: impl Into<PathBuf>, cwd: &Path) -> Self {
        GovernedCommand {
            program: program.into(),
            args: Vec::new(),
            cwd: cwd.to_path_buf(),
            stdin: None,
            stdout: Capture::Collect,
            stderr: Capture::Collect,
            timeout: None,
        }
    }

    /// `sh -c <script>` in `cwd`, both streams collected.
    pub fn shell(cwd: &Path, script: &str) -> Self {
        Self::new(SHELL, cwd).arg("-c").arg(script)
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

/// Whether an exited command's collected streams were read to their end.
/// A pipe still held when the drain bound passes belongs to a process
/// that left the command's group, and reading stops there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drain {
    /// Every collected stream was read to its end.
    Complete,
    /// The drain bound stopped reading while a pipe was still held open:
    /// the output may lack what the holder wrote.
    Truncated,
}

/// How a governed subprocess ended. `stdout` and `stderr` hold what the
/// child wrote to a collected stream, and are empty otherwise. `drain`
/// says whether an exit read both streams to their end.
#[derive(Debug)]
pub enum Outcome {
    Exited {
        status: ExitStatus,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        drain: Drain,
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

impl Outcome {
    /// The exit code the log records for how this command ended: the
    /// process's own code, or `-1` for one a signal killed. A command the
    /// engine stopped, by its timeout or by the run's cancellation, has no
    /// exit of its own and is recorded as [`STOPPED_EXIT_CODE`], which is
    /// `-2`: the one value no process exits with.
    pub fn exit_code(&self) -> i32 {
        match self {
            Outcome::Exited { status, .. } => status.code().unwrap_or(-1),
            Outcome::TimedOut { .. } | Outcome::Cancelled { .. } => STOPPED_EXIT_CODE,
        }
    }
}

/// Runs `command` to its end under the engine's governance: in its own
/// process group, registered while it lives, killed with its whole
/// tree when its timeout elapses, the supervision's token fires or its
/// caller drops it before it ends. The leader stays waitable until the
/// process group has been closed and its pipes have drained. This keeps
/// its PID from being reused while any signal can still target the
/// group.
pub async fn spawn_governed(
    command: GovernedCommand,
    supervision: Supervision<'_>,
) -> Result<Outcome, SpawnError> {
    let described = command.describe();
    let (mut child, pgid) = spawn_leader(&command, supervision, &described)?;
    let _registration = process_registry::register(supervision.registry, Some(pgid));

    let timeout_at = command
        .timeout
        .map(|timeout| tokio::time::Instant::now() + timeout);
    let stdout = Captured::default();
    let stderr = Captured::default();
    let mut pipes = start_pipes(&mut child, command.stdin, &stdout, &stderr);
    let mut child = Leader::new(child, pgid);

    let mut observe_error = None;
    let mut exit = ExitWatch::new(pgid);
    let mut waited = tokio::select! {
        biased;
        _ = supervision.cancel.cancelled() => Waited::Cancelled,
        _ = wait_for_deadline(timeout_at) => Waited::TimedOut,
        exited = exit.exited() => match exited {
            Ok(()) => Waited::Exited,
            Err(error) => {
                observe_error = Some(error);
                Waited::Failed
            }
        },
    };

    let cleanup = close_group(&mut child, pgid, supervision, timeout_at, &mut waited).await;
    let (pipe_failure, drain) = drain_pipes(
        &mut pipes,
        pgid,
        supervision,
        timeout_at,
        &mut waited,
        &cleanup,
        &described,
    )
    .await;
    let status = reap(&mut child, supervision, timeout_at, &mut waited).await;
    let settled = Settled {
        waited,
        drain,
        observe_error,
        cleanup,
        pipe_failure,
        status,
    };
    outcome(described, pgid, settled, &stdout, &stderr)
}

/// The pipes a command's output is read through, one task per pipe, each
/// yielding the stream it serves and how reading or writing it ended.
type Pipes = JoinSet<(PipeKind, std::io::Result<()>)>;

/// How a command's end settled: how it was stopped, and what its group, its
/// pipes and its leader failed with, if anything.
struct Settled {
    waited: Waited,
    drain: Drain,
    observe_error: Option<std::io::Error>,
    cleanup: Result<(), GroupError>,
    pipe_failure: Option<PipeFailure>,
    status: std::io::Result<ExitStatus>,
}

/// Stops reading the pipes: the group is killed once more, a failure of that
/// kill is kept when the group had closed, and every pipe is aborted and
/// waited for.
async fn abort_pipes(
    pipes: &mut Pipes,
    pgid: Pid,
    cleanup: &Result<(), GroupError>,
    pipe_failure: &mut Option<PipeFailure>,
) {
    // The leader is intentionally still unreaped, so the process group
    // id is still its identity while cleanup is repeated here.
    let second_cleanup = force_kill_group(pgid).await;
    if cleanup.is_ok() {
        if let Err(error) = second_cleanup {
            pipe_failure.get_or_insert(PipeFailure::Group(error));
        }
    }
    pipes.abort_all();
    while pipes.join_next().await.is_some() {}
}

/// Collects the leader, which is the first point at which its pid may safely
/// be reused. The wait stays cut by a cancellation or a deadline that fires
/// once the group is closed.
async fn reap(
    child: &mut Leader,
    supervision: Supervision<'_>,
    timeout_at: Option<tokio::time::Instant>,
    waited: &mut Waited,
) -> std::io::Result<ExitStatus> {
    // The group has been signalled and its members killed. The
    // leader is now collected, which is the first point at which its pid
    // may safely be reused.
    loop {
        tokio::select! {
            biased;
            status = child.wait() => break status,
            _ = supervision.cancel.cancelled(), if *waited == Waited::Exited => {
                *waited = Waited::Cancelled;
            }
            _ = wait_for_deadline(timeout_at), if *waited == Waited::Exited => {
                *waited = Waited::TimedOut;
            }
        }
    }
}

/// The outcome of a command that ended: the failure that settled it when it
/// has one, and otherwise how it was stopped and what it wrote.
fn outcome(
    command: String,
    pgid: Pid,
    settled: Settled,
    stdout_buf: &Captured,
    stderr_buf: &Captured,
) -> Result<Outcome, SpawnError> {
    let captured = captured_output(stdout_buf.snapshot(), stderr_buf.snapshot());
    if let Some(source) = settled.observe_error {
        return Err(SpawnError::Observe {
            command,
            source,
            output: captured,
        });
    }
    if let Err(source) = settled.cleanup {
        return Err(SpawnError::Kill {
            command,
            source: Box::new(source),
            output: captured,
        });
    }
    if let Some(failure) = settled.pipe_failure {
        return Err(pipe_error(command, captured, failure));
    }
    let status = match settled.status {
        Ok(status) => status,
        Err(source) => {
            return Err(SpawnError::Wait {
                command,
                source,
                output: captured,
            })
        }
    };
    let CapturedOutput { stdout, stderr } = *captured;
    Ok(match settled.waited {
        Waited::Exited => Outcome::Exited {
            status,
            stdout,
            stderr,
            drain: settled.drain,
        },
        Waited::TimedOut | Waited::Failed => Outcome::TimedOut {
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

/// The error a failed pipe, or a group that could not be closed while the
/// pipes were read, makes of the command: with what it had written.
fn pipe_error(command: String, output: Box<CapturedOutput>, failure: PipeFailure) -> SpawnError {
    match failure {
        PipeFailure::Io(stream, source) => SpawnError::Read {
            command,
            stream,
            source,
            output,
        },
        PipeFailure::Join(stream, source) => SpawnError::ReadTask {
            command,
            stream,
            source,
            output,
        },
        PipeFailure::Group(source) => SpawnError::Kill {
            command,
            source: Box::new(source),
            output,
        },
    }
}
