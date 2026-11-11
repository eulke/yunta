//! See [`crate`]. The state's number for a piece of work it asked for.

/// The state's number for a piece of work it asked for: what the answer comes back
/// under, so an answer to work the state no longer waits on is told apart and
/// dropped. Not a run's id, nor anything Yunta records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ticket(u64);

impl Ticket {
    /// The ticket after this one.
    pub(crate) fn next(self) -> Ticket {
        Ticket(self.0 + 1)
    }

    /// The first ticket a state hands out.
    pub(crate) fn first() -> Ticket {
        Ticket(1)
    }
}
