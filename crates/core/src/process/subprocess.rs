//! The one way an adapter runs its CLI. A session's subprocess is born
//! in its own process group, owned by the session that opened it, and
//! killed with its whole tree when that session is killed or dropped.
//! Its stdout is read line by line — tolerant of bytes that are not
//! UTF-8, bounded in line length — into the adapter's own parser; its
//! stderr is drained to the trace log; its prompt travels by stdin.
//! `claude_code` and `codex` differ only in the arguments they build,
//! the lines they parse and the settings they read.

use std::collections::VecDeque;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use crate::{AdapterError, AdapterId, Pid, Result};
use async_trait::async_trait;
use futures::stream::{self, BoxStream};
use tokio::process::Child;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::events::{SessionEnd, SessionExit};
use crate::port::session::SessionEnv;
use crate::port::{AgentEvent, AgentSession, ProbeReport};
use crate::process::signal::{signal_group, Signal};

mod launch;

pub use launch::open;

/// Hands `prompt` to a just-spawned CLI on its stdin and closes the
/// pipe, so the CLI sees end-of-input and the prompt never appears in
/// an argument list. Called once the CLI's output is being read, so a
/// CLI that talks before it listens cannot deadlock the exchange. A CLI
/// that exits before reading closes the pipe on its side; that is the
/// session's own ending, reported by its stream, never a failure of the
/// write.
pub async fn write_prompt(
    mut stdin: tokio::process::ChildStdin,
    prompt: &str,
    adapter: &'static AdapterId,
) -> Result<()> {
    use tokio::io::AsyncWriteExt;

    let io_error = |action: &str, source: std::io::Error| AdapterError::AdapterIo {
        adapter: adapter.clone(),
        action: action.to_string(),
        source,
    };
    match stdin.write_all(prompt.as_bytes()).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => return Ok(()),
        Err(e) => return Err(io_error("write the prompt to the subprocess's stdin", e)),
    }
    match stdin.shutdown().await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(e) => Err(io_error("close the subprocess's stdin", e)),
    }
}

/// The longest line the reader accepts from a CLI. A stream-json event
/// carrying a whole tool result stays far below it; the bound exists
/// so a runaway writer cannot grow this process's memory without end.
pub const MAX_LINE_BYTES: usize = 1024 * 1024;

/// How long a probe waits for a CLI to answer `--version`, which takes
/// milliseconds when the binary is there and works.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// Turns one line of a CLI's output into the events it carries.
pub trait LineParser: Send {
    fn parse(&mut self, line: &str) -> Vec<AgentEvent>;
}

/// What a launch sets up before its CLI starts, and takes back once the
/// session it opened has ended. `Sync` because the session that holds it
/// is shared by reference across the awaits of its own methods.
pub trait Held: Send + Sync {
    /// Takes back what was set up. The session is over by the time it is
    /// asked.
    fn release(self: Box<Self>) -> Result<()>;
}

/// Everything a CLI session needs to start.
pub struct Launch<'a> {
    pub adapter: &'static AdapterId,
    pub binary: &'a Path,
    pub args: Vec<String>,
    pub cwd: &'a Path,
    /// Both maps reach the child's environment; only the values of
    /// `secret` are taken out of its stderr tail.
    pub env: &'a SessionEnv,
    pub prompt: &'a str,
    pub parser: Box<dyn LineParser>,
    /// What the launch set up for the session, given back when the session
    /// ends, or when the launch fails before it. `None` when the launch set
    /// nothing up.
    pub held: Option<Box<dyn Held>>,
}

/// `binary --version` under [`PROBE_TIMEOUT`]: its first answer as the
/// version, or why the CLI could not be asked.
pub async fn probe_version(binary: &Path) -> ProbeReport {
    let asked = tokio::process::Command::new(binary)
        .arg("--version")
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(PROBE_TIMEOUT, asked).await {
        Ok(Ok(output)) if output.status.success() => ProbeReport::Healthy {
            version: Some(String::from_utf8_lossy(&output.stdout).trim().to_string()),
        },
        Ok(Ok(output)) => ProbeReport::Unhealthy {
            diagnostic: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        },
        Ok(Err(e)) => ProbeReport::Unhealthy {
            diagnostic: format!("`{}` could not be run: {e}", binary.display()),
        },
        Err(_elapsed) => ProbeReport::Unhealthy {
            diagnostic: format!(
                "`{} --version` did not answer within {}s",
                binary.display(),
                PROBE_TIMEOUT.as_secs()
            ),
        },
    }
}

