//! A terminal's screen as a person reads it: every row as the terminal holds it once a
//! drawing is read the way a terminal reads one, each drawing taken whole, and what was
//! drawn with its paint dropped, since a color changes how a word looks and never which
//! word it is.

use surco::paint::{SYNC_BEGIN, SYNC_END};

/// A terminal's screen at one moment, as a person reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screen {
    /// Every row, with what it ends in spaces gone.
    pub rows: Vec<String>,
    /// The row and the column the cursor stands on.
    pub cursor: (u16, u16),
    pub cursor_hidden: bool,
    /// Whether the terminal is on its other screen.
    pub alternate: bool,
    /// Whether the terminal marks what is pasted into it.
    pub bracketed_paste: bool,
    /// Whether the terminal reports the mouse to the run, rather than selecting with it.
    pub mouse_captured: bool,
}

impl Screen {
    /// The screen of a terminal `columns` by `rows` once it read `drawn`: a drawing still
    /// on its way is out of sight, as a terminal that takes one whole keeps it.
    pub(crate) fn of(drawn: &str, (columns, rows): (u16, u16)) -> Screen {
        let mut parser = vt100::Parser::new(rows, columns, 0);
        parser.process(drawn_whole(drawn).as_bytes());
        let screen = parser.screen();
        Screen {
            rows: screen
                .rows(0, columns)
                .map(|row| row.trim_end().to_string())
                .collect(),
            cursor: screen.cursor_position(),
            cursor_hidden: screen.hide_cursor(),
            alternate: screen.alternate_screen(),
            bracketed_paste: screen.bracketed_paste(),
            mouse_captured: screen.mouse_protocol_mode() != vt100::MouseProtocolMode::None,
        }
    }

    /// Whether any row holds `needle`.
    pub fn holds(&self, needle: &str) -> bool {
        self.rows.iter().any(|row| row.contains(needle))
    }

    /// How many rows hold `needle`.
    pub fn count(&self, needle: &str) -> usize {
        self.rows.iter().filter(|row| row.contains(needle)).count()
    }
}

/// `drawn` without a drawing that has opened and not ended: what a terminal that takes
/// each drawing whole has shown so far.
fn drawn_whole(drawn: &str) -> &str {
    match (drawn.rfind(SYNC_BEGIN), drawn.rfind(SYNC_END)) {
        (Some(begin), Some(end)) if end > begin => drawn,
        (Some(begin), _) => &drawn[..begin],
        _ => drawn,
    }
}

/// `text` without the sequences that paint it (`ESC [ … m`).
pub(super) fn unpainted(text: &str) -> String {
    const INTRODUCER: &str = "\u{1b}[";
    let mut kept = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(INTRODUCER) {
        kept.push_str(&rest[..at]);
        let after = &rest[at + INTRODUCER.len()..];
        let parameters = after
            .find(|character: char| !(character.is_ascii_digit() || character == ';'))
            .unwrap_or(after.len());
        rest = match after[parameters..].strip_prefix('m') {
            Some(painted) => painted,
            None => {
                kept.push_str(INTRODUCER);
                after
            }
        };
    }
    kept.push_str(rest);
    kept
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::CLEAR_ROW;

    #[test]
    fn paint_is_dropped_and_what_moves_the_cursor_is_kept() {
        let drawn =
            format!("\u{1b}[1mdecision\u{1b}[0m: \u{1b}[35mneeds you\u{1b}[0m{CLEAR_ROW}\u{1b}[2A");
        assert_eq!(
            unpainted(&drawn),
            format!("decision: needs you{CLEAR_ROW}\u{1b}[2A")
        );
    }
}
