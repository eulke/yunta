//! The line a person types an answer on.
//!
//! The editing itself is a function of the line and one stroke and
//! nothing else, so what a key does to an answer is settled here rather
//! than at a terminal: an arrow moves the cursor instead of leaving its
//! own characters in the text, and a line break that arrived inside a
//! paste folds into the line instead of ending it.
//!
//! What the terminal shows is one row, whatever the line grows to: the
//! line scrolls under the row rather than wrapping below it, so the
//! cursor stays on the text it edits. [`Field::parts`] is the part of it
//! the row has space for, taken around the cursor.

use surco::line::Line;
use surco::look::Look;
use surco::surface::Placed;
use surco::text::one_line;
use surco::width::cell_width;

use super::held::{hold, Held, Press};
use super::keys::Stroke;
use super::{Console, NoAnswer, ANSWER};

/// Said once, when the answer being recorded came from text that
/// spanned more than one line: a question takes one line, and the
/// difference belongs to whoever pasted it rather than to a silent
/// truncation.
pub(super) const FOLDED: &str = "the text pasted here spanned more than one line, \
                      and this answer is those lines joined into one";

/// A finished answer.
pub(crate) struct Typed {
    /// The line, with every run of whitespace collapsed to one space.
    pub(crate) value: String,
    /// The line as it was typed, with the line breaks a paste brought: what a person
    /// comes back to, so pressing enter on it again says the same of it.
    pub(crate) line: String,
    /// Whether folding it took line breaks out of pasted text.
    pub(crate) folded: bool,
}

/// The line being typed: its characters, and where the cursor sits
/// among them.
///
/// A line break kept inside a paste is one of those characters, so what
/// was pasted is still recoverable when the answer is assembled; it is
/// drawn as the space it becomes.
#[derive(Debug, Default)]
pub(crate) struct Field {
    characters: Vec<char>,
    cursor: usize,
}

/// What one stroke did to the line.
pub(crate) enum Edit {
    /// The line took it; nothing is decided yet.
    Editing,
    Answered(Typed),
    Declined,
    Interrupted,
}

impl Field {
    /// A line that already holds `text`, line breaks included, with the cursor at its
    /// end: an answer a person came back to.
    pub(super) fn holding(text: &str) -> Self {
        let characters: Vec<char> = text.chars().collect();
        let cursor = characters.len();
        Self { characters, cursor }
    }

    /// The line as it is drawn — one line, whatever it holds.
    pub(crate) fn drawn(&self) -> String {
        self.characters
            .iter()
            .map(|character| if *character == '\n' { ' ' } else { *character })
            .collect()
    }

    /// What a row of `room` cells shows of the line, split at the
    /// cursor: the characters before it and the ones after it.
    ///
    /// The row is filled backwards from the cursor first and forwards
    /// with what is left, so the end of a line being typed stays in
    /// sight and the cursor is never drawn off the row.
    pub(crate) fn parts(&self, room: usize) -> (String, String) {
        let room = room.max(1);
        let drawn = self.drawn();
        let at = self.cursor.min(drawn.chars().count());
        let head: Vec<char> = drawn.chars().take(at).collect();
        let (behind, taken) = fitting(head.into_iter().rev(), room);
        let (ahead, _) = fitting(drawn.chars().skip(at), room.saturating_sub(taken));
        (
            behind.into_iter().rev().collect(),
            ahead.into_iter().collect(),
        )
    }

    /// What `stroke` asks of the line, applied.
    pub(crate) fn apply(&mut self, stroke: Stroke) -> Edit {
        let end = self.characters.len();
        let at = self.cursor.min(end);
        match stroke {
            Stroke::Insert(character) => self.insert(character),
            Stroke::PastedBreak => self.insert('\n'),
            Stroke::Backspace if at > 0 => {
                self.characters.remove(at - 1);
                self.cursor = at - 1;
            }
            Stroke::Delete if at < end => {
                self.characters.remove(at);
            }
            Stroke::Left => self.cursor = at.saturating_sub(1),
            Stroke::Right => self.cursor = at.saturating_add(1).min(end),
            Stroke::Home => self.cursor = 0,
            Stroke::End => self.cursor = end,
            Stroke::Enter => return Edit::Answered(self.finish()),
            Stroke::Decline => return Edit::Declined,
            Stroke::Interrupt => return Edit::Interrupted,
            Stroke::Backspace
            | Stroke::Delete
            | Stroke::Up
            | Stroke::Down
            | Stroke::Tab
            | Stroke::BackTab
            | Stroke::Ignored => {}
        }
        Edit::Editing
    }

