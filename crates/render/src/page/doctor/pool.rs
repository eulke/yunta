//! See [`super`]. What the project's checkouts keep of the disk, as figures and as the words
//! `doctor` and `gc` say them in: the sentence whole, and the clause of the builds against
//! their budget that a glance beside a new run says alone.

use std::fmt;

use surco::figures::Bytes;

/// What the project's checkouts keep of the disk, as their pool measures it:
/// what `doctor` and `gc` say the pool keeps. Not [`super::CheckoutsKeep`], how
/// that stands against the disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolKept {
    pub checkouts: usize,
    /// What their counted builds take together.
    pub builds: u64,
    /// Builds nobody counted: they add nothing to `builds`, and are said apart.
    pub uncounted: usize,
    pub budget: u64,
    /// What the disk has free.
    pub available: u64,
    /// What of it always stays free.
    pub keep_free: u64,
}

impl PoolKept {
    /// What the builds take of their budget, and how many builds nobody
    /// counted where there are any: `builds 0 B of a 98.9 GB budget`.
    pub fn builds(&self) -> String {
        let counted = format!(
            "builds {} of a {} budget",
            Bytes(self.builds),
            Bytes(self.budget)
        );
        match self.uncounted {
            0 => counted,
            uncounted => format!(
                "{counted} and {} not counted",
                surco::text::counted(uncounted, "build")
            ),
        }
    }
}

/// How many checkouts, what their builds take, then the disk's free space
/// against the part kept free: `0 checkouts, builds 0 B of a 98.9 GB budget;
/// 74.2 GB free, 24.7 GB kept free`.
impl fmt::Display for PoolKept {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}, {}; {} free, {} kept free",
            surco::text::counted(self.checkouts, "checkout"),
            self.builds(),
            Bytes(self.available),
            Bytes(self.keep_free)
        )
    }
}
