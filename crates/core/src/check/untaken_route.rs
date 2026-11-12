//! See [`super`]. A gate's route no answer follows.

use crate::OptionId;

/// An internal gate's `on:` route that no answer ever follows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UntakenRoute {
    /// `options:` does not declare this option, so no one can choose it.
    Undeclared(OptionId),
    /// `abort` pauses the run whatever `on:` says.
    Abort,
}
