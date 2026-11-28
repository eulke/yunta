//! A plan's desk on a terminal of an exact size. Every row it draws stands inside a margin,
//! so nothing reaches the last column, where a terminal decides on its own whether the next
//! character wraps. It fills the rows it is given. Its foot (`foot`) asks the question under
//! what is read, its keys folded as a legend folds; words typed for one of its options stand
//! there in place of the options, in the box every line asked of a person is typed in, with
//! the cursor there.
//!
//! Its head's far words, what needs the person and where, give way whole before its title
//! loses a part, as the trail names the node too. Short of rows, the head gives up what was
//! asked, then the plan's gist, then the desk its blank rows: under the head, under the
//! pane's title and over the question. A pane on a screen standing short of rows holds it
//! close ([`Desk::drawn_close`]), every blank row of its own given way at once.

use super::card::Placed;
use super::pane::Spacing;
use super::screen::title_parts;
use super::{at_ends, Terminal, APART};
use crate::components::{Desk, Laid, Pane};
use crate::glyphs::Glyphs;
use crate::line::{Line, Tone};
use crate::look::{Look, Width};
use crate::width::cell_width;

/// The cells left free at each end of a row.
const MARGIN: usize = 1;
/// The cells the rule between the rail and the pane takes, with a space at each side.
const RULE: usize = 3;
/// The fewest columns a terminal has a rail at: under this the pane has too little left.
const WIDE: usize = 90;
/// The rail's share of the columns, in tenths: a third of the screen leaves the pane what code
/// needs.
const RAIL_TENTHS: usize = 3;
/// The rail's fewest cells: under them a task's name no longer stands on a row.
const RAIL_LEAST: usize = 28;
/// The rail's most cells: past them it takes from a pane that could use them.
const RAIL_MOST: usize = 44;
/// The most cells of a line a person reads in one sweep, as a line anywhere else gets.
const PANE_MOST: usize = 120;
/// The rows the rail and the pane get before the head gives up what it can spare: what was
/// asked, then what the plan is about.
const ROOM_TO_READ: usize = 10;
/// The rows they get before the rows left blank go too.
const LEAST_TO_READ: usize = 5;
/// The fewest cells a title keeps when something stands at the far end of its row: under them
/// what stands there goes instead.
const LEAST_TITLE: usize = 20;
/// The rows the pane's title and the blank row under it take.
const PANE_HEAD: usize = 2;

/// The columns of a desk on a terminal of some width, so the head, the pane and the foot
/// cannot disagree about them.
struct Measures {
    /// The rail's cells, where it has one.
    rail: Option<usize>,
    /// The pane's cells.
    pane: usize,
    /// The cells of a row of the head or the foot: the rail and the pane together.
    whole: usize,
}

impl Measures {
    /// The columns on a terminal `columns` wide.
    fn of(columns: usize) -> Self {
        let inner = columns.saturating_sub(2 * MARGIN);
        let rail =
            (columns >= WIDE).then(|| (columns * RAIL_TENTHS / 10).clamp(RAIL_LEAST, RAIL_MOST));
        let beside = rail.map_or(0, |rail| rail + RULE);
        let pane = inner.saturating_sub(beside).min(PANE_MOST);
        Measures {
            rail,
            pane,
            whole: beside + pane,
        }
    }
}

impl Desk {
    /// The cells the pane's lines may take on a terminal as wide as `look` says.
    pub fn pane_cells(look: &Look) -> usize {
        Measures::of(look.width.cells()).pane
    }

    /// The desk drawn whole, as a document holds it: every group of its rail and every line
    /// of its pane in sight.
    pub(crate) fn drawn_whole(&self, look: &Look) -> Vec<Line> {
        self.drawn(look, self.whole_rows(look)).lines
    }

    /// The rows the desk takes with nothing given up: its whole head, the longer of its rail
    /// and its pane, and its foot.
    fn whole_rows(&self, look: &Look) -> usize {
        let measures = Measures::of(look.width.cells());
        let head = 2 + usize::from(self.gist.is_some()) + usize::from(self.asked.is_some());
        let foot = self.foot_lines(look, measures.whole).0.len();
        let pane = PANE_HEAD + pane_lines(&self.pane, look, measures.pane).len();
        let rail = measures
            .rail
            .map_or(0, |cells| rail_length(&self.rail, look, cells));
        head + pane.max(rail).max(ROOM_TO_READ) + foot
    }

