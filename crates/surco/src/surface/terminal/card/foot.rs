//! See [`super`]. The keys a card says as its foot, where it says its own, as a card a
//! terminal holds by itself does: under a blank row where everything stands, folded as a
//! legend folds, between two keys and never inside one, each key strong and what it does
//! muted, as a desk's foot folds its own. A card whose keys stand in a screen's legend draws
//! none, and short of rows they give way.

use super::Keeping;
use crate::components::Card;
use crate::line::Line;
use crate::look::Look;

impl Card {
    /// The keys the card answers to in `room` cells, under a blank row where everything
    /// stands, folded between two keys: none where they stand in a screen's legend, or where
    /// they give way.
    pub(super) fn foot(&self, look: &Look, room: usize, keeping: Keeping) -> Vec<Line> {
        let gave_way = matches!(keeping, Keeping::TheQuestion | Keeping::TheRoundAlone);
        if self.legend.is_empty() || gave_way {
            return Vec::new();
        }
        (keeping == Keeping::Everything)
            .then(Line::new)
            .into_iter()
            .chain(self.legend.folded(look, room))
            .collect()
    }
}

#[cfg(test)]
mod tests;
