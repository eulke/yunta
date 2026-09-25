//! A CLI child owned by an integration test. Output is drained while the
//! test waits, every wait uses the shared deadline, and unwinding kills and
//! reaps the process group before the fixture is dropped.

use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use yunta_core::process::group::{child_has_exited_unreaped, force_kill_group};
use yunta_core::process::signal::{signal_group, signal_process, Signal};
use yunta_core::Pid;

use crate::WAIT_DEADLINE;

type SharedBytes = Arc<Mutex<Vec<u8>>>;

/// A CLI subprocess owned by a test. Use `wait_with_output` to finish it;
/// dropping it early (including during panic) kills its group, reaps the
/// leader and joins both output readers.
pub struct CliChild {
    child: Child,
    pgid: Pid,
    command: String,
    stdout: SharedBytes,
    stderr: SharedBytes,
    readers: Vec<JoinHandle<io::Result<()>>>,
    status: Option<ExitStatus>,
}

impl CliChild {
    /// Starts a command in a dedicated process group, drains both output
    /// streams immediately and optionally tees them into one live log file.
    pub fn spawn(command: Command, log: Option<&Path>) -> io::Result<Self> {
        Self::spawn_with_logs(command, log, log)
    }

    /// Like [`Self::spawn`], with independent live logs for stdout and
    /// stderr. A CLI progress test can observe stderr without mixing in the
    /// command's final stdout summary.
    pub fn spawn_with_stderr_log(command: Command, stderr_log: Option<&Path>) -> io::Result<Self> {
        Self::spawn_with_logs(command, None, stderr_log)
    }

    fn spawn_with_logs(
        mut command: Command,
        stdout_log: Option<&Path>,
        stderr_log: Option<&Path>,
    ) -> io::Result<Self> {
        let description = format!("{command:?}");
        let stdout_log = stdout_log.map(std::fs::File::create).transpose()?;
        let stderr_log = stderr_log.map(std::fs::File::create).transpose()?;
        let stdout_log = stdout_log.map(|file| Arc::new(Mutex::new(file)));
        let stderr_log = stderr_log.map(|file| Arc::new(Mutex::new(file)));
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command.spawn()?;
        let pgid = match Pid::try_from(child.id()) {
            Ok(pgid) => pgid,
            Err(_) => {
                return Err(match child.kill().and_then(|()| child.wait().map(|_| ())) {
                    Ok(()) => io::Error::other("the spawned CLI has no valid pid"),
                    Err(error) => io::Error::other(format!(
                        "the spawned CLI has no valid pid; cleanup failed: {error}"
                    )),
                });
            }
        };
        let stdout = Arc::new(Mutex::new(Vec::new()));
        let stderr = Arc::new(Mutex::new(Vec::new()));
        let mut readers = Vec::with_capacity(2);
        if let Some(pipe) = child.stdout.take() {
            readers.push(read_output(pipe, Arc::clone(&stdout), stdout_log));
        }
        if let Some(pipe) = child.stderr.take() {
            readers.push(read_output(pipe, Arc::clone(&stderr), stderr_log));
        }
        Ok(Self {
            child,
            pgid,
            command: description,
            stdout,
            stderr,
            readers,
            status: None,
        })
    }

    pub fn pid(&self) -> Pid {
        self.pgid
    }

