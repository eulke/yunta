//! See [`super`]. The strokes the walk hands every state it tries strokes on: every stroke
//! but typing, and typing each key a stage binds, with one no stage binds.

use yunta_ui::prompt::Stroke;

/// Each character a stage binds, and one no stage binds.
const KEYS: [char; 25] = [
    'q', 'j', 'k', ' ', 'r', 'c', 'x', 'a', 'd', 'f', 'w', 'p', 'n', 't', 'i', 'u', 'e', 'v', 'o',
    's', 'g', 'h', 'm', '1', 'z',
];

/// Every stroke but typing, and typing each key of [`KEYS`].
pub(super) fn strokes() -> Vec<Stroke> {
    let mut strokes = vec![
        Stroke::Enter,
        Stroke::PastedBreak,
        Stroke::Backspace,
        Stroke::Delete,
        Stroke::Left,
        Stroke::Right,
        Stroke::Home,
        Stroke::End,
        Stroke::PageUp,
        Stroke::PageDown,
        Stroke::Decline,
        Stroke::Interrupt,
        Stroke::Up,
        Stroke::Down,
        Stroke::Tab,
        Stroke::BackTab,
        Stroke::Ignored,
    ];
    strokes.extend(KEYS.map(Stroke::Insert));
    strokes
}
