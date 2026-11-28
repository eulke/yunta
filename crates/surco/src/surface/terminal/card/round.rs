//! A round of questions on a card: what was answered, what is being answered with its
//! field or its list under it, and what is to come. The list a question is picked from
//! stands as every list of choices does, under the row it is narrowed on and with no blank
//! row between.
//!
//! A round longer than its rows folds the questions above the one being answered, then
//! those under it, each run counted on one row where it stood, as a list says what it
//! hides. With its card's question given way too, the question being answered keeps one
//! row, then the row its list is narrowed on gives way while nothing is typed there, and
//! last the list keeps the rows around its pointer. What is being answered is never
//! folded, nor a line being typed. Every printable key types on the row a list is narrowed
//! on, so the cursor stands there wherever it stands, as it stands on a line being typed.

use super::super::asked::{filter_cursor, filter_row, listed, Columns};
use super::super::Terminal;
use crate::components::{Card, Choices, RoundRow, RoundStanding, TypedRow};
use crate::glyphs::{Glyphs, Mark};
use crate::line::{Hidden, Line, Tone};
use crate::look::Look;
use crate::width::cell_width;

/// The cells a question's mark and the space after it take: what wraps stands under it.
const MARK: usize = 2;

/// The cells what is being answered stands in from the question it answers: its field
/// or its list, set apart from the row above and below it.
const UNDER: usize = 4;

/// The cells between a question and what stands beside it on its row: one would read as
/// a word's space.
const BESIDE: usize = 2;

/// How far a round folds, from every row drawn whole to the least it is answered in: each
/// fold keeps what the ones before it gave way.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Fold {
    /// Every row whole.
    Nothing,
    /// The questions above the one being answered are counted on one row.
    Above,
    /// Those under it are counted on one row too.
    AboveAndUnder,
    /// The question being answered keeps one row, cut at the line.
    AskedOnOneRow,
    /// The row its list is narrowed on gives way while nothing is typed there.
    NoFilterRow,
    /// Its list keeps the rows around its pointer, counting those it hides.
    Windowed,
}

