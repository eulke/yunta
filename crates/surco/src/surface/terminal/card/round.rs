//! A round of questions on a card: what was answered, what is being answered with its
//! field or its list under it, and what is to come. A round longer than the terminal
//! folds what was answered, then what is to come; what is being answered is never
//! folded.

use super::narrowed;
use crate::components::{Card, Choices, RoundRow, RoundStanding, TypedRow};
use crate::glyphs::{Glyphs, Mark};
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::cell_width;
use crate::words::{ANSWERED, TO_ANSWER};

/// The cells a question's mark and the space after it take: what wraps stands under it.
const MARK: usize = 2;

/// The cells what is being answered stands in from the question it answers: its field
/// or its list, set apart from the row above and below it.
const UNDER: usize = 4;

/// The cells between a question and what stands beside it on its row: one would read as
/// a word's space.
const BESIDE: usize = 2;

/// Which rows of a round are drawn whole.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fold {
    /// All of them.
    Nothing,
    /// What was answered is counted on one row.
    Answered,
    /// What is to come is counted on one row too.
    AnsweredAndAhead,
}

/// A list open on a row of a round: where it goes among the row's lines, and what it
/// holds.
struct Open<'a> {
    at: usize,
    choices: Option<&'a Choices>,
    filter: Option<&'a str>,
}

/// A row of a round as it is drawn: its lines, the cursor among them when a field is
/// open, and the list open on it, when one is.
struct Drawn<'a> {
    lines: Vec<Line>,
    cursor: Option<(usize, usize)>,
    list: Option<Open<'a>>,
}

impl<'a> Drawn<'a> {
    /// A row with nothing open on it.
    fn at_rest(lines: Vec<Line>) -> Self {
        Drawn {
            lines,
            cursor: None,
            list: None,
        }
    }
}

impl Card {
    /// The cells a field in a round may fill with `prompt` before it: what a field on a
    /// card may fill, less the margin it stands in from its question.
    pub(crate) fn typed_room_in_round(look: &Look, prompt: &str) -> usize {
        Card::typed_room(look, prompt).saturating_sub(UNDER)
    }
}

/// A question with its answer as the history keeps them once the round is over: the
/// answer whole, wrapped under the question's mark.
pub fn said_whole(asked: &str, said: &str, look: &Look) -> Vec<Line> {
    let text = Line::new()
        .push(Tone::Plain, asked)
        .plain(" ".repeat(BESIDE))
        .push(Tone::Strong, said);
    opened(
        Mark::Done,
        Tone::Done,
        text,
        look.width.cells(),
        look.glyphs,
    )
}

/// The round's rows in at most `most` lines, with where the cursor stands among them
/// when a field is open. The rows are drawn whole first, then folded one fold at a time
/// until they fit.
pub(super) fn lines(
    rows: &[RoundRow],
    look: &Look,
    room: usize,
    most: usize,
) -> (Vec<Line>, Option<(usize, usize)>) {
    let mut drawn = laid_out(rows, look, room, Fold::Nothing, most);
    for fold in [Fold::Answered, Fold::AnsweredAndAhead] {
        if drawn.0.len() > most {
            drawn = laid_out(rows, look, room, fold, most);
        }
    }
    drawn
}

/// The rows of a round laid out under `fold`, with the lists open on them given what the
/// other rows leave of `most` lines.
fn laid_out(
    rows: &[RoundRow],
    look: &Look,
    room: usize,
    fold: Fold,
    most: usize,
) -> (Vec<Line>, Option<(usize, usize)>) {
    let drawn = rows_drawn(rows, look, room, fold);
    let fixed: usize = drawn.iter().map(|row| row.lines.len()).sum();
    let budget = most.saturating_sub(fixed);
    let mut lines: Vec<Line> = Vec::new();
    let mut cursor = None;
    for row in drawn {
        let start = lines.len();
        if let Some((at, cell)) = row.cursor {
            cursor = Some((start + at, cell));
        }
        let mut own = row.lines;
        if let Some(open) = row.list {
            let shown = narrowed(
                open.choices,
                open.filter,
                look,
                room.saturating_sub(UNDER),
                budget,
            );
            own.splice(open.at..open.at, shown.into_iter().map(indented));
        }
        lines.extend(own);
    }
    (lines, cursor)
}

