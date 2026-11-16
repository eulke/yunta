//! See [`crate`]. What lasting work hands back while it lasts.

use yunta_api::FollowNews;
use yunta_render::page::RanCase;

/// What lasting work hands back while it lasts, before its outcome: not an answer,
/// which ends the work, but what it found on the way.
#[derive(Debug, Clone)]
pub enum News {
    /// What a following handed: the run read whole as it starts, then what each look at
    /// it found.
    Following(FollowNews),
    /// The name of every case about to run, in the order they run, before the
    /// first one does.
    CasesNamed(Vec<String>),
    /// A case that ended, and how.
    Case(RanCase),
}
