//! A decision held in place at a terminal: its question, what a reader needs to
//! answer, and either every option with one pointed at or the option chosen with
//! the words it is said with. Every row opens on the decision's edge and is
//! tinted, and the row pointed at is set apart where a layer draws fills.

use crate::components::{Card, CardBody, ChoiceRow, Choices, TypedRow};
use crate::glyphs::{Glyphs, Mark};
use crate::line::{Fill, Line, Tone};
use crate::look::Look;
use crate::text::counted;
use crate::width::{cell_width, id_column, middle_cut, INDENT};

/// A prompt's lines, and where the cursor stands when a field is open: the row,
/// counted from its first line, and the cell.
pub struct Placed {
    pub lines: Vec<Line>,
    pub cursor: Option<(usize, usize)>,
}

/// The cells the edge and the space after it take at the start of a row.
const EDGE: usize = 2;

/// The cells the pointer and the space after it take: a row not pointed at keeps
/// them, so no row moves when the pointer does.
const POINTER: usize = 2;

/// The cells between an option's name and what it says: one would read as a word's
/// space.
const NAME_GAP: usize = 2;

/// The fewest cells what an option says is wrapped in beside its name. Fewer are
/// too few for a sentence, so it goes under the name instead.
const LEAST_TEXT: usize = 12;

/// The cells between the question and the card's far edge on one row.
const HEAD_GAP: usize = 2;

/// The cell the cursor needs past the last character of a field.
const CURSOR_CELL: usize = 1;

/// The cells the mark of a line that needs a person, or a refusal, and the space
/// after it take: the rest of such a line wraps under them.
const MARKED: usize = 2;

/// The cells the word `then` and the two spaces after it take: the rest of a route
/// wraps under them.
const THEN_MARGIN: usize = 6;

/// How much of a card a terminal has room for, what gives way first to what stands
/// to the last.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Keeping {
    /// All of it.
    Everything,
    /// The question, the options and the keys: what stands under the question, the
    /// route, and a far edge that takes a row of its own give way.
    WhatAnswers,
    /// The question and the options: the keys give way too.
    TheQuestion,
}

impl Card {
    /// The card as a prompt draws it on a terminal of `rows` rows, with where the
    /// cursor stands when a field is open. A short terminal keeps the question and
    /// the row pointed at: the list gives way first, counting what it leaves out,
    /// then what stands under the question and the route, and then the keys. A
    /// terminal too short for a question and one row is too short to be asked on.
    pub fn drawn(&self, look: &Look, rows: usize) -> Placed {
        let most = rows.saturating_sub(1);
        let whole = self.laid_out(look, most, Keeping::Everything);
        let placed = [Keeping::WhatAnswers, Keeping::TheQuestion]
            .into_iter()
            .fold(whole, |placed, keeping| match placed.lines.len() <= most {
                true => placed,
                false => self.laid_out(look, most, keeping),
            });
        Placed {
            lines: placed
                .lines
                .into_iter()
                .map(|row| on_the_edge(row, look.glyphs))
                .collect(),
            cursor: placed.cursor,
        }
    }

    /// The cells a field on a card may fill with `prompt` before it: what the card's
    /// edge, the prompt and the cell the cursor needs past the last character leave.
    pub fn typed_room(look: &Look, prompt: &str) -> usize {
        look.width
            .cells()
            .saturating_sub(EDGE + cell_width(prompt) + CURSOR_CELL)
    }

    /// The card's rows, before their edge, in at most `most` rows where the list can
    /// give way that far, holding what `keeping` says.
    fn laid_out(&self, look: &Look, most: usize, keeping: Keeping) -> Placed {
        let room = look.width.cells().saturating_sub(EDGE);
        let mut head = self.head(look, room, keeping);
        if keeping == Keeping::Everything {
            for about in self.about.iter().filter(|about| !about.is_empty()) {
                head.extend(Line::new().plain(about.as_str()).wrap(room));
            }
        }
        head.push(Line::new());
        let separator = format!(" {} ", look.glyphs.sep());
        let keys = self.keys.join(separator.as_str());
        let foot: Vec<Line> = match keys.is_empty() || keeping == Keeping::TheQuestion {
            true => Vec::new(),
            false => std::iter::once(Line::new())
                .chain(Line::new().push(Tone::Muted, keys).wrap(room))
                .collect(),
        };
        let (middle, cursor) = match &self.body {
            CardBody::Choosing { choices, then } => {
                let route = then
                    .as_deref()
                    .filter(|then| keeping == Keeping::Everything && !then.is_empty())
                    .map(|then| then_lines(then, room));
                let route_rows = route.as_ref().map_or(0, |route| route.len() + 1);
                let left = most.saturating_sub(head.len() + foot.len() + route_rows);
                let mut middle = choices.fitted(look, room, left);
                if let Some(route) = route {
                    middle.push(Line::new());
                    middle.extend(route);
                }
                (middle, None)
            }
            CardBody::Saying {
                chosen,
                asks,
                typed,
                refused,
            } => saying(chosen, asks, typed, refused.as_deref(), look, room),
            CardBody::Round(rows) => {
                let left = most.saturating_sub(head.len() + foot.len());
                round::lines(rows, look, room, left)
            }
        };
        let cursor = cursor.map(|(row, cell)| (head.len() + row, EDGE + cell));
        let lines = head.into_iter().chain(middle).chain(foot).collect();
        Placed { lines, cursor }
    }

