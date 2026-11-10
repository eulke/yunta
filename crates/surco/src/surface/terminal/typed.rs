//! The row words are typed on, at a terminal: as much of them around the cursor as
//! fits its cells, so the end of a line being typed stays in sight and the cursor
//! is never drawn off the row.

use crate::components::TypedRow;
use crate::width::cell_width;

impl TypedRow {
    /// What a row of `room` cells shows of the words being typed, split at the
    /// cursor: the characters before it and the ones after it.
    ///
    /// The row is filled backwards from the cursor first and forwards with what is
    /// left, so the end of a line being typed stays in sight and the cursor is never
    /// drawn off the row.
    pub fn fitted(&self, room: usize) -> (String, String) {
        let room = room.max(1);
        let (behind, taken) = fitting(self.before.chars().rev(), room);
        let (ahead, _) = fitting(self.after.chars(), room.saturating_sub(taken));
        (
            behind.into_iter().rev().collect(),
            ahead.into_iter().collect(),
        )
    }
}

/// The longest run of `characters` that fits `room` cells, taken in the order they
/// arrive, and the cells it takes.
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

#[cfg(test)]
mod tests;
