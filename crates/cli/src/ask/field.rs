//! The line a person types an answer on.
//!
//! The editing itself is a function of the line and one stroke and
//! nothing else, so what a key does to an answer is settled here rather
//! than at a terminal: an arrow moves the cursor instead of leaving its
//! own characters in the text, and a line break that arrived inside a
//! paste folds into the line instead of ending it.
//!
//! What the terminal shows is one row, whatever the line grows to. A
//! redraw clears the row the cursor is on and a cursor move stays on it,
//! so a line drawn wider than the row would leave every earlier draw
//! standing on the rows below it and put the cursor out of reach of its
//! own text. The line scrolls under the row instead: [`Field::row`] is
//! the part of it the row has space for, taken around the cursor.

use yunta_core::text::one_line;

use super::keys::{Stroke, Strokes, PASTE_OFF, PASTE_ON};
use super::{Answered, Console, NoAnswer};
use crate::error::warn;
use crate::render::cell_width;

/// Said once, when the answer being recorded came from text that
/// spanned more than one line: a question takes one line, and the
/// difference belongs to whoever pasted it rather than to a silent
/// truncation.
const FOLDED: &str = "the text pasted here spanned more than one line, \
                      and this answer is those lines joined into one";

/// A finished answer.
pub(crate) struct Typed {
    /// The line, with every run of whitespace collapsed to one space.
    pub(crate) value: String,
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
    /// The line as it is drawn — one line, whatever it holds.
    pub(crate) fn drawn(&self) -> String {
        self.characters
            .iter()
            .map(|character| if *character == '\n' { ' ' } else { *character })
            .collect()
    }

    /// What a row of `room` cells shows of the line, and how many of
    /// those cells come after the cursor.
    ///
    /// The row is filled backwards from the cursor first and forwards
    /// with what is left, so the end of a line being typed stays in
    /// sight and the cursor is never drawn off the row.
    pub(crate) fn row(&self, room: usize) -> (String, usize) {
        let room = room.max(1);
        let drawn = self.drawn();
        let at = self.cursor.min(drawn.chars().count());
        let head: Vec<char> = drawn.chars().take(at).collect();
        let (behind, taken) = fitting(head.into_iter().rev(), room);
        let (ahead, after) = fitting(drawn.chars().skip(at), room.saturating_sub(taken));
        (behind.into_iter().rev().chain(ahead).collect(), after)
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
            Stroke::Backspace | Stroke::Delete | Stroke::Ignored => {}
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
        let typed: String = self.characters.iter().collect();
        Typed {
            value: one_line(&typed),
            folded: typed.lines().filter(|line| !line.trim().is_empty()).count() > 1,
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

/// Reads one answer, drawn after `prompt` and edited in place.
///
/// Returns when the person presses Enter: the line stays on screen as
/// what was recorded, which is the collapsed value rather than the
/// keystrokes it came from. Escape ends the prompt with no answer and
/// Ctrl-C stops the run.
pub(crate) fn ask_line(console: &Console, prompt: &str) -> Answered<Typed> {
    let marked = Marked::on(console)?;
    let mut field = Field::default();
    let mut strokes = Strokes::default();
    let typed = loop {
        // Read every draw: the terminal a run is answered on can be
        // resized while the answer is being typed.
        let room = console.width().saturating_sub(cell_width(prompt) + 1);
        let (row, back) = field.row(room);
        console.draw_line(prompt, &row, back)?;
        match field.apply(strokes.read(console.read_key()?)) {
            Edit::Editing => {}
            Edit::Answered(typed) => break typed,
            Edit::Declined => return Err(closed(console, NoAnswer::Declined)),
            Edit::Interrupted => {
                console.interrupt();
                return Err(closed(console, NoAnswer::Interrupted));
            }
        }
    };
    drop(marked);
    console.end_line(&format!("{prompt}{}", typed.value))?;
    if typed.folded {
        console.say(FOLDED)?;
    }
    Ok(typed)
}

/// Ends the line the abandoned prompt was drawing on and hands back
/// `why`, so whatever is said about the run next starts on a line of
/// its own instead of running into a half-typed answer.
fn closed(console: &Console, why: NoAnswer) -> NoAnswer {
    match console.say("") {
        Ok(()) => why,
        Err(failed) => NoAnswer::Unreadable(failed),
    }
}

/// The terminal's paste marking, held for one line.
///
/// Requested when the line opens and withdrawn when it closes on every
/// path out — answered, declined, interrupted or unreadable — so a mode
/// this process asked for never outlives the prompt that needed it.
struct Marked<'a> {
    console: &'a Console,
}

impl<'a> Marked<'a> {
    fn on(console: &'a Console) -> std::io::Result<Self> {
        console.term().write_str(PASTE_ON)?;
        Ok(Self { console })
    }
}

impl Drop for Marked<'_> {
    fn drop(&mut self) {
        if let Err(e) = self.console.term().write_str(PASTE_OFF) {
            warn(format!(
                "could not restore the terminal's paste marking: {e}"
            ));
        }
    }
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
        assert_eq!(field.row(ROOM).1, 0);
    }

    #[test]
    fn the_cursor_sits_in_cells_even_where_a_character_takes_two() {
        let mut field = Field::default();
        for stroke in insert("実装") {
            field.apply(stroke);
        }
        field.apply(Stroke::Home);
        assert_eq!(field.row(ROOM).1, 4);
    }

    #[test]
    fn a_line_wider_than_the_row_is_drawn_as_the_part_of_it_around_the_cursor() {
        // A row that redrew a line wider than itself would wrap it onto
        // rows the next redraw does not clear, leaving every earlier
        // draw standing underneath.
        const ROW: usize = 8;
        let mut field = Field::default();
        for stroke in insert("abcdefghijkl") {
            field.apply(stroke);
        }
        let (drawn, after) = field.row(ROW);
        assert_eq!(drawn, "efghijkl", "the end of the line stays in sight");
        assert_eq!(after, 0, "the cursor sits where the typing is");

        field.apply(Stroke::Home);
        let (drawn, after) = field.row(ROW);
        assert_eq!(drawn, "abcdefgh", "the row follows the cursor back");
        assert_eq!(after, ROW, "the cursor sits before everything drawn");
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
        let (drawn, after) = field.row(ROW);
        assert_eq!(
            drawn, "ュー",
            "a character two cells wide is never cut in half"
        );
        assert_eq!(cell_width(&drawn), 4);
        assert_eq!(after, 0);
    }
}