/// A live CLI session: the process, its readers and the events they
/// produce. Dropping it kills the whole process group.
pub struct SubprocessSession {
    adapter: &'static AdapterId,
    child: Child,
    /// Spawned with `process_group(0)`: the child's pid is its group's.
    pgid: Pid,
    /// The child's exit was collected, so its pid — and with it the
    /// group's id — may already belong to someone else.
    reaped: bool,
    reader: JoinHandle<()>,
    stderr_drain: JoinHandle<()>,
    /// The last [`TAIL_LINES`](crate::events::TAIL_LINES) lines the child wrote, redacted.
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
    receiver: Option<mpsc::UnboundedReceiver<AgentEvent>>,
    /// What the launch set up, taken back by [`AgentSession::release`] or,
    /// failing that, when the session is dropped.
    held: Option<Box<dyn Held>>,
}

impl SubprocessSession {
    fn kill_group_now(&self) -> Result<()> {
        if self.reaped {
            return Ok(());
        }
        signal_group(self.pgid, Signal::SIGKILL).map_err(|e| e.into_adapter_error(self.adapter))
    }

    async fn force_kill_group(&self) -> Result<()> {
        if self.reaped {
            return Ok(());
        }
        super::group::force_kill_group(self.pgid)
            .await
            .map_err(|error| AdapterError::AdapterIo {
                adapter: self.adapter.clone(),
                action: "close the session's process group".to_string(),
                source: std::io::Error::other(error),
            })
    }

    /// Closes the group for `kill`. A group whose members were not seen
    /// empty is not closed, so the cleanup's error is the answer: the fallback
    /// signal is sent once more, and its own failure only adds a warning.
    async fn close_group(&self) -> Result<()> {
        let Err(cleanup) = self.force_kill_group().await else {
            return Ok(());
        };
        if let Err(error) = self.kill_group_now() {
            tracing::warn!(adapter = %self.adapter, pgid = %self.pgid, error = %error, "fallback signal after group cleanup failed");
        }
        Err(cleanup)
    }

    /// The readers hold the pipes; dropping them before the wait means
    /// no process can sit on a full pipe nobody reads from.
    ///
    /// Called once the group is closed: what is still buffered on stderr
    /// reads at once, and a pipe still open after a moment is held by a
    /// process that left the group — a detached daemon — which the
    /// session does not wait for.
    async fn finish_readers(&mut self, abort_stderr: bool) -> Result<()> {
        self.reader.abort();
        if abort_stderr {
            self.stderr_drain.abort();
        }
        let stdout = (&mut self.reader).await;
        let stderr = match tokio::time::timeout(Duration::from_secs(2), &mut self.stderr_drain)
            .await
        {
            Ok(drained) => drained,
            Err(_) => {
                tracing::debug!(adapter = %self.adapter, pgid = %self.pgid, "a process outside the session's group still holds its stderr; reading stops here");
                self.stderr_drain.abort();
                (&mut self.stderr_drain).await
            }
        };
        for (action, result) in [
            ("read the session's stdout", stdout),
            ("read the session's stderr", stderr),
        ] {
            if let Err(error) = result {
                if !error.is_cancelled() {
                    return Err(AdapterError::AdapterIo {
                        adapter: self.adapter.clone(),
                        action: action.to_string(),
                        source: std::io::Error::other(error),
                    });
                }
            }
        }
        Ok(())
    }
}