/// The rows of a round as `fold` draws them. What was answered, and under the fold that
/// says so what is to come, becomes one row where its first row stood.
fn rows_drawn<'a>(rows: &'a [RoundRow], look: &Look, room: usize, fold: Fold) -> Vec<Drawn<'a>> {
    let total = rows.len();
    let answered = rows
        .iter()
        .filter(|row| matches!(row.stands, RoundStanding::Answered(_)))
        .count();
    let ahead = rows
        .iter()
        .filter(|row| matches!(row.stands, RoundStanding::Ahead))
        .count();
    let (mut answered_shown, mut ahead_shown) = (false, false);
    let mut drawn: Vec<Drawn<'a>> = Vec::new();
    for row in rows {
        if !folds(fold, &row.stands) {
            drawn.push(whole(row, look, room));
            continue;
        }
        match &row.stands {
            RoundStanding::Answered(_) if !answered_shown => {
                answered_shown = true;
                let word = format!("{answered} of {total} {ANSWERED}");
                drawn.push(summary(Mark::Done, Tone::Done, word, look.glyphs));
            }
            RoundStanding::Ahead if !ahead_shown => {
                ahead_shown = true;
                let word = format!("{ahead} of {total} {TO_ANSWER}");
                drawn.push(summary(Mark::Absent, Tone::Faint, word, look.glyphs));
            }
            _ => {}
        }
    }
    drawn
}

/// Whether `fold` counts a row with `stands` on one row instead of drawing it whole.
fn folds(fold: Fold, stands: &RoundStanding) -> bool {
    match (fold, stands) {
        (Fold::Nothing, _) => false,
        (_, RoundStanding::Answered(_)) => true,
        (Fold::AnsweredAndAhead, RoundStanding::Ahead) => true,
        _ => false,
    }
}

/// A run of folded rows as one row: `mark` in `tone`, then what the count says.
fn summary<'a>(mark: Mark, tone: Tone, word: String, glyphs: Glyphs) -> Drawn<'a> {
    let line = Line::new()
        .push(tone, glyphs.mark(mark).to_string())
        .plain(" ")
        .push(Tone::Muted, word);
    Drawn::at_rest(vec![line])
}

/// One row of a round drawn whole, as the state it stands in says it.
fn whole<'a>(row: &'a RoundRow, look: &Look, room: usize) -> Drawn<'a> {
    let glyphs = look.glyphs;
    let note = row.note.as_deref();
    match &row.stands {
        RoundStanding::Answered(said) => {
            Drawn::at_rest(vec![answer(&row.asked, said, room, glyphs)])
        }
        RoundStanding::Ahead => {
            let question = asking(&row.asked, Tone::Muted, note);
            Drawn::at_rest(opened(Mark::Absent, Tone::Faint, question, room, glyphs))
        }
        RoundStanding::Typing { typed, refused } => typing(row, typed, refused, look, room),
        RoundStanding::Picking {
            choices,
            filter,
            refused,
        } => picking(row, choices, filter, refused, look, room),
    }
}

/// An answered question on one line: its mark, the question and the answer beside it. A
/// line too narrow for both gives the question's cells first, and neither takes less
/// than half of what is free unless the other leaves it.
fn answer(asked: &str, said: &str, room: usize, glyphs: Glyphs) -> Line {
    let free = room.saturating_sub(MARK + BESIDE);
    let answer_cells = cell_width(said).min(free.saturating_sub(cell_width(asked).min(free / 2)));
    let asked_cells = free - answer_cells;
    Line::new()
        .push(Tone::Done, glyphs.mark(Mark::Done).to_string())
        .plain(" ")
        .then(
            Line::new()
                .push(Tone::Plain, asked)
                .cut(asked_cells, glyphs),
        )
        .plain(" ".repeat(BESIDE))
        .then(
            Line::new()
                .push(Tone::Strong, said)
                .cut(answer_cells, glyphs),
        )
}

