//! Closing a governed command's process group: killing it until no member
//! can execute, and reading its pipes to their end once it is closed.

use std::time::Duration;

use yunta_core::process::group::{force_kill_group, GroupError};
use yunta_core::process::signal::{signal_group, Signal};
use yunta_core::Pid;

use super::pipes::PipeFailure;
use super::{abort_pipes, wait_for_deadline, Drain, Leader, PipeKind, Pipes, Supervision, Waited};

/// Closes the group: kills it until no member can execute, and when that
/// kill is refused, makes one direct attempt at the group and at the leader.
/// While the group is closed, a cancellation or a deadline that fires moves
/// a wait that stays in its `Exited` state on.
pub(super) async fn close_group(
    child: &mut Leader,
    pgid: Pid,
    supervision: Supervision<'_>,
    timeout_at: Option<tokio::time::Instant>,
    waited: &mut Waited,
) -> Result<(), GroupError> {
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
            _ = supervision.cancel.cancelled(), if *waited == Waited::Exited => {
                *waited = Waited::Cancelled;
            }
            _ = wait_for_deadline(timeout_at), if *waited == Waited::Exited => {
                *waited = Waited::TimedOut;
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
    cleanup_result
}

/// Reads the pipes to their end once the group is closed. A pipe held
/// when the drain bound passes belongs to a process outside the group, and
/// reading stops there; a failing pipe, or a group that could not be closed,
/// stops it at once. Returns the first failure the pipes left, and whether the
/// output may be short.
pub(super) async fn drain_pipes(
    pipes: &mut Pipes,
    pgid: Pid,
    supervision: Supervision<'_>,
    timeout_at: Option<tokio::time::Instant>,
    waited: &mut Waited,
    cleanup: &Result<(), GroupError>,
    described: &str,
) -> (Option<PipeFailure>, Drain) {
    // Nothing in the group can write any more: what is still buffered
    // reads at once, and a pipe still open after a moment is held by a
    // process that left the group — a detached daemon — which this
    // command does not wait for. The moment is long enough for a process
    // starved of CPU to read what is buffered: it is paid only when a
    // pipe is held, since without a holder the end of the pipe comes first.
    let drained_by = tokio::time::Instant::now() + Duration::from_secs(2);
    let mut pipe_failure = None;
    let mut abort = cleanup.is_err();
    let mut drain = Drain::Complete;
    while !pipes.is_empty() && !abort {
        tokio::select! {
            biased;
            joined = pipes.join_next() => match joined {
                Some(Ok((stream, Err(error)))) => {
                    pipe_failure.get_or_insert(PipeFailure::Io(stream, error));
                    abort = true;
                }
                Some(Err(error)) => {
                    pipe_failure.get_or_insert(PipeFailure::Join(PipeKind::Stdout, error));
                    abort = true;
                }
                _ => {}
            },
            _ = supervision.cancel.cancelled(), if *waited == Waited::Exited => {
                *waited = Waited::Cancelled;
                abort = true;
            }
            _ = wait_for_deadline(timeout_at), if *waited == Waited::Exited => {
                *waited = Waited::TimedOut;
                abort = true;
            }
            _ = wait_for_deadline(Some(drained_by)) => {
                tracing::debug!(command = %described, "a process outside the command's group still holds its output; reading stops here");
                abort = true;
                drain = Drain::Truncated;
            }
        }
    }
    if abort {
        abort_pipes(pipes, pgid, cleanup, &mut pipe_failure).await;
    }
    (pipe_failure, drain)
}
