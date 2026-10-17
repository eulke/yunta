//! Starting a CLI session: the child in its own process group, its three
//! pipes, the reader that parses what it says, and the drain that keeps
//! the tail of its stderr.

use std::collections::VecDeque;
use std::process::Stdio;
use std::sync::{Arc, Mutex, PoisonError};

use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::{write_prompt, Launch, LineParser, SubprocessSession};
use crate::events::TAIL_LINES;
use crate::port::{AgentEvent, AgentSession};
use crate::process::lines::LineReader;
use crate::process::stream::StreamGuard;
use crate::{AdapterError, AdapterId, Pid, Redactor, Result};

/// Starts the CLI and hands back the session that owns it. The readers
/// run before the prompt is written, so a CLI that talks before it
/// listens cannot deadlock the exchange.
pub async fn open(launch: Launch<'_>) -> Result<Box<dyn AgentSession>> {
    let adapter = launch.adapter;
    let mut child = spawn_child(&launch)?;
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
    let (stdin, stdout, stderr) = take_pipes(&mut child, &launch)?;

    let (tx, rx) = mpsc::unbounded_channel();
    let reader = spawn_reader(stdout, launch.parser, adapter, tx);
    // The tail is shared with the session: the drain writes it as the
    // child speaks, and `exit` reads it once the child is gone, which is
    // the only moment anybody asks.
    let stderr_tail = Arc::new(Mutex::new(VecDeque::with_capacity(TAIL_LINES)));
    // The child's environment is where this system puts its secrets, and a
    // CLI that fails at startup is the one liable to echo what it was given
    // back at stderr: the tail keeps no value of `secret`.
    let redactor = Redactor::of_values(launch.env.secret.values().cloned());
    let stderr_drain = drain_stderr(stderr, redactor, adapter, Arc::clone(&stderr_tail));
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
        held: launch.held,
    };
    write_prompt(stdin, launch.prompt, adapter).await?;
    Ok(Box::new(session))
}

/// The CLI's process, with its arguments and environment and its three
/// pipes open, in a process group of its own. Killed with the session that
/// holds it.
fn spawn_child(launch: &Launch<'_>) -> Result<Child> {
    let mut std_cmd = std::process::Command::new(launch.binary);
    std_cmd
        .args(&launch.args)
        .current_dir(launch.cwd)
        .envs(&launch.env.plain)
        .envs(
            launch
                .env
                .secret
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
    tokio::process::Command::from(std_cmd)
        .kill_on_drop(true)
        .spawn()
        .map_err(|source| AdapterError::AdapterIo {
            adapter: launch.adapter.clone(),
            action: format!("spawn `{}`", launch.binary.display()),
            source,
        })
}

/// The child's three pipes, each taken once. A pipe the child did not open
/// is an adapter error that names it.
fn take_pipes(
    child: &mut Child,
    launch: &Launch<'_>,
) -> Result<(ChildStdin, ChildStdout, ChildStderr)> {
    let missing_pipe = |name: &str| AdapterError::Adapter {
        adapter: launch.adapter.clone(),
        message: format!("`{}` has no {name} pipe", launch.binary.display()),
    };
    let stdin = child.stdin.take().ok_or_else(|| missing_pipe("stdin"))?;
    let stdout = child.stdout.take().ok_or_else(|| missing_pipe("stdout"))?;
    let stderr = child.stderr.take().ok_or_else(|| missing_pipe("stderr"))?;
    Ok((stdin, stdout, stderr))
}

/// Parses the CLI's stdout line by line into events and sends them to the
/// session, until a terminal event or a line the guard refuses ends it.
fn spawn_reader(
    stdout: ChildStdout,
    mut parser: Box<dyn LineParser>,
    adapter: &'static AdapterId,
    tx: mpsc::UnboundedSender<AgentEvent>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut lines = LineReader::new(stdout);
        let mut guard = StreamGuard::default();
        while let Some(line) = lines.next_line().await {
            for event in parser.parse(&line) {
                let Some(event) = guard.admit(event, adapter.as_str()) else {
                    return;
                };
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
    })
}

/// Drains the CLI's stderr to the trace log and into the tail the session
/// keeps: each line redacted, and the tail bounded by [`TAIL_LINES`].
fn drain_stderr(
    stderr: ChildStderr,
    redactor: Redactor,
    adapter: &'static AdapterId,
    tail: Arc<Mutex<VecDeque<String>>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut lines = LineReader::new(stderr);
        while let Some(line) = lines.next_line().await {
            let line = redactor.text(&line);
            tracing::debug!(adapter = %adapter, "stderr: {line}");
            let mut tail = tail.lock().unwrap_or_else(PoisonError::into_inner);
            if tail.len() == TAIL_LINES {
                tail.pop_front();
            }
            tail.push_back(line);
        }
    })
}
