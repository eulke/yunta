//! The row words are typed on, boxed: three rows at the width it is given, the words one
//! cell inside the uprights and fitted around the cursor. Every expected row is written by
//! hand.

use crate::components::TypedRow;
use crate::glyphs::Glyphs;
use crate::line::Tone;
use crate::surface::terminal::Placed;
use crate::width::cell_width;

fn typed(before: &str, after: &str) -> TypedRow {
    TypedRow {
        prompt: "> ".to_string(),
        before: before.to_string(),
        after: after.to_string(),
    }
}

fn texts(placed: &Placed) -> Vec<String> {
    placed.lines.iter().map(|line| line.text()).collect()
}

/// At 40 cells the row is a box of three rows, each 40 cells wide: round corners above and
/// below, and the words between the uprights with a cell inside each. The cursor stands
/// on the middle row, after the words before it.
#[test]
fn a_typed_row_is_a_box_of_three_rows_with_the_words_between_its_uprights() {
    let placed = typed("keep the ", "lock").drawn(40, Glyphs::Unicode, None);
    assert_eq!(
        texts(&placed),
        [
            "╭──────────────────────────────────────╮",
            "│ keep the lock                        │",
            "╰──────────────────────────────────────╯",
        ]
    );
    assert!(
        texts(&placed).iter().all(|row| cell_width(row) == 40),
        "{:#?}",
        texts(&placed)
    );
    assert_eq!(placed.cursor, Some((1, "│ keep the ".chars().count())));
    let middle = &placed.lines[1];
    let tones: Vec<Tone> = middle
        .spans()
        .iter()
        .filter(|span| !span.text.trim().is_empty())
        .map(|span| span.tone)
        .collect();
    assert_eq!(
        tones,
        [Tone::Muted, Tone::Strong, Tone::Muted],
        "the words stand out between muted edges"
    );
}

/// A line wider than its box would pass the upright, and the cursor would stand outside
/// the box: the box shows the part of the line on the cursor's side, the end of a line
/// being typed, or its start when the cursor went back to it.
#[test]
fn words_longer_than_the_box_keep_the_cursors_side_in_sight() {
    let at_the_end = typed("abcdefghijkl", "").drawn(12, Glyphs::Unicode, None);
    assert_eq!(
        texts(&at_the_end),
        ["╭──────────╮", "│ fghijkl  │", "╰──────────╯"]
    );
    assert_eq!(
        at_the_end.cursor,
        Some((1, 9)),
        "the cursor sits after them"
    );

    let at_the_start = typed("", "abcdefghijkl").drawn(12, Glyphs::Unicode, None);
    assert_eq!(
        texts(&at_the_start),
        ["╭──────────╮", "│ abcdefg  │", "╰──────────╯"]
    );
    assert_eq!(
        at_the_start.cursor,
        Some((1, 2)),
        "the cursor sits before them"
    );
}

/// At the first request's width, 72 cells, the box is the design's, in both glyph sets.
#[test]
fn the_box_draws_the_designs_rows_for_the_same_words_at_the_same_width() {
    let words = typed("add support for installing a pack globally", "");
    assert_eq!(
        texts(&words.drawn(72, Glyphs::Unicode, None)),
        [
            "╭──────────────────────────────────────────────────────────────────────╮",
            "│ add support for installing a pack globally                           │",
            "╰──────────────────────────────────────────────────────────────────────╯",
        ]
    );
    assert_eq!(
        texts(&words.drawn(72, Glyphs::Ascii, None)),
        [
            ".----------------------------------------------------------------------.",
            "| add support for installing a pack globally                           |",
            "'----------------------------------------------------------------------'",
        ]
    );
}

/// In ASCII the box's corners are `.` above and `'` below, its edges `-` and `|`.
#[test]
fn in_ascii_the_corners_are_dots_above_and_quotes_below() {
    let placed = typed("trunk", "").drawn(12, Glyphs::Ascii, None);
    assert_eq!(
        texts(&placed),
        [".----------.", "| trunk    |", "'----------'"]
    );
}

/// Ten cells leave six inside the box and five before the cursor's: two characters of two
/// cells, and one cell nothing can go in.
#[test]
fn a_box_holds_whole_characters_however_many_cells_they_take() {
    let placed = typed("実装レビュー", "").drawn(10, Glyphs::Unicode, None);
    assert_eq!(
        texts(&placed),
        ["╭────────╮", "│ ュー   │", "╰────────╯"],
        "a character two cells wide is never cut in half"
    );
    assert_eq!(placed.cursor, Some((1, 6)));
}

/// While nothing is typed, what is asked of the words stands faint where they go, the
/// cursor on its first cell; once a word is typed, the word stands there instead.
#[test]
fn a_placeholder_stands_faint_in_the_box_while_nothing_is_typed() {
    let empty = typed("", "").drawn(30, Glyphs::Unicode, Some("what should change?"));
    assert_eq!(empty.lines[1].text(), "│ what should change?        │");
    assert_eq!(empty.cursor, Some((1, 2)));
    assert!(
        empty.lines[1]
            .spans()
            .iter()
            .any(|span| span.text == "what should change?" && span.tone == Tone::Faint),
        "{:?}",
        empty.lines[1].spans()
    );
    let typing = typed("k", "").drawn(30, Glyphs::Unicode, Some("what should change?"));
    assert_eq!(typing.lines[1].text(), format!("│ k{} │", " ".repeat(25)));
}
