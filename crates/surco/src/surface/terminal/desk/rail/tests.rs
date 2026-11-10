//! The rail of places a person stands on, on a terminal: a long one follows the
//! pointer, a pointer past nine steps keeps its column, and a big count its key.

use super::super::tests::{desk, drawn, look, numbered, stop};
use crate::components::{RailGroup, RailRow};
use crate::glyphs::Glyphs;

/// A rail longer than the terminal keeps the row pointed at in sight.
#[test]
fn a_long_rail_follows_the_pointer() {
    let mut long = desk(numbered(1));
    long.rail = vec![RailGroup {
        title: Some("steps".to_string()),
        rows: (0..40)
            .map(|at| stop("", &format!("task-{at}"), at == 30))
            .collect(),
    }];
    let rows = drawn(&long, &look(Glyphs::Unicode, 100), 14);
    assert!(
        rows.iter().any(|row| row.contains("▸ task-30")),
        "{rows:#?}"
    );
}

/// A step's number wider than its place widens every row of its group, so the pointer stays
/// in one column.
#[test]
fn the_pointer_keeps_its_column_past_nine_steps() {
    let mut long = desk(numbered(1));
    long.rail[0].rows = vec![stop("9", "ninth", false), stop("10", "tenth", true)];
    let rows = drawn(&long, &look(Glyphs::Unicode, 100), 24);
    assert!(
        rows.iter().any(|row| row.contains(" 9    ninth")),
        "{rows:#?}"
    );
    assert!(
        rows.iter().any(|row| row.contains(" 10 ▸ tenth")),
        "{rows:#?}"
    );
}

/// A count of any size leaves its key in sight.
#[test]
fn a_big_count_keeps_its_key() {
    let mut big = desk(numbered(1));
    big.rail[1].rows[0] = RailRow::Tally {
        what: "decided for you".to_string(),
        count: 120,
        key: Some('d'),
    };
    let rows = drawn(&big, &look(Glyphs::Unicode, 100), 24);
    assert!(
        rows.iter()
            .any(|row| row.contains("decided for you         120  d")),
        "{rows:#?}"
    );
}
