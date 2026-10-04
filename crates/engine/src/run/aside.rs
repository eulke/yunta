//! The lineage's measurement, taken beside the run's first steps.
//!
//! A run born in a checkout holding exactly the commit it opened on
//! measures its suite in another checkout of that commit — one of the
//! run's pool, with no branch — while the steps that do not read the
//! measurement go on. A step that reads it waits for it. When the
//! invocation ends before the suite has answered, the measurement stops
//! and records nothing; the checkout keeps what the suite built, so the
//! next wake measures again on a warm build.

use std::future::Future;
use std::pin::Pin;

use tokio_util::sync::CancellationToken;

use super::{RunCtx, RunError};

/// The measurement under way beside the run's steps, if one is.
pub(super) struct Aside<'a> {
    stop: CancellationToken,
    under_way: Option<Measurement<'a>>,
}

/// A measurement under way.
type Measurement<'a> = Pin<Box<dyn Future<Output = Result<(), RunError>> + 'a>>;

impl<'a> Aside<'a> {
    /// Nothing under way, and stopped by whatever stops the run.
    pub(super) fn new(run: &CancellationToken) -> Self {
        Aside {
            stop: run.child_token(),
            under_way: None,
        }
    }

    /// Starts measuring `suite`, unless a measurement is under way.
    pub(super) fn start(&mut self, ctx: &'a RunCtx<'_>, suite: &str) {
        if self.under_way.is_none() {
            let stop = self.stop.clone();
            self.under_way = Some(Box::pin(measure_aside(ctx, suite.to_string(), stop)));
        }
    }

    /// Waits for the measurement under way: `false` when none is.
    pub(super) async fn finish(&mut self) -> Result<bool, RunError> {
        match self.under_way.take() {
            Some(measurement) => measurement.await.map(|()| true),
            None => Ok(false),
        }
    }

    /// `step`, with the measurement under way going on beside it.
    pub(super) async fn beside<T>(
        &mut self,
        step: impl Future<Output = Result<T, RunError>>,
    ) -> Result<T, RunError> {
        let Some(measurement) = self.under_way.as_mut() else {
            return step.await;
        };
        tokio::pin!(step);
        tokio::select! {
            biased;
            stepped = &mut step => stepped,
            measured = measurement => {
                self.under_way = None;
                // The step goes on to its own end whatever the measurement
                // came to: dropping it would cut its sessions off with no
                // close on the log. A failed measurement says so after.
                let stepped = step.await;
                measured.and(stepped)
            }
        }
    }

    /// Stops the measurement under way and waits for it to let go of what
    /// it holds. It records nothing, and the next wake measures again.
    pub(super) async fn stop(&mut self) {
        if let Some(measurement) = self.under_way.take() {
            self.stop.cancel();
            if let Err(error) = measurement.await {
                tracing::debug!(%error, "the measurement stopped short");
            }
        }
    }
}

/// Measures `suite` in a checkout of the run's pool at the commit the run
/// opened on, held while the suite runs. `stop` ends it early, recording
/// nothing.
async fn measure_aside(
    ctx: &RunCtx<'_>,
    suite: String,
    stop: CancellationToken,
) -> Result<(), RunError> {
    let supervision = ctx.supervision(&stop);
    let base = &ctx.manifest.base_commit;
    let (checkout, _held) = match ctx.pool.open_detached(base, supervision).await {
        // Stopped while its checkout was being made, it measured nothing,
        // like a suite stopped while it ran.
        Err(stopped) if stopped.cancelled() => return Ok(()),
        opened => opened?,
    };
    let measured = super::baseline::measure_in(ctx, suite, &checkout, supervision).await;
    if let Err(error) = crate::worktree::put_back(&checkout, supervision).await {
        tracing::debug!(%error, "the checkout the suite was measured in stays out of the pool");
    }
    measured
}
