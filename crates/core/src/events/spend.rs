//! What the work of a run or a node has spent: what has settled, and what
//! open usage has reported that nothing has settled.

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
