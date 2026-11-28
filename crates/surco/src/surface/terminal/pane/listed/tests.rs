//! A region that lists runs under headings, drawn in panes of fewer and fewer rows: whole, a
//! cell inside its edges; its lists close together; then a window that keeps the run pointed
//! at in sight and counts what it hides at the marks' column; and, standing close, without
//! what stands above its lists. Every expected row is written by hand.

use crate::components::{Edge, Region, RunLine, Runs};
use crate::doc::{Block, Doc};
use crate::glyphs::Glyphs;
use crate::line::{Fill, Line};
use crate::look::{Look, Width};
use crate::surface::Terminal;
use crate::words::Standing;

/// A run's row, `handle` its handle, pointed at where `pointed`.
fn run(handle: &str, pointed: bool) -> RunLine {
    RunLine {
        standing: Standing::Stopped,
        handle: handle.to_string(),
        workflow: "fragua".to_string(),
        mode: "quick".to_string(),
        furrow: vec![Standing::Finished],
        said: "stopped".to_string(),
        edge: Edge::Age("1h".to_string()),
        pointed,
        marked: false,
    }
}

/// A row a new run starts from, two lists under their headings, the run `pointed` pointed
/// at, and a line under them: what the field's region holds.
fn region(pointed: &str) -> Region {
    let runs = |handles: &[&str], pointed: Option<&str>| Runs {
        rows: handles
            .iter()
            .map(|handle| run(handle, Some(*handle) == pointed))
            .collect(),
    };
    let doc = Doc::new()
        .with(Block::Rows(vec![Line::new().plain("start one")]))
        .with(Block::Heading("first".to_string()))
        .with(runs(&["AAAAAA", "BBBBBB"], Some(pointed)))
        .with(Block::Heading("second".to_string()))
        .with(runs(&["CCCCCC", "DDDDDD", "EEEEEE"], Some(pointed)))
        .with(Block::Rows(vec![Line::new().plain("keys")]));
    listing(doc)
}

/// A region listing the runs `doc` holds, as the field's is.
fn listing(doc: Doc) -> Region {
    Region::new(doc).listing_runs()
}

/// The region, `DDDDDD` pointed at, drawn in a pane `rows` high, 40 cells wide, standing close
/// where `short`: each row's text, its trailing blanks cut.
fn drawn(rows: usize, short: bool) -> Vec<String> {
    drawn_pointing("DDDDDD", rows, short)
}

/// The region, `pointed` pointed at, drawn as [`drawn`] draws it.
fn drawn_pointing(pointed: &str, rows: usize, short: bool) -> Vec<String> {
    let look = Look {
        glyphs: Glyphs::Unicode,
        width: Width::exact(40),
        ..Look::plain()
    };
    let terminal = Terminal::on(look);
    let paned = match short {
        true => terminal.pane_short_of_rows(&region(pointed), rows),
        false => terminal.pane(&region(pointed), rows),
    };
    paned
        .lines
        .iter()
        .map(|line| line.text().trim_end().to_string())
        .collect()
}

