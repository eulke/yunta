//! What a keystroke means to the line a person is typing.
//!
//! Two readings a terminal does not hand over on its own are made here.
//!
//! The first is a pasted line break. A terminal delivers pasted text as
//! if it had been typed, so a line break inside it is the same byte as
//! the Enter that ends an answer — which is how one paste of two lines
//! answers two questions with nobody pressing anything between them.
//! [`PASTE_ON`] asks the terminal to mark the block it pastes; between
//! the markers a break is [`Stroke::PastedBreak`] and belongs to the
//! answer being typed, and only outside them does Enter end it. A
//! terminal that does not honor the request marks nothing, and a pasted
//! break arrives as the Enter it looks like.
//!
//! The second is everything a key sends that is not a character. An
//! arrow is an escape, a bracket and a letter; a line read whole keeps
//! all three, which is how an arrow pressed mid-answer ends up inside
//! the stored answer. Here a sequence is followed to the character that
//! closes it and named — a cursor move when it is one, nothing when it
//! is not.

use dialoguer::console::Key;

/// Asks the terminal to mark pasted text (`DECSET 2004`): what arrives
/// between the two markers was pasted rather than typed.
pub(crate) const PASTE_ON: &str = "\x1b[?2004h";

/// Withdraws the request, leaving the terminal in the mode it was
/// handed over in.
pub(crate) const PASTE_OFF: &str = "\x1b[?2004l";

/// The parameters of the sequence a terminal opens a paste with.
const PASTE_BEGIN: &str = "200";

/// The parameters of the sequence it closes one with.
const PASTE_END: &str = "201";

/// The most parameter characters one sequence is followed through. A
/// mouse report or a device answer carries more than anything named
/// here, and giving up on the scan lets its tail through as typing
/// rather than reading the terminal's own traffic as parameters
/// forever.
const PARAMETER_LIMIT: usize = 16;

/// What one keystroke asks of the line being typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stroke {
    /// A character at the cursor.
    Insert(char),
    /// The answer is finished.
    Enter,
    /// A line break inside pasted text: the paste continues and the
    /// answer stays one line.
    PastedBreak,
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
    /// Escape: this prompt is declined.
    Decline,
    /// Ctrl-C: the run is to be stopped.
    Interrupt,
    /// Nothing the line acts on.
    Ignored,
}

/// The reading of the key stream: the escape sequence being followed,
/// if any, and whether the terminal has said it is pasting.
#[derive(Debug, Default)]
pub(crate) struct Strokes {
    /// The parameters of the sequence under way, present from its
    /// introducer until the character that closes it.
    parameters: Option<String>,
    pasting: bool,
}

impl Strokes {
    /// What `key` asks of the line.
    pub(crate) fn read(&mut self, key: Key) -> Stroke {
        if self.parameters.is_some() {
            match key {
                Key::Char(character) => {
                    self.scan(character);
                    return Stroke::Ignored;
                }
                // Anything else is not the terminal finishing that
                // sequence: it is abandoned and the key stands alone.
                _ => self.parameters = None,
            }
        }
        match key {
            // An escape sequence `console` has no name for arrives as
            // its own characters, introducer first.
            Key::UnknownEscSeq(characters) => self.open_sequence(characters),
            Key::Enter if self.pasting => Stroke::PastedBreak,
            Key::Enter => Stroke::Enter,
            Key::Escape if self.pasting => Stroke::Ignored,
            Key::Escape => Stroke::Decline,
            // The read holds the terminal in raw mode, where the line
            // discipline's signal keys are off and Ctrl-C is a byte like
            // any other: `console` recognizes it there and reports it as
            // this key rather than as text.
            Key::CtrlC => Stroke::Interrupt,
            Key::Backspace => Stroke::Backspace,
            Key::Del => Stroke::Delete,
            Key::ArrowLeft => Stroke::Left,
            Key::ArrowRight => Stroke::Right,
            Key::Home => Stroke::Home,
            Key::End => Stroke::End,
            // A tab inside pasted text is a space — the single cell
            // every run of whitespace in a one-line answer becomes.
            Key::Tab if self.pasting => Stroke::Insert(' '),
            Key::Char(character) if !character.is_control() => Stroke::Insert(character),
            _ => Stroke::Ignored,
        }
    }