#[async_trait]
impl AgentSession for SubprocessSession {
    fn events(&mut self) -> BoxStream<'_, AgentEvent> {
        match self.receiver.take() {
            Some(rx) => Box::pin(stream::unfold(rx, |mut rx| async move {
                rx.recv().await.map(|event| (event, rx))
            })),
            // A second call gets an already-exhausted stream rather than
            // a panic — the caller's misuse, not a reason to crash.
            None => Box::pin(stream::empty()),
        }
    }

    async fn interrupt(&mut self) -> Result<()> {
        if self.reaped {
            return Ok(());
        }
        signal_group(self.pgid, Signal::SIGINT).map_err(|e| e.into_adapter_error(self.adapter))
    }

    async fn allow_exit(&mut self, grace: Duration) {
        if self.reaped {
            return;
        }
        let mut exit = super::exit::ExitWatch::new(self.pgid);
        if let Ok(Err(error)) = tokio::time::timeout(grace, exit.exited()).await {
            tracing::debug!(adapter = %self.adapter, pgid = %self.pgid, %error, "could not watch the session leave");
        }
    }

    async fn kill(&mut self) -> Result<()> {
        let group = self.close_group().await;
        if let Err(error) = self.child.start_kill() {
            tracing::warn!(adapter = %self.adapter, pgid = %self.pgid, error = %error, "fallback signal to the session leader failed");
        }
        if let Err(error) = self.finish_readers(true).await {
            tracing::warn!(adapter = %self.adapter, pgid = %self.pgid, error = %error, "a reader of the killed session's output failed");
        }
        match self.child.wait().await {
            Ok(status) => {
                self.reaped = true;
                tracing::debug!(adapter = %self.adapter, pgid = %self.pgid, %status, "session killed");
                group
            }
            Err(e) => {
                tracing::warn!(adapter = %self.adapter, pgid = %self.pgid, error = %e, "failed to collect the killed session's exit");
                group?;
                Err(AdapterError::AdapterIo {
                    adapter: self.adapter.clone(),
                    action: "collect the killed session's exit".to_string(),
                    source: e,
                })
            }
        }
    }

    fn pgid(&self) -> Option<Pid> {
        (!self.reaped).then_some(self.pgid)
    }

    async fn exit(&mut self) -> Result<Option<SessionExit>> {
        // The group dies first, as in `kill`, so nothing can still be
        // writing and no reader can be left holding a pipe open. The
        // stdout reader is then dropped — its stream is exhausted, which
        // is why anyone is asking — while the stderr reader runs to the
        // end of its pipe rather than being cut off: what the child said
        // on its way out is the whole point of the question, and a pipe
        // whose only writer is a dead process ends by itself. The status
        // is collected last, a wait bounded by a process already dead.
        let cleanup = self.force_kill_group().await;
        if cleanup.is_err() {
            if let Err(error) = self.kill_group_now() {
                tracing::warn!(adapter = %self.adapter, pgid = %self.pgid, error = %error, "fallback signal after group cleanup failed");
            }
            if let Err(error) = self.child.start_kill() {
                tracing::warn!(adapter = %self.adapter, pgid = %self.pgid, error = %error, "fallback signal to the session leader failed");
            }
        }
        let pipes = self.finish_readers(cleanup.is_err()).await;
        let status = match self.child.wait().await {
            Ok(status) => {
                self.reaped = true;
                status
            }
            Err(e) => {
                tracing::warn!(adapter = %self.adapter, pgid = %self.pgid, error = %e, "failed to collect a dead session's exit");
                cleanup?;
                pipes?;
                return Err(AdapterError::AdapterIo {
                    adapter: self.adapter.clone(),
                    action: "collect a dead session's exit".to_string(),
                    source: e,
                });
            }
        };
        cleanup?;
        pipes?;
        Ok(Some(SessionExit {
            end: end_of(status),
            stderr_tail: self
                .stderr_tail
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .iter()
                .cloned()
                .collect(),
        }))
    }

    async fn release(&mut self) -> Result<()> {
        match self.held.take() {
            Some(held) => held.release(),
            None => Ok(()),
        }
    }
}

/// How a collected status says the process ended. A status is one or the
/// other on every platform this runs on; neither is [`SessionEnd::Unknown`],
/// which is what a reader makes of a `type` a newer build wrote.
fn end_of(status: std::process::ExitStatus) -> SessionEnd {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt as _;
        if let Some(signal) = status.signal() {
            return SessionEnd::Signal { signal };
        }
    }
    SessionEnd::Code {
        code: status.code().unwrap_or_default(),
    }
}

/// The tokio child's own `kill_on_drop` reaches the leader alone; the
/// group is what must die, whether the session ended or was abandoned.
impl Drop for SubprocessSession {
    fn drop(&mut self) {
        if let Err(e) = self.kill_group_now() {
            tracing::warn!(adapter = %self.adapter, pgid = %self.pgid, error = %e, "failed to kill a dropped session's process group");
        }
        self.reader.abort();
        self.stderr_drain.abort();
    }
}
