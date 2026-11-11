//! See [`crate`]. What lasting work hands back while it lasts.

use yunta_api::{FollowedRun, RunNews};

/// What lasting work hands back while it lasts, before its outcome: not an answer,
/// which ends the work, but what it found on the way.
#[derive(Debug, Clone)]
pub enum News {
    /// The run a following is on as it starts: its log read whole.
    Following(Box<FollowedRun>),
    /// What a look at a followed run found.
    Run(RunNews),
}