    /// Starts following the sequence `characters` opens, feeding it
    /// whatever of the sequence arrived with the introducer.
    ///
    /// An escape followed by anything but `[` introduces no sequence
    /// this line reads, and types nothing either.
    fn open_sequence(&mut self, characters: Vec<char>) -> Stroke {
        let mut characters = characters.into_iter();
        if characters.next() != Some('[') {
            return Stroke::Ignored;
        }
        self.parameters = Some(String::new());
        for character in characters {
            if self.parameters.is_some() {
                self.scan(character);
            }
        }
        Stroke::Ignored
    }

    /// Takes `character` as part of the sequence under way.
    ///
    /// A sequence runs until a character in the final range closes it,
    /// and everything before that is its parameters. Two of them mean
    /// something to a line being typed — the paste markers — and every
    /// other one is consumed whole, which is what keeps its characters
    /// out of the answer.
    fn scan(&mut self, character: char) {
        let Some(mut parameters) = self.parameters.take() else {
            return;
        };
        if is_final(character) {
            match parameters.as_str() {
                PASTE_BEGIN => self.pasting = true,
                PASTE_END => self.pasting = false,
                _ => {}
            }
            return;
        }
        if parameters.len() < PARAMETER_LIMIT {
            parameters.push(character);
            self.parameters = Some(parameters);
        }
    }
}

/// Whether `character` closes an escape sequence: `ECMA-48` reserves
/// this range for the character that names one, everything below it
/// being parameters.
fn is_final(character: char) -> bool {
    ('\u{40}'..='\u{7e}').contains(&character)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One escape sequence as `console` reports it: the introducer with
    /// the first parameters, then every remaining character on its own.
    fn sequence(parameters: &str, last: char) -> Vec<Key> {
        let mut rest: Vec<char> = parameters.chars().collect();
        let mut introducer = vec!['['];
        introducer.extend(rest.drain(..rest.len().min(2)));
        let mut keys = vec![Key::UnknownEscSeq(introducer)];
        keys.extend(rest.into_iter().map(Key::Char));
        keys.push(Key::Char(last));
        keys
    }

    fn strokes(keys: Vec<Key>) -> Vec<Stroke> {
        let mut reading = Strokes::default();
        keys.into_iter().map(|key| reading.read(key)).collect()
    }

    #[test]
    fn a_line_break_between_the_paste_markers_never_ends_the_answer() {
        let mut keys = sequence(PASTE_BEGIN, '~');
        keys.push(Key::Char('a'));
        keys.push(Key::Enter);
        keys.push(Key::Char('b'));
        keys.extend(sequence(PASTE_END, '~'));
        keys.push(Key::Enter);
        let read = strokes(keys);
        assert!(
            read.contains(&Stroke::PastedBreak),
            "the break inside the paste was read as something else: {read:?}"
        );
        assert_eq!(
            read.iter()
                .filter(|stroke| **stroke == Stroke::Enter)
                .count(),
            1,
            "only the key pressed after the paste ends the answer: {read:?}"
        );
    }

    #[test]
    fn an_escape_sequence_reaches_the_line_as_nothing_it_can_type() {
        for stroke in strokes(sequence("1", '~')) {
            assert_eq!(stroke, Stroke::Ignored);
        }
        // Parameters that open with the digits a paste marker opens
        // with are not a paste marker.
        for stroke in strokes(sequence("20", '~')) {
            assert_eq!(stroke, Stroke::Ignored);
        }
        // Neither is a sequence closed by a letter.
        for stroke in strokes(sequence("1;5", 'C')) {
            assert_eq!(stroke, Stroke::Ignored);
        }
    }

    #[test]
    fn a_key_that_is_not_a_character_abandons_a_sequence_and_stands_alone() {
        let read = strokes(vec![Key::UnknownEscSeq(vec!['[', '2']), Key::Enter]);
        assert_eq!(read.last(), Some(&Stroke::Enter));
    }

    #[test]
    fn escape_declines_and_ctrl_c_interrupts() {
        assert_eq!(strokes(vec![Key::Escape]), vec![Stroke::Decline]);
        assert_eq!(strokes(vec![Key::CtrlC]), vec![Stroke::Interrupt]);
    }

    #[test]
    fn an_arrow_key_moves_the_cursor_and_types_nothing() {
        assert_eq!(
            strokes(vec![Key::ArrowLeft, Key::ArrowRight]),
            vec![Stroke::Left, Stroke::Right]
        );
    }
}
