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
        motion: crate::motion::Motion::Moves,
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
        key: None,
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

/// `card` in `rows` rows: each row's text.
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
        Legend::new().arrows("choose").key("enter", "answer"),
    );
    assert_eq!(
        drawn(&card, &look(Glyphs::Unicode, 80), usize::MAX),
        [
            "▎ ◆ 3 answers before `grill` goes on",
            "▎ nothing is recorded until every one is answered",
            "▎",
            "▎ ✓ Which environment?  production",
            "▎ ◆ Which tier?",
            "▎      ▸ type to filter",
            "▎     ▸ basic",
            "▎       pro",
            "▎ ◌ Anything to add?  optional, assumes nothing",
            "▎",
            "▎ ↑↓ choose · enter answer",
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
        Legend::new().key("enter", "answer"),
    );
    let rows = drawn(&card, &look(Glyphs::Ascii, 80), usize::MAX);
    assert_eq!(rows[3], "| + Which environment?  production");
    assert_eq!(rows[4], "| ? Which tier?");
    assert_eq!(rows[5], "|      * type to filter");
    assert_eq!(rows[6], "|     * basic");
    assert_eq!(rows[8], "| . Anything to add?");
}

/// A question being typed opens its boxed field under it with the cursor in the box,
/// past its margin, the box's upright and the cell inside it, and what stands before
/// the cursor; what the engine refused stands under the box.
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
        Legend::new().key("enter", "answer"),
    );
    let placed = card.drawn(&look(Glyphs::Unicode, 80), usize::MAX);
    let rows: Vec<String> = placed.lines.iter().map(|line| line.text()).collect();
    assert_eq!(
        rows[4..=8],
        [
            "▎ ◆ What changed?".to_string(),
            format!("▎     ╭{}╮", "─".repeat(72)),
            format!("▎     │ the auth middle{} │", " ".repeat(55)),
            format!("▎     ╰{}╯", "─".repeat(72)),
            "▎     ✗ `summary` is `required` and nothing answers it".to_string(),
        ]
    );
    assert_eq!(placed.cursor, Some((6, 17)));
}

/// What was typed to narrow a list stands over it after the pointer, a cell right of the
/// choices' pointer, and a list it leaves nothing of says so where its choices stand.
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
        Legend::new().key("enter", "answer"),
    );
    let rows = drawn(&narrowed, &look(Glyphs::Unicode, 80), usize::MAX);
    assert_eq!(
        rows[3..=5],
        ["▎ ◆ Which tier?", "▎      ▸ pr", "▎     ▸ pro"]
    );
    let none = card(vec![row(None, "zz")], Legend::new().key("enter", "answer"));
    let rows = drawn(&none, &look(Glyphs::Unicode, 80), usize::MAX);
    assert_eq!(
        rows[3..=5],
        ["▎ ◆ Which tier?", "▎      ▸ zz", "▎       nothing matches"]
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
        Legend::new().key("enter", "answer"),
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
        Legend::new().key("enter", "answer"),
    );
    let rows = drawn(&card, &look(Glyphs::Unicode, 60), usize::MAX);
    assert!(rows[3].ends_with("…  staging"), "{rows:#?}");
    assert!(rows[3].starts_with("▎ ✓ Which environment"), "{rows:#?}");
    assert!(cell_width(&rows[3]) <= 60, "{rows:#?}");
}

/// The box a field in a round is typed in spans what the field's own margin leaves too.
#[test]
fn a_fields_box_in_a_round_spans_what_its_margin_leaves() {
    assert_eq!(Card::typed_room_in_round(&look(Glyphs::Unicode, 80)), 74);
}

