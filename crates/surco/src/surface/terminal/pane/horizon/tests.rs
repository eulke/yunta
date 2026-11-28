//! A region whose document ends with a horizon, in a pane: its blocks first, then, a blank
//! row under them, the horizon in every row they leave; given fewer than the horizon's least,
//! or no braille to draw it in, the blocks alone, standing where they stood; given too few for
//! the blocks, their blank rows giving way before their rows scroll.

use std::path::{Path, PathBuf};

use crate::components::{
    Checklist, Fields, Found, Horizon, Measure, Region, TypedAnswer, TypedRow,
};
use crate::doc::{Block, Doc};
use crate::figures::files_moving;
use crate::glyphs::Glyphs;
use crate::golden::assert_golden;
use crate::line::{Line, Tone};
use crate::look::{Look, Width};
use crate::motion::Motion;
use crate::paint::Paint;
use crate::surface::Terminal;

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/pane")
}

/// A terminal whose pane is `cells` cells across, drawing with `glyphs`, unpainted.
fn terminal(glyphs: Glyphs, cells: usize) -> Terminal {
    Terminal::on(Look {
        glyphs,
        paint: Paint::Plain,
        width: Width::exact(cells),
        motion: Motion::Moves,
    })
}

fn texts(lines: &[Line]) -> Vec<String> {
    lines.iter().map(Line::text).collect()
}

/// Three blocks of a row each, `one`, `two` and `three`, over the harvested field.
fn three_over_the_field() -> Region {
    let row = |said: &str| Block::Rows(vec![Line::new().plain(said)]);
    Region::new(
        Doc::new()
            .with(row("one"))
            .with(row("two"))
            .with(row("three"))
            .with(Horizon { tone: Tone::Done }),
    )
}

/// The rows three blocks of a row each take, a blank row between two.
const THREE_BLOCKS: [&str; 5] = ["one", "", "two", "", "three"];

/// Given more rows than its blocks take, a pane draws the blocks, a blank row, then the
/// horizon in every row left, as the horizon draws itself in that many rows.
#[test]
fn three_blocks_and_a_horizon_take_their_rows_and_the_horizon_every_row_left() {
    let terminal = terminal(Glyphs::Unicode, 40);
    let paned = terminal.pane(&three_over_the_field(), 12);
    let drawn = texts(&paned.lines);
    assert_eq!(drawn.len(), 12, "{drawn:#?}");
    assert_eq!(drawn[..5], THREE_BLOCKS, "{drawn:#?}");
    assert_eq!(drawn[5], "", "a blank row parts the blocks from the field");
    let field = terminal.horizon(&Horizon { tone: Tone::Done }, 6);
    assert_eq!(drawn[6..], texts(&field), "{drawn:#?}");
    assert!(!paned.laid.scrolls(), "{:?}", paned.laid);
}

/// Given fewer rows than the horizon's least under its blocks, a pane draws the blocks
/// alone, where they stood: the rows left stay blank.
#[test]
fn given_fewer_rows_than_the_horizons_least_the_blocks_stand_alone() {
    let terminal = terminal(Glyphs::Unicode, 40);
    let paned = terminal.pane(&three_over_the_field(), 8);
    assert_eq!(texts(&paned.lines), THREE_BLOCKS);
}

/// A layer with no braille draws no horizon: the blocks stand alone, however many rows the
/// pane has.
#[test]
fn ascii_draws_the_blocks_alone() {
    let terminal = terminal(Glyphs::Ascii, 40);
    let paned = terminal.pane(&three_over_the_field(), 12);
    assert_eq!(texts(&paned.lines), THREE_BLOCKS);
}

/// Short of rows for all the blocks, their blank rows give way before their rows do, and
/// what still stands under the window is counted.
#[test]
fn short_of_rows_for_the_blocks_their_blank_rows_give_way_first() {
    let terminal = terminal(Glyphs::Unicode, 40);
    let drawn = |rows| texts(&terminal.pane(&three_over_the_field(), rows).lines);
    assert_eq!(drawn(3), ["one", "two", "three"]);
    assert_eq!(drawn(2), ["one", "… 2 more lines"]);
}

/// A line asked over the field, `abc` typed in its box, a row under it: what a screen that
/// opens on one question over the field holds.
fn asked_over_the_field() -> Region {
    let asked = TypedAnswer {
        asked: Line::new().push(Tone::Strong, "What?"),
        typed: TypedRow {
            prompt: "> ".to_string(),
            before: "abc".to_string(),
            after: String::new(),
        },
        placeholder: None,
        hint: None,
    };
    Region::new(
        Doc::new()
            .with(asked)
            .with(Block::Rows(vec![Line::new().plain("under")]))
            .with(Horizon { tone: Tone::Faint }),
    )
}

