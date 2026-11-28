//! A card at a pane's foot under what it rests on: what stands above takes every row the
//! card leaves, whole where the look is taller than the shortest, a row each of its options
//! at the shortest; it gives way from its top where it is read at its end and scrolls from the
//! place asked for; above a card standing whole it says itself in fewer rows first, then the
//! blank row under the title goes; spaced close, no blank row stands among it; and a line
//! being typed on the card keeps its box in the fewest rows.

use crate::components::{
    Card, CardBody, ChoiceRow, Choices, Laid, Legend, Note, Prose, Region, Sight, TypedRow,
};
use crate::doc::Doc;
use crate::glyphs::Glyphs;
use crate::line::Line;
use crate::look::{Look, Width};
use crate::surface::Terminal;

/// The cells a pane is drawn at here.
const CELLS: usize = 58;

/// A card asking whether to ship, with two options and no keys of its own: they stand in the
/// screen's legend.
fn card() -> Card {
    let option = |name: &str, said: &str| ChoiceRow {
        name: name.to_string(),
        said: Some(said.to_string()),
        cost: None,
        key: None,
    };
    Card {
        question: "Ship it?".to_string(),
        edge: Some("ship".to_string()),
        about: Vec::new(),
        body: CardBody::Choosing {
            choices: Choices {
                rows: vec![
                    option("approve", "On to `pr`"),
                    option("abort", "Pause here"),
                ],
                at: 0,
            },
            then: None,
        },
        legend: Legend::new(),
    }
}

/// Six lines of what the card rests on, then the card: a region read at its end, as a pane
/// opens on what stands above a card.
fn resting(sight: Sight) -> Region {
    let doc = (1..=6).fold(Doc::new(), |doc, at| {
        doc.with(Prose(format!("ground {at}")))
    });
    Region::new(doc.with(card())).in_sight(sight)
}

/// `region` drawn in a pane `rows` high, each row's words.
fn drawn(region: &Region, rows: usize) -> (Vec<String>, Laid) {
    let look = Look {
        width: Width::exact(CELLS),
        ..Look::plain()
    };
    let paned = Terminal::on(look).pane(region, rows);
    let lines = paned.lines.iter().map(Line::text).collect();
    (lines, paned.laid)
}

/// `region` drawn in a pane `rows` high at the shortest look, each row's words.
fn drawn_shortest(region: &Region, rows: usize) -> (Vec<String>, Laid) {
    let look = Look {
        width: Width::exact(CELLS),
        ..Look::plain()
    };
    let paned = Terminal::on(look).pane_short_of_rows(region, rows);
    let lines = paned.lines.iter().map(Line::text).collect();
    (lines, paned.laid)
}

/// Where the rows hold it all, what stands above keeps every row, however many: never only
/// half of them, and the card stands at the foot under the rows left blank.
#[test]
fn what_stands_above_a_card_keeps_every_row_its_options_leave() {
    let (lines, laid) = drawn(&resting(Sight::at_the_end()), 17);
    let grounds: Vec<&String> = lines
        .iter()
        .filter(|line| line.contains("ground"))
        .collect();
    assert_eq!(grounds.len(), 6, "{lines:#?}");
    let star = Glyphs::Ascii.pointer();
    assert!(lines[13].contains("Ship it?"), "{lines:#?}");
    assert!(lines[16].contains("abort"), "{lines:#?}");
    assert!(lines[15].contains(&format!("{star} approve")), "{lines:#?}");
    assert!(!laid.scrolls(), "{laid:?}");
}

/// At the shortest look, the card keeps its question and each option on a row; what stands
/// above keeps the rows those leave, its last ones where it is read at its end, its first where
/// it is read from its top, and says what was laid out of it for the keys that scroll.
#[test]
fn short_of_rows_what_stands_above_gives_way_from_its_top_and_scrolls_back() {
    let titled = |sight| resting(sight).titled(["ship", "gate"]);
    let (at_its_end, laid) = drawn_shortest(&titled(Sight::at_the_end()), 5);
    assert_eq!(
        at_its_end[..2],
        ["  ground 5", "  ground 6"],
        "{at_its_end:#?}"
    );
    assert!(at_its_end[2].contains("Ship it?"), "{at_its_end:#?}");
    assert!(at_its_end[4].contains("abort"), "{at_its_end:#?}");
    assert_eq!(
        laid,
        Laid {
            top: 4,
            page: 2,
            most: 4
        }
    );
    let (from_its_top, laid) = drawn_shortest(&titled(Sight::from_line(0)), 5);
    assert_eq!(
        from_its_top[..2],
        ["  ground 1", "  ground 2"],
        "{from_its_top:#?}"
    );
    assert_eq!(laid.top, 0);
}

