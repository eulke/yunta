//! A plan's desk on a terminal of an exact size. Every expected row here is written by
//! hand, and what a row ends in spaces is gone before it is compared.

use crate::components::{Desk, DeskFoot, FootRow, Laid, Legend, Pane, RailGroup, RailRow, Sight};
use crate::doc::{Block, Doc};
use crate::glyphs::Glyphs;
use crate::line::{Fill, Line};
use crate::look::{Look, Width};
use crate::paint::Paint;
use crate::surface::Placed;
use crate::surface::Terminal;
use crate::width::cell_width;

pub(super) fn look(glyphs: Glyphs, cells: usize) -> Look {
    Look {
        glyphs,
        paint: Paint::Plain,
        width: Width::exact(cells),
        motion: crate::motion::Motion::Moves,
    }
}

/// Rows of the pane, numbered, as many as asked.
pub(in crate::surface::terminal) fn numbered(rows: usize) -> Vec<Line> {
    (1..=rows)
        .map(|at| Line::new().plain(format!("line {at}")))
        .collect()
}

/// A desk whose pane has `rows` numbered rows and a pointer on the row at `at`, which
/// it keeps in sight.
fn pointing(rows: usize, at: usize) -> Desk {
    let mut lines = numbered(rows);
    lines[at] = Line::new().plain(format!("{} line {}", Glyphs::Unicode.pointer(), at + 1));
    let mut pointing = desk(lines);
    pointing.pane.sight.follows_pointer = true;
    pointing
}

pub(super) fn stop(lead: &str, name: &str, pointed: bool) -> RailRow {
    RailRow::Stop {
        lead: lead.to_string(),
        name: name.to_string(),
        pointed,
    }
}

/// The rail as a plan's desk composes it: its places in one untitled group, what it says
/// counted, and what weighs on it.
fn rail() -> Vec<RailGroup> {
    let tally = |what: &str, count: usize, key: Option<char>| RailRow::Tally {
        what: what.to_string(),
        count,
        key,
    };
    vec![
        RailGroup {
            title: None,
            rows: vec![
                stop("", "the change", false),
                stop("1", "storage", true),
                stop("2", "catalog", false),
            ],
        },
        RailGroup {
            title: Some("what it says".to_string()),
            rows: vec![
                tally("decided for you", 4, Some('d')),
                tally("out of scope", 3, None),
                tally("risks", 2, None),
            ],
        },
        RailGroup {
            title: Some("what weighs on it".to_string()),
            rows: vec![RailRow::Weighs(
                "the tests fail before the work, as they must".to_string(),
            )],
        },
    ]
}

pub(in crate::surface::terminal) fn desk(lines: Vec<Line>) -> Desk {
    Desk {
        title: vec![
            "plan".to_string(),
            "EKPBGE".to_string(),
            "harvest".to_string(),
            "2 tasks in 2 steps".to_string(),
        ],
        edge: Some("needs you  approve-plan".to_string()),
        gist: Some("Install packs in the state directory.".to_string()),
        asked: Some("add global install".to_string()),
        rail: rail(),
        pane: Pane {
            title: "Store packs".to_string(),
            edge: Some("step 1 of 2".to_string()),
            place: Some("task 1 of 2".to_string()),
            doc: Doc::new().with(Block::Lines(lines)),
            sight: Sight::default(),
        },
        foot: DeskFoot {
            question: "Plan registered. Approve?".to_string(),
            row: FootRow::Options {
                named: vec!["approve".to_string(), "adjust".to_string()],
                more: 0,
            },
            legend: Legend::new()
                .key("tab", "next place")
                .key("p", "proof")
                .key("esc", "back"),
        },
    }
}

/// `desk` as a terminal `rows` high holds it, over its last row: each row's text.
pub(super) fn drawn(desk: &Desk, look: &Look, rows: usize) -> Vec<String> {
    desk.drawn(look, rows - 1)
        .lines
        .iter()
        .map(|row| row.text().trim_end().to_string())
        .collect()
}

/// A row of a terminal 100 cells wide: the rail in its 30 cells, the rule, the pane.
fn row(rail: &str, pane: &str) -> String {
    format!(" {rail:<30} │ {pane}").trim_end().to_string()
}