/// A question with its note beside it, in the muted tone, apart by `BESIDE` spaces.
fn asking(asked: &str, tone: Tone, note: Option<&str>) -> Line {
    let line = Line::new().push(tone, asked);
    match note {
        Some(note) => line.plain(" ".repeat(BESIDE)).push(Tone::Muted, note),
        None => line,
    }
}

/// `text` wrapped in `room` cells less the mark: its first line opened by `mark` in
/// `tone` and a space, and every later line under the mark's width.
fn opened(mark: Mark, tone: Tone, text: Line, room: usize, glyphs: Glyphs) -> Vec<Line> {
    text.wrap(room.saturating_sub(MARK))
        .into_iter()
        .enumerate()
        .map(|(at, part)| match at {
            0 => Line::new()
                .push(tone, glyphs.mark(mark).to_string())
                .plain(" ")
                .then(part),
            _ => part.under(&" ".repeat(MARK)),
        })
        .collect()
}

/// The question of a row being answered, opened by its mark, with its note beside it.
fn being_answered(row: &RoundRow, look: &Look, room: usize) -> Vec<Line> {
    let question = asking(&row.asked, Tone::Strong, row.note.as_deref());
    opened(Mark::NeedsYou, Tone::NeedsYou, question, room, look.glyphs)
}

/// A question being typed: its field under it, with the cursor on the field past what
/// stands before it, and what the engine refused under the field.
fn typing<'a>(
    row: &RoundRow,
    typed: &TypedRow,
    refused: &[String],
    look: &Look,
    room: usize,
) -> Drawn<'a> {
    let mut lines = being_answered(row, look, room);
    let field_at = lines.len();
    let (before, after) = typed.fitted(Card::typed_room_in_round(look, &typed.prompt));
    let cursor = (
        field_at,
        UNDER + cell_width(&typed.prompt) + cell_width(&before),
    );
    let field = Line::new()
        .push(Tone::Muted, typed.prompt.as_str())
        .plain(before)
        .plain(after);
    lines.push(indented(field));
    lines.extend(refusals(refused, room, look.glyphs));
    Drawn {
        lines,
        cursor: Some(cursor),
        list: None,
    }
}

/// A question being picked from a list: the list is laid in after its question, and
/// what the engine refused comes after the list.
fn picking<'a>(
    row: &'a RoundRow,
    choices: &'a Option<Choices>,
    filter: &'a Option<String>,
    refused: &[String],
    look: &Look,
    room: usize,
) -> Drawn<'a> {
    let mut lines = being_answered(row, look, room);
    let at = lines.len();
    lines.extend(refusals(refused, room, look.glyphs));
    Drawn {
        lines,
        cursor: None,
        list: Some(Open {
            at,
            choices: choices.as_ref(),
            filter: filter.as_deref(),
        }),
    }
}

/// What the engine refused, each under the field or list it was given for, opened by
/// its mark.
fn refusals(refused: &[String], room: usize, glyphs: Glyphs) -> Vec<Line> {
    refused
        .iter()
        .flat_map(|said| {
            let text = Line::new().plain(said.as_str());
            opened(
                Mark::Failed,
                Tone::Failed,
                text,
                room.saturating_sub(UNDER),
                glyphs,
            )
        })
        .map(indented)
        .collect()
}

/// `line` moved under the field or list it belongs to.
fn indented(line: Line) -> Line {
    line.under(&" ".repeat(UNDER))
}

#[cfg(test)]
mod tests;