/// Words being typed for an option on a card at the foot of a pane in its fewest rows keep
/// their line, its box and the cursor in it: what the option says past its row gives way
/// first.
#[test]
fn a_line_being_typed_on_a_card_keeps_its_box_and_its_cursor_short_of_rows() {
    let saying = Card {
        body: CardBody::Saying {
            choices: Choices {
                rows: vec![ChoiceRow {
                    name: "go".to_string(),
                    said: Some("Resolve the gate: nothing in the workflow waits on it".to_string()),
                    cost: None,
                    key: None,
                }],
                at: 0,
            },
            asks: "add a note".to_string(),
            typed: TypedRow {
                prompt: "> ".to_string(),
                before: "words".to_string(),
                after: String::new(),
            },
            refused: None,
        },
        ..card()
    };
    let region = Region::new(Doc::new().with(saying)).titled(["approve", "gate"]);
    let look = Look {
        width: Width::exact(CELLS),
        ..Look::plain()
    };
    let paned = Terminal::on(look).pane_short_of_rows(&region, 5);
    let lines: Vec<String> = paned.lines.iter().map(Line::text).collect();
    assert_eq!(lines.len(), 5, "{lines:#?}");
    assert!(lines[1].contains("go"), "{lines:#?}");
    assert!(lines[3].contains("words"), "{lines:#?}");
    assert_eq!(paned.cursor.map(|(row, _)| row), Some(3), "{lines:#?}");
}

/// Taller than the shortest look, the card stands whole, the blank row under its question
/// among it; what stands above makes the room: the blank row parting it from the card goes,
/// then it says itself in fewer rows as the region says, then the blank row under the title
/// goes; where none of those is room enough, it gives way from its top.
#[test]
fn above_a_card_standing_whole_the_blank_rows_and_what_stands_above_give_way() {
    let shorter = vec![vec![Prose("ground 1".to_string()).into()]];
    let titled = Region::new(Doc::new().with_all(grounds(3)).with(card()))
        .abridged(shorter)
        .titled(["ship", "gate"]);
    let (whole, _) = drawn(&titled, 11);
    assert_eq!(
        whole,
        [
            "",
            "  ground 1",
            "",
            "  ground 2",
            "",
            "  ground 3",
            "",
            "| ? Ship it?                                          ship",
            "|",
            "| * approve  On to `pr`",
            "|   abort    Pause here",
        ],
        "every row stands"
    );
    let (unparted, _) = drawn(&titled, 10);
    assert_eq!(unparted[5], "  ground 3", "{unparted:#?}");
    assert!(unparted[6].contains("Ship it?"), "{unparted:#?}");
    let (abridged, _) = drawn(&titled, 6);
    assert_eq!(abridged[..2], ["", "  ground 1"], "{abridged:#?}");
    assert!(abridged[2].contains("Ship it?"), "{abridged:#?}");
    let (untitled, _) = drawn(&titled, 5);
    assert_eq!(untitled[0], "  ground 1", "{untitled:#?}");
    assert!(untitled[1].contains("Ship it?"), "{untitled:#?}");
    assert_eq!(untitled[2], "|", "the card keeps its blank row");
}

/// The options a card asking whether to ship may offer, as many as a test asks for.
const OPTIONS: [&str; 6] = ["approve", "abort", "adjust", "retry", "skip", "close"];

/// Two grounds, then a card of the first `options` of [`OPTIONS`] under the region's title,
/// with a note under the card where it stands `noted`: read at its end.
fn under_a_title(options: usize, noted: bool) -> Region {
    let mut asked = card();
    if let CardBody::Choosing { choices, .. } = &mut asked.body {
        choices.rows = OPTIONS[..options]
            .iter()
            .map(|name| ChoiceRow {
                name: name.to_string(),
                said: Some(format!("{name} it")),
                cost: None,
                key: None,
            })
            .collect();
    }
    let doc = Doc::new().with_all(grounds(2)).with(asked);
    let doc = match noted {
        true => doc.with(Note("approving opens the pull request".to_string())),
        false => doc,
    };
    Region::new(doc)
        .titled(["ship", "gate"])
        .in_sight(Sight::at_the_end())
}

/// The title gives way exactly where the card's fewest rows do not stand under it, as the pane
/// draws it there: kept, the card's question and each of its options stand under the title;
/// given way, they do not. Short of rows or not, aired or close, with a note under the card or
/// none, a card of two options and one of six.
#[test]
fn the_title_gives_way_exactly_where_the_cards_least_does_not_stand_under_it() {
    let look = Look {
        width: Width::exact(CELLS),
        ..Look::plain()
    };
    let terminal = Terminal::on(look);
    let mut wrong = Vec::new();
    for (options, noted) in [(2, false), (2, true), (6, false), (6, true)] {
        let region = under_a_title(options, noted);
        for short_of_rows in [false, true] {
            for rows in 0..=16 {
                let paned = match short_of_rows {
                    true => terminal.pane_short_of_rows(&region, rows),
                    false => terminal.pane(&region, rows),
                };
                let lines: Vec<String> = paned.lines.iter().map(Line::text).collect();
                let stands = std::iter::once("Ship it?")
                    .chain(OPTIONS[..options].iter().copied())
                    .all(|said| lines.iter().any(|line| line.contains(said)));
                if terminal.keeps_its_title(&region, rows, short_of_rows) != stands {
                    wrong.push(format!(
                        "{options} options, noted {noted}, short {short_of_rows}, {rows} rows: {lines:?}"
                    ));
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// `count` lines of what a card rests on.
fn grounds(count: usize) -> Vec<crate::doc::Block> {
    (1..=count)
        .map(|at| Prose(format!("ground {at}")).into())
        .collect()
}