/// Every fold past drawing every row whole, in the order a round gives way.
const FOLDS: [Fold; 5] = [
    Fold::Above,
    Fold::AboveAndUnder,
    Fold::AskedOnOneRow,
    Fold::NoFilterRow,
    Fold::Windowed,
];

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
    /// The cells the box a field in a round is typed in spans: what the box on a card
    /// spans, less the margin it stands in from its question.
    pub(crate) fn typed_room_in_round(look: &Look) -> usize {
        Card::typed_room(look).saturating_sub(UNDER)
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
/// until they fit: while the card's question stands over them (`headed`), only the
/// questions above and under the one being answered fold, so the card's question gives
/// way before anything of what is being answered does.
pub(super) fn lines(
    rows: &[RoundRow],
    look: &Look,
    room: usize,
    most: usize,
    headed: bool,
) -> (Vec<Line>, Option<(usize, usize)>) {
    let deepest = match headed {
        true => Fold::AboveAndUnder,
        false => Fold::Windowed,
    };
    let mut drawn = laid_out(rows, look, room, Fold::Nothing, most);
    for fold in FOLDS.into_iter().take_while(|fold| *fold <= deepest) {
        if drawn.0.len() > most {
            drawn = laid_out(rows, look, room, fold, most);
        }
    }
    drawn
}

/// The rows of a round laid out under `fold`, with a list open on them given what the
/// other rows leave of `most` lines once the fold keeps only its rows around its pointer.
fn laid_out(
    rows: &[RoundRow],
    look: &Look,
    room: usize,
    fold: Fold,
    most: usize,
) -> (Vec<Line>, Option<(usize, usize)>) {
    let drawn = rows_drawn(rows, look, room, fold);
    let fixed: usize = drawn.iter().map(|row| row.lines.len()).sum();
    let left = most.saturating_sub(fixed);
    let mut lines: Vec<Line> = Vec::new();
    let mut cursor = None;
    for row in drawn {
        let start = lines.len();
        if let Some((at, cell)) = row.cursor {
            cursor = Some((start + at, cell));
        }
        let mut own = row.lines;
        if let Some(open) = row.list {
            let room = room.saturating_sub(UNDER);
            if narrowed_on(&open, fold) {
                let lands = filter_cursor(open.filter, look.glyphs, room);
                cursor = Some((start + open.at, UNDER + lands));
            }
            let shown = list(&open, look, room, fold, left);
            own.splice(open.at..open.at, shown.into_iter().map(indented));
        }
        lines.extend(own);
    }
    (lines, cursor)
}

/// The list open on a round's row, in `room` cells: the row it is narrowed on, but where
/// `fold` lets it give way while nothing is typed there, then its rows, every one of them
/// until `fold` keeps only those around its pointer in the `left` lines the round leaves.
fn list(open: &Open, look: &Look, room: usize, fold: Fold, left: usize) -> Vec<Line> {
    let narrowed = narrowed_on(open, fold).then(|| filter_row(open.filter, look.glyphs));
    let budget = match fold {
        Fold::Windowed => left.saturating_sub(usize::from(narrowed.is_some())),
        _ => usize::MAX,
    };
    narrowed
        .into_iter()
        .chain(listed(
            open.choices,
            look,
            (room, Columns::over(&Terminal::on(*look))),
            budget,
        ))
        .collect()
}

/// Whether the row `open` is narrowed on stands under `fold`: until the fold that lets it
/// give way, and wherever something is typed on it.
fn narrowed_on(open: &Open, fold: Fold) -> bool {
    fold < Fold::NoFilterRow || open.filter.is_some()
}

/// The rows of a round as `fold` draws them: the questions above the one being answered,
/// whole or counted; that one, its question whole or on one row; and those under it, whole
/// or counted. A round with no question in turn holds every row above it.
fn rows_drawn<'a>(rows: &'a [RoundRow], look: &Look, room: usize, fold: Fold) -> Vec<Drawn<'a>> {
    let at = rows
        .iter()
        .position(|row| {
            matches!(
                row.stands,
                RoundStanding::Typing { .. } | RoundStanding::Picking { .. }
            )
        })
        .unwrap_or(rows.len());
    let (above, rest) = rows.split_at(at);
    let (turn, under) = match rest.split_first() {
        Some((turn, under)) => (Some(turn), under),
        None => (None, rest),
    };
    let side = |part: &'a [RoundRow], hidden: Hidden, folds: bool| -> Vec<Drawn<'a>> {
        match folds {
            true => Line::hidden(part.len(), hidden, look.glyphs)
                .map(|count| Drawn::at_rest(vec![count]))
                .into_iter()
                .collect(),
            false => part
                .iter()
                .map(|row| whole(row, look, room, fold))
                .collect(),
        }
    };
    let mut drawn = side(above, Hidden::Above, fold >= Fold::Above);
    drawn.extend(turn.map(|row| whole(row, look, room, fold)));
    drawn.extend(side(under, Hidden::Under, fold >= Fold::AboveAndUnder));
    drawn
}

