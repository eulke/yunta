//! A round of questions held in place at a terminal. Every expected row here is
//! written by hand.

use super::said_whole;
use crate::components::{
    Card, CardBody, ChoiceRow, Choices, Legend, RoundRow, RoundStanding, TypedRow,
};
use crate::glyphs::Glyphs;
use crate::look::{Look, Width};
use crate::paint::Paint;
use crate::width::cell_width;

fn look(glyphs: Glyphs, cells: usize) -> Look {
    Look {
        glyphs,
        paint: Paint::Plain,
        width: Width::row(cells, None),
    }
}

fn answered(asked: &str, said: &str) -> RoundRow {
    RoundRow {
        asked: asked.to_string(),
        note: None,
        stands: RoundStanding::Answered(said.to_string()),
    }
}

fn ahead(asked: &str, note: Option<&str>) -> RoundRow {
    RoundRow {
        asked: asked.to_string(),
        note: note.map(str::to_string),
        stands: RoundStanding::Ahead,
    }
}

fn value(name: &str) -> ChoiceRow {
    ChoiceRow {
        name: name.to_string(),
        said: None,
        cost: None,
    }
}

fn picking(asked: &str) -> RoundRow {
    RoundRow {
        asked: asked.to_string(),
        note: None,
        stands: RoundStanding::Picking {
            choices: Some(Choices {
                rows: vec![value("basic"), value("pro")],
                at: 0,
            }),
            filter: None,
            refused: Vec::new(),
        },
    }
}

fn typing(asked: &str, before: &str, after: &str, refused: &[&str]) -> RoundRow {
    RoundRow {
        asked: asked.to_string(),
        note: None,
        stands: RoundStanding::Typing {
            typed: TypedRow {
                prompt: "> ".to_string(),
                before: before.to_string(),
                after: after.to_string(),
            },
            refused: refused.iter().map(|said| (*said).to_string()).collect(),
        },
    }
}

fn card(rows: Vec<RoundRow>, legend: Legend) -> Card {
    Card {
        question: "3 answers before `grill` goes on".to_string(),
        edge: None,
        about: vec!["nothing is recorded until every one is answered".to_string()],
        body: CardBody::Round(rows),
        legend,
    }
}

fn drawn(card: &Card, look: &Look, rows: usize) -> Vec<String> {
    card.drawn(look, rows)
        .lines
        .iter()
        .map(|line| line.text())
        .collect()
}

#[test]
fn a_round_holds_what_was_answered_what_is_asked_and_what_is_to_come() {
    let card = card(
        vec![
            answered("Which environment?", "production"),
            picking("Which tier?"),
            ahead("Anything to add?", Some("optional, assumes nothing")),
        ],
        Legend::new().arrows("move").key("enter", "answers"),
    );
    assert_eq!(
        drawn(&card, &look(Glyphs::Unicode, 80), usize::MAX),
        [
            "▎ ◆ 3 answers before `grill` goes on",
            "▎ nothing is recorded until every one is answered",
            "▎",
            "▎ ✓ Which environment?  production",
            "▎ ◆ Which tier?",
            "▎     ▸ basic",
            "▎       pro",
            "▎ ◌ Anything to add?  optional, assumes nothing",
            "▎",
            "▎ ↑↓ move · enter answers",
        ]
    );
}

#[test]
fn ascii_draws_its_own_marks() {
    let card = card(
        vec![
            answered("Which environment?", "production"),
            picking("Which tier?"),
            ahead("Anything to add?", None),
        ],
        Legend::new().key("enter", "answers"),
    );
    let rows = drawn(&card, &look(Glyphs::Ascii, 80), usize::MAX);
    assert_eq!(rows[3], "| + Which environment?  production");
    assert_eq!(rows[4], "| ? Which tier?");
    assert_eq!(rows[5], "|     * basic");
    assert_eq!(rows[7], "| . Anything to add?");
}

/// A question being typed opens its field under it with the cursor on it, past its
/// mark and what stands before the cursor, and what the engine refused stands under
/// the field.
#[test]
fn a_typed_question_opens_its_field_under_it_and_says_what_was_refused() {
    let card = card(
        vec![
            answered("Which environment?", "production"),
            typing(
                "What changed?",
                "the auth ",
                "middle",
                &["`summary` is `required` and nothing answers it"],
            ),
            ahead("Anything to add?", None),
        ],
        Legend::new().key("enter", "answers"),
    );
    let placed = card.drawn(&look(Glyphs::Unicode, 80), usize::MAX);
    let rows: Vec<String> = placed.lines.iter().map(|line| line.text()).collect();
    assert_eq!(
        rows[4..=6],
        [
            "▎ ◆ What changed?",
            "▎     > the auth middle",
            "▎     ✗ `summary` is `required` and nothing answers it",
        ]
    );
    assert_eq!(placed.cursor, Some((5, 17)));
}

