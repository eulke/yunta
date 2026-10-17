//! The process a session keeps running when its `kill` is refused.
//!
//! A session the engine cannot stop is one whose process outlives the stop,
//! and the engine must keep its group in the process registry for as long as
//! that process runs. The mock has no process of its own, so a session whose
//! script refuses `kill` gets a stand-in: a process in a group of its own,
//! which outlives the engine that started it, as a real CLI's process does.

use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};

use yunta_core::process::signal::{signal_group, Signal};
use yunta_core::Pid;

/// How long a stand-in that nothing stops runs on. A run parks and is
/// cancelled well inside this bound; a test that never stops its stand-in
/// leaves no process behind for longer than this.
const LIFETIME_SECS: &str = "60";

/// A live stand-in, known by the process group it leads.
pub(super) struct StandIn {
    group: Pid,
}

impl StandIn {
    /// Starts a stand-in. `None` when the system will not start one: the
    /// session then names no group, and its refused `kill` is still refused.
    pub(super) fn start() -> Option<StandIn> {
        let child = Command::new("sleep")
            .arg(LIFETIME_SECS)
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let group = Pid::try_from(child.id()).ok()?;
        reap(child);
        Some(StandIn { group })
    }

    /// The group the stand-in leads, which the process registry names.
    pub(super) fn group(&self) -> Pid {
        self.group
    }

    /// Kills the stand-in's whole group.
    pub(super) fn end(&self) {
        // best-effort: a group that already ended has nothing left to kill
        signal_group(self.group, Signal::SIGKILL).ok();
    }
}

/// Collects the stand-in's exit in the background, so an ended stand-in is
/// not a zombie that still answers as a live group.
fn reap(mut child: Child) {
    std::thread::spawn(move || {
        // best-effort: nothing waits on the status; only the exit matters
        child.wait().ok();
    });
}
