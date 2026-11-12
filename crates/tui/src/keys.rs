//! What a key or a paste means to the application, by the table the CLI's key reader
//! keeps, so a key does the same in the application as at a prompt: a key press is its
//! [`Stroke`], a release is nothing, and Ctrl-Z is the terminal's own key, which suspends
//! the application rather than reaching it. A paste is its characters, each line break a
//! [`Stroke::PastedBreak`] and a tab a space, while a line is being typed, and nothing
//! otherwise: pasted text answers no question it was not typed into.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use yunta_ui::prompt::Stroke;

/// The stroke `key` gives the application; none for a release, nor for the terminal's
/// own Ctrl-Z.
pub(crate) fn stroke(key: KeyEvent) -> Option<Stroke> {
    if key.kind == KeyEventKind::Release || suspends(key) {
        return None;
    }
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    Some(match key.code {
        KeyCode::Char('c') if control => Stroke::Interrupt,
        KeyCode::Char(character) if !control && !character.is_control() => {
            Stroke::Insert(character)
        }
        KeyCode::Enter => Stroke::Enter,
        KeyCode::Esc => Stroke::Decline,
        KeyCode::Backspace => Stroke::Backspace,
        KeyCode::Delete => Stroke::Delete,
        KeyCode::Left => Stroke::Left,
        KeyCode::Right => Stroke::Right,
        KeyCode::Up => Stroke::Up,
        KeyCode::Down => Stroke::Down,
        KeyCode::Home => Stroke::Home,
        KeyCode::End => Stroke::End,
        KeyCode::PageUp => Stroke::PageUp,
        KeyCode::PageDown => Stroke::PageDown,
        KeyCode::Tab => Stroke::Tab,
        KeyCode::BackTab => Stroke::BackTab,
        _ => Stroke::Ignored,
    })
}

/// Whether `key` is Ctrl-Z pressed: the terminal's own key, which suspends the
/// application.
pub(crate) fn suspends(key: KeyEvent) -> bool {
    key.kind != KeyEventKind::Release
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && key.code == KeyCode::Char('z')
}

/// What `text`, pasted, gives the application: its characters while `typing`, each line
/// break one [`Stroke::PastedBreak`] and a tab a space, as the CLI's reader takes a paste;
/// nothing otherwise.
pub(crate) fn pasted(text: &str, typing: bool) -> Vec<Stroke> {
    if !typing {
        return Vec::new();
    }
    let unbroken = text.replace("\r\n", "\n").replace('\r', "\n");
    unbroken
        .chars()
        .filter_map(|character| match character {
            '\n' => Some(Stroke::PastedBreak),
            '\t' => Some(Stroke::Insert(' ')),
            control if control.is_control() => None,
            character => Some(Stroke::Insert(character)),
        })
        .collect()
}

#[cfg(test)]
mod tests;
