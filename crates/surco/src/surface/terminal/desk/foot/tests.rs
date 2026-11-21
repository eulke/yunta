//! The foot of a plan's desk on a terminal: the option being confirmed says what it does,
//! and keys too wide to stand beside the answer fold under it between two keys. Every expected
//! row here is written by hand.

use super::super::tests::{desk, drawn, ends, look, numbered};
use crate::components::{ChoiceRow, FootRow, Legend};
use crate::glyphs::Glyphs;

/// The option a number named stands with what it does, its keys at the row's far end.
#[test]
fn a_choice_being_confirmed_says_what_it_does() {
    let mut confirming = desk(numbered(1));
    confirming.foot.row = FootRow::Confirming(ChoiceRow {
        name: "approve".to_string(),
        said: Some("On to `implement`".to_string()),
        cost: Some("resolves this gate".to_string()),
        key: None,
    });
    confirming.foot.legend = Legend::new().key("enter", "confirm").key("esc", "go back");
    let rows = drawn(&confirming, &look(Glyphs::Unicode, 100), 24);
    assert_eq!(
        rows.last().map(String::as_str),
        Some(
            format!(
                " {}",
                ends(
                    "approve  On to `implement`: resolves this gate",
                    "enter confirm · esc go back",
                    98
                )
            )
            .as_str()
        ),
    );
}

/// Keys that do not fit beside the options take a row of their own under them.
#[test]
fn keys_that_do_not_fit_beside_the_options_go_under_them() {
    let mut crowded = desk(numbered(1));
    crowded.foot.legend = (0..3).fold(Legend::new(), |legend, _| {
        legend.key("a", "very long way to say what a key does")
    });
    let rows = drawn(&crowded, &look(Glyphs::Unicode, 100), 24);
    let at = rows
        .iter()
        .position(|row| row.contains("1 approve"))
        .expect("the options");
    assert_eq!(rows[at], " 1 approve   2 adjust", "{rows:#?}");
    assert!(rows[at + 1].contains("a very long way"), "{rows:#?}");
    assert_eq!(rows.len(), 23, "the pane gave up the rows: {rows:#?}");
}

/// Keys wider than the foot's row fold onto rows of their own under the options, as a
/// legend folds: between two keys, never inside one.
#[test]
fn keys_wider_than_the_row_fold_between_two_keys() {
    let mut folded = desk(numbered(1));
    folded.foot.row = FootRow::Options {
        named: vec!["approve".into(), "adjust".into(), "abort".into()],
        more: 0,
    };
    folded.foot.legend = Legend::new()
        .key("1 2 3", "answer")
        .key("tab", "next place")
        .arrows("scroll")
        .key("p", "proof")
        .key("d", "decided for you")
        .key("esc", "leave it waiting");
    let foot = |cells: usize| {
        let rows = drawn(&folded, &look(Glyphs::Unicode, cells), 24);
        rows[rows.len() - 3..].to_vec()
    };
    assert_eq!(
        foot(80),
        [
            " 1 approve   2 adjust   3 abort",
            " 1 2 3 answer · tab next place · ↑↓ scroll · p proof · d decided for you",
            " esc leave it waiting",
        ]
    );
    assert_eq!(
        foot(60),
        [
            " 1 approve   2 adjust   3 abort",
            " 1 2 3 answer · tab next place · ↑↓ scroll · p proof",
            " d decided for you · esc leave it waiting",
        ]
    );
}
