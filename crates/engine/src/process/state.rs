use std::time::Duration;

use yunta_core::Pid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Waited {
    Exited,
    TimedOut,
    Cancelled,
    Failed,
}

pub(super) async fn wait_for_deadline(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

pub(super) fn child_has_exited(pgid: Pid) -> std::io::Result<bool> {
    yunta_core::process::group::child_has_exited_unreaped(pgid)
}

pub(super) fn observation_interval() -> tokio::time::Interval {
    let mut interval = tokio::time::interval(Duration::from_millis(10));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    interval
}