    /// The question under the mark of what needs a person, wrapped under itself,
    /// with the card's far edge on its first row when both fit and, when `keeping`
    /// holds everything, on a row of its own, cut to the line, when they do not.
    fn head(&self, look: &Look, room: usize, keeping: Keeping) -> Vec<Line> {
        let question = Line::new().push(Tone::Strong, self.question.as_str());
        let mut head = marked(Mark::NeedsYou, question, room, look.glyphs);
        if let Some(edge) = &self.edge {
            place_edge(&mut head, edge, room, keeping, look.glyphs);
        }
        head
    }
}

/// Puts `edge` at the far end of the question's first row when both fit in `room`,
/// padded so it ends at `room`. Otherwise, when `keeping` holds everything, it is a
/// muted row of its own, last in `head`, cut in the middle when it is wider than the
/// line.
fn place_edge(head: &mut Vec<Line>, edge: &str, room: usize, keeping: Keeping, glyphs: Glyphs) {
    let first = cell_width(&head[0].text());
    let edge_cells = cell_width(edge);
    match first + HEAD_GAP + edge_cells <= room {
        true => {
            let gap = " ".repeat(room - first - edge_cells);
            let row = std::mem::take(&mut head[0])
                .plain(gap)
                .push(Tone::Muted, edge);
            head[0] = row;
        }
        false if keeping == Keeping::Everything => {
            head.push(Line::new().push(Tone::Muted, cut_to(edge, room, glyphs)));
        }
        false => {}
    }
}

/// The option chosen, what is asked of the words, the row they are typed on, and why
/// the words given were not taken; with where the cursor stands among those rows.
fn saying(
    chosen: &ChoiceRow,
    asks: &str,
    typed: &TypedRow,
    refused: Option<&str>,
    look: &Look,
    room: usize,
) -> (Vec<Line>, Option<(usize, usize)>) {
    let mut lines = chosen.lines(true, cell_width(&chosen.name) + NAME_GAP, room, look.glyphs);
    lines.push(Line::new());
    lines.extend(Line::new().push(Tone::Strong, asks).wrap(room));
    let cursor = (
        lines.len(),
        cell_width(&typed.prompt) + cell_width(&typed.before),
    );
    lines.push(
        Line::new()
            .push(Tone::Muted, typed.prompt.as_str())
            .plain(typed.before.as_str())
            .plain(typed.after.as_str()),
    );
    if let Some(refused) = refused {
        lines.extend(marked(
            Mark::Failed,
            Line::new().plain(refused),
            room,
            look.glyphs,
        ));
    }
    (lines, Some(cursor))
}

/// A list with what narrows it over it, in at most `budget` lines of `room` cells: what
/// was typed, when anything was, and the rows it leaves, or that it leaves none.
pub fn narrowed(
    choices: Option<&Choices>,
    filter: Option<&str>,
    look: &Look,
    room: usize,
    budget: usize,
) -> Vec<Line> {
    let mut lines: Vec<Line> = filter
        .map(|typed| {
            Line::new()
                .push(Tone::Muted, "filter ")
                .push(Tone::Strong, typed)
        })
        .into_iter()
        .collect();
    let left = budget.saturating_sub(lines.len());
    match choices {
        Some(choices) => lines.extend(choices.fitted(look, room, left)),
        None => lines.push(Line::new().push(Tone::Muted, "nothing matches")),
    }
    lines
}

