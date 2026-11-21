//! A decision held in place at a terminal. Every expected row here is written by
//! hand. A card's rows open on its edge, and a row's text stands where the longest
//! option name leaves it.

use std::path::Path;

use super::Said;
use crate::components::{Card, CardBody, ChoiceRow, Choices, Legend, TypedRow};
use crate::figures::files_moving;
use crate::glyphs::Glyphs;
use crate::golden::{assert_golden, ENVIRONMENTS, RICH};
use crate::line::{Fill, Tone};
use crate::look::{Look, Width};
use crate::paint::Paint;
use crate::width::cell_width;

fn row(name: &str, said: Option<&str>, cost: Option<&str>) -> ChoiceRow {
    ChoiceRow {
        name: name.to_string(),
        said: said.map(str::to_string),
        cost: cost.map(str::to_string),
        key: None,
    }
}

fn rows() -> Vec<ChoiceRow> {
    vec![
        row(
            "approve",
            Some("On to `implement`"),
            Some("resolves this gate"),
        ),
        row(
            "adjust",
            Some("Back to `plan`"),
            Some("sends what you say, and asks again once it completes"),
        ),
        row(
            "abort",
            Some("Pause here"),
            Some("nothing runs; this decision stays open"),
        ),
    ]
}

pub(in crate::surface::terminal) fn card(body: CardBody, glyphs: Glyphs) -> Card {
    Card {
        question: "Plan and its tests registered. Approve?".to_string(),
        edge: Some(format!("approve-plan {} 3GQF5Q", glyphs.sep())),
        about: vec![files_moving("8 files".to_string(), 295, 73, glyphs)],
        body,
        legend: Legend::new()
            .arrows("choose")
            .key("enter", "answer")
            .key("n", "add a note")
            .key("esc", "leave it for later"),
    }
}

fn choosing(at: usize, then: Option<&str>) -> CardBody {
    CardBody::Choosing {
        choices: Choices { rows: rows(), at },
        then: then.map(str::to_string),
    }
}

/// Words being typed for `adjust` under every option, `before` and `after` the cursor, and
/// why the words given were not taken where they were not.
fn saying((before, after): (&str, &str), refused: Option<&str>) -> CardBody {
    CardBody::Saying {
        choices: Choices {
            rows: rows(),
            at: 1,
        },
        asks: "what should change?".to_string(),
        typed: TypedRow {
            prompt: "> ".to_string(),
            before: before.to_string(),
            after: after.to_string(),
        },
        refused: refused.map(str::to_string),
    }
}

fn look(glyphs: Glyphs, cells: usize) -> Look {
    Look {
        glyphs,
        paint: Paint::Plain,
        width: Width::row(cells, None),
        motion: crate::motion::Motion::Moves,
    }
}

fn drawn(card: &Card, look: &Look) -> Vec<String> {
    card.drawn(look, usize::MAX)
        .lines
        .iter()
        .map(|line| line.text())
        .collect()
}

#[test]
fn a_card_asks_and_points_at_one_option_with_what_follows_it() {
    let then = "review ×2, conform → ship";
    assert_eq!(
        drawn(
            &card(choosing(0, Some(then)), Glyphs::Unicode),
            &look(Glyphs::Unicode, 80)
        ),
        [
            "▎ ◆ Plan and its tests registered. Approve?                approve-plan · 3GQF5Q",
            "▎ 8 files, +295 −73",
            "▎",
            "▎ ▸ approve  On to `implement`: resolves this gate",
            "▎   adjust   Back to `plan`: sends what you say, and asks again once it",
            "▎            completes",
            "▎   abort    Pause here: nothing runs; this decision stays open",
            "▎",
            "▎ then  review ×2, conform → ship",
            "▎",
            "▎ ↑↓ choose · enter answer · n add a note · esc leave it for later",
        ]
    );
}

/// The pointer moves and nothing else does: every option says what it says
/// wherever the pointer stands. An option with no way on says no `then`.
#[test]
fn the_pointer_moves_and_no_row_changes_place() {
    let rows = drawn(
        &card(choosing(2, None), Glyphs::Unicode),
        &look(Glyphs::Unicode, 80),
    );
    assert_eq!(
        rows[3],
        "▎   approve  On to `implement`: resolves this gate"
    );
    assert_eq!(
        rows[6],
        "▎ ▸ abort    Pause here: nothing runs; this decision stays open"
    );
    assert_eq!(rows[7], "▎");
    assert_eq!(
        rows[8],
        "▎ ↑↓ choose · enter answer · n add a note · esc leave it for later"
    );
}

