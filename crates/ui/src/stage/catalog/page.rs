//! See [`super`]. What stands in the catalog's place, and how it is in sight. A page (a
//! workflow's check, the cases, a pack's audit, a fetched pack's inventory, what writing,
//! installing or updating one came to) scrolls by the keys that scroll, and starts at its
//! first line when another stands; a list led by the pointer, or a question, keeps the
//! row pointed at in sight.

use std::mem::{discriminant, Discriminant};

use surco::components::{Laid, Legend, Sight};

use super::packs::PacksStanding;
use super::{CatalogStage, Shown};

/// What stands in the catalog's place, by its kind and, for the packs, by theirs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Standing {
    shown: Discriminant<Shown>,
    packs: Option<PacksStanding>,
}

impl CatalogStage {
    /// What stands in the catalog's place now.
    pub(super) fn standing(&self) -> Standing {
        let packs = match &self.shown {
            Shown::Packs(packs) => Some(packs.standing()),
            _ => None,
        };
        Standing {
            shown: discriminant(&self.shown),
            packs,
        }
    }

    /// Whether a page stands in the catalog's place, which scrolls.
    pub(super) fn shows_a_page(&self) -> bool {
        match &self.shown {
            Shown::Checked { .. } | Shown::Cases(_) => true,
            Shown::Packs(packs) => packs.shows_a_page(),
            Shown::Catalog | Shown::Checking { .. } | Shown::New(_) => false,
        }
    }

    /// Back at the first line where what stands is no longer what stood, `before`.
    pub(super) fn rewound_unless(&mut self, before: Standing) {
        if self.standing() != before {
            self.scroll.rewound();
        }
    }

    /// What of the catalog's region a person asked to see: a page from the line it was
    /// scrolled to, anything else with the row pointed at in sight.
    pub(super) fn sight(&self) -> Sight {
        match self.shows_a_page() {
            true => self.scroll.sight(),
            false => Sight::following_pointer(),
        }
    }

    /// `legend`, led by the keys that scroll where a page stands taller than what shows
    /// of it.
    pub(super) fn scrolled(&self, legend: Legend) -> Legend {
        match self.shows_a_page() && self.scroll.scrolls() {
            true => yunta_render::screen::scrolling(legend),
            false => legend,
        }
    }

    /// What a surface laid out of the catalog's region, which the keys that scroll move
    /// from.
    pub(crate) fn laid(&mut self, laid: Laid) {
        self.scroll.laid(laid);
    }
}
