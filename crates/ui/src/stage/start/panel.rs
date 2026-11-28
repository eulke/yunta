//! See [`super`]. The panel beside a new run's rows, at the right, as wide as its words keep
//! ([`WhatItTakes::CELLS`]): what the chosen workflow usually takes, from the past runs the
//! catalog read with it, and what this machine answers for, from what `doctor` found as the new
//! run opened; each said nothing of until it is read. What of the machine stands in the way of
//! the chosen workflow's run is what the rows hold in the panel's stead where it gives way.

use surco::components::{Measure, Region, Side, SideAt};
use surco::look::Look;
use yunta_render::page::{Page, WhatItTakes};

use super::StartStage;

impl StartStage {
    /// What the chosen workflow usually takes and what this machine answers for, beside the
    /// rows at the right in the panel's cells, and what of it stands in the way of the chosen
    /// workflow's run, which the rows hold where the panel gives way.
    pub(super) fn panel(&self, look: &Look) -> Side {
        let chosen = self.chosen_readable();
        let takes = WhatItTakes {
            past: chosen.and_then(|(_, readable)| readable.past.as_deref()),
            machine: self.machine.as_deref(),
        };
        let instead = chosen
            .map(|(name, _)| takes.in_the_way(name))
            .unwrap_or_default();
        Side {
            region: Region {
                measure: Some(Measure::of(WhatItTakes::CELLS)),
                ..Region::new(takes.document(look))
            },
            at: SideAt::Right,
            focused: false,
            instead,
        }
    }
}
