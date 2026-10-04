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
