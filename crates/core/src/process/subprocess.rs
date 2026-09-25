//! The one way an adapter runs its CLI. A session's subprocess is born
//! in its own process group, owned by the session that opened it, and
//! killed with its whole tree when that session is killed or dropped.
//! Its stdout is read line by line — tolerant of bytes that are not
//! UTF-8, bounded in line length — into the adapter's own parser; its
//! stderr is drained to the trace log; its prompt travels by stdin.
//! `claude_code` and `codex` differ only in the arguments they build,
//! the lines they parse and the settings they read.

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use crate::{AdapterError, AdapterId, Pid, Result, Secret};
use async_trait::async_trait;
use futures::stream::{self, BoxStream};
use tokio::process::Child;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::lines::LineReader;
use crate::events::{SessionEnd, SessionExit, STDERR_TAIL_LINES};
use crate::port::{AgentError, AgentEvent, AgentSession, ProbeReport};
use crate::process::signal::{signal_group, Signal};

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

/// Everything a CLI session needs to start.
pub struct Launch<'a> {
    pub adapter: &'static AdapterId,
    pub binary: &'a Path,
    pub args: Vec<String>,
    pub cwd: &'a Path,
    /// Secrets included; they reach the child's environment and
    /// nothing else.
    pub env: &'a HashMap<String, Secret<String>>,
    pub prompt: &'a str,
    pub parser: Box<dyn LineParser>,
}

/// Starts the CLI and hands back the session that owns it. The readers
/// run before the prompt is written, so a CLI that talks before it
/// listens cannot deadlock the exchange.
pub async fn open(launch: Launch<'_>) -> Result<Box<dyn AgentSession>> {
    let adapter = launch.adapter;
    let mut std_cmd = std::process::Command::new(launch.binary);
    std_cmd
        .args(&launch.args)
        .current_dir(launch.cwd)
        .envs(
            launch
                .env
                .iter()
                .map(|(name, value)| (name, value.expose())),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // The whole session's tree dies together: a group-targeted signal
    // reaches every descendant the CLI spawns, its own tool
    // subprocesses included, not just the CLI process itself.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        std_cmd.process_group(0);
    }
    let mut child = tokio::process::Command::from(std_cmd)
        .kill_on_drop(true)
        .spawn()
        .map_err(|source| AdapterError::AdapterIo {
            adapter: adapter.clone(),
            action: format!("spawn `{}`", launch.binary.display()),
            source,
        })?;
    let pgid = child
        .id()
        .and_then(|id| Pid::try_from(id).ok())
        .ok_or_else(|| AdapterError::Adapter {
            adapter: adapter.clone(),
            message: format!(
                "`{}` exited before it could be tracked",
                launch.binary.display()
            ),
        })?;
    let missing_pipe = |name: &str| AdapterError::Adapter {
        adapter: adapter.clone(),
        message: format!("`{}` has no {name} pipe", launch.binary.display()),
    };
    let stdin = child.stdin.take().ok_or_else(|| missing_pipe("stdin"))?;
    let stdout = child.stdout.take().ok_or_else(|| missing_pipe("stdout"))?;
    let stderr = child.stderr.take().ok_or_else(|| missing_pipe("stderr"))?;

    let (tx, rx) = mpsc::unbounded_channel();
    let mut parser = launch.parser;
    let reader = tokio::spawn(async move {
        let mut lines = LineReader::new(stdout);
        let mut opened = false;
        while let Some(line) = lines.next_line().await {
            for event in parser.parse(&line) {
                let event = match event {
                    AgentEvent::SessionOpened { .. } | AgentEvent::Failed { .. } => event,
                    other if opened => other,
                    // The session opens before anything happens in it: a
                    // CLI that reports work under no session is not
                    // speaking the protocol, and a retry would read the
                    // same lines.
                    other => AgentEvent::Failed {
                        error: AgentError::message(format!(
                            "the CLI reported {} before opening the session",
                            describe(&other)
                        )),
                        retryable: false,
                    },
                };
                opened = true;
                let terminal = matches!(
                    event,
                    AgentEvent::Completed { .. } | AgentEvent::Failed { .. }
                );
                if tx.send(event).is_err() || terminal {
                    // Exactly one terminal event: nothing after it is
                    // part of the session.
                    return;
                }
            }
        }
    });
    // The tail is shared with the session: the drain writes it as the
    // child speaks, and `exit` reads it once the child is gone, which is
    // the only moment anybody asks.
    let stderr_tail = Arc::new(Mutex::new(VecDeque::with_capacity(STDERR_TAIL_LINES)));
    let secrets: Vec<Secret<String>> = launch.env.values().cloned().collect();
    let tail = Arc::clone(&stderr_tail);
    let stderr_drain = tokio::spawn(async move {
        let mut lines = LineReader::new(stderr);
        while let Some(line) = lines.next_line().await {
            tracing::debug!(adapter = %adapter, "stderr: {line}");
            let mut tail = tail.lock().unwrap_or_else(PoisonError::into_inner);
            if tail.len() == STDERR_TAIL_LINES {
                tail.pop_front();
            }
            tail.push_back(redacted(line, &secrets));
        }
    });
    // Owned before the prompt goes out: a failed write drops the
    // session, and the drop takes the tree with it.
    let session = SubprocessSession {
        adapter,
        child,
        pgid,
        reaped: false,
        reader,
        stderr_drain,
        stderr_tail,
        receiver: Some(rx),
    };
    write_prompt(stdin, launch.prompt, adapter).await?;
    Ok(Box::new(session))
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
    /// The last [`STDERR_TAIL_LINES`] lines the child wrote, redacted.
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
    receiver: Option<mpsc::UnboundedReceiver<AgentEvent>>,
}