#[test]
fn ascii_draws_its_own_edge_and_pointer() {
    let rows = drawn(
        &card(choosing(1, None), Glyphs::Ascii),
        &look(Glyphs::Ascii, 80),
    );
    assert_eq!(
        rows[0],
        "| ? Plan and its tests registered. Approve?                approve-plan ; 3GQF5Q"
    );
    assert_eq!(
        rows[3],
        "|   approve  On to `implement`: resolves this gate"
    );
    assert_eq!(
        rows[4],
        "| * adjust   Back to `plan`: sends what you say, and asks again once it"
    );
}

/// Where a field is open, the card keeps every option, the pointer on the one chosen, and
/// under them the words it is said with, in a box across the card, and says where the cursor
/// stands: on the box's middle row, past the edge, the box's upright and the cell inside it,
/// and the part of the row before it.
#[test]
fn a_field_opens_under_every_option_with_the_cursor_on_it() {
    let mut card = card(saying(("keep the ", "lock"), None), Glyphs::Unicode);
    card.legend = Legend::new().key("enter", "send it").key("esc", "go back");
    let placed = card.drawn(&look(Glyphs::Unicode, 80), usize::MAX);
    let rows: Vec<String> = placed.lines.iter().map(|line| line.text()).collect();
    let level = "─".repeat(76);
    assert_eq!(
        rows,
        [
            "▎ ◆ Plan and its tests registered. Approve?                approve-plan · 3GQF5Q"
                .to_string(),
            "▎ 8 files, +295 −73".to_string(),
            "▎".to_string(),
            "▎   approve  On to `implement`: resolves this gate".to_string(),
            "▎ ▸ adjust   Back to `plan`: sends what you say, and asks again once it".to_string(),
            "▎            completes".to_string(),
            "▎   abort    Pause here: nothing runs; this decision stays open".to_string(),
            format!("▎ ╭{level}╮"),
            format!("▎ │ keep the lock{} │", " ".repeat(61)),
            format!("▎ ╰{level}╯"),
            "▎".to_string(),
            "▎ enter send it · esc go back".to_string(),
        ]
    );
    assert_eq!(placed.cursor, Some((8, 13)));
}

/// While nothing is typed, what is asked of the words stands in the box where they go; what
/// a field refuses is said under its box, and the cursor stays in the box, on the first cell.
#[test]
fn a_refused_answer_is_said_under_its_field() {
    let body = saying(("", ""), Some("this option needs an answer"));
    let placed = card(body, Glyphs::Unicode).drawn(&look(Glyphs::Unicode, 80), usize::MAX);
    let rows: Vec<String> = placed.lines.iter().map(|line| line.text()).collect();
    let asked = format!("▎ │ what should change?{} │", " ".repeat(55));
    assert_eq!(rows[8], asked);
    assert_eq!(rows[9], format!("▎ ╰{}╯", "─".repeat(76)));
    assert_eq!(rows[10], "▎ ✗ this option needs an answer");
    assert_eq!(placed.cursor, Some((8, 4)));
}

/// The name of an option with no words of its own stands alone, and a row with
/// only a cost says it in the cost's tone.
#[test]
fn a_row_says_only_what_it_has() {
    let choices = Choices {
        rows: vec![row("yes", None, None), row("no", None, Some("stops here"))],
        at: 0,
    };
    let lines = choices.lines(&look(Glyphs::Unicode, 80), 80, usize::MAX, Said::Wrapped);
    let texts: Vec<String> = lines.iter().map(|line| line.text()).collect();
    assert_eq!(texts, ["▸ yes", "  no   stops here"]);
    assert_eq!(
        lines[1].spans().last().map(|span| span.tone),
        Some(Tone::Muted)
    );
}

/// A list longer than its room shows the rows around the pointer and counts the
/// rest, so the pointer is never off the screen.
#[test]
fn a_long_list_shows_the_rows_around_the_pointer_and_counts_the_rest() {
    let choices = Choices {
        rows: ["a", "b", "c", "d", "e"]
            .map(|name| row(name, None, None))
            .into(),
        at: 4,
    };
    let lines = choices.lines(&look(Glyphs::Unicode, 80), 80, 3, Said::Wrapped);
    let texts: Vec<String> = lines.iter().map(|line| line.text()).collect();
    assert_eq!(texts, ["  c", "  d", "▸ e", "… 2 more options"]);
}

