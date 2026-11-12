/// What one keystroke asks of the line being typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stroke {
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
    /// The page above, for a reader that moves by pages.
    PageUp,
    /// The page below, the way PageUp is the one above.
    PageDown,
    /// Escape: this prompt is declined.
    Decline,
    /// Ctrl-C: the run is to be stopped.
    Interrupt,
    Up,
    Down,
    /// The next row of a list, the way Down is.
    Tab,
    /// The row before, the way Up is.
    BackTab,
    /// Nothing the line acts on.
    Ignored,
}

impl Stroke {
    /// Whether the stroke adds text to the line being typed: a character, or a line
    /// break inside pasted text.
    pub fn types(self) -> bool {
        matches!(self, Stroke::Insert(_) | Stroke::PastedBreak)
    }
}
