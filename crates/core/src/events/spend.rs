//! What the work of a run or a node has spent: what has settled, and what
//! open usage has reported that nothing has settled.
//!
//! Usage a session reported is spent whether or not a terminal has written
//! it down, so the spend is one figure, [`Spend::so_far`], the two together:
//! what the frame, `status`, a node's row and `stats` show, what the run's
//! token budget and each session's share of it are measured against, and
//! what a closing run records. It is folded from the log, never kept as a
//! stored total that could drift from the events. A closed run spends what
//! its `run_finished` recorded, with nothing left in flight.

use crate::events::TokenUsage;

/// `settled`: closed work and settled asking rounds. `in_flight`: open usage nothing has settled.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Spend {
    pub settled: TokenUsage,
    pub in_flight: TokenUsage,
}

impl Spend {
    /// The figure every surface shows.
    pub fn so_far(&self) -> TokenUsage {
        self.settled + self.in_flight
    }
}

impl std::ops::Add for Spend {
    type Output = Spend;

    fn add(self, rhs: Spend) -> Spend {
        Spend {
            settled: self.settled + rhs.settled,
            in_flight: self.in_flight + rhs.in_flight,
        }
    }
}

impl std::iter::Sum for Spend {
    fn sum<I: Iterator<Item = Spend>>(iter: I) -> Spend {
        iter.fold(Spend::default(), |total, spend| total + spend)
    }
}
