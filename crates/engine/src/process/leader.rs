//! The process a governed command started, owned until it is reaped.

use std::process::ExitStatus;

use tokio::process::Child;
use yunta_core::process::group::force_kill_group;
use yunta_core::process::signal::{signal_group, Signal};
use yunta_core::Pid;

/// The leader of a governed command's process group, held unreaped so
/// its pid keeps naming the group. A caller that stops waiting drops the
/// command mid-flight, and with it every step that would have
/// closed the group, while the child's own kill-on-drop reaches only the
/// leader. So dropping this before the leader is reaped closes the whole
/// group itself.
pub(super) struct Leader {
    pgid: Pid,
    /// `None` once reaped: from then on its pid may name another
    /// process, so nothing here signals the group again.
    child: Option<Child>,
}

impl Leader {
    pub(super) fn new(child: Child, pgid: Pid) -> Self {
        Self {
            pgid,
            child: Some(child),
        }
    }

    pub(super) fn start_kill(&mut self) -> std::io::Result<()> {
        self.child.as_mut().map_or(Ok(()), Child::start_kill)
    }

    /// Reaps the leader.
    pub(super) async fn wait(&mut self) -> std::io::Result<ExitStatus> {
        let Some(child) = self.child.as_mut() else {
            return Err(std::io::Error::other(
                "the command's leader was already reaped",
            ));
        };
        let status = child.wait().await;
        self.child = None;
        status
    }
}

impl Drop for Leader {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        let pgid = self.pgid;
        // At once, and with no runtime needed: this reaches every member
        // but one caught in the middle of an `exec`, which a group signal
        // can pass over.
        if let Err(error) = signal_group(pgid, Signal::SIGKILL) {
            tracing::warn!(pgid = %pgid, error = %error, "could not kill the process group of a command dropped before it ended");
        }
        // Then the group is closed the way a finished command's is, and
        // only after that is the leader reaped and its pid let go.
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                if let Err(error) = force_kill_group(pgid).await {
                    tracing::warn!(pgid = %pgid, error = %error, "could not close the process group of a command dropped before it ended");
                }
                if let Err(error) = child.wait().await {
                    tracing::warn!(pgid = %pgid, error = %error, "could not reap the leader of a command dropped before it ended");
                }
            });
        }
    }
}
