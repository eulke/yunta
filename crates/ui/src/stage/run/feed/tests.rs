//! A node's feed in its window: render is handed the rows around where the feed stands and
//! no more, however long the feed grows, counted from its first row, so a feed scrolled back
//! keeps its rows in sight as more arrive and counts them under its window.

use std::collections::BTreeMap;
use std::path::Path;

use surco::components::{Laid, Region};
use surco::doc::Doc;
use surco::glyphs::Glyphs;
use surco::look::{Look, Width};
use surco::motion::Motion;
use surco::paint::Paint;
use surco::surface::Terminal;
use yunta_core::events::{session, ToolAction, ToolTarget};
use yunta_replay::frame::{Happening, Moment};

use super::window;
use crate::prompt::Stroke;
use crate::scroll::{Scroll, ScrolledTo, MARGIN};

/// The rows a pane gives the feed's window.
const ROWS: usize = 12;

/// A terminal 80 cells wide, in Unicode glyphs, unpainted.
fn look() -> Look {
    Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::exact(80),
        motion: Motion::Still,
    }
}

/// A node's calls from its `from`th on, `count` of them, the `at`th reading `file-{at}.rs`.
fn calls(from: usize, count: usize) -> Vec<Moment> {
    (from..from + count)
        .map(|at| {
            let read = session::happening::Happening::Called {
                tool: None,
                action: ToolAction::Read,
                target: Some(ToolTarget::of_path(Path::new(&format!("file-{at}.rs")))),
                session: None,
            };
            Moment::sample(Some("implement"), Happening::Session(read))
        })
        .collect()
}

/// How many rows of the feed of `moments` the window where `scroll` stands hands render, and
/// how many it counts above and under them.
struct Handed {
    rows: usize,
    before: usize,
    after: usize,
}

/// What the window of the feed of `moments`, where `scroll` stands, hands render.
fn handed(moments: &[Moment], scroll: &Scroll) -> Handed {
    let listed: Vec<usize> = (0..moments.len()).collect();
    let feed = window(&listed, moments, &BTreeMap::new(), scroll, &look());
    Handed {
        rows: feed.rows.len(),
        before: feed.before,
        after: feed.after,
    }
}

/// The window of the feed of `moments`, where `scroll` stands, drawn in a pane of [`ROWS`]
/// rows: each row's text, and what was laid out handed back to `scroll`, as a surface hands
/// it back after it draws.
fn drawn(moments: &[Moment], scroll: &mut Scroll) -> Vec<String> {
    let listed: Vec<usize> = (0..moments.len()).collect();
    let feed = window(&listed, moments, &BTreeMap::new(), scroll, &look());
    let region = Region::new(Doc::new().with(feed)).in_sight(scroll.sight());
    let paned = Terminal::on(look()).pane(&region, ROWS);
    scroll.laid(paned.laid);
    paned.lines.iter().map(|line| line.text()).collect()
}

/// `stroke` pressed on the feed of `moments`, then the feed drawn again.
fn pressed(moments: &[Moment], scroll: &mut Scroll, stroke: Stroke) -> Vec<String> {
    scroll.pressed(stroke);
    drawn(moments, scroll)
}

/// The rows `file-{from}.rs` to the one before `file-{to}.rs`, as the feed says each call.
fn reading(from: usize, to: usize) -> Vec<String> {
    (from..to)
        .map(|at| format!("read    file-{at}.rs"))
        .collect()
}

#[test]
fn a_node_of_ten_thousand_moments_hands_render_no_more_rows_than_a_page_and_its_margin() {
    let moments = calls(0, 10_000);
    let mut feed = Scroll::at_the_end();
    let unlaid = handed(&moments, &feed);
    assert!(
        unlaid.rows <= MARGIN,
        "before a layout, a margin: {}",
        unlaid.rows
    );
    assert_eq!(unlaid.after, 0, "its newest rows");

    drawn(&moments, &mut feed);
    let Laid { page, .. } = feed.laid_last();
    let at_the_end = handed(&moments, &feed);
    assert!(
        at_the_end.rows <= page + MARGIN,
        "at its end, a page and its margin: {} of a page of {page}",
        at_the_end.rows
    );
    assert_eq!(at_the_end.before + at_the_end.rows, 10_000);

    for _ in 0..3 {
        pressed(&moments, &mut feed, Stroke::Up);
    }
    let back = handed(&moments, &feed);
    assert!(
        back.rows <= page + 2 * MARGIN,
        "scrolled back, a page and its margin on each side: {}",
        back.rows
    );
    assert_eq!(back.before + back.rows + back.after, 10_000);
}

#[test]
fn scrolled_back_ten_moments_more_leave_the_rows_in_sight_and_raise_the_count_under_them_by_ten() {
    let mut moments = calls(0, 100);
    let mut feed = Scroll::at_the_end();
    drawn(&moments, &mut feed);
    let mut before = Vec::new();
    for _ in 0..3 {
        before = pressed(&moments, &mut feed, Stroke::Up);
    }
    let in_sight = ["… 86 above".to_string()]
        .into_iter()
        .chain(reading(86, 96))
        .collect::<Vec<_>>();
    assert_eq!(before[..ROWS - 1], in_sight);
    assert_eq!(before[ROWS - 1], "… 4 under");

    moments.extend(calls(100, 10));
    let after = drawn(&moments, &mut feed);
    assert_eq!(after[..ROWS - 1], in_sight, "the rows in sight stand");
    assert_eq!(after[ROWS - 1], "… 14 under", "ten more under them");
    assert_eq!(feed.to(), ScrolledTo::Row(86));
}

#[test]
fn a_feeds_place_and_the_rows_above_and_under_it_at_its_start_its_middle_and_its_end() {
    let moments = calls(0, 100);
    let mut feed = Scroll::at_the_end();
    let at_the_end = drawn(&moments, &mut feed);
    assert_eq!(feed.to(), ScrolledTo::End);
    assert_eq!(at_the_end[0], "… 89 above");
    assert_eq!(at_the_end[1..], reading(89, 100), "its newest last");

    let at_its_start = pressed(&moments, &mut feed, Stroke::Home);
    assert_eq!(feed.to(), ScrolledTo::Row(0));
    assert_eq!(at_its_start[..ROWS - 1], reading(0, 11));
    assert_eq!(at_its_start[ROWS - 1], "… 89 under");

    let in_its_middle = pressed(&moments, &mut feed, Stroke::PageDown);
    assert_eq!(
        feed.to(),
        ScrolledTo::Row(10),
        "a page on, by the rows in sight"
    );
    assert_eq!(in_its_middle[0], "… 10 above");
    assert_eq!(in_its_middle[1..ROWS - 1], reading(10, 20));
    assert_eq!(in_its_middle[ROWS - 1], "… 80 under");

    let back_at_its_end = pressed(&moments, &mut feed, Stroke::End);
    assert_eq!(feed.to(), ScrolledTo::End);
    assert_eq!(back_at_its_end, at_the_end);
}
