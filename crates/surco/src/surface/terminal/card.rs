//! A decision held in place at a terminal: its question, what a reader needs to
//! answer, and every option with one pointed at; where words are typed for the one
//! pointed at, the box they are typed in stands under the options, what is asked of
//! them faint in it while nothing is typed. Every row opens on the decision's edge and is
//! tinted, and the row pointed at is set apart where a layer draws fills.
//!
//! A card whose keys stand in a screen's legend draws none of its own and ends on its
//! last option; one printed with its keys says them as its foot, with what follows the
//! option pointed at over them. A card with no option to choose and no keys of its own, as
//! one standing for a decision that could not be read, ends on what stands under its
//! question. Short of rows, what gives way never takes an option:
//! the blank rows and the route go first, the question keeps one row beside its far
//! edge and what stands under it is cut to the rows left, then each option keeps one
//! row, and only then does the list count what it leaves out.

use super::at_ends;
use crate::components::{Card, CardBody, Choices, Laid, TypedRow};
use crate::glyphs::{Glyphs, Mark};
use crate::line::{Fill, Line, Tone};
use crate::look::Look;
use crate::width::{cell_width, INDENT};

/// A prompt's lines, where the cursor stands when a field is open (the row, counted
/// from its first line, and the cell), and what was laid out of a pane it scrolls.
pub struct Placed {
    pub lines: Vec<Line>,
    pub cursor: Option<(usize, usize)>,
    pub laid: Option<Laid>,
}

/// The cells the edge and the space after it take at the start of a row.
const EDGE: usize = 2;

/// The cells between the question and the card's far edge on one row.
const HEAD_GAP: usize = 2;

/// The fewest cells a question cut to one row keeps beside its far edge: under them it
/// no longer says what is asked, and the edge gives way instead.
const LEAST_QUESTION: usize = 20;

/// The cells the mark of a line that needs a person, or a refusal, and the space
/// after it take: the rest of such a line wraps under them.
const MARKED: usize = 2;

/// The cells the word `then` and the two spaces after it take: the rest of a route
/// wraps under them.
const THEN_MARGIN: usize = 6;

/// How much of a card a terminal has room for, what gives way first to what stands
/// to the last. A card's options stand at every one; a round gives way by levels of its
/// own once its blank rows have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Keeping {
    /// All of it.
    Everything,
    /// The question on one row beside its far edge, what stands under it in the rows the
    /// rest leaves, and the options and the keys: the blank rows and the route give way.
    Spare,
    /// Each option on one row too, cut at the line, and the list in the rows left,
    /// counting what it leaves out.
    OneRowEach,
    /// The question and the options: the keys give way too, and the line words are typed
    /// on stays.
    TheQuestion,
    /// A round's own rows and the keys: the card's question and what stands under it give
    /// way, since the round's questions say what is asked, and the round folds further.
    TheRound,
    /// A round's own rows: the keys give way too.
    TheRoundAlone,
}

impl Keeping {
    /// Whether the card's question and what stands under it give way here.
    fn headless(self) -> bool {
        matches!(self, Keeping::TheRound | Keeping::TheRoundAlone)
    }
}

