//! A card's options as rows: each option's name in a column as wide as the longest, and
//! beside it what it does and what it costs, wrapped under itself where the rows are
//! there and cut on the option's own row where they are not; a list longer than its
//! rows keeps a window around the option pointed at and counts what it leaves out.

use crate::components::{ChoiceRow, Choices};
use crate::glyphs::Glyphs;
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::text::counted;
use crate::width::{cell_width, id_column, middle_cut};

/// The cells the pointer and the space after it take: a row not pointed at keeps
/// them, so no row moves when the pointer does.
const POINTER: usize = 2;

/// The cells between an option's name and what it says: one would read as a word's
/// space.
const NAME_GAP: usize = 2;

/// The fewest cells what an option says is wrapped in beside its name. Fewer are
/// too few for a sentence, so it goes under the name instead.
const LEAST_TEXT: usize = 12;

/// How what an option says stands where it passes the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::surface::terminal) enum Said {
    /// Wrapped under itself.
    Wrapped,
    /// On the option's row, cut at the line.
    Cut,
}

impl Choices {
    /// The list in `room` cells: its rows, at most `most` of them around the one
    /// pointed at, and how many are left out. A row's text stands where the longest
    /// name leaves it, as `said` says.
    pub(super) fn lines(&self, look: &Look, room: usize, most: usize, said: Said) -> Vec<Line> {
        let glyphs = look.glyphs;
        let name = id_column(self.rows.iter().map(|row| row.name.as_str())) + NAME_GAP;
        let len = self.rows.len();
        let most = most.max(1);
        let (start, end) = match len <= most {
            true => (0, len),
            false => {
                let start = (self.at + 1).saturating_sub(most).min(len - most);
                (start, start + most)
            }
        };
        let mut lines: Vec<Line> = self.rows[start..end]
            .iter()
            .enumerate()
            .flat_map(|(offset, row)| {
                row.lines(start + offset == self.at, name, room, glyphs, said)
            })
            .collect();
        let left = len - (end - start);
        if left > 0 {
            let more = format!("{} {}", glyphs.ellipsis(), counted(left, "more option"));
            lines.push(Line::new().push(Tone::Muted, more));
        }
        lines
    }

    /// The list in at most `budget` lines of `room` cells, as long as a row and what it
    /// leaves out can stand: the window narrows until it fits, since a row may wrap.
    pub(super) fn fitted(&self, look: &Look, room: usize, budget: usize, said: Said) -> Vec<Line> {
        let mut most = self.rows.len();
        let mut lines = self.lines(look, room, most, said);
        while lines.len() > budget && most > 1 {
            most -= 1;
            lines = self.lines(look, room, most, said);
        }
        lines
    }
}

impl ChoiceRow {
    /// This option, pointed at or not, with its name in a column `name` cells wide:
    /// what it does in the plain tone, and what it costs in the muted one, as one
    /// sentence wrapped under itself, or cut on the option's row where `said` says so.
    /// What it does not say it does not say, and a name with nothing beside it ends
    /// where it does, cut at the row's end as every row is.
    pub(in crate::surface::terminal) fn lines(
        &self,
        pointed: bool,
        name: usize,
        room: usize,
        glyphs: Glyphs,
        said: Said,
    ) -> Vec<Line> {
        let width = room.saturating_sub(POINTER);
        let name_tone = match pointed {
            true => Tone::Strong,
            false => Tone::Plain,
        };
        let opening = Line::pointed(pointed, glyphs);
        let fill = opening.fill();
        let text = self.text();
        let beside = width.saturating_sub(name);
        let mut lines: Vec<Line> = Vec::new();
        if text.text().is_empty() || beside < LEAST_TEXT {
            let alone = Line::new().push(name_tone, self.name.as_str());
            lines.push(opening.then(alone.cut(width, glyphs)));
            if !text.text().is_empty() && said == Said::Wrapped {
                let margin = " ".repeat(POINTER);
                lines.extend(text.wrap(width).into_iter().map(|part| part.under(&margin)));
            }
        } else if said == Said::Cut {
            let column = middle_cut(&self.name, name, glyphs);
            lines.push(
                opening
                    .push(name_tone, column)
                    .then(text.cut(beside, glyphs)),
            );
        } else {
            let column = middle_cut(&self.name, name, glyphs);
            let margin = " ".repeat(POINTER + name);
            for (at, part) in text.wrap(beside).into_iter().enumerate() {
                lines.push(match at {
                    0 => opening.clone().push(name_tone, column.clone()).then(part),
                    _ => part.under(&margin),
                });
            }
        }
        // The lines an option wraps to stand on the fill its opening stands on.
        match fill {
            Some(fill) => lines.into_iter().map(|line| line.filled(fill)).collect(),
            None => lines,
        }
    }

    /// What the option says: `said`, then `: ` and `cost` when both are given, the
    /// cost in the muted tone, so a row says what it has and no more.
    pub(in crate::surface::terminal) fn text(&self) -> Line {
        let mut text = Line::new().push(Tone::Plain, self.said.as_deref().unwrap_or_default());
        if self.said.is_some() && self.cost.is_some() {
            text = text.plain(": ");
        }
        text.push(Tone::Muted, self.cost.as_deref().unwrap_or_default())
    }
}

/// `text` in exactly `width` cells when it is wider, cut in the middle so that both
/// of its ends stay, and as it is when it fits.
pub(super) fn cut_to(text: &str, width: usize, glyphs: Glyphs) -> String {
    match cell_width(text) > width {
        true => middle_cut(text, width, glyphs),
        false => text.to_string(),
    }
}