/// A round longer than the terminal folds the questions above the one being answered
/// first, and those under it next, each run of them counted on a row of its own where
/// the questions stand. What is being answered is never folded, its box whole.
#[test]
fn a_long_round_folds_what_was_answered_and_then_what_is_to_come() {
    let card = card(long_round(), Legend::new().key("enter", "answer"));
    let [top, field, bottom] = [
        format!("▎     ╭{}╮", "─".repeat(72)),
        format!("▎     │{}│", " ".repeat(72)),
        format!("▎     ╰{}╯", "─".repeat(72)),
    ];
    let tall = drawn(&card, &look(Glyphs::Unicode, 80), 14);
    assert_eq!(
        tall,
        [
            "▎ ◆ 3 answers before `grill` goes on".to_string(),
            "▎ nothing is recorded until every one is answered".to_string(),
            "▎".to_string(),
            "▎ … 4 above".to_string(),
            "▎ ◆ Five?".to_string(),
            top.clone(),
            field.clone(),
            bottom.clone(),
            "▎ ◌ Six?".to_string(),
            "▎ ◌ Seven?".to_string(),
            "▎ ◌ Eight?".to_string(),
            "▎".to_string(),
            "▎ enter answer".to_string(),
        ]
    );
    let short = drawn(&card, &look(Glyphs::Unicode, 80), 11);
    assert_eq!(
        short[3..=8],
        [
            "▎ … 4 above".to_string(),
            "▎ ◆ Five?".to_string(),
            top,
            field,
            bottom,
            "▎ … 3 under".to_string(),
        ]
    );
    assert!(short.len() <= 11, "{short:#?}");
}

/// Short of the rows its folds leave, a round's card gives its head way, the round's own
/// questions saying what is asked; then the question being answered keeps one row, and the
/// row its list is narrowed on gives way while nothing is typed there. Its choices stand.
#[test]
fn shorter_still_its_head_gives_way_then_the_question_keeps_one_row() {
    let card = card(
        vec![
            answered("Which environment?", "production"),
            picking("Which tier of the service should this release be deployed on?"),
            ahead("Anything to add?", None),
        ],
        Legend::new(),
    );
    assert_eq!(
        drawn(&card, &look(Glyphs::Unicode, 40), 6),
        [
            "▎ … 1 above",
            "▎ ◆ Which tier of the service should th…",
            "▎      ▸ type to filter",
            "▎     ▸ basic",
            "▎       pro",
            "▎ … 1 under",
        ]
    );
    assert_eq!(
        drawn(&card, &look(Glyphs::Unicode, 40), 5),
        [
            "▎ … 1 above",
            "▎ ◆ Which tier of the service should th…",
            "▎     ▸ basic",
            "▎       pro",
            "▎ … 1 under",
        ]
    );
}

/// The cursor stands in the field's box wherever folding leaves it.
#[test]
fn the_cursor_follows_the_field_through_a_fold() {
    let card = card(long_round(), Legend::new().key("enter", "answer"));
    let placed = card.drawn(&look(Glyphs::Unicode, 80), 12);
    assert_eq!(placed.cursor, Some((6, 8)));
}

/// On a question picked from a list every printable key types on the row it is narrowed
/// on, so the cursor stands there: at the first cell of `type to filter` while nothing is
/// typed, after the words typed otherwise, at any width and through the folds that keep
/// that row.
#[test]
fn the_cursor_stands_on_the_row_a_list_is_narrowed_on() {
    let rows = |filter: Option<&str>| {
        let mut asked = picking("Which tier?");
        if let RoundStanding::Picking { filter: typed, .. } = &mut asked.stands {
            *typed = filter.map(str::to_string);
        }
        vec![answered("Which environment?", "production"), asked]
    };
    let legend = || Legend::new().arrows("choose").key("enter", "answer");
    for cells in [120, 60] {
        let open = card(rows(None), legend());
        let placed = open.drawn(&look(Glyphs::Unicode, cells), usize::MAX);
        assert_eq!(placed.cursor, Some((5, 9)), "{cells} cells, nothing typed");
        let narrowed = card(rows(Some("pr")), legend());
        let placed = narrowed.drawn(&look(Glyphs::Ascii, cells), usize::MAX);
        assert_eq!(placed.cursor, Some((5, 11)), "{cells} cells, `pr` typed");
    }
    let folded = card(rows(Some("pr")), legend()).drawn(&look(Glyphs::Unicode, 40), 4);
    let at = folded.cursor.expect("a cursor through the fold");
    assert!(
        folded.lines[at.0].text().ends_with("▸ pr"),
        "{:#?}",
        folded
            .lines
            .iter()
            .map(|line| line.text())
            .collect::<Vec<_>>()
    );
}
