//! See [`crate`]. What lasting work hands back while it lasts.

use yunta_api::{FollowedRun, RunNews};
use yunta_render::page::RanCase;

/// What lasting work hands back while it lasts, before its outcome: not an answer,
/// which ends the work, but what it found on the way.
#[derive(Debug, Clone)]
pub enum News {
    /// The run a following is on as it starts: its log read whole.
    Following(Box<FollowedRun>),
    /// What a look at a followed run found.
    Run(RunNews),
    /// The name of every case about to run, in the order they run, before the
    /// first one does.
    CasesNamed(Vec<String>),
    /// A case that ended, and how.
    Case(RanCase),
}
