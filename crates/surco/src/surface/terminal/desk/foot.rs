//! See [`super`]. The foot of a plan's desk: a blank row, the question, then what answers
//! it, with the keys that do something there; where words are typed for an option, the box
//! they are typed in under it, and why the words given were not taken.
//!
//! The keys fold as a legend folds ([`Legend`](crate::components::Legend)), between two keys
//! and never inside one, as a card's foot folds its own: on the answer's row, at its far end,
//! where they all fit beside it on one row; else on rows of their own under it, each as many
//! keys as the row holds whole.

use super::super::legend::separator;
use super::super::{at_ends, APART};
use crate::components::{Desk, FootRow};
use crate::glyphs::Mark;
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::text::counted;
use crate::width::cell_width;

impl Desk {
    /// The foot's rows in `measure` cells of `look`: a blank row, the question, then what
    /// answers it, with its keys beside it or folded under it; where words are typed for an
    /// option, the box they are typed in under it, and why the words given were not taken.
    /// With where the cursor stands among those rows while words are typed.
    pub(super) fn foot_lines(
        &self,
        look: &Look,
        measure: usize,
    ) -> (Vec<Line>, Option<(usize, usize)>) {
        let glyphs = look.glyphs;
        let question = Line::new()
            .push(Tone::NeedsYou, glyphs.mark(Mark::NeedsYou).to_string())
            .plain(" ")
            .push(Tone::Strong, self.foot.question.as_str())
            .cut(measure, glyphs);
        let mut lines = vec![Line::new(), question];
        let answers = self.answers();
        let keys = self.keys(look, measure);
        match keys.as_slice() {
            [] => lines.push(answers.cut(measure, glyphs)),
            [one] if cell_width(&answers.text()) + APART + cell_width(&one.text()) <= measure => {
                lines.push(at_ends(answers, Some(one.clone()), measure));
            }
            _ => {
                lines.push(answers.cut(measure, glyphs));
                lines.extend(keys);
            }
        }
        let FootRow::Saying {
            asks,
            typed,
            refused,
            ..
        } = &self.foot.row
        else {
            return (lines, None);
        };
        let boxed = typed.drawn(measure, glyphs, Some(asks));
        let field_at = lines.len();
        let cursor = boxed.cursor.map(|(row, cell)| (field_at + row, cell));
        lines.extend(boxed.lines);
        lines.extend(refused.as_deref().map(|refused| {
            Line::new()
                .push(
                    Tone::of(Mark::Failed),
                    glyphs.mark(Mark::Failed).to_string(),
                )
                .plain(" ")
                .plain(refused)
                .cut(measure, glyphs)
        }));
        (lines, cursor)
    }

    /// The keys in `measure` cells, folded between two keys as the legend folds them, then
    /// how many options stand on the card past the last number, after the last key where it
    /// fits on that row, else on a row of its own.
    fn keys(&self, look: &Look, measure: usize) -> Vec<Line> {
        let mut keys = self.foot.legend.folded(look, measure);
        let Some(more) = self.more_on_the_card() else {
            return keys;
        };
        let sep = separator(look);
        let beside = keys
            .last()
            .map_or(0, |last| cell_width(&last.text()) + cell_width(&sep));
        match keys.last_mut() {
            Some(last) if beside + cell_width(&more) <= measure => {
                *last = std::mem::take(last)
                    .push(Tone::Faint, sep)
                    .push(Tone::Muted, more);
            }
            _ => keys.push(Line::new().push(Tone::Muted, more)),
        }
        keys
    }

    /// How many options stand on the card past the last number, where any do.
    fn more_on_the_card(&self) -> Option<String> {
        match &self.foot.row {
            FootRow::Options { more, .. } if *more > 0 => {
                Some(format!("{} on the card", counted(*more, "more option")))
            }
            _ => None,
        }
    }

    /// The answer as it stands: the options by their numbers, or the option about to be
    /// chosen, or being said in words, with what it does.
    fn answers(&self) -> Line {
        match &self.foot.row {
            FootRow::Options { named, .. } => {
                named
                    .iter()
                    .enumerate()
                    .fold(Line::new(), |line, (at, name)| {
                        // Options stand three spaces apart, after the first.
                        let gap = if at == 0 { "" } else { "   " };
                        line.plain(gap)
                            .push(Tone::Strong, (at + 1).to_string())
                            .push(Tone::Plain, format!(" {name}"))
                    })
            }
            FootRow::Confirming(option) | FootRow::Saying { chosen: option, .. } => Line::new()
                .push(Tone::Strong, option.name.as_str())
                .plain("  ")
                .then(option.text()),
        }
    }
}

#[cfg(test)]
mod tests;
