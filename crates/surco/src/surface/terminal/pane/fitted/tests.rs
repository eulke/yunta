//! A region with no prompt and no feed in a pane too short for all of it: the first of what
//! it says in fewer rows that fits, else all of it, scrolled; a page on a screen short of
//! rows, its blank rows given way.

use crate::components::{Region, Sight};
use crate::doc::{Block, Doc};
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