/// On a narrow line a row's text wraps under itself and no row passes the edge.
#[test]
fn no_row_of_a_card_outruns_its_line() {
    for cells in 30..=120 {
        for glyphs in [Glyphs::Unicode, Glyphs::Ascii] {
            let look = look(glyphs, cells);
            for line in drawn(&card(choosing(1, Some("a → b → c")), glyphs), &look) {
                assert!(cell_width(&line) <= cells, "at {cells}: {line}");
            }
        }
    }
}

/// A name, an edge or a refusal wider than the line is cut or wrapped to it.
#[test]
fn what_is_too_wide_for_the_line_is_held_to_it() {
    let long = "a-name-far-longer-than-any-line-this-card-is-drawn-on-should-ever-hold";
    let body = CardBody::Choosing {
        choices: Choices {
            rows: vec![row(long, Some("says what it does"), None)],
            at: 0,
        },
        then: None,
    };
    let mut wide = card(body, Glyphs::Unicode);
    wide.edge = Some(long.to_string());
    let body = CardBody::Saying {
        choices: Choices {
            rows: vec![row(long, None, None)],
            at: 0,
        },
        asks: "what should change?".to_string(),
        typed: TypedRow {
            prompt: "> ".to_string(),
            before: "words".to_string(),
            after: String::new(),
        },
        refused: Some("this option needs an answer".to_string()),
    };
    let saying = card(body, Glyphs::Unicode);
    for cells in 30..=70 {
        for card in [&wide, &saying] {
            for line in drawn(card, &look(Glyphs::Unicode, cells)) {
                assert!(cell_width(&line) <= cells, "at {cells}: {line}");
            }
        }
    }
}

pub(in crate::surface::terminal) fn eight_rows() -> Vec<ChoiceRow> {
    ["a", "b", "c", "d", "e", "f", "g", "h"]
        .map(|name| row(name, Some("does it"), None))
        .into()
}

/// Eight options pointed at their seventh, with a route: a card short of rows.
fn eight_with_a_route() -> Card {
    let body = CardBody::Choosing {
        choices: Choices {
            rows: eight_rows(),
            at: 6,
        },
        then: Some("x → y".to_string()),
    };
    card(body, Glyphs::Unicode)
}

/// A short terminal keeps the question and every option: the blank rows and the route
/// give way first, and what stands under the question keeps the rows left.
#[test]
fn on_a_short_terminal_the_blank_rows_and_the_route_give_way_first() {
    assert_eq!(
        in_rows(&eight_with_a_route(), 11),
        [
            "▎ ◆ Plan and its tests registered. Approve?                approve-plan · 3GQF5Q",
            "▎ 8 files, +295 −73",
            "▎   a  does it",
            "▎   b  does it",
            "▎   c  does it",
            "▎   d  does it",
            "▎   e  does it",
            "▎   f  does it",
            "▎ ▸ g  does it",
            "▎   h  does it",
            "▎ ↑↓ choose · enter answer · n add a note · esc leave it for later",
        ]
    );
}

/// Shorter still, what stands under the question gives way, then the list counts what it
/// leaves out around the row pointed at: the question, the options and the keys are what
/// a person answers with.
#[test]
fn shorter_still_the_list_counts_what_it_leaves_out_and_the_keys_stay() {
    assert_eq!(
        in_rows(&eight_with_a_route(), 6),
        [
            "▎ ◆ Plan and its tests registered. Approve?                approve-plan · 3GQF5Q",
            "▎   e  does it",
            "▎   f  does it",
            "▎ ▸ g  does it",
            "▎ … 5 more options",
            "▎ ↑↓ choose · enter answer · n add a note · esc leave it for later",
        ]
    );
}

/// With no room for the keys beside the row pointed at, a person who has answered a card
/// before still has the question and the row pointed at: the keys give way last.
#[test]
fn shortest_of_all_the_keys_give_way_before_the_question() {
    assert_eq!(
        in_rows(&eight_with_a_route(), 3),
        [
            "▎ ◆ Plan and its tests registered. Approve?                approve-plan · 3GQF5Q",
            "▎ ▸ g  does it",
            "▎ … 7 more options",
        ]
    );
}

/// `card` in `rows` rows of a line 80 cells wide: each row's text.
fn in_rows(card: &Card, rows: usize) -> Vec<String> {
    card.drawn(&look(Glyphs::Unicode, 80), rows)
        .lines
        .iter()
        .map(|line| line.text())
        .collect()
}