/// `left` and `right` at the two ends of `cells` cells.
pub(super) fn ends(left: &str, right: &str, cells: usize) -> String {
    let gap = cells - left.chars().count() - right.chars().count();
    format!("{left}{}{right}", " ".repeat(gap))
}

/// The head's first row on a terminal `cells` wide, which keeps a margin at each end.
fn title_row(cells: usize) -> String {
    format!(
        " {}",
        ends(
            "plan · EKPBGE · harvest · 2 tasks in 2 steps",
            "needs you  approve-plan",
            cells - 2
        )
    )
}

/// The pane's own first row on a terminal 100 cells wide: 65 cells.
fn pane_title() -> String {
    ends("Store packs", "step 1 of 2", 65)
}

/// The foot's last row on a terminal `cells` wide.
fn options_row(cells: usize) -> String {
    format!(
        " {}",
        ends(
            "1 approve   2 adjust",
            "tab next place · p proof · esc back",
            cells - 2
        )
    )
}

/// What a desk drawn on a terminal laid out of its pane.
fn laid_pane(laid: &Placed) -> Laid {
    laid.laid.expect("a desk lays its pane out")
}

#[test]
fn a_wide_terminal_has_the_head_the_rail_the_pane_and_the_foot() {
    let rows = drawn(&desk(numbered(3)), &look(Glyphs::Unicode, 100), 24);
    assert_eq!(
        rows,
        [
            title_row(100),
            " Install packs in the state directory.".to_string(),
            " you asked  add global install".to_string(),
            String::new(),
            row("    the change", &pane_title()),
            row("1 ▸ storage", ""),
            row("2   catalog", "line 1"),
            row("", "line 2"),
            row("what it says", "line 3"),
            row("decided for you           4  d", ""),
            row("out of scope              3", ""),
            row("risks                     2", ""),
            row("", ""),
            row("what weighs on it", ""),
            row("▲ the tests fail before the", ""),
            row("  work, as they must", ""),
            row("", ""),
            row("", ""),
            row("", ""),
            row("", ""),
            String::new(),
            " ◆ Plan registered. Approve?".to_string(),
            options_row(100),
        ]
    );
    assert_eq!(rows.len(), 23, "one row under the desk is never drawn on");
}

#[test]
fn the_panes_measure_follows_the_terminal() {
    let cells = |columns| Desk::pane_cells(&look(Glyphs::Unicode, columns));
    assert_eq!(cells(100), 65, "a rail of 30, its rule and two margins");
    assert_eq!(cells(90), 57, "a rail is never under 28");
    assert_eq!(cells(89), 87, "with no rail the pane has all of it");
    assert_eq!(cells(300), 120, "and it is never wider than a line is read");
}

/// The rows it was asked for, whatever it holds.
#[test]
fn it_fills_the_rows_it_is_given_and_nothing_reaches_the_last_column() {
    let desks = [desk(Vec::new()), desk(numbered(500))];
    for desk in &desks {
        for cells in [20, 40, 60, 89, 90, 100, 118, 200, 300] {
            for rows in 1..=59 {
                let drawn = desk.drawn(&look(Glyphs::Unicode, cells), rows);
                assert_eq!(drawn.lines.len(), rows, "{cells}x{rows}");
                for line in &drawn.lines {
                    let width = cell_width(&line.text());
                    assert!(width < cells, "{cells}x{rows}: {} {width}", line.text());
                }
            }
        }
    }
}

/// What gives way on a short terminal: the groups of the rail from the last one, and of
/// the head what was asked and then what the plan is about. The pointer stays in sight.
#[test]
fn a_short_terminal_keeps_the_pointer_the_pane_and_the_question() {
    let rows = drawn(&desk(numbered(30)), &look(Glyphs::Unicode, 100), 14);
    assert_eq!(
        rows,
        [
            title_row(100),
            String::new(),
            row("    the change", &pane_title()),
            row("1 ▸ storage", ""),
            row("2   catalog", "line 1"),
            row("", "line 2"),
            row("", "line 3"),
            row("", "line 4"),
            row("", "line 5"),
            row("", "… 25 more lines"),
            String::new(),
            " ◆ Plan registered. Approve?".to_string(),
            options_row(100),
        ],
    );
}

