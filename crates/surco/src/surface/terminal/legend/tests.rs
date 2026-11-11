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
