//! The one way an adapter runs its CLI. A session's subprocess is born
//! in its own process group, owned by the session that opened it, and
//! killed with its whole tree when that session is killed or dropped.
//! Its stdout is read line by line — tolerant of bytes that are not
//! UTF-8, bounded in line length — into the adapter's own parser; its
//! stderr is drained to the trace log; its prompt travels by stdin.
//! `claude_code` and `codex` differ only in the arguments they build,
//! the lines they parse and the settings they read.

use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::{self, BoxStream};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Child;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use yunta_core::{AdapterError, AdapterId, Pid, Result, Secret};

use crate::session::{write_prompt, AgentError, AgentEvent, AgentSession, ProbeReport};
use crate::signal::{signal_group, Signal};

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
                        error: AgentError {
                            message: format!(
                                "the CLI reported {} before opening the session",
                                describe(&other)
                            ),
                        },
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
    let stderr_drain = tokio::spawn(async move {
        let mut lines = LineReader::new(stderr);
        while let Some(line) = lines.next_line().await {
            tracing::debug!(adapter = %adapter, "stderr: {line}");
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
    receiver: Option<mpsc::UnboundedReceiver<AgentEvent>>,
}

impl SubprocessSession {
    fn kill_group(&self) -> Result<()> {
        if self.reaped {
            return Ok(());
        }
        signal_group(self.pgid, Signal::SIGKILL).map_err(|e| e.into_adapter_error(self.adapter))
    }

    /// The readers hold the pipes; dropping them before the wait means
    /// no process can sit on a full pipe nobody reads from.
    fn close_pipes(&self) {
        self.reader.abort();
        self.stderr_drain.abort();
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
        signal_group(self.pgid, Signal::SIGINT).map_err(|e| e.into_adapter_error(self.adapter))
    }

    async fn kill(&mut self) -> Result<()> {
        self.kill_group()?;
        self.close_pipes();
        match self.child.wait().await {
            Ok(status) => {
                self.reaped = true;
                tracing::debug!(adapter = %self.adapter, pgid = %self.pgid, %status, "session killed");
            }
            Err(e) => {
                tracing::warn!(adapter = %self.adapter, pgid = %self.pgid, error = %e, "failed to collect the killed session's exit");
            }
        }
        Ok(())
    }

    fn pgid(&self) -> Option<Pid> {
        Some(self.pgid)
    }
}

/// The tokio child's own `kill_on_drop` reaches the leader alone; the
/// group is what must die, whether the session ended or was abandoned.
impl Drop for SubprocessSession {
    fn drop(&mut self) {
        if let Err(e) = self.kill_group() {
            tracing::warn!(adapter = %self.adapter, pgid = %self.pgid, error = %e, "failed to kill a dropped session's process group");
        }
        self.close_pipes();
    }
}

/// Lines from a pipe: decoded with replacement characters where the
/// bytes are not UTF-8, without their line ending, and at most
/// [`MAX_LINE_BYTES`] long — a longer one is discarded whole, with a
/// warning, and reading goes on with the next. A last line without a
/// line ending is still a line.
struct LineReader<R> {
    reader: BufReader<R>,
}

impl<R: AsyncRead + Unpin> LineReader<R> {
    fn new(pipe: R) -> Self {
        LineReader {
            reader: BufReader::new(pipe),
        }
    }

    async fn next_line(&mut self) -> Option<String> {
        let mut line = Vec::new();
        let mut discarding = false;
        loop {
            let available = match self.reader.fill_buf().await {
                Ok(available) => available,
                Err(e) => {
                    tracing::warn!(error = %e, "error reading a subprocess pipe");
                    return None;
                }
            };
            if available.is_empty() {
                return (!discarding && !line.is_empty()).then(|| decode(&line));
            }
            let (chunk, ended) = match available
                .iter()
                .position(|byte| *byte == b'\n')
                .and_then(|at| available.get(..at))
            {
                Some(chunk) => (chunk, true),
                None => (available, false),
            };
            let consumed = chunk.len() + usize::from(ended);
            if !discarding && line.len() + chunk.len() > MAX_LINE_BYTES {
                tracing::warn!(
                    limit = MAX_LINE_BYTES,
                    "discarding a subprocess line longer than the limit"
                );
                line.clear();
                discarding = true;
            } else if !discarding {
                line.extend_from_slice(chunk);
            }
            self.reader.consume(consumed);
            if ended {
                if discarding {
                    discarding = false;
                    continue;
                }
                return Some(decode(&line));
            }
        }
    }
}

/// An event's kind, for a message about one that came out of order.
fn describe(event: &AgentEvent) -> &'static str {
    match event {
        AgentEvent::SessionOpened { .. } => "a session opening",
        AgentEvent::ToolUse { .. } => "a tool use",
        AgentEvent::Usage { .. } => "token usage",
        AgentEvent::Note { .. } => "a note",
        AgentEvent::Completed { .. } => "a completion",
        AgentEvent::Failed { .. } => "a failure",
    }
}

fn decode(line: &[u8]) -> String {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    String::from_utf8_lossy(line).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn lines_of(bytes: &'static [u8]) -> Vec<String> {
        let mut reader = LineReader::new(bytes);
        let mut lines = Vec::new();
        while let Some(line) = reader.next_line().await {
            lines.push(line);
        }
        lines
    }

    #[tokio::test]
    async fn bytes_that_are_not_utf8_become_replacement_characters() {
        assert_eq!(
            lines_of(b"caf\xff\xfe\nok\n").await,
            vec!["caf\u{FFFD}\u{FFFD}".to_string(), "ok".to_string()]
        );
    }

    #[tokio::test]
    async fn a_last_line_without_a_line_ending_is_still_a_line() {
        assert_eq!(
            lines_of(b"first\r\nlast").await,
            vec!["first".to_string(), "last".to_string()]
        );
    }

    #[tokio::test]
    async fn a_line_over_the_limit_is_discarded_and_reading_goes_on() {
        let oversized: &'static [u8] = Box::leak(
            [vec![b'x'; MAX_LINE_BYTES + 1], b"\nafter\n".to_vec()]
                .concat()
                .into_boxed_slice(),
        );
        assert_eq!(lines_of(oversized).await, vec!["after".to_string()]);
    }
}