    fn insert(&mut self, character: char) {
        let at = self.cursor.min(self.characters.len());
        self.characters.insert(at, character);
        self.cursor = at + 1;
    }

    /// The answer the line holds.
    fn finish(&self) -> Typed {
        let line: String = self.characters.iter().collect();
        Typed {
            value: one_line(&line),
            folded: line.lines().filter(|row| !row.trim().is_empty()).count() > 1,
            line,
        }
    }
}

/// The longest run of `characters` that fits `room` display cells,
/// taken in the order they arrive, and the cells it takes.
fn fitting(characters: impl Iterator<Item = char>, room: usize) -> (Vec<char>, usize) {
    let mut taken = 0;
    let mut kept = Vec::new();
    for character in characters {
        let mut buffer = [0u8; 4];
        let width = cell_width(character.encode_utf8(&mut buffer));
        if taken + width > room {
            break;
        }
        taken += width;
        kept.push(character);
    }
    (kept, taken)
}

/// A line being answered, under `asked`.
struct LineAsked<'a> {
    asked: &'a Line,
    field: Field,
}

impl Held for LineAsked<'_> {
    type Answer = Typed;

    /// The question, and the row the words are typed on, with the cursor on it. Read
    /// every draw: the terminal can be resized while the answer is being typed.
    fn drawn(&self, look: &Look, _rows: usize) -> Placed {
        let room = look.width.cells().saturating_sub(cell_width(ANSWER) + 1);
        let (before, after) = self.field.parts(room);
        let cursor = (1, cell_width(ANSWER) + cell_width(&before));
        Placed {
            lines: vec![
                self.asked.clone(),
                Line::new().plain(ANSWER).plain(before).plain(after),
            ],
            cursor: Some(cursor),
        }
    }

    /// Enter answers with the line, whatever it holds; every other stroke edits it.
    fn press(&mut self, stroke: Stroke) -> Press<Typed> {
        match self.field.apply(stroke) {
            Edit::Editing => Press::Stay,
            Edit::Answered(typed) => Press::Answer(typed),
            Edit::Declined => Press::Decline,
            Edit::Interrupted => Press::Interrupt,
        }
    }

    /// Every key is a key of the line.
    fn typing(&self) -> bool {
        true
    }
}