/// What was typed to narrow a list stands over it, and a list it leaves nothing of
/// says so.
#[test]
fn a_filter_stands_over_its_list() {
    let row = |choices: Option<Choices>, filter: &str| RoundRow {
        asked: "Which tier?".to_string(),
        note: None,
        stands: RoundStanding::Picking {
            choices,
            filter: Some(filter.to_string()),
            refused: Vec::new(),
        },
    };
    let narrowed = card(
        vec![row(
            Some(Choices {
                rows: vec![value("pro")],
                at: 0,
            }),
            "pr",
        )],
        Legend::new().key("enter", "answers"),
    );
    let rows = drawn(&narrowed, &look(Glyphs::Unicode, 80), usize::MAX);
    assert_eq!(
        rows[3..=5],
        ["▎ ◆ Which tier?", "▎     filter pr", "▎     ▸ pro"]
    );
    let none = card(vec![row(None, "zz")], Legend::new().key("enter", "answers"));
    let rows = drawn(&none, &look(Glyphs::Unicode, 80), usize::MAX);
    assert_eq!(
        rows[3..=5],
        [
            "▎ ◆ Which tier?",
            "▎     filter zz",
            "▎     nothing matches"
        ]
    );
}

/// An answer too long for its row is cut at the row's end: it is said whole when the
/// round ends. A question wraps under itself.
#[test]
fn a_long_answer_is_cut_and_a_long_question_wraps() {
    let long = "x".repeat(200);
    let card = card(
        vec![
            answered("What changed?", &long),
            ahead(
                "A question long enough that it has to wrap on a line this narrow to be read",
                None,
            ),
        ],
        Legend::new().key("enter", "answers"),
    );
    for cells in 30..=100 {
        for row in drawn(&card, &look(Glyphs::Unicode, cells), usize::MAX) {
            assert!(cell_width(&row) <= cells, "at {cells}: {row}");
        }
    }
    let rows = drawn(&card, &look(Glyphs::Unicode, 40), usize::MAX);
    let at = rows
        .iter()
        .position(|row| row.contains("What changed?"))
        .expect("the answered row");
    assert_eq!(
        rows[at], "▎ ✓ What changed?  xxxxxxxxxxxxxxxxxxxx…",
        "{rows:#?}"
    );
    assert_eq!(rows[at + 1], "▎ ◌ A question long enough that it has");
    assert_eq!(rows[at + 2], "▎   to wrap on a line this narrow to be");
    assert_eq!(rows[at + 3], "▎   read");
}

/// What a card cuts at the row's end, the history keeps whole, under the question's mark.
#[test]
fn the_history_keeps_an_answer_whole_under_its_mark() {
    let rows: Vec<String> = said_whole(
        "What changed?",
        "the lock moved to the outer call and the retry went away",
        &look(Glyphs::Unicode, 30),
    )
    .iter()
    .map(|row| row.text())
    .collect();
    assert_eq!(
        rows,
        [
            "✓ What changed? the lock moved",
            "  to the outer call and the",
            "  retry went away",
        ],
    );
}

fn long_round() -> Vec<RoundRow> {
    vec![
        answered("One?", "1"),
        answered("Two?", "2"),
        answered("Three?", "3"),
        answered("Four?", "4"),
        typing("Five?", "", "", &[]),
        ahead("Six?", None),
        ahead("Seven?", None),
        ahead("Eight?", None),
    ]
}

/// A long question gives way before its answer: an answer on its row is what the
/// row is for.
#[test]
fn a_long_question_gives_way_before_its_answer() {
    let card = card(
        vec![answered(
            "Which environment should this change be deployed to first?",
            "staging",
        )],
        Legend::new().key("enter", "answers"),
    );
    let rows = drawn(&card, &look(Glyphs::Unicode, 60), usize::MAX);
    assert!(rows[3].ends_with("…  staging"), "{rows:#?}");
    assert!(rows[3].starts_with("▎ ✓ Which environment"), "{rows:#?}");
    assert!(cell_width(&rows[3]) <= 60, "{rows:#?}");
}

/// What a field may fill in a round is what the field's own margin leaves too.
#[test]
fn a_field_in_a_round_fills_what_its_margin_leaves() {
    assert_eq!(
        Card::typed_room_in_round(&look(Glyphs::Unicode, 80), "> "),
        71
    );
}

/// A round longer than the terminal folds what was answered first, and what is to
/// come next. What is being answered is never folded.
#[test]
fn a_long_round_folds_what_was_answered_and_then_what_is_to_come() {
    let card = card(long_round(), Legend::new().key("enter", "answers"));
    let tall = drawn(&card, &look(Glyphs::Unicode, 80), 13);
    assert_eq!(
        tall,
        [
            "▎ ◆ 3 answers before `grill` goes on",
            "▎ nothing is recorded until every one is answered",
            "▎",
            "▎ ✓ 4 of 8 answered",
            "▎ ◆ Five?",
            "▎     > ",
            "▎ ◌ Six?",
            "▎ ◌ Seven?",
            "▎ ◌ Eight?",
            "▎",
            "▎ enter answers",
        ]
    );
    let short = drawn(&card, &look(Glyphs::Unicode, 80), 10);
    assert_eq!(
        short[3..=6],
        [
            "▎ ✓ 4 of 8 answered",
            "▎ ◆ Five?",
            "▎     > ",
            "▎ ◌ 3 of 8 to answer"
        ]
    );
    assert!(short.len() <= 9, "{short:#?}");
}

/// The cursor stands on the field wherever folding leaves it.
#[test]
fn the_cursor_follows_the_field_through_a_fold() {
    let card = card(long_round(), Legend::new().key("enter", "answers"));
    let placed = card.drawn(&look(Glyphs::Unicode, 80), 13);
    assert_eq!(placed.cursor, Some((5, 8)));
}
