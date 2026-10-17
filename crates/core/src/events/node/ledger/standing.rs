//! Whether some node has an attempt open, and whether every open attempt
//! waits for its service: counts the ledger keeps as each record changes,
//! so a reader that asks on every event reads no record.

use super::{NodeLedger, NodeRecord};

impl NodeLedger {
    /// Whether some node has an attempt open.
    pub fn attempting(&self) -> bool {
        self.tally.open > 0
    }

    /// Whether some node lost its service and every open attempt belongs to
    /// a node that did: nothing is at work while the sessions wait.
    pub fn attempts_offline(&self) -> bool {
        self.tally.offline > 0 && self.tally.open_offline == self.tally.open
    }

    /// Moves the counts from what one record was before an event to what
    /// it is after it.
    pub(super) fn track(&mut self, before: Standing, after: Standing) {
        self.tally.add(after);
        self.tally.take(before);
    }
}

/// How many records stand each way.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Tally {
    open: usize,
    offline: usize,
    open_offline: usize,
}

impl Tally {
    fn add(&mut self, standing: Standing) {
        self.open += usize::from(standing.open);
        self.offline += usize::from(standing.offline);
        self.open_offline += usize::from(standing.open && standing.offline);
    }

    /// Takes out a standing this tally counted: never more than it holds.
    fn take(&mut self, standing: Standing) {
        self.open -= usize::from(standing.open);
        self.offline -= usize::from(standing.offline);
        self.open_offline -= usize::from(standing.open && standing.offline);
    }
}

/// Whether a record's attempt is open and whether its session is without
/// its service: what the tally counts of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Standing {
    open: bool,
    offline: bool,
}

impl NodeRecord {
    /// What the tally counts of this record.
    pub(super) fn standing(&self) -> Standing {
        Standing {
            open: self.open_since.is_some(),
            offline: self.unreachable,
        }
    }
}