/// Asks for one line under `asked`, with `kept` standing where no answer is given: a line
/// left empty keeps it, and so does Escape or a terminal that cannot be drawn on.
///
/// What stays on the screen is the question and the value that stands, which is the
/// collapsed line rather than the keystrokes it came from. `None` where Ctrl-C stopped
/// what is running: nothing of the prompt stays, and no value stands.
pub(crate) fn ask_line(console: &Console, asked: &Line, kept: &str) -> Option<String> {
    let mut prompt = LineAsked {
        asked,
        field: Field::default(),
    };
    let typed = match hold(console, &mut prompt) {
        Ok(typed) => Some(typed),
        Err(NoAnswer::Interrupted) => return None,
        Err(_) => None,
    };
    let folded = typed.as_ref().is_some_and(|typed| typed.folded);
    let stands = typed
        .map(|typed| typed.value)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| kept.to_string());
    let said = [
        console.look().paint.line(asked),
        format!("{ANSWER}{stands}"),
    ];
    for line in said
        .iter()
        .map(String::as_str)
        .chain(folded.then_some(FOLDED))
    {
        // best-effort: the value stands whether or not it could be said.
        console.say(line).ok();
    }
    Some(stands)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A row with room for anything these tests type on it.
    const ROOM: usize = 40;

    fn typed(strokes: &[Stroke]) -> Typed {
        let mut field = Field::default();
        for stroke in strokes {
            if let Edit::Answered(answer) = field.apply(*stroke) {
                return answer;
            }
        }
        field.finish()
    }

    fn insert(text: &str) -> Vec<Stroke> {
        text.chars().map(Stroke::Insert).collect()
    }

    #[test]
    fn a_pasted_line_break_joins_the_lines_instead_of_answering() {
        let mut strokes = insert("alpha");
        strokes.push(Stroke::PastedBreak);
        strokes.extend(insert("beta"));
        strokes.push(Stroke::Enter);
        let answer = typed(&strokes);
        assert_eq!(answer.value, "alpha beta");
        assert!(answer.folded, "the answer came from two lines and says so");
    }

    #[test]
    fn a_break_that_ends_a_single_pasted_line_folds_nothing() {
        let mut strokes = insert("alpha");
        strokes.push(Stroke::PastedBreak);
        strokes.push(Stroke::Enter);
        let answer = typed(&strokes);
        assert_eq!(answer.value, "alpha");
        assert!(!answer.folded, "one pasted line is one line");
    }

    #[test]
    fn a_cursor_move_leaves_nothing_of_itself_in_the_answer() {
        let mut strokes = insert("ac");
        strokes.push(Stroke::Left);
        strokes.extend(insert("b"));
        strokes.push(Stroke::Home);
        strokes.push(Stroke::End);
        strokes.push(Stroke::Enter);
        assert_eq!(typed(&strokes).value, "abc");
    }

    #[test]
    fn backspace_and_delete_act_on_the_character_beside_the_cursor() {
        let mut strokes = insert("abXc");
        strokes.push(Stroke::Left);
        strokes.push(Stroke::Backspace);
        strokes.push(Stroke::Enter);
        assert_eq!(typed(&strokes).value, "abc");

        let mut strokes = insert("abXc");
        strokes.push(Stroke::Home);
        strokes.push(Stroke::Delete);
        strokes.push(Stroke::Enter);
        assert_eq!(typed(&strokes).value, "bXc");
    }

    #[test]
    fn an_edit_at_either_end_of_an_empty_line_is_no_edit() {
        let mut field = Field::default();
        for stroke in [
            Stroke::Backspace,
            Stroke::Delete,
            Stroke::Left,
            Stroke::Right,
            Stroke::Home,
            Stroke::End,
        ] {
            field.apply(stroke);
        }
        assert_eq!(field.finish().value, "");
        let (_, after) = field.parts(ROOM);
        assert_eq!(cell_width(&after), 0);
    }

    #[test]
    fn the_cursor_sits_in_cells_even_where_a_character_takes_two() {
        let mut field = Field::default();
        for stroke in insert("実装") {
            field.apply(stroke);
        }
        field.apply(Stroke::Home);
        let (_, after) = field.parts(ROOM);
        assert_eq!(cell_width(&after), 4);
    }

    #[test]
    fn a_line_wider_than_the_row_is_drawn_as_the_part_of_it_around_the_cursor() {
        // A line wider than its row would wrap below it, and the cursor
        // would be on another row than the text it edits.
        const ROW: usize = 8;
        let mut field = Field::default();
        for stroke in insert("abcdefghijkl") {
            field.apply(stroke);
        }
        let (before, after) = field.parts(ROW);
        let drawn = format!("{before}{after}");
        assert_eq!(drawn, "efghijkl", "the end of the line stays in sight");
        assert_eq!(cell_width(&after), 0, "the cursor sits where the typing is");

        field.apply(Stroke::Home);
        let (before, after) = field.parts(ROW);
        let drawn = format!("{before}{after}");
        assert_eq!(drawn, "abcdefgh", "the row follows the cursor back");
        assert_eq!(
            cell_width(&after),
            ROW,
            "the cursor sits before everything drawn"
        );
    }

    #[test]
    fn a_row_holds_whole_characters_however_many_cells_they_take() {
        // Five cells hold two characters of two cells and leave one
        // cell nothing can go in.
        const ROW: usize = 5;
        let mut field = Field::default();
        for stroke in insert("実装レビュー") {
            field.apply(stroke);
        }
        let (before, after) = field.parts(ROW);
        let drawn = format!("{before}{after}");
        assert_eq!(
            drawn, "ュー",
            "a character two cells wide is never cut in half"
        );
        assert_eq!(cell_width(&drawn), 4);
        assert_eq!(cell_width(&after), 0);
    }
}