    /// Reports an exit only after closing any descendants and collecting
    /// the leader. An active child remains waitable and keeps its pid from
    /// being reused while its group may still need a signal.
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if let Some(status) = self.status {
            return Ok(Some(status));
        }
        if !child_has_exited_unreaped(self.pgid)? {
            return Ok(None);
        }
        self.close_group()?;
        let status = self.child.wait()?;
        self.status = Some(status);
        Ok(Some(status))
    }

    /// Waits for and reaps the leader without closing its group. This is
    /// for a test that deliberately simulates an engine crash: its
    /// registered descendants must remain for `yunta cancel` to find.
    /// The test owns cleanup of those separately registered groups.
    pub fn wait_until_exit(&mut self) -> io::Result<()> {
        if self.status.is_some() {
            return Ok(());
        }
        let deadline = Instant::now() + WAIT_DEADLINE;
        loop {
            if child_has_exited_unreaped(self.pgid)? {
                self.status = Some(self.child.wait()?);
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!(
                        "`{}` did not exit within {WAIT_DEADLINE:?}\nstdout:\n{}\nstderr:\n{}",
                        self.command,
                        String::from_utf8_lossy(&self.output_bytes(&self.stdout)),
                        String::from_utf8_lossy(&self.output_bytes(&self.stderr)),
                    ),
                ));
            }
            thread::yield_now();
        }
    }

    /// Waits up to [`WAIT_DEADLINE`] while the reader threads keep draining
    /// output. Timeout diagnostics include the command and everything read
    /// so far; `Drop` cleans up the process group as the error unwinds.
    pub fn wait_with_output(mut self) -> io::Result<Output> {
        let deadline = Instant::now() + WAIT_DEADLINE;
        let status = loop {
            if let Some(status) = self.try_wait()? {
                break status;
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!(
                        "`{}` did not exit within {WAIT_DEADLINE:?}\nstdout:\n{}\nstderr:\n{}",
                        self.command,
                        String::from_utf8_lossy(&self.output_bytes(&self.stdout)),
                        String::from_utf8_lossy(&self.output_bytes(&self.stderr)),
                    ),
                ));
            }
            thread::yield_now();
        };

        self.join_readers()?;
        Ok(Output {
            status,
            stdout: self.output_bytes(&self.stdout),
            stderr: self.output_bytes(&self.stderr),
        })
    }

    fn output_bytes(&self, stream: &SharedBytes) -> Vec<u8> {
        stream
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn close_group(&self) -> io::Result<()> {
        force_kill_process_group(self.pgid)
    }

    fn wait_for_exit_bounded(&self) {
        let deadline = Instant::now() + WAIT_DEADLINE;
        while Instant::now() < deadline {
            match child_has_exited_unreaped(self.pgid) {
                Ok(true) => return,
                Ok(false) => {}
                Err(error) => {
                    eprintln!(
                        "could not inspect `{}` while dropping it: {error}",
                        self.command
                    );
                    return;
                }
            }
            thread::yield_now();
        }
    }

    fn reap(&mut self) {
        match self.child.wait() {
            Ok(status) => self.status = Some(status),
            Err(error) => eprintln!(
                "could not reap `{}` while dropping it: {error}",
                self.command
            ),
        }
    }

    fn join_readers(&mut self) -> io::Result<()> {
        let mut failure = None;
        for reader in self.readers.drain(..) {
            match reader.join() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    failure.get_or_insert(error);
                }
                Err(_) => {
                    failure.get_or_insert_with(|| io::Error::other("CLI output reader panicked"));
                }
            }
        }
        failure.map_or(Ok(()), Err)
    }
}

/// Closes a process group from a synchronous integration test using the
/// same stop, stable-observation and kill sequence as production.
pub fn force_kill_process_group(pgid: Pid) -> io::Result<()> {
    thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .map_err(io::Error::other)?
            .block_on(force_kill_group(pgid))
            .map_err(io::Error::other)
    })
    .join()
    .map_err(|_| io::Error::other("process-group cleanup thread panicked"))?
}

impl Drop for CliChild {
    fn drop(&mut self) {
        if self.status.is_none() {
            match child_has_exited_unreaped(self.pgid) {
                Ok(true) => {}
                Ok(false) | Err(_) => {
                    if let Err(error) = signal_process(self.pgid, Signal::SIGINT) {
                        if !error.is_gone() {
                            eprintln!(
                                "could not interrupt `{}` while dropping it: {error}",
                                self.command
                            );
                        }
                    }
                    self.wait_for_exit_bounded();
                }
            }
            if let Err(error) = self.close_group() {
                eprintln!(
                    "could not close `{}` while dropping it: {error}",
                    self.command
                );
                if let Err(error) = signal_group(self.pgid, Signal::SIGKILL) {
                    eprintln!(
                        "could not kill `{}` while dropping it: {error}",
                        self.command
                    );
                }
                if let Err(error) = self.child.kill() {
                    eprintln!(
                        "could not kill leader of `{}` while dropping it: {error}",
                        self.command
                    );
                }
            }
            self.reap();
        }
        if let Err(error) = self.join_readers() {
            eprintln!(
                "could not join output readers for `{}`: {error}",
                self.command
            );
        }
    }
}

fn read_output<R: Read + Send + 'static>(
    mut pipe: R,
    capture: SharedBytes,
    log: Option<Arc<Mutex<std::fs::File>>>,
) -> JoinHandle<io::Result<()>> {
    thread::spawn(move || {
        let mut bytes = [0_u8; 8192];
        loop {
            let read = pipe.read(&mut bytes)?;
            if read == 0 {
                return Ok(());
            }
            capture
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend_from_slice(&bytes[..read]);
            if let Some(log) = &log {
                let mut log = log.lock().unwrap_or_else(PoisonError::into_inner);
                log.write_all(&bytes[..read])?;
                log.flush()?;
            }
        }
    })
}