/// Below ninety columns the pane has the screen, and says where it is, which the rail
/// says when there is one.
#[test]
fn a_narrow_terminal_has_no_rail_and_the_pane_says_which_task() {
    let rows = drawn(&desk(numbered(2)), &look(Glyphs::Unicode, 80), 13);
    assert_eq!(
        rows,
        [
            title_row(80),
            String::new(),
            format!(" {}", ends("Store packs", "task 1 of 2 · step 1 of 2", 78)),
            String::new(),
            " line 1".to_string(),
            " line 2".to_string(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            " ◆ Plan registered. Approve?".to_string(),
            options_row(80),
        ],
    );
}

#[test]
fn the_pane_scrolls_and_counts_what_is_below_it() {
    let mut scrolled = desk(numbered(30));
    scrolled.pane.sight.top = 5;
    let laid = scrolled.drawn(&look(Glyphs::Unicode, 100), 23);
    let text: Vec<String> = laid.lines.iter().map(Line::text).collect();
    let pane = laid_pane(&laid);
    let body = |row: &str| text.iter().any(|line| line.ends_with(row));
    assert!(body("line 6") && !body("line 5"), "{text:#?}");
    assert_eq!(pane.top, 5);
    assert_eq!(pane.page, 14, "the rows the pane's body has");
    assert_eq!(pane.most, 16, "its last rows are all in sight from there");
    assert!(body("… 12 more lines"), "{text:#?}");

    scrolled.pane.sight.top = usize::MAX;
    let last = scrolled.drawn(&look(Glyphs::Unicode, 100), 23);
    let at_end = laid_pane(&last).top;
    assert_eq!(at_end, 16, "a scroll past the end stops at the end");
    let text: Vec<String> = last.lines.iter().map(Line::text).collect();
    assert!(
        text.iter().any(|line| line.ends_with("line 30")),
        "{text:#?}"
    );
    assert!(!text.iter().any(|line| line.contains("more line")));
}

/// One row of the pane can be asked to stay in sight: a pointer the person moved.
#[test]
fn the_row_a_pointer_stands_on_stays_in_sight() {
    let top = |desk: &Desk| laid_pane(&desk.drawn(&look(Glyphs::Unicode, 100), 23)).top;
    assert_eq!(top(&pointing(60, 40)), 40);
    assert_eq!(top(&pointing(60, 3)), 0);
    let mut scrolled_past = pointing(60, 3);
    scrolled_past.pane.sight.top = 30;
    assert_eq!(top(&scrolled_past), 3);
}

/// A choice about to be made says what it does and how to make it.
#[test]
fn ascii_draws_it_with_what_ascii_has() {
    let mut plain = desk(numbered(3));
    plain.title = vec!["plan".to_string(), "EKPBGE".to_string()];
    plain.foot.legend = Legend::new().key("tab", "next place").key("p", "proof");
    let rows = drawn(&plain, &look(Glyphs::Ascii, 100), 24);
    assert!(rows.iter().all(|row| row.is_ascii()), "{rows:#?}");
    assert!(
        rows.iter().any(|row| row.contains("1 * storage")),
        "{rows:#?}"
    );
    assert!(
        rows.iter().any(|row| row.contains(" | line 1")),
        "{rows:#?}"
    );
}

/// A title or a path too long for its place is cut, what stands at the title's far end
/// having given way first.
#[test]
fn what_does_not_fit_its_place_is_cut() {
    let mut long = desk(numbered(1));
    long.title = vec!["w".repeat(200)];
    long.gist = Some("g".repeat(300));
    long.foot.question = "q".repeat(300);
    long.rail[0].rows = vec![stop("1", &format!("storage-{}", "x".repeat(60)), true)];
    let rows = drawn(&long, &look(Glyphs::Unicode, 100), 30);
    assert!(!rows[0].contains("needs you"), "{rows:#?}");
    assert!(rows[0].ends_with('…'), "{rows:#?}");
    assert!(rows[1].ends_with('…'), "{rows:#?}");
    assert!(
        rows.iter()
            .any(|row| row.contains("1 ▸ storage-x") && row.contains("x… │")),
        "a place's name is cut at the rail's edge: {rows:#?}"
    );
    assert!(rows.iter().any(|row| row.contains("q…")), "{rows:#?}");
}

/// What fits stands whole. What stands at the far end of a row goes only when what it
/// shares the row with would be left with nothing to read.
#[test]
fn the_pane_says_where_it_is_as_long_as_there_is_room() {
    let rows = drawn(&desk(numbered(1)), &look(Glyphs::Unicode, 40), 24);
    assert_eq!(
        rows[4],
        format!(" {}", ends("Store packs", "task 1 of 2 · step 1 of 2", 38)),
        "{rows:#?}"
    );
    let mut long = desk(numbered(1));
    long.pane.title = "t".repeat(60);
    let rows = drawn(&long, &look(Glyphs::Unicode, 60), 24);
    assert!(
        rows[4].ends_with("task 1 of 2 · step 1 of 2") && rows[4].contains('…'),
        "{rows:#?}"
    );
}

/// What stands at the far end of the title's row gives way whole before the title gives
/// up a part, as the trail names the node too; then the title keeps as many of its parts
/// whole as fit: a handle is never half said.
#[test]
fn a_title_gives_up_whole_parts_from_its_end() {
    let mut named = desk(numbered(1));
    named.title = ["plan", "EKPBGE", "reviewed", "2 tasks in 2 steps"]
        .map(str::to_string)
        .to_vec();
    let whole = "plan · EKPBGE · reviewed · 2 tasks in 2 steps";
    let first = |cells: usize| drawn(&named, &look(Glyphs::Unicode, cells), 24).remove(0);
    let edge = "needs you  approve-plan";
    assert_eq!(first(100), format!(" {}", ends(whole, edge, 98)));
    assert_eq!(first(60), format!(" {whole}"));
    assert_eq!(first(30), " plan · EKPBGE · reviewed");
}

/// Held close, as a pane on a screen short of rows holds it, a desk lets its own blank rows
/// go, under its head, under its pane's title and over its question, and reads its pane in
/// the rows they leave; held aired in as many rows, it keeps them.
#[test]
fn a_desk_held_close_reads_its_pane_in_its_own_blank_rows() {
    let (desk, look) = (desk(numbered(9)), look(Glyphs::Unicode, 60));
    let rows = |laid: Placed| laid.lines.iter().map(Line::text).collect::<Vec<_>>();
    let close = rows(desk.drawn_close(&look, 10));
    assert!(close.iter().all(|row| !row.trim().is_empty()), "{close:#?}");
    assert!(close[1].starts_with(" Store packs"), "{close:#?}");
    assert_eq!(
        close[2..8],
        [
            " line 1",
            " line 2",
            " line 3",
            " line 4",
            " line 5",
            " … 4 more lines"
        ]
    );
    let aired = rows(desk.drawn(&look, 10));
    assert_eq!([aired[1].trim(), aired[3].trim()], ["", ""], "{aired:#?}");
}

/// A pointed row is set apart across the rail where a layer draws fills, and the fill
/// stops at the rule.
#[test]
fn the_row_pointed_at_is_set_apart_up_to_the_rule() {
    let laid = desk(numbered(3)).drawn(&look(Glyphs::Unicode, 100), 23);
    let pointed = laid
        .lines
        .iter()
        .find(|line| line.text().contains("▸ storage"))
        .expect("the row pointed at");
    let filled = |text: &str| {
        pointed
            .spans()
            .iter()
            .filter(|span| span.fill == Some(Fill::Selection))
            .any(|span| span.text.contains(text))
    };
    assert!(filled("storage"), "{:?}", pointed.spans());
    assert!(!filled("│"), "{:?}", pointed.spans());
}

/// Held, a desk takes every cell the terminal has, past the measure a line of text
/// gets, and still leaves its last column alone.
#[test]
fn a_held_desk_is_as_wide_as_the_terminal() {
    let line = look(Glyphs::Unicode, 100);
    let held = Terminal::on(line).held(&Block::from(desk(numbered(3))), 180, 30);
    let widest = held
        .lines
        .iter()
        .map(|row| cell_width(&row.text()))
        .max()
        .unwrap_or(0);
    assert!(widest > line.width.cells() && widest < 180, "{widest}");
}