impl Card {
    /// The card as a prompt draws it in at most `rows` rows, with where the cursor
    /// stands when a field is open. Short of rows, it keeps the question and every
    /// option: the blank rows and the route give way first, with the question's rows
    /// past its first and what stands under it past the rows left; then what each option
    /// says past its row, then the list from the ends away from the row pointed at,
    /// counting what it leaves out, and then the keys. Fewer rows than a question and
    /// one row are too few to be asked in. A round keeps its own rows instead: after its
    /// blank rows, the question over it and what stands under it give way, then its keys.
    pub fn drawn(&self, look: &Look, most: usize) -> Placed {
        let whole = self.laid_out(look, most, Keeping::Everything);
        let short = match self.body {
            CardBody::Round(_) => [Keeping::Spare, Keeping::TheRound, Keeping::TheRoundAlone],
            _ => [Keeping::Spare, Keeping::OneRowEach, Keeping::TheQuestion],
        };
        let placed =
            short
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
            laid: None,
        }
    }

    /// The rows the card takes whole, every option's words and its blank rows standing: what
    /// it keeps in a pane taller than the shortest look before anything of its own gives way.
    pub(crate) fn whole(&self, look: &Look) -> usize {
        self.laid_out(look, usize::MAX, Keeping::Everything)
            .lines
            .len()
    }

    /// The rows the card keeps before what stands above it in a pane gives way: its
    /// question on one row, what stands under it whole, each option on one row and its
    /// keys, its blank rows and its route given way. What stands above it gives way from
    /// its top down to the rows these leave, and only then does the card's own order of
    /// giving way go on, so its options never give way to what stands above it.
    pub(crate) fn least(&self, look: &Look) -> usize {
        self.laid_out(look, usize::MAX, Keeping::OneRowEach)
            .lines
            .len()
    }

    /// The cells the box a card's words are typed in spans: what the card's edge leaves
    /// of the line.
    pub(crate) fn typed_room(look: &Look) -> usize {
        look.width.cells().saturating_sub(EDGE)
    }

    /// The card's rows, before their edge, in at most `most` rows where what gives way
    /// can give way that far, holding what `keeping` says.
    fn laid_out(&self, look: &Look, most: usize, keeping: Keeping) -> Placed {
        let room = look.width.cells().saturating_sub(EDGE);
        let spaced = keeping == Keeping::Everything;
        let head = self.head(look, room, keeping);
        let foot = self.foot(look, room, keeping);
        // Everything stands: what stands under the question whole, and a blank row under it.
        let whole = match spaced {
            true => self.reason(room, usize::MAX, look.glyphs),
            false => Vec::new(),
        };
        let left = most.saturating_sub(head.len() + whole.len() + usize::from(spaced) + foot.len());
        let (middle, cursor) = self.middle(look, room, left, keeping);
        let reason = match (spaced, keeping.headless()) {
            (true, _) => whole,
            (false, true) => Vec::new(),
            (false, false) => {
                let rows = most.saturating_sub(head.len() + middle.len() + foot.len());
                self.reason(room, rows, look.glyphs)
            }
        };
        // A card with nothing to choose and no keys of its own ends on its reason.
        let parted = spaced && !(middle.is_empty() && foot.is_empty());
        let over = head.len() + reason.len() + usize::from(parted);
        Placed {
            cursor: cursor.map(|(row, cell)| (over + row, EDGE + cell)),
            lines: head
                .into_iter()
                .chain(reason)
                .chain(parted.then(Line::new))
                .chain(middle)
                .chain(foot)
                .collect(),
            laid: None,
        }
    }

    /// What the card holds under its question, in the `left` rows the rest leaves where it
    /// gives way that far: every option, with what follows the one pointed at where the
    /// card says its keys and everything stands, or with the words the one pointed at is
    /// said with under them; or a round. With where the cursor stands among them.
    fn middle(
        &self,
        look: &Look,
        room: usize,
        left: usize,
        keeping: Keeping,
    ) -> (Vec<Line>, Option<(usize, usize)>) {
        match &self.body {
            CardBody::Choosing { choices, then } => {
                // What follows an option is the printed card's, which says its keys as its
                // foot: a screen's card leaves it to the printed page.
                let route = then
                    .as_deref()
                    .filter(|then| keeping == Keeping::Everything && !then.is_empty())
                    .filter(|_| !self.legend.is_empty())
                    .map(|then| then_lines(then, room));
                let route_rows = route.as_ref().map_or(0, |route| route.len() + 1);
                let one_row_each = matches!(keeping, Keeping::OneRowEach | Keeping::TheQuestion);
                let mut middle = match one_row_each {
                    true => choices.fitted(look, room, left.saturating_sub(route_rows), Said::Cut),
                    false => choices.lines(look, room, usize::MAX, Said::Wrapped),
                };
                if let Some(route) = route {
                    middle.push(Line::new());
                    middle.extend(route);
                }
                (middle, None)
            }
            CardBody::Saying {
                choices,
                asks,
                typed,
                refused,
            } => {
                let words = Words {
                    asks,
                    typed,
                    refused: refused.as_deref(),
                };
                saying(choices, &words, (left, keeping), look, room)
            }
            CardBody::Round(rows) => round::lines(rows, look, room, left, !keeping.headless()),
        }
    }

    /// The question under the mark of what needs a person. When everything stands, it
    /// is wrapped under itself, with the card's far edge on its first row when both fit
    /// and on a row of its own, cut to the line, when they do not; short of rows, it is
    /// one row, cut beside its edge; and over a round shorter still, it gives way.
    fn head(&self, look: &Look, room: usize, keeping: Keeping) -> Vec<Line> {
        let question = Line::new().push(Tone::Strong, self.question.as_str());
        if keeping.headless() {
            return Vec::new();
        }
        if keeping != Keeping::Everything {
            return vec![self.question_row(question, room, look.glyphs)];
        }
        let mut head = marked(Mark::NeedsYou, question, room, look.glyphs);
        if let Some(edge) = &self.edge {
            place_edge(&mut head, edge, room, look.glyphs);
        }
        head
    }

    /// `question` on one row of `room` cells under the mark of what needs a person, cut
    /// at the line, with the card's far edge whole at the row's end while the question
    /// keeps the cells it needs, or `LEAST_QUESTION`, beside it; else the edge gives way.
    fn question_row(&self, question: Line, room: usize, glyphs: Glyphs) -> Line {
        let opened = |cells: usize| {
            Line::new()
                .push(
                    Tone::of(Mark::NeedsYou),
                    glyphs.mark(Mark::NeedsYou).to_string(),
                )
                .plain(" ")
                .then(question.cut(cells, glyphs))
        };
        let needs = cell_width(&question.text());
        let beside = self
            .edge
            .as_deref()
            .map(|edge| {
                (
                    edge,
                    room.saturating_sub(MARKED + HEAD_GAP + cell_width(edge)),
                )
            })
            .filter(|(_, cells)| *cells >= LEAST_QUESTION.min(needs));
        match beside {
            Some((edge, cells)) => at_ends(
                opened(cells),
                Some(Line::new().push(Tone::Muted, edge)),
                room,
            ),
            None => opened(room.saturating_sub(MARKED)),
        }
    }

    /// What stands under the question, each part wrapped in `room` cells, in at most
    /// `rows` rows: where more stands past them, the last row says what follows it too,
    /// cut at the line.
    fn reason(&self, room: usize, rows: usize, glyphs: Glyphs) -> Vec<Line> {
        let mut wrapped: Vec<Line> = self
            .about
            .iter()
            .filter(|about| !about.is_empty())
            .flat_map(|about| Line::new().plain(about.as_str()).wrap(room))
            .collect();
        if wrapped.len() <= rows {
            return wrapped;
        }
        let Some(last) = rows.checked_sub(1) else {
            return Vec::new();
        };
        let rest: Vec<String> = wrapped.drain(last..).map(|line| line.text()).collect();
        wrapped.push(Line::new().plain(rest.join(" ")).cut(room, glyphs));
        wrapped
    }
}