    /// The desk drawn in `most` rows, with what a key that scrolls it has to know of its
    /// pane, and where the cursor stands while words are typed at its foot. Short of rows,
    /// the head gives up what was asked, then the gist, then the desk its blank rows.
    pub fn drawn(&self, look: &Look, most: usize) -> Placed {
        self.spaced(look, most, Spacing::Aired)
    }

    /// The desk drawn in `most` rows `look`'s width across, as [`Desk::drawn`] draws it,
    /// standing close, as a screen standing short of rows holds it: its own blank rows given
    /// way, under its head, under its pane's title and over its question.
    pub(super) fn drawn_close(&self, look: &Look, most: usize) -> Placed {
        let across = Look {
            width: Width::exact(look.width.cells()),
            ..*look
        };
        self.spaced(&across, most, Spacing::Close)
    }

    /// The desk drawn in `most` rows, spaced as `spacing` says, or standing close where its
    /// rows leave fewer than [`LEAST_TO_READ`] to read.
    fn spaced(&self, look: &Look, most: usize, spacing: Spacing) -> Placed {
        let measures = Measures::of(look.width.cells());
        let glyphs = look.glyphs;
        let (mut foot, mut cursor) = self.foot_lines(look, measures.whole);
        let mut head = self.head_lines(glyphs, measures.whole, most, foot.len());
        let spacing = match most.saturating_sub(head.len() + foot.len()) < LEAST_TO_READ {
            true => Spacing::Close,
            false => spacing,
        };
        if spacing == Spacing::Close {
            // The blank row under the head and the blank row over the question go.
            head.pop();
            foot.remove(0);
            cursor = cursor.map(|(row, cell)| (row.saturating_sub(1), cell));
        }
        let height = most.saturating_sub(head.len() + foot.len());
        let (middle, laid) = self.middle(look, &measures, (height, spacing));
        let above = head.len() + middle.len();
        let margin = " ".repeat(MARGIN);
        let mut lines: Vec<Line> = head
            .into_iter()
            .chain(middle)
            .chain(foot)
            .map(|line| match line.text().is_empty() {
                true => line,
                false => line.under(&margin),
            })
            .collect();
        lines.truncate(most);
        Placed {
            lines,
            cursor: cursor
                .map(|(row, cell)| (above + row, MARGIN + cell))
                .filter(|(row, _)| *row < most),
            laid: Some(laid),
        }
    }

    /// The rows between the head and the foot, `height` of them spaced as `spacing` says: the
    /// rail beside the pane, or the pane alone.
    fn middle(
        &self,
        look: &Look,
        measures: &Measures,
        (height, spacing): (usize, Spacing),
    ) -> (Vec<Line>, Laid) {
        let glyphs = look.glyphs;
        let has_rail = measures.rail.is_some();
        let at = PaneAt {
            cells: measures.pane,
            height,
            has_rail,
            spacing,
        };
        let (pane, laid) = pane_rows(&self.pane, look, at);
        let lines = match measures.rail {
            Some(cells) => {
                let rule = format!(" {} ", glyphs.gutter());
                let rail = rail_rows(&self.rail, look, cells, height);
                rail.into_iter()
                    .zip(pane)
                    .map(|(rail, row)| {
                        Line::new()
                            .then(rail)
                            .push(Tone::Faint, rule.as_str())
                            .then(row)
                    })
                    .collect()
            }
            None => pane,
        };
        (lines, laid)
    }