/// One stderr line as the log keeps it: every value this session's
/// environment carried replaced by `[redacted]`.
///
/// The child's environment is where this system puts its secrets — the
/// run tools' token among them — and a CLI that fails at startup is
/// exactly the one liable to echo what it was given back at stderr.
fn redacted(line: String, secrets: &[Secret<String>]) -> String {
    secrets.iter().fold(line, |line, secret| {
        let value = secret.expose();
        if value.is_empty() {
            line
        } else {
            line.replace(value, "[redacted]")
        }
    })
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

    /// The readers hold the pipes; dropping them before the wait means
    /// no process can sit on a full pipe nobody reads from.
    async fn finish_readers(&mut self, abort_stderr: bool) -> Result<()> {
        self.reader.abort();
        if abort_stderr {
            self.stderr_drain.abort();
        }
        let stdout = (&mut self.reader).await;
        let stderr = (&mut self.stderr_drain).await;
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

    async fn kill(&mut self) -> Result<()> {
        let cleanup = self.force_kill_group().await;
        if cleanup.is_err() {
            if let Err(error) = self.kill_group_now() {
                tracing::warn!(adapter = %self.adapter, pgid = %self.pgid, error = %error, "fallback signal after group cleanup failed");
            }
        }
        if let Err(error) = self.child.start_kill() {
            tracing::warn!(adapter = %self.adapter, pgid = %self.pgid, error = %error, "fallback signal to the session leader failed");
        }
        let pipes = self.finish_readers(true).await;
        let wait = self.child.wait().await;
        match wait {
            Ok(status) => {
                self.reaped = true;
                tracing::debug!(adapter = %self.adapter, pgid = %self.pgid, %status, "session killed");
            }
            Err(e) => {
                tracing::warn!(adapter = %self.adapter, pgid = %self.pgid, error = %e, "failed to collect the killed session's exit");
                cleanup?;
                pipes?;
                return Err(AdapterError::AdapterIo {
                    adapter: self.adapter.clone(),
                    action: "collect the killed session's exit".to_string(),
                    source: e,
                });
            }
        }
        cleanup?;
        pipes
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

/// An event's kind, for a message about one that came out of order.
fn describe(event: &AgentEvent) -> &'static str {
    match event {
        AgentEvent::SessionOpened { .. } => "a session opening",
        AgentEvent::RunToolsMounted { .. } => "the run tools it holds",
        AgentEvent::ToolUse { .. } => "a tool use",
        AgentEvent::WriteRefused { .. } => "a write it refused",
        AgentEvent::Usage { .. } => "token usage",
        AgentEvent::Note { .. } => "a note",
        AgentEvent::Completed { .. } => "a completion",
        AgentEvent::Failed { .. } => "a failure",
    }
}