/// One row of a round drawn whole, as the state it stands in says it, the question being
/// answered on one row where `fold` says so.
fn whole<'a>(row: &'a RoundRow, look: &Look, room: usize, fold: Fold) -> Drawn<'a> {
    let glyphs = look.glyphs;
    let note = row.note.as_deref();
    match &row.stands {
        RoundStanding::Answered(said) => {
            Drawn::at_rest(vec![answer(&row.asked, said, room, glyphs)])
        }
        RoundStanding::Ahead => {
            let question = asking(&row.asked, Tone::Muted, note, room.saturating_sub(MARK));
            Drawn::at_rest(marked(Mark::Absent, Tone::Faint, question, glyphs))
        }
        RoundStanding::Typing { typed, refused } => typing(row, typed, refused, look, room, fold),
        RoundStanding::Picking {
            choices,
            filter,
            refused,
        } => picking(row, choices, filter, refused, look, room, fold),
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

/// A question in `tone` wrapped in `cells`, with its note beside it in the muted tone,
/// apart by `BESIDE` spaces: the note goes on along the question's last row while its
/// words fit there, and wraps under it past them, so the gap that sets it apart stands
/// wherever the two share a row.
fn asking(asked: &str, tone: Tone, note: Option<&str>, cells: usize) -> Vec<Line> {
    let mut lines = Line::new().push(tone, asked).wrap(cells);
    let Some(note) = note else {
        return lines;
    };
    let last = lines.pop().unwrap_or_default();
    let room = cells.saturating_sub(cell_width(&last.text()) + BESIDE);
    let words: Vec<&str> = note.split_whitespace().collect();
    let mut used = 0;
    let beside = words
        .iter()
        .take_while(|word| {
            used += cell_width(word) + usize::from(used > 0);
            used <= room
        })
        .count();
    let rest = Line::new().push(Tone::Muted, words[beside..].join(" "));
    match beside {
        0 => lines.push(last),
        _ => lines.push(
            last.plain(" ".repeat(BESIDE))
                .push(Tone::Muted, words[..beside].join(" ")),
        ),
    }
    lines.extend(
        (beside < words.len())
            .then(|| rest.wrap(cells))
            .into_iter()
            .flatten(),
    );
    lines
}

/// A question in `tone` and its note on one row of `cells`, cut at the line.
fn asking_on_one_row(
    asked: &str,
    tone: Tone,
    note: Option<&str>,
    cells: usize,
    glyphs: Glyphs,
) -> Line {
    let question = Line::new().push(tone, asked);
    let line = match note {
        Some(note) => question.plain(" ".repeat(BESIDE)).push(Tone::Muted, note),
        None => question,
    };
    line.cut(cells, glyphs)
}

/// `text` wrapped in `room` cells less the mark: its first line opened by `mark` in
/// `tone` and a space, and every later line under the mark's width.
fn opened(mark: Mark, tone: Tone, text: Line, room: usize, glyphs: Glyphs) -> Vec<Line> {
    marked(mark, tone, text.wrap(room.saturating_sub(MARK)), glyphs)
}

/// `parts`, the first opened by `mark` in `tone` and a space, every later one under the
/// mark's width.
fn marked(mark: Mark, tone: Tone, parts: Vec<Line>, glyphs: Glyphs) -> Vec<Line> {
    parts
        .into_iter()
        .enumerate()
        .map(|(at, part)| match at {
            0 => mark_opening(mark, tone, glyphs).then(part),
            _ => part.under(&" ".repeat(MARK)),
        })
        .collect()
}

/// `mark` in `tone`, and the space after it.
fn mark_opening(mark: Mark, tone: Tone, glyphs: Glyphs) -> Line {
    Line::new()
        .push(tone, glyphs.mark(mark).to_string())
        .plain(" ")
}

/// The question of a row being answered, opened by its mark, with its note beside it:
/// wrapped under itself, or on one row cut at the line where `fold` says so.
fn being_answered(row: &RoundRow, look: &Look, room: usize, fold: Fold) -> Vec<Line> {
    let (asked, note, cells) = (&row.asked, row.note.as_deref(), room.saturating_sub(MARK));
    let question = match fold >= Fold::AskedOnOneRow {
        true => vec![asking_on_one_row(
            asked,
            Tone::Strong,
            note,
            cells,
            look.glyphs,
        )],
        false => asking(asked, Tone::Strong, note, cells),
    };
    marked(Mark::NeedsYou, Tone::NeedsYou, question, look.glyphs)
}

/// A question being typed: its boxed field under it, with the cursor in the box past
/// what stands before it, and what the engine refused under the field.
fn typing<'a>(
    row: &RoundRow,
    typed: &TypedRow,
    refused: &[String],
    look: &Look,
    room: usize,
    fold: Fold,
) -> Drawn<'a> {
    let mut lines = being_answered(row, look, room, fold);
    let field_at = lines.len();
    let boxed = typed.drawn(Card::typed_room_in_round(look), look.glyphs, None);
    let cursor = boxed
        .cursor
        .map(|(row, cell)| (field_at + row, UNDER + cell));
    lines.extend(boxed.lines.into_iter().map(indented));
    lines.extend(refusals(refused, room, look.glyphs));
    Drawn {
        lines,
        cursor,
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
    fold: Fold,
) -> Drawn<'a> {
    let mut lines = being_answered(row, look, room, fold);
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
