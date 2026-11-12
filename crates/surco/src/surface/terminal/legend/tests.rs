use super::*;
use crate::doc::Doc;
use crate::glyphs::Glyphs;
use crate::look::Width;
use crate::paint::Paint;
use crate::surface::{Html, Markdown, Surface, Terminal};

/// A plain look `cells` wide, in Unicode.
fn look(cells: usize) -> Look {
    Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::of(Some(cells), None),
        motion: crate::motion::Motion::Moves,
    }
}

/// A legend with no key has nothing to say on any surface.
#[test]
fn an_empty_legend_draws_nothing() {
    let doc = || Doc::new().with(Legend::new());
    assert!(Legend::new().lines(&look(80)).is_empty());
    assert_eq!(Terminal::on(look(80)).draw(&doc()).trim(), "");
    assert_eq!(Markdown.draw(&doc()).trim(), "");
    assert!(!Html.body(&doc()).contains("legend"));
}

/// A legend wider than its line goes on the next one between two keys, never
/// between a key and what it does.
#[test]
fn a_narrow_legend_goes_on_between_two_keys() {
    let legend = Legend::new()
        .arrows("moves to another node")
        .key("enter", "opens the one pointed at")
        .key("s", "shows what it cost")
        .key("q", "leaves, and the run keeps going");

    let lines: Vec<String> = legend.lines(&look(60)).iter().map(Line::text).collect();

    assert_eq!(
        lines,
        [
            "↑↓ moves to another node · enter opens the one pointed at",
            "s shows what it cost · q leaves, and the run keeps going",
        ]
    );
}

/// On a screen, the legend is one row: every key where they all fit; where they do not,
/// the first keys that fit, a mark that more stand between, and the last key.
#[test]
fn the_legend_row_keeps_its_first_keys_and_its_last() {
    let legend = Legend::new()
        .arrows("node")
        .key("s", "stats")
        .key("g", "graph")
        .key("?", "the keys here");
    let wide = Terminal::on(Look {
        width: Width::exact(78),
        ..look(78)
    });
    assert_eq!(
        wide.legend(&legend).text(),
        "↑↓ node · s stats · g graph · ? the keys here"
    );
    let narrow = Terminal::on(Look {
        width: Width::exact(40),
        ..look(40)
    });
    assert_eq!(
        narrow.legend(&legend).text(),
        "↑↓ node · s stats · … · ? the keys here"
    );
}

/// A last key too wide to stand beside even the first leaves the first keys that fit,
/// cut.
#[test]
fn a_last_key_too_wide_leaves_the_first_keys_cut() {
    let legend = Legend::new()
        .key("q", "field, it keeps going")
        .key("s", "stats");
    let narrow = Terminal::on(Look {
        width: Width::exact(12),
        ..look(12)
    });
    assert_eq!(narrow.legend(&legend).text(), "q field, it…");
}

/// A legend of one key wider than its row is cut at the row's width, as a row of first keys
/// is: the row a screen keeps for its keys is one row.
#[test]
fn one_key_wider_than_its_row_is_cut() {
    let legend = Legend::new().key("q", "leaves, and the run keeps going");
    let narrow = Terminal::on(Look {
        width: Width::exact(12),
        ..look(12)
    });
    assert_eq!(narrow.legend(&legend).text(), "q leaves, a…");
}
