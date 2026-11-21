//! The keys a card says as its foot, at a terminal narrower than they are: they fold as a
//! legend folds, between two keys and never inside one. Every expected row here is written
//! by hand.

use super::super::tests::{card, eight_rows};
use crate::components::{Card, CardBody, ChoiceRow, Choices, Legend, RoundRow, RoundStanding};
use crate::glyphs::Glyphs;
use crate::look::{Look, Width};
use crate::paint::Paint;

/// A terminal `cells` wide, in Unicode, with no paint.
fn look(cells: usize) -> Look {
    Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::row(cells, None),
        motion: crate::motion::Motion::Moves,
    }
}

/// The last `rows` rows of `card` drawn whole at a terminal `cells` wide.
fn foot(card: &Card, cells: usize, rows: usize) -> Vec<String> {
    let drawn: Vec<String> = card
        .drawn(&look(cells), usize::MAX)
        .lines
        .iter()
        .map(|line| line.text())
        .collect();
    drawn[drawn.len() - rows..].to_vec()
}

/// A choosing card's keys wider than its room at 80 columns fold before the key that does
/// not fit whole.
#[test]
fn a_cards_keys_wider_than_its_room_fold_between_two_keys() {
    let body = CardBody::Choosing {
        choices: Choices {
            rows: eight_rows(),
            at: 0,
        },
        then: None,
    };
    let mut choosing = card(body, Glyphs::Unicode);
    choosing.legend = Legend::new()
        .arrows("choose")
        .key("enter", "answer")
        .key("n", "add a note")
        .key("r", "read the plan")
        .key("esc", "leave it waiting");
    assert_eq!(
        foot(&choosing, 80, 2),
        [
            "▎ ↑↓ choose · enter answer · n add a note · r read the plan",
            "▎ esc leave it waiting",
        ]
    );
}

/// A round's keys stand on one row at 80 columns, and fold between two keys at 60.
#[test]
fn a_rounds_keys_fold_between_two_keys_where_they_do_not_fit() {
    let value = |name: &str| ChoiceRow {
        name: name.to_string(),
        said: None,
        cost: None,
        key: None,
    };
    let asked = RoundRow {
        asked: "Which tier?".to_string(),
        note: None,
        stands: RoundStanding::Picking {
            choices: Some(Choices {
                rows: vec![value("basic"), value("pro")],
                at: 0,
            }),
            filter: None,
            refused: Vec::new(),
        },
    };
    let round = Card {
        question: "ask asks".to_string(),
        edge: None,
        about: Vec::new(),
        body: CardBody::Round(vec![asked]),
        legend: Legend::new()
            .arrows("choose")
            .key("enter", "answer")
            .key("shift-tab", "go back")
            .key("esc", "leave it for later"),
    };
    assert_eq!(
        foot(&round, 80, 1),
        ["▎ ↑↓ choose · enter answer · shift-tab go back · esc leave it for later"]
    );
    assert_eq!(
        foot(&round, 60, 2),
        [
            "▎ ↑↓ choose · enter answer · shift-tab go back",
            "▎ esc leave it for later",
        ]
    );
}
