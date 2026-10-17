//! What a test's subprocesses answer to.

use tokio_util::sync::CancellationToken;
use yunta_engine::process::Supervision;
use yunta_testkit_core::FixedClock;

/// The owner of everything a test spawns outside a run: a token the
/// test may trip and the fixed clock every test tells the time by.
///
/// A [`Supervision`] borrows its token and its clock, so they need
/// somewhere to live that outlives the call. This is that place — one
/// value a test holds, instead of the pair of locals every test used to
/// declare before it could spawn anything.
#[derive(Default)]
pub struct Owner {
    cancellation: CancellationToken,
    clock: FixedClock,
}

impl Owner {
    pub fn new() -> Self {
        Owner::default()
    }

    /// The token this owner stops its subprocesses with — cloned by a
    /// test that trips it from another task.
    pub fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
    }

    /// The supervision outside any run: this owner's token and clock,
    /// no registry and no env overrides.
    pub fn supervision(&self) -> Supervision<'_> {
        Supervision::outside_any_run(&self.cancellation, &self.clock)
    }
}