/// A question that wraps keeps its first row on a short, narrow terminal: the far
/// edge gives up the row it would take, and the question is what a person reads first.
#[test]
fn a_long_question_keeps_its_first_row_on_a_short_narrow_terminal() {
    let body = CardBody::Choosing {
        choices: Choices {
            rows: eight_rows(),
            at: 0,
        },
        then: None,
    };
    let mut asked = card(body, Glyphs::Unicode);
    asked.question =
        "Plan and its tests registered, with every change it makes and each risk it takes?"
            .to_string();
    let placed = asked.drawn(&look(Glyphs::Unicode, 40), 9);
    let rows: Vec<String> = placed.lines.iter().map(|line| line.text()).collect();
    assert!(rows.len() <= 9, "{rows:#?}");
    assert!(rows[0].starts_with("▎ ◆ Plan and its tests"), "{rows:#?}");
    assert!(rows.iter().any(|row| row.starts_with("▎ ▸ a")), "{rows:#?}");
}

/// The box a field is typed in spans what the card's edge leaves of the line.
#[test]
fn a_fields_box_spans_what_the_edge_leaves() {
    assert_eq!(Card::typed_room(&look(Glyphs::Unicode, 80)), 78);
}

/// Every row is the decision's: tinted where a layer draws fills, its edge in the
/// person's tone, and the row pointed at set apart.
#[test]
fn every_row_is_tinted_and_the_row_pointed_at_is_set_apart() {
    let lines = card(choosing(1, None), Glyphs::Unicode)
        .drawn(&look(Glyphs::Unicode, 80), usize::MAX)
        .lines;
    for line in &lines {
        assert_eq!(line.spans()[0].tone, Tone::NeedsYou, "{}", line.text());
    }
    let pointed = lines
        .iter()
        .filter(|line| line.fill() == Some(Fill::Selection))
        .map(|line| line.text());
    assert_eq!(
        pointed.collect::<Vec<_>>(),
        [
            "▎ ▸ adjust   Back to `plan`: sends what you say, and asks again once it",
            "▎            completes",
        ]
    );
    assert!(lines
        .iter()
        .all(|line| matches!(line.fill(), Some(Fill::Tint | Fill::Selection))));
}

/// A card in every environment, 24-bit ones among them.
#[test]
fn a_card_matches_its_goldens_in_every_environment() {
    let goldens = Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/card");
    for environment in ENVIRONMENTS.iter().chain(&RICH) {
        let look = Look::of(environment);
        let then = format!(
            "review {times}2, conform {arrow} ship",
            arrow = look.glyphs.arrow(),
            times = look.glyphs.times(),
        );
        let drawn: String = card(choosing(0, Some(&then)), look.glyphs)
            .drawn(&look, usize::MAX)
            .lines
            .iter()
            .map(|line| look.paint.line(line) + "\n")
            .collect();
        assert_golden(&environment.golden(&goldens, "choosing"), &drawn);
    }
}

/// A card whose keys stand in a screen's legend draws none of its own, and leaves what
/// follows an option to the card printed with its keys: it ends on its last option.
#[test]
fn a_card_with_no_keys_draws_no_foot_and_ends_on_its_last_option() {
    let mut on_a_screen = card(
        choosing(1, Some("plan, then this gate asks again")),
        Glyphs::Unicode,
    );
    on_a_screen.legend = Legend::new();
    assert_eq!(
        drawn(&on_a_screen, &look(Glyphs::Unicode, 80)),
        [
            "▎ ◆ Plan and its tests registered. Approve?                approve-plan · 3GQF5Q",
            "▎ 8 files, +295 −73",
            "▎",
            "▎   approve  On to `implement`: resolves this gate",
            "▎ ▸ adjust   Back to `plan`: sends what you say, and asks again once it",
            "▎            completes",
            "▎   abort    Pause here: nothing runs; this decision stays open",
        ]
    );
}

/// A card's keys read in one muted run, each key beside what it does and the
/// glyph set's separator between two, in each glyph set.
#[test]
fn a_cards_legend_reads_as_its_keys_read() {
    for (glyphs, foot) in [
        (
            Glyphs::Unicode,
            "▎ ↑↓ choose · enter answer · n add a note · esc leave it for later",
        ),
        (
            Glyphs::Ascii,
            "| up/down choose ; enter answer ; n add a note ; esc leave it for later",
        ),
    ] {
        let rows = drawn(&card(choosing(0, None), glyphs), &look(glyphs, 100));
        assert_eq!(rows.last().map(String::as_str), Some(foot), "{rows:#?}");
    }
}
