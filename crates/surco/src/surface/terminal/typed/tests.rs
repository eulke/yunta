//! The row words are typed on, fitted to its cells around the cursor.

use crate::components::TypedRow;
use crate::width::cell_width;

/// A line wider than its row would wrap below it, and the cursor would be on another
/// row than the text it edits: the row shows the part of it around the cursor.
#[test]
fn a_line_wider_than_the_row_is_drawn_as_the_part_of_it_around_the_cursor() {
    const ROW: usize = 8;
    let typed = |before: &str, after: &str| TypedRow {
        prompt: "> ".to_string(),
        before: before.to_string(),
        after: after.to_string(),
    };
    let (before, after) = typed("abcdefghijkl", "").fitted(ROW);
    assert_eq!(
        format!("{before}{after}"),
        "efghijkl",
        "the end of the line stays in sight"
    );
    assert_eq!(cell_width(&after), 0, "the cursor sits where the typing is");

    let (before, after) = typed("", "abcdefghijkl").fitted(ROW);
    assert_eq!(
        format!("{before}{after}"),
        "abcdefgh",
        "the row follows the cursor back"
    );
    assert_eq!(
        cell_width(&after),
        ROW,
        "the cursor sits before everything drawn"
    );
}

/// Five cells hold two characters of two cells and leave one cell nothing can go in.
#[test]
fn a_row_holds_whole_characters_however_many_cells_they_take() {
    const ROW: usize = 5;
    let typed = TypedRow {
        prompt: "> ".to_string(),
        before: "実装レビュー".to_string(),
        after: String::new(),
    };
    let (before, after) = typed.fitted(ROW);
    let drawn = format!("{before}{after}");
    assert_eq!(
        drawn, "ュー",
        "a character two cells wide is never cut in half"
    );
    assert_eq!(cell_width(&drawn), 4);
    assert_eq!(cell_width(&after), 0);
}