/// `text` wrapped in `room` cells, its first row opened by `mark` in the mark's own
/// tone and a space, and every later row under the width the mark takes.
fn marked(mark: Mark, text: Line, room: usize, glyphs: Glyphs) -> Vec<Line> {
    text.wrap(room.saturating_sub(MARKED))
        .into_iter()
        .enumerate()
        .map(|(at, part)| match at {
            0 => Line::new()
                .push(Tone::of(mark), glyphs.mark(mark).to_string())
                .plain(" ")
                .then(part),
            _ => part.under(INDENT),
        })
        .collect()
}

impl Choices {
    /// The list in `room` cells: its rows, at most `most` of them around the one
    /// pointed at, and how many are left out. A row's text stands where the longest
    /// name leaves it.
    pub fn lines(&self, look: &Look, room: usize, most: usize) -> Vec<Line> {
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
            .flat_map(|(offset, row)| row.lines(start + offset == self.at, name, room, glyphs))
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
    pub fn fitted(&self, look: &Look, room: usize, budget: usize) -> Vec<Line> {
        let mut most = self.rows.len();
        let mut lines = self.lines(look, room, most);
        while lines.len() > budget && most > 1 {
            most -= 1;
            lines = self.lines(look, room, most);
        }
        lines
    }
}

impl ChoiceRow {
    /// This option, pointed at or not, with its name in a column `name` cells wide:
    /// what it does in the plain tone, and what it costs in the muted one, as one
    /// sentence wrapped under itself. What it does not say it does not say, and a
    /// name with nothing beside it ends where it does.
    fn lines(&self, pointed: bool, name: usize, room: usize, glyphs: Glyphs) -> Vec<Line> {
        let width = room.saturating_sub(POINTER);
        let (mark, mark_tone, name_tone) = match pointed {
            true => (glyphs.pointer().to_string(), Tone::NeedsYou, Tone::Strong),
            false => (" ".to_string(), Tone::Plain, Tone::Plain),
        };
        let opening = Line::new().push(mark_tone, mark).plain(" ");
        let text = self.text();
        let beside = width.saturating_sub(name);
        let mut lines: Vec<Line> = Vec::new();
        if text.text().is_empty() || beside < LEAST_TEXT {
            lines.push(opening.push(name_tone, cut_to(&self.name, width, glyphs)));
            if !text.text().is_empty() {
                let margin = " ".repeat(POINTER);
                lines.extend(text.wrap(width).into_iter().map(|part| part.under(&margin)));
            }
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
        match pointed {
            true => lines
                .into_iter()
                .map(|line| line.filled(Fill::Selection))
                .collect(),
            false => lines,
        }
    }

    /// What the option says: `said`, then `: ` and `cost` when both are given, the
    /// cost in the muted tone, so a row says what it has and no more.
    pub(super) fn text(&self) -> Line {
        let mut text = Line::new().push(Tone::Plain, self.said.as_deref().unwrap_or_default());
        if self.said.is_some() && self.cost.is_some() {
            text = text.plain(": ");
        }
        text.push(Tone::Muted, self.cost.as_deref().unwrap_or_default())
    }
}

/// The route of a card that has one, wrapped in `room` cells: `then` in the muted
/// tone and two spaces, then the route, its later parts under the word's width.
fn then_lines(then: &str, room: usize) -> Vec<Line> {
    Line::new()
        .plain(then)
        .wrap(room.saturating_sub(THEN_MARGIN))
        .into_iter()
        .enumerate()
        .map(|(at, part)| match at {
            0 => Line::new().push(Tone::Muted, "then").plain("  ").then(part),
            _ => part.under(&" ".repeat(THEN_MARGIN)),
        })
        .collect()
}

/// `text` in exactly `width` cells when it is wider, cut in the middle so that both
/// of its ends stay, and as it is when it fits.
fn cut_to(text: &str, width: usize, glyphs: Glyphs) -> String {
    match cell_width(text) > width {
        true => middle_cut(text, width, glyphs),
        false => text.to_string(),
    }
}

/// A row of a card with its edge in front of it, in the person's tone: the edge alone
/// when the row is empty, and otherwise the edge, a space and the row. The row keeps
/// its own fill, a pointed row's, and a row with none is a tint.
fn on_the_edge(row: Line, glyphs: Glyphs) -> Line {
    let fill = row.fill().unwrap_or(Fill::Tint);
    let edge = Line::new().push(Tone::NeedsYou, glyphs.decision_edge().to_string());
    let drawn = match row.text().is_empty() {
        true => edge,
        false => edge.plain(" ").then(row),
    };
    drawn.filled(fill)
}

mod round;
pub use round::said_whole;

#[cfg(test)]
mod tests;
