//! Signals to processes and process groups. Every call carries the
//! kernel's own answer — an `errno` inside a typed error, or a
//! [`Liveness`] — instead of a `kill` binary's exit status, which
//! depended on the `PATH` and lost the reason. The engine's tree kill,
//! the CLI's `cancel`, the adapters' sessions and the tests all signal
//! through here, and the workspace stays free of `unsafe`.

use std::fmt;

use nix::errno::Errno;
use nix::sys::signal;
pub use nix::sys::signal::Signal;
use thiserror::Error;
use yunta_core::{AdapterError, AdapterId, Pid};

/// What a signal is addressed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Process(Pid),
    Group(Pid),
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Target::Process(pid) => write!(f, "process {pid}"),
            Target::Group(pgid) => write!(f, "process group {pgid}"),
        }
    }
}

/// The kernel refused a signal; `source` is its `errno`.
#[derive(Debug, Error)]
#[error("failed to send {signal} to {target}")]
pub struct SignalError {
    pub target: Target,
    pub signal: Signal,
    #[source]
    pub source: Errno,
}

impl SignalError {
    /// The target no longer exists (`ESRCH`).
    pub fn is_gone(&self) -> bool {
        self.source == Errno::ESRCH
    }

    /// The refusal as an adapter's own error, the `errno` travelling as
    /// the OS error it is.
    pub fn into_adapter_error(self, adapter: &AdapterId) -> AdapterError {
        AdapterError::AdapterIo {
            adapter: adapter.clone(),
            action: format!("send {} to the session's {}", self.signal, self.target),
            source: std::io::Error::from(self.source),
        }
    }
}

/// Sends `signal` to `pid`. A process that is already gone is an error
/// here: whoever signals one process wants to know it was not there.
pub fn signal_process(pid: Pid, signal: Signal) -> Result<(), SignalError> {
    signal::kill(raw(pid), signal).map_err(|source| SignalError {
        target: Target::Process(pid),
        signal,
        source,
    })
}

/// Sends `signal` to every process in `pgid`'s group. A group that no
/// longer exists is the state every interrupt and every kill wants, so
/// `ESRCH` is success; any other refusal is an error.
pub fn signal_group(pgid: Pid, signal: Signal) -> Result<(), SignalError> {
    match signal::killpg(raw(pgid), signal) {
        Ok(()) | Err(Errno::ESRCH) => Ok(()),
        Err(source) => Err(SignalError {
            target: Target::Group(pgid),
            signal,
            source,
        }),
    }
}

/// What the kernel answers when asked about `pid` with the null signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Liveness {
    /// A process has that pid and this process may signal it. A zombie
    /// — exited, not yet reaped by its parent — still answers this way.
    Alive,
    /// No process has that pid.
    Dead,
    /// A process has that pid but refuses this process's signals: it
    /// runs as another user, so whether it is the one asked about
    /// cannot be told from here.
    Unknown,
}

pub fn liveness(pid: Pid) -> Liveness {
    match signal::kill(raw(pid), None) {
        Ok(()) => Liveness::Alive,
        Err(Errno::ESRCH) => Liveness::Dead,
        Err(_) => Liveness::Unknown,
    }
}

fn raw(pid: Pid) -> nix::unistd::Pid {
    nix::unistd::Pid::from_raw(pid.as_i32())
}
