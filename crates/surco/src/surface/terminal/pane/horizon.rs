//! See [`super`]. A region whose document ends with a horizon, and holds neither a prompt nor
//! a feed: the field in perspective under what a screen says, which says nothing itself.
//!
//! Its blocks are laid out first, as a region with no horizon lays them out, a blank row
//! between two. The horizon takes the rows they leave, a blank row parting it from them,
//! drawn in every one of them. Given fewer than the least it stands in, no braille to draw it
//! in, or a pane that stands close, which keeps no blank row, it gives way whole: it says
//! nothing a person loses, so the blocks stand where they stood and the rows it would take
//! stay blank. Short of rows for the blocks themselves, their blank rows give way before
//! their rows do, then what stands under the window is counted, as a pane read from its top
//! counts it.

use super::{closed_up, Paned, Spacing, Terminal};
use crate::components::{Horizon, Sight};
use crate::doc::Block;
use crate::line::Line;

impl Terminal {
    /// `above`, the blocks of a region over the horizon `horizon` its document ends with,
    /// in a pane `rows` high spaced by `spacing`, the first of `abridged` that fits standing
    /// for them where they do not, scrolled from `sight` where they still do not: the blocks,
    /// then the horizon in every row they leave after a blank row, where it stands.
    pub(super) fn over_a_horizon(
        &self,
        above: &[Block],
        horizon: &Horizon,
        abridged: &[Vec<Block>],
        sight: Sight,
        rows: usize,
        spacing: Spacing,
    ) -> Paned {
        let fitted = self.fitted(above, abridged, rows, spacing);
        let mut lines = match fitted.len() > rows {
            true => closed_up(fitted, Spacing::Close),
            false => fitted,
        };
        let parted = usize::from(!lines.is_empty());
        let left = rows.saturating_sub(lines.len() + parted);
        let field = match spacing {
            Spacing::Aired => self.horizon(horizon, left),
            Spacing::Close => Vec::new(),
        };
        if !field.is_empty() {
            lines.extend(std::iter::repeat_with(Line::new).take(parted));
            lines.extend(field);
        }
        let (lines, laid) = self.scrolled_lines(&lines, sight, rows);
        Paned {
            lines,
            cursor: None,
            laid,
        }
    }
}

#[cfg(test)]
mod tests;
