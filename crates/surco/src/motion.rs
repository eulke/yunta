//! Whether a live view moves, and what it shows when it does. Only what is
//! evidence moves: a frame of a spin stands for an event that arrived, so a spin
//! that stops says something, and no clock turns it.

use crate::glyphs::Glyphs;

/// The word that keeps a view still, as a reader sets it in `YUNTA_MOTION`.
const OFF: &str = "off";

/// Whether a live view moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Motion {
    /// A running node's mark turns as its events arrive.
    #[default]
    Moves,
    /// Every mark stands still; every word stays.
    Still,
}

impl Motion {
    /// The motion a reader asked for, from the value of `YUNTA_MOTION`: `off`
    /// keeps it still, and anything else, or nothing, moves.
    pub fn of(asked: Option<&str>) -> Self {
        match asked.map(str::trim) {
            Some(value) if value.eq_ignore_ascii_case(OFF) => Self::Still,
            _ => Self::Moves,
        }
    }
}

/// The frame `events` events bring a spin to.
pub fn frame(glyphs: Glyphs, events: u64) -> &'static str {
    let frames = glyphs.spin();
    let at = usize::try_from(events).unwrap_or(usize::MAX) % frames.len().max(1);
    frames.get(at).copied().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_off_keeps_a_view_still() {
        assert_eq!(Motion::of(None), Motion::Moves);
        assert_eq!(Motion::of(Some("on")), Motion::Moves);
        assert_eq!(Motion::of(Some("")), Motion::Moves);
        assert_eq!(Motion::of(Some("off")), Motion::Still);
        assert_eq!(Motion::of(Some(" OFF ")), Motion::Still);
    }

    #[test]
    fn a_spin_goes_a_frame_per_event_and_comes_back_around() {
        assert_eq!(frame(Glyphs::Unicode, 0), "⠋");
        assert_eq!(frame(Glyphs::Unicode, 3), "⠸");
        assert_eq!(frame(Glyphs::Unicode, 10), "⠋");
        assert_eq!(frame(Glyphs::Ascii, 1), "/");
        assert_eq!(frame(Glyphs::Ascii, 3), "\\");
        assert_eq!(frame(Glyphs::Ascii, 4), "|");
    }
}
