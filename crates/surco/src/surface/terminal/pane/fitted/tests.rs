//! A region with no prompt and no feed in a pane too short for all of it: the first of what
//! it says in fewer rows that fits, else all of it, scrolled; a page on a screen short of
//! rows, its blank rows given way.

use crate::components::{Check, Checklist, Fields, Fix, Found, Region, Section, Sight};
use crate::doc::{Block, Doc};
use crate::glyphs::Glyphs;
use crate::golden::ENVIRONMENTS;
use crate::line::Line;
use crate::look::{Look, Width};
use crate::surface::Terminal;

/// The look of a pane 80 cells wide, in Unicode with no paint: a pane's width is exact.
fn pane_look() -> Look {
    Look {
        width: Width::exact(ENVIRONMENTS[1].width),
        ..Look::of(&ENVIRONMENTS[1])
    }
}

fn texts(lines: &[Line]) -> Vec<String> {
    lines.iter().map(Line::text).collect()
}

/// A region with no prompt and no feed that says itself in fewer rows draws the first of
/// those that fits where its rows are too few for all of it, and all of it where they are
/// not; where none fits, all of it scrolls.
#[test]
fn a_region_with_no_feed_too_long_for_its_rows_says_itself_in_fewer() {
    let terminal = Terminal::on(pane_look());
    let rows = |lines: &[&str]| {
        let lines = lines.iter().map(|line| Line::new().plain(*line)).collect();
        vec![Block::Rows(lines)]
    };
    let doc = || {
        Doc::new()
            .with(Block::Heading("what it waits on".to_string()))
            .with_all(rows(&["one", "two", "three"]))
            .with_all(rows(&["free", "needed"]))
    };
    let fewer = vec![
        rows(&["free", "needed", "and more"]),
        rows(&["free", "needed"]),
    ];
    let region = Region::new(doc()).abridged(fewer);
    assert_eq!(
        texts(&terminal.pane(&region, 7).lines),
        [
            "what it waits on",
            "one",
            "two",
            "three",
            "",
            "free",
            "needed"
        ]
    );
    assert_eq!(
        texts(&terminal.pane(&region, 3).lines),
        ["free", "needed", "and more"]
    );
    assert_eq!(texts(&terminal.pane(&region, 2).lines), ["free", "needed"]);
    let (scrolled, _) = terminal.scrolled(&doc(), Sight::default(), 1);
    assert_eq!(texts(&terminal.pane(&region, 1).lines), texts(&scrolled));
}

/// A page too long for its pane keeps its blank rows and counts the rows it hides under its
/// window; on a screen standing short of rows, it stands close, its blank rows given way
/// before its rows are counted.
#[test]
fn a_page_on_a_screen_short_of_rows_gives_its_blank_rows_way() {
    let terminal = Terminal::on(pane_look());
    let lines = |at: usize| (at..at + 6).map(|line| Line::new().plain(format!("line {line}")));
    let blank_between = lines(1).chain([Line::new()]).chain(lines(7)).collect();
    let page = Region::new(Doc::new().with(Block::Rows(blank_between)));
    let aired = texts(&terminal.pane(&page, 10).lines);
    assert_eq!(aired[6], "", "it keeps its blank rows");
    assert_eq!(aired[9], "... 4 more lines");
    let close = texts(&terminal.pane_short_of_rows(&page, 10).lines);
    assert_eq!(close[6], "line 7", "its blank row given way");
    assert_eq!(close[9], "... 3 more lines");
}

