//! A row of words with one chosen, at a terminal: each word with a cell either side of it, the
//! room a pill takes though none is filled, and a cell between two of them, the first word's
//! room from the row's third cell; the pointer in the cell before the chosen word's room, two
//! before the word, so it stands a cell in from where a list's pointer moves. The chosen word
//! is strong and the rest muted; the pointer takes the tone of a pointer a person moves where
//! the keys act on the row, and is muted elsewhere. No word moves when the choice does: each
//! keeps the cell before its room whether the pointer stands there or not.
//!
//! A row too narrow for every word keeps the chosen one in sight: from the earliest word that
//! leaves the chosen one room, as many words as fit, an ellipsis standing on each side that
//! lost words, at the row's start for those before and where the next word would start for
//! those after. A chosen word wider than the row alone is cut at the row's end, its pointer
//! kept. Not a list's window, which keeps rows around its pointer and counts what it hides: a
//! row of words is one row, and says only that it hides some.

use super::Terminal;
use crate::components::WordChoice;
use crate::glyphs::Glyphs;
use crate::line::{Line, Tone};
use crate::width::cell_width;

impl Terminal {
    /// The row at this look's width: every word where they fit; else the words around the
    /// chosen one that fit, with an ellipsis on each side that lost words. No words, no row.
    pub fn word_choice(&self, choice: &WordChoice) -> Option<Line> {
        let count = choice.words.len();
        if count == 0 {
            return None;
        }
        let glyphs = self.look.glyphs;
        let cells = self.look.width.cells();
        let chosen = choice.chosen_word();
        let fits =
            |window: (usize, usize)| cell_width(&row(choice, window, glyphs).text()) <= cells;
        let start = (0..=chosen)
            .find(|&start| fits((start, chosen + 1)))
            .unwrap_or(chosen);
        let end = (chosen + 2..=count)
            .take_while(|&end| fits((start, end)))
            .last()
            .unwrap_or(chosen + 1);
        Some(row(choice, (start, end), glyphs).cut(cells, glyphs))
    }
}

/// The words of `choice` from `start` to before `end` on one row: a blank cell where none
/// is lost before them, else the ellipsis; each word after the cell the pointer stands in
/// when it is chosen and the first cell of its pill; the last cell of a pill after every
/// word but the row's last; and where words are lost after them, the cell before the next
/// word's pill and the ellipsis.
fn row(choice: &WordChoice, (start, end): (usize, usize), glyphs: Glyphs) -> Line {
    let chosen = choice.chosen_word();
    let ellipsis = || Line::new().push(Tone::Muted, glyphs.ellipsis());
    let lost_after = end < choice.words.len();
    let pointer = match choice.focused {
        true => Tone::NeedsYou,
        false => Tone::Muted,
    };
    let mut row = match start {
        0 => Line::new().plain(" "),
        _ => ellipsis(),
    };
    for at in start..end {
        let (slot, tone) = match at == chosen {
            true => (
                Line::new().push(pointer, glyphs.pointer().to_string()),
                Tone::Strong,
            ),
            false => (Line::new().plain(" "), Tone::Muted),
        };
        row = row
            .then(slot)
            .plain(" ")
            .push(tone, choice.words[at].as_str());
        if at + 1 < end || lost_after {
            row = row.plain(" ");
        }
    }
    match lost_after {
        true => row.plain(" ").then(ellipsis()),
        false => row,
    }
}

#[cfg(test)]
mod tests;