/// The first characters of `rows`, through a run's handle: what says where each row stands.
fn heads(rows: &[String]) -> Vec<String> {
    rows.iter()
        .map(|row| {
            row.chars()
                .take(12)
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

#[test]
fn a_region_listing_runs_stands_whole_a_cell_inside_its_edges_its_lists_apart() {
    assert_eq!(
        heads(&drawn(14, false)),
        [
            " start one",
            "",
            " first",
            "   ■ AAAAAA",
            "   ■ BBBBBB",
            "",
            " second",
            "   ■ CCCCCC",
            " ▸ ■ DDDDDD",
            "   ■ EEEEEE",
            "",
            " keys",
        ]
    );
}

/// A region listing runs stands a cell inside its edges whatever it holds at the moment: with
/// no run yet, its first row stands where it stands over a list, so the row keeps its cell as
/// the first run comes.
#[test]
fn a_region_listing_no_run_yet_stands_its_first_row_where_it_stands_over_a_list() {
    let start = || Block::Rows(vec![Line::new().plain("start one")]);
    let none = listing(Doc::new().with(start()));
    let one = listing(
        Doc::new()
            .with(start())
            .with(Block::Heading("first".to_string()))
            .with(Runs {
                rows: vec![run("AAAAAA", true)],
            }),
    );
    let look = Look {
        glyphs: Glyphs::Unicode,
        width: Width::exact(40),
        ..Look::plain()
    };
    let first_row = |region: &Region| {
        let paned = Terminal::on(look).pane(region, 6);
        paned.lines.first().map(Line::text).unwrap_or_default()
    };
    assert_eq!(first_row(&one), " start one");
    assert_eq!(first_row(&none), first_row(&one), "with no run yet");
}

#[test]
fn short_of_rows_its_lists_close_up_before_any_run_gives_way() {
    assert_eq!(
        heads(&drawn(11, false)),
        [
            " start one",
            "",
            " first",
            "   ■ AAAAAA",
            "   ■ BBBBBB",
            " second",
            "   ■ CCCCCC",
            " ▸ ■ DDDDDD",
            "   ■ EEEEEE",
            "",
            " keys",
        ]
    );
}

#[test]
fn shorter_its_window_opens_at_the_earliest_heading_that_keeps_the_pointer_and_counts_the_rest() {
    assert_eq!(
        heads(&drawn(9, false)),
        [
            " start one",
            "",
            "   … 2 above",
            " second",
            "   ■ CCCCCC",
            " ▸ ■ DDDDDD",
            "   ■ EEEEEE",
            "",
            " keys",
        ]
    );
    assert_eq!(
        heads(&drawn_pointing("AAAAAA", 9, false)),
        [
            " start one",
            "",
            " first",
            " ▸ ■ AAAAAA",
            "   ■ BBBBBB",
            "   … 3 under",
            "",
            " keys",
        ],
        "no heading stands at the window's foot"
    );
}

#[test]
fn standing_close_what_stands_above_its_lists_gives_way_before_its_window_opens() {
    assert_eq!(
        heads(&drawn(8, true)),
        [
            " first",
            "   ■ AAAAAA",
            "   ■ BBBBBB",
            " second",
            "   ■ CCCCCC",
            " ▸ ■ DDDDDD",
            "   ■ EEEEEE",
            " keys",
        ]
    );
    assert_eq!(
        heads(&drawn(6, true)),
        [
            "   … 2 above",
            " second",
            "   ■ CCCCCC",
            " ▸ ■ DDDDDD",
            "   ■ EEEEEE",
            " keys",
        ]
    );
}

#[test]
fn the_run_pointed_at_stands_on_the_selection_from_the_regions_edge() {
    let look = Look {
        glyphs: Glyphs::Unicode,
        width: Width::exact(40),
        ..Look::plain()
    };
    let paned = Terminal::on(look).pane(&region("DDDDDD"), 14);
    let pointed: Vec<&Line> = paned
        .lines
        .iter()
        .filter(|line| line.fill() == Some(Fill::Selection))
        .collect();
    assert_eq!(pointed.len(), 1, "{:#?}", paned.lines);
    assert!(
        pointed[0].text().starts_with(" ▸ "),
        "{}",
        pointed[0].text()
    );
    assert!(
        pointed[0]
            .spans()
            .iter()
            .all(|span| span.fill.is_none() || span.fill == Some(Fill::Selection)),
        "the cell before its words stands on the selection with the rest"
    );
}

#[test]
fn in_a_pane_too_short_for_what_stands_around_the_lists_the_run_pointed_at_stays_in_sight() {
    assert_eq!(
        heads(&drawn(4, false)),
        ["   … 2 above", "   ■ CCCCCC", " ▸ ■ DDDDDD", "   ■ EEEEEE"],
        "what stands above and under the lists gives way before the run pointed at"
    );
    assert_eq!(
        heads(&drawn(1, false)),
        [" ▸ ■ DDDDDD"],
        "the run pointed at stands alone"
    );
}