    /// The head's rows: the title, what the plan is about and what was asked while the pane
    /// has rows to spare for them, and the blank row that ends the head.
    fn head_lines(&self, glyphs: Glyphs, measure: usize, most: usize, foot: usize) -> Vec<Line> {
        let mut gist = self
            .gist
            .as_deref()
            .map(|gist| Line::new().push(Tone::Plain, gist).cut(measure, glyphs));
        let mut asked = self.asked.as_deref().map(|asked| {
            let line = Line::new().push(Tone::Muted, "you asked").plain("  ");
            line.push(Tone::Plain, asked).cut(measure, glyphs)
        });
        // The title and the blank row under the head count whatever stays.
        let left = |gist: &Option<Line>, asked: &Option<Line>| {
            let shown = 2 + foot + usize::from(gist.is_some()) + usize::from(asked.is_some());
            most.saturating_sub(shown)
        };
        if left(&gist, &asked) < ROOM_TO_READ {
            asked = None;
        }
        if left(&gist, &asked) < ROOM_TO_READ {
            gist = None;
        }
        let mut head = vec![self.title_row(glyphs, measure)];
        head.extend(gist);
        head.extend(asked);
        head.push(Line::new());
        head
    }

    /// The title row: the parts of the title, and what stands at its far end where the whole
    /// title stands beside it; where it does not, the far end gives way whole before the title
    /// gives up a part.
    fn title_row(&self, glyphs: Glyphs, measure: usize) -> Line {
        let title = title_parts(&self.title, measure, glyphs, Tone::Strong);
        let Some(edge) = self.edge.as_deref() else {
            return title;
        };
        let whole = self.title.join(&glyphs.spaced_sep());
        match cell_width(&whole) + APART + cell_width(edge) <= measure {
            true => at_ends(title, Some(Line::new().push(Tone::Muted, edge)), measure),
            false => title,
        }
    }
}

/// The pane's title row in `cells`: its title in strong, and at its far end where it is among
/// the places a person can stand on. A rail says that itself when there is one.
fn pane_title_row(pane: &Pane, glyphs: Glyphs, cells: usize, has_rail: bool) -> Line {
    let sep = glyphs.spaced_sep();
    let place = if has_rail {
        None
    } else {
        pane.place.as_deref()
    };
    let parts: Vec<&str> = place.into_iter().chain(pane.edge.as_deref()).collect();
    let title = Line::new().push(Tone::Strong, pane.title.as_str());
    if parts.is_empty() {
        return title.cut(cells, glyphs);
    }
    let end = Line::new().push(Tone::Muted, parts.join(sep.as_str()));
    let room = cells.saturating_sub(cell_width(&end.text()) + APART);
    // Both stand whole when they fit; the end goes only when the title would be left too few.
    if room >= LEAST_TITLE || cell_width(&title.text()) <= room {
        at_ends(title.cut(room, glyphs), Some(end), cells)
    } else {
        title.cut(cells, glyphs)
    }
}

/// Where a desk's pane stands: its cells across, its rows, whether a rail stands beside it,
/// which says its place among the places for it, and how it is spaced.
struct PaneAt {
    cells: usize,
    height: usize,
    has_rail: bool,
    spacing: Spacing,
}

/// The pane's rows where `at` says: its title, a blank row where it is aired, then the lines
/// it is scrolled to, and a row that counts the lines below when they do not all fit. Returns
/// the rows and what was laid out of the pane.
fn pane_rows(pane: &Pane, look: &Look, at: PaneAt) -> (Vec<Line>, Laid) {
    let PaneAt {
        cells,
        height,
        has_rail,
        spacing,
    } = at;
    let mut rows = vec![pane_title_row(pane, look.glyphs, cells, has_rail)];
    if spacing == Spacing::Aired {
        rows.push(Line::new());
    }
    let page = height.saturating_sub(rows.len());
    let (body, laid) = Terminal::on(measure(look, cells)).scrolled(&pane.doc, pane.sight, page);
    rows.extend(body);
    rows.truncate(height);
    rows.resize(height, Line::new());
    (rows, laid)
}

/// What the pane holds, laid out at its measure of `cells`.
fn pane_lines(pane: &Pane, look: &Look, cells: usize) -> Vec<Line> {
    Terminal::on(measure(look, cells)).lines(&pane.doc)
}

/// `look` at the pane's measure of `cells`.
fn measure(look: &Look, cells: usize) -> Look {
    Look {
        width: Width::row(cells, None),
        ..*look
    }
}

mod foot;
mod rail;
use rail::{rail_length, rail_rows};

#[cfg(test)]
pub(super) mod tests;