/// The rows a line asked and a row under it take, the box as wide as `cells`.
fn asked_rows(cells: usize) -> Vec<String> {
    let edge = "─".repeat(cells - 2);
    let inside = format!("│ abc{}│", " ".repeat(cells - 6));
    vec![
        "What?".to_string(),
        format!("╭{edge}╮"),
        inside,
        format!("╰{edge}╯"),
        String::new(),
        "under".to_string(),
    ]
}

/// A line asked over the field is held as a prompt is, its cursor after what was typed in
/// its box, and the field takes every row under it after a blank row.
#[test]
fn a_line_asked_over_the_field_keeps_its_cursor_and_the_field_takes_the_rows_left() {
    let terminal = terminal(Glyphs::Unicode, 40);
    let paned = terminal.pane(&asked_over_the_field(), 14);
    let drawn = texts(&paned.lines);
    assert_eq!(drawn[..6], asked_rows(40), "{drawn:#?}");
    assert_eq!(drawn[6], "", "a blank row parts the words from the field");
    let field = terminal.horizon(&Horizon { tone: Tone::Faint }, 7);
    assert_eq!(drawn[7..], texts(&field), "{drawn:#?}");
    assert_eq!(paned.cursor, Some((2, 5)), "after `abc`, inside the box");
}

/// Words set to a measure narrower than the pane stand in a column that wide, centred, the
/// cursor moved with them; the field spans the pane under them. On a pane of 27 rows and more
/// they stand two rows lower, the field taking what they leave; on fewer, they open the pane.
#[test]
fn words_set_to_a_measure_stand_centred_over_a_field_that_spans_the_pane() {
    let terminal = terminal(Glyphs::Unicode, 40);
    let region = asked_over_the_field().measured(Measure::of(20).lowered());
    let centred = |row: &String| match row.is_empty() {
        true => String::new(),
        false => format!("{}{row}", " ".repeat(10)),
    };
    let words: Vec<String> = asked_rows(20).iter().map(centred).collect();
    for (rows, lowered) in [(26, 0), (27, 2)] {
        let paned = terminal.pane(&region, rows);
        let drawn = texts(&paned.lines);
        assert!(drawn[..lowered].iter().all(String::is_empty), "{drawn:#?}");
        assert_eq!(drawn[lowered..lowered + 6], words, "{drawn:#?}");
        let field = terminal.horizon(&Horizon { tone: Tone::Faint }, rows - lowered - 7);
        assert_eq!(drawn[lowered + 7..], texts(&field), "{drawn:#?}");
        assert_eq!(paned.cursor, Some((lowered + 2, 15)), "{drawn:#?}");
    }
    let narrow = terminal.pane(
        &asked_over_the_field().measured(Measure::of(60).lowered()),
        14,
    );
    assert_eq!(
        texts(&narrow.lines)[..6],
        asked_rows(40),
        "a measure past the pane"
    );
}

/// What a finished run's receipt holds over the harvested field, in `glyphs`: each check it
/// was held to, then what it spent, changed and opened.
fn receipt(glyphs: Glyphs) -> Region {
    let mut checks = Checklist::default();
    checks.push(
        Found::Holds,
        "criteria",
        "1 of 1 green, red before the work",
    );
    checks.push(
        Found::Holds,
        "baseline",
        "cargo test --workspace still passes",
    );
    checks.push(Found::Holds, "scope", "7 files, 2 of them granted by you");
    checks.push(Found::Holds, "review", "4 findings, 4 answered");
    checks.push(Found::Holds, "event chain", "intact");
    let facts = Fields::new()
        .push_if("cost", "27.1M tokens per verified task, 1 reroute")
        .push_if(
            "changed",
            files_moving("7 files".to_string(), 65, 24, glyphs),
        )
        .push_if(
            "pull request",
            "#31: https://github.com/eulke/yunta/pull/31",
        );
    Region::new(
        Doc::new()
            .with(checks)
            .with(facts)
            .with(Horizon { tone: Tone::Done }),
    )
}

/// The receipt's pane where a run's screen gives it rows: at 80 columns, 78 cells and 15
/// rows, the harvested field in the five its blocks leave; at 60, 58 cells and 6 rows, too
/// few for the field and for every block. Each is named for its glyphs and its screen's
/// columns, as the horizon's goldens are.
const PANES: [(&str, Glyphs, usize, usize); 4] = [
    ("unicode-80", Glyphs::Unicode, 78, 15),
    ("ascii-80", Glyphs::Ascii, 78, 15),
    ("unicode-60", Glyphs::Unicode, 58, 6),
    ("ascii-60", Glyphs::Ascii, 58, 6),
];

#[test]
fn a_receipt_over_the_harvested_field_matches_its_goldens() {
    for (name, glyphs, cells, rows) in PANES {
        let paned = terminal(glyphs, cells).pane(&receipt(glyphs), rows);
        let drawn: String = paned
            .lines
            .iter()
            .map(|line| format!("{}\n", Paint::Plain.line(line)))
            .collect();
        assert_golden(&goldens().join(format!("receipt.{name}.txt")), &drawn);
    }
}
