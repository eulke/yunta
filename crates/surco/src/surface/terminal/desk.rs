//! A plan's desk on a terminal of an exact size. Every row it draws stands inside a margin,
//! so nothing reaches the last column, where a terminal decides on its own whether the next
//! character wraps. It fills the rows it is given, less the one a prompt never draws on.

use super::card::Placed;
use crate::components::{Desk, FootRow, Pane};
use crate::glyphs::{Glyphs, Mark};
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::text::counted;
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
/// The cells between what stands at the two ends of a row, when both stand on it.
const APART: usize = 2;
/// The fewest cells a title keeps when something stands at the far end of its row: under them
/// what stands there goes instead.
const LEAST_TITLE: usize = 20;
/// The rows the pane's title and the blank row under it take.
const PANE_HEAD: usize = 2;

/// A desk drawn, with what a key that scrolls it has to know: the first row of the pane it
/// shows, how many rows its body has, and the highest first row it can show.
pub struct LaidDesk {
    pub placed: Placed,
    pub top: usize,
    pub page: usize,
    pub most: usize,
}

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

    /// The desk drawn in `rows` rows of a terminal, less the row a prompt never draws on. On
    /// a short one the head gives up what was asked, then the gist, then its blank rows.
    pub fn drawn(&self, look: &Look, rows: usize) -> LaidDesk {
        let measures = Measures::of(look.width.cells());
        let glyphs = look.glyphs;
        let most = rows.saturating_sub(1);
        let mut foot = self.foot_lines(glyphs, measures.whole);
        let mut head = self.head_lines(glyphs, measures.whole, most, foot.len());
        if most.saturating_sub(head.len() + foot.len()) < LEAST_TO_READ {
            // The blank row under the head and the blank row over the question go.
            head.pop();
            foot.remove(0);
        }
        let height = most.saturating_sub(head.len() + foot.len());
        let (middle, top, page, pane_most) = self.middle(look, &measures, height);
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
        LaidDesk {
            placed: Placed {
                lines,
                cursor: None,
            },
            top,
            page,
            most: pane_most,
        }
    }

    /// The rows between the head and the foot: the rail beside the pane, or the pane alone.
    fn middle(
        &self,
        look: &Look,
        measures: &Measures,
        height: usize,
    ) -> (Vec<Line>, usize, usize, usize) {
        let glyphs = look.glyphs;
        let has_rail = measures.rail.is_some();
        let (pane, top, page, most) = pane_rows(&self.pane, look, measures.pane, height, has_rail);
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
        (lines, top, page, most)
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

    /// The title row: the parts of the title, and what stands at its far end.
    fn title_row(&self, glyphs: Glyphs, measure: usize) -> Line {
        let edge = self
            .edge
            .as_deref()
            .map(|edge| Line::new().push(Tone::Muted, edge));
        let beside = edge
            .as_ref()
            .map_or(0, |edge| cell_width(&edge.text()) + APART);
        let room = measure.saturating_sub(beside);
        let first = self.title.first().map_or(0, |part| cell_width(part));
        if room < LEAST_TITLE && first > room {
            return title_parts(&self.title, measure, glyphs);
        }
        at_ends(title_parts(&self.title, room, glyphs), edge, measure)
    }

    /// The foot's rows: a blank row, the question, then what answers it, with its keys.
    fn foot_lines(&self, glyphs: Glyphs, measure: usize) -> Vec<Line> {
        let question = Line::new()
            .push(Tone::NeedsYou, glyphs.mark(Mark::NeedsYou).to_string())
            .plain(" ")
            .push(Tone::Strong, self.foot.question.as_str())
            .cut(measure, glyphs);
        let mut lines = vec![Line::new(), question];
        // The keys stand at the far end of the answer's row when both fit, and under it when not.
        let left = self.answers();
        let sep = format!(" {} ", glyphs.sep());
        let keys = self.foot.keys.join(sep.as_str());
        let far = Line::new().push(Tone::Muted, keys.as_str());
        if keys.is_empty() {
            lines.push(left.cut(measure, glyphs));
        } else if cell_width(&left.text()) + APART + cell_width(&keys) <= measure {
            lines.push(at_ends(left, Some(far), measure));
        } else {
            lines.push(left.cut(measure, glyphs));
            lines.extend(far.wrap(measure));
        }
        lines
    }

    /// The answer as it stands: the options by their numbers, or the option about to be
    /// chosen with what it does.
    fn answers(&self) -> Line {
        match &self.foot.row {
            FootRow::Options(names) => {
                names
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
            FootRow::Confirming(option) => Line::new()
                .push(Tone::Strong, option.name.as_str())
                .plain("  ")
                .then(option.text()),
        }
    }
}

/// `left` at the start of `cells` cells and `right` at its far end, with the space between
/// them as wide as the row needs. Without a `right`, `left` stands alone.
fn at_ends(left: Line, right: Option<Line>, cells: usize) -> Line {
    let Some(right) = right else {
        return left;
    };
    let gap = cells.saturating_sub(cell_width(&left.text()) + cell_width(&right.text()));
    left.plain(" ".repeat(gap)).then(right)
}

/// The title's parts in `cells`: as many leading parts as fit stand whole, the first in
/// strong and the rest muted. A first part too wide for the cells is cut.
fn title_parts(parts: &[String], cells: usize, glyphs: Glyphs) -> Line {
    let sep = format!(" {} ", glyphs.sep());
    let mut width = 0;
    let kept = parts
        .iter()
        .enumerate()
        .take_while(|(at, part)| {
            let joint = if *at == 0 { 0 } else { cell_width(&sep) };
            width += joint + cell_width(part);
            width <= cells
        })
        .count();
    match parts.first() {
        None => Line::new(),
        Some(first) if kept == 0 => Line::new()
            .push(Tone::Strong, first.as_str())
            .cut(cells, glyphs),
        Some(_) => {
            parts[..kept]
                .iter()
                .enumerate()
                .fold(Line::new(), |line, (at, part)| match at {
                    0 => line.push(Tone::Strong, part.as_str()),
                    _ => line
                        .push(Tone::Muted, sep.as_str())
                        .push(Tone::Muted, part.as_str()),
                })
        }
    }
}

/// The pane's title row in `cells`: its title in strong, and at its far end where it is among
/// the places a person can stand on. A rail says that itself when there is one.
fn pane_title_row(pane: &Pane, glyphs: Glyphs, cells: usize, has_rail: bool) -> Line {
    let sep = format!(" {} ", glyphs.sep());
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

/// The pane's rows in `height` rows: its title and a blank row, then the lines it is scrolled
/// to, and a row that counts the lines below when they do not all fit. Returns the rows, the
/// first line shown, the body's rows and the last first line a scroll can reach.
fn pane_rows(
    pane: &Pane,
    look: &Look,
    cells: usize,
    height: usize,
    has_rail: bool,
) -> (Vec<Line>, usize, usize, usize) {
    let glyphs = look.glyphs;
    let page = height.saturating_sub(PANE_HEAD);
    let most = pane.lines.len().saturating_sub(page);
    let top = scroll_top(pane, most, page);
    let mut rows = vec![pane_title_row(pane, glyphs, cells, has_rail), Line::new()];
    rows.extend(pane_body(&pane.lines, (top, page, most), cells, glyphs));
    rows.truncate(height);
    rows.resize(height, Line::new());
    (rows, top, page, most)
}

/// The first line the pane shows: the one it was scrolled to, held within its last lines and
/// moved so that the line a pointer stands on is in sight.
fn scroll_top(pane: &Pane, most: usize, page: usize) -> usize {
    let top = pane.top.min(most);
    let Some(row) = pane.keep else {
        return top;
    };
    // Past the last line every row of the body shows a line, so none is kept for a count.
    let shown = if top == most {
        page
    } else {
        page.saturating_sub(1)
    };
    if row < top || row >= top + shown {
        row.min(most)
    } else {
        top
    }
}

/// The body rows of a pane scrolled to `top` (with its `page` rows and `most`), each cut to
/// `cells`. When lines lie below the last one shown, the row that counts them takes its place.
fn pane_body(
    lines: &[Line],
    (top, page, most): (usize, usize, usize),
    cells: usize,
    glyphs: Glyphs,
) -> Vec<Line> {
    if top == most {
        return lines[top..]
            .iter()
            .map(|line| line.cut(cells, glyphs))
            .collect();
    }
    if page == 0 {
        return Vec::new();
    }
    let end = top + page - 1;
    let more = format!(
        "{} {}",
        glyphs.ellipsis(),
        counted(lines.len() - end, "more line")
    );
    let mut body: Vec<Line> = lines[top..end]
        .iter()
        .map(|line| line.cut(cells, glyphs))
        .collect();
    body.push(Line::new().push(Tone::Muted, more).cut(cells, glyphs));
    body
}

mod rail;
use rail::rail_rows;

#[cfg(test)]
mod tests;
