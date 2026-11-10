//! The line a person types an answer on.
//!
//! The editing itself is a function of the line and one stroke and
//! nothing else, so what a key does to an answer is settled here rather
//! than at a terminal: an arrow moves the cursor instead of leaving its
//! own characters in the text, and a line break that arrived inside a
//! paste folds into the line instead of ending it.
//!
//! A surface shows the line split at the cursor, whole ([`TypedLine::split`]):
//! a terminal shows the part of it its row has space for, taken around the
//! cursor, so the cursor stays on the text it edits.

use surco::text::one_line;

use super::Stroke;

/// A finished answer.
pub struct Typed {
    /// The line, with every run of whitespace collapsed to one space.
    pub value: String,
    /// The line as it was typed, with the line breaks a paste brought: what a person
    /// comes back to, so pressing enter on it again says the same of it.
    pub line: String,
    /// Whether folding it took line breaks out of pasted text.
    pub folded: bool,
}

/// The line being typed: its characters, and where the cursor sits
/// among them.
///
/// A line break kept inside a paste is one of those characters, so what
/// was pasted is still recoverable when the answer is assembled; it is
/// drawn as the space it becomes.
#[derive(Debug, Default)]
pub struct TypedLine {
    characters: Vec<char>,
    cursor: usize,
}

/// What one stroke did to the line.
pub enum Edit {
    /// The line took it; nothing is decided yet.
    Editing,
    Answered(Typed),
    Declined,
    Interrupted,
}

impl TypedLine {
    /// A line that already holds `text`, line breaks included, with the cursor at its
    /// end: an answer a person came back to.
    pub fn holding(text: &str) -> Self {
        let characters: Vec<char> = text.chars().collect();
        let cursor = characters.len();
        Self { characters, cursor }
    }

    /// The line as it is drawn — one line, whatever it holds.
    pub fn drawn(&self) -> String {
        self.characters
            .iter()
            .map(|character| if *character == '\n' { ' ' } else { *character })
            .collect()
    }

    /// The line as it is drawn, split at the cursor: the characters before
    /// it and the ones after it, whole. A surface shows what fits.
    pub fn split(&self) -> (String, String) {
        let drawn = self.drawn();
        let at = self.cursor.min(drawn.chars().count());
        (
            drawn.chars().take(at).collect(),
            drawn.chars().skip(at).collect(),
        )
    }

    /// What `stroke` asks of the line, applied.
    pub fn apply(&mut self, stroke: Stroke) -> Edit {
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
            | Stroke::PageUp
            | Stroke::PageDown
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

#[cfg(test)]
mod tests {
    use surco::width::cell_width;

    use super::*;

    fn typed(strokes: &[Stroke]) -> Typed {
        let mut field = TypedLine::default();
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
        let mut field = TypedLine::default();
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
        let (_, after) = field.split();
        assert_eq!(cell_width(&after), 0);
    }

    #[test]
    fn the_cursor_sits_in_cells_even_where_a_character_takes_two() {
        let mut field = TypedLine::default();
        for stroke in insert("実装") {
            field.apply(stroke);
        }
        field.apply(Stroke::Home);
        let (_, after) = field.split();
        assert_eq!(cell_width(&after), 4);
    }
}