/// A page on a screen standing short of rows says each labelled value and each checklist row
/// on its own row, cut at the pane's edge, where a taller screen wraps them under their first
/// column; what a person pastes stands as written, where a taller screen sets it.
#[test]
fn a_page_on_a_screen_short_of_rows_cuts_each_value_to_its_row() {
    let cells = 40;
    let terminal = Terminal::on(Look {
        width: Width::exact(cells),
        glyphs: Glyphs::Unicode,
        ..pane_look()
    });
    let page = || {
        let found = "none declared: a workflow's agent nodes cannot run";
        let fix = Fix {
            said: "declare a runner in .yunta/config.yaml".to_string(),
            paste: vec!["runners:".to_string()],
        };
        let mut checks = Checklist::default();
        checks.push_check(Check::new(Found::Caution, "runners", found).fix(fix));
        let now = "0 checkouts, builds 0 B of a 98.9 GB budget";
        let fields = Fields::new().push_if("now", now);
        Region::new(Doc::new().with(fields).with(checks))
    };
    let aired = texts(&terminal.pane(&page(), 20).lines);
    assert_eq!(aired.len(), 8, "a taller screen wraps them: {aired:#?}");
    let close = texts(&terminal.pane_short_of_rows(&page(), 20).lines);
    assert_eq!(close.len(), 4, "a row each: {close:#?}");
    for row in &close[..3] {
        assert!(
            row.ends_with('…') && crate::width::cell_width(row) <= cells,
            "{close:#?}"
        );
    }
    assert!(
        close[0].starts_with("  now          0 checkouts"),
        "{close:#?}"
    );
    assert!(
        close[1].starts_with("  ▲ runners  none declared"),
        "{close:#?}"
    );
    assert!(
        close[2].starts_with("    fix      declare a runner"),
        "{close:#?}"
    );
    assert_eq!(
        close.last(),
        aired.last(),
        "what is pasted stands where it stood"
    );
}

/// A page that keeps its values and its checks inside sections, as a workflow checked in the
/// catalog's place does, says each on its own row on a screen standing short of rows too: a
/// section's blocks are drawn one step in by the same rule as the page's own.
#[test]
fn a_page_on_a_screen_short_of_rows_cuts_each_value_in_a_section_to_its_row() {
    let cells = 40;
    let terminal = Terminal::on(Look {
        width: Width::exact(cells),
        glyphs: Glyphs::Unicode,
        ..pane_look()
    });
    let page = || {
        let mut checks = Checklist::default();
        checks.push(
            Found::Caution,
            "runners",
            "none declared: a workflow's agent nodes cannot run",
        );
        let fields = Fields::new().push_if("now", "0 checkouts, builds 0 B of a 98.9 GB budget");
        let section = Section {
            lead: None,
            title: Line::new().plain("lint-fix"),
            blocks: vec![fields.into(), checks.into()],
        };
        Region::new(Doc::new().with(section))
    };
    let aired = texts(&terminal.pane(&page(), 20).lines);
    assert!(aired.len() > 4, "a taller screen wraps them: {aired:#?}");
    let close = texts(&terminal.pane_short_of_rows(&page(), 20).lines);
    assert_eq!(close.len(), 3, "the title and a row each: {close:#?}");
    for row in &close[1..] {
        assert!(
            row.ends_with('…') && crate::width::cell_width(row) <= cells,
            "{close:#?}"
        );
    }
}

/// A page read along a pointer through its rows, as the packs installed are, stands close on
/// a screen standing short of rows as a page read from its top does, its pointed row kept; a
/// taller screen keeps its blank rows.
#[test]
fn a_page_along_a_pointer_stands_close_on_a_screen_short_of_rows() {
    let terminal = Terminal::on(pane_look());
    let page = || {
        let mut packs = Checklist::default().pointing_at("acme/demo".to_string());
        packs.push(Found::Holds, "acme/demo", "main, as its lock records it");
        let lock = Fields::new().push_if("lock", ".yunta/yunta.lock");
        let doc = Doc::new().with(packs).with(lock);
        Region::new(doc).in_sight(Sight::following_pointer())
    };
    let aired = texts(&terminal.pane(&page(), 10).lines);
    assert_eq!(aired.len(), 3, "a blank row between the two: {aired:#?}");
    let close = texts(&terminal.pane_short_of_rows(&page(), 10).lines);
    assert_eq!(close.len(), 2, "a row each: {close:#?}");
    assert!(close[0].starts_with("* + acme/demo"), "{close:#?}");
    assert!(close[1].starts_with("  lock"), "{close:#?}");
}