/// Puts `edge` at the far end of the question's first row when both fit in `room`,
/// padded so it ends at `room`. Otherwise it is a muted row of its own, last in `head`,
/// cut in the middle when it is wider than the line.
fn place_edge(head: &mut Vec<Line>, edge: &str, room: usize, glyphs: Glyphs) {
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
        false => head.push(Line::new().push(Tone::Muted, cut_to(edge, room, glyphs))),
    }
}

/// What a card asks of the words an option is said with, the row they are typed on, and
/// why the words given were not taken.
struct Words<'c> {
    asks: &'c str,
    typed: &'c TypedRow,
    refused: Option<&'c str>,
}

/// Every option, the pointer on the one the words are said with, then the box they are typed
/// in, what is asked of them faint in it while nothing is typed, and why the words given were
/// not taken, holding what `keeping` says in the `left` rows the rest leaves; with where the
/// cursor stands among those rows. Short of rows the line being typed never gives way: what
/// each option says gives way past its row first, then the list counts what it leaves out.
fn saying(
    choices: &Choices,
    words: &Words<'_>,
    (left, keeping): (usize, Keeping),
    look: &Look,
    room: usize,
) -> (Vec<Line>, Option<(usize, usize)>) {
    let boxed = words
        .typed
        .drawn(Card::typed_room(look), look.glyphs, Some(words.asks));
    let refusal = words
        .refused
        .map(|refused| marked(Mark::Failed, Line::new().plain(refused), room, look.glyphs))
        .unwrap_or_default();
    let mut lines = match keeping {
        Keeping::OneRowEach | Keeping::TheQuestion => {
            let under = boxed.lines.len() + refusal.len();
            choices.fitted(look, room, left.saturating_sub(under), Said::Cut)
        }
        Keeping::Everything | Keeping::Spare | Keeping::TheRound | Keeping::TheRoundAlone => {
            choices.lines(look, room, usize::MAX, Said::Wrapped)
        }
    };
    let field_at = lines.len();
    let cursor = boxed.cursor.map(|(row, cell)| (field_at + row, cell));
    lines.extend(boxed.lines);
    lines.extend(refusal);
    (lines, cursor)
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

mod foot;
pub(super) mod options;
use options::{cut_to, Said};

mod round;
pub use round::said_whole;

#[cfg(test)]
pub(super) mod tests;
