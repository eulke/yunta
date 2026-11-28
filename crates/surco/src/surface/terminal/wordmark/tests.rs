//! The name in block letters at a terminal: two rows centred in the cells it is given, the
//! name in its own letters in ASCII, and nothing where the letters do not fit. Every expected
//! row is written by hand; those of the screens are their targets' rows, from the region's
//! first cell.

use std::path::{Path, PathBuf};

use crate::components::Wordmark;
use crate::doc::Doc;
use crate::glyphs::Glyphs;
use crate::golden::{assert_golden, Environment, ENVIRONMENTS};
use crate::line::{Line, Tone};
use crate::look::{Look, Width};
use crate::motion::Motion;
use crate::paint::Paint;
use crate::surface::{Html, Markdown, Surface, Terminal};

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/wordmark")
}

/// The letters' first row, as the targets draw it.
const TOP: &str = "█ █ █ █ █▄ █ ▀█▀ ▄▀█";

/// The letters' second row, as the targets draw it: its first cell blank, under the `y`'s arms.
const FOOT: &str = " █  █▄█ █ ▀█  █  █▀█";

/// The rows drawn `cells` wide in `glyphs`, unpainted.
fn drawn(cells: usize, glyphs: Glyphs) -> Vec<Line> {
    let look = Look {
        glyphs,
        paint: Paint::Plain,
        width: Width::exact(cells),
        motion: Motion::Still,
    };
    Terminal::on(look).wordmark()
}

/// The text of each row `cells` wide in `glyphs`.
fn texts(cells: usize, glyphs: Glyphs) -> Vec<String> {
    drawn(cells, glyphs).iter().map(Line::text).collect()
}

/// `row` after `gap` blank cells.
fn after(gap: usize, row: &str) -> String {
    format!("{}{row}", " ".repeat(gap))
}

/// Two rows, each centred: at 120 cells fifty blank cells stand on each side of the letters'
/// twenty, at 60 cells twenty.
#[test]
fn in_unicode_the_name_is_two_rows_centred_at_120_and_at_60_cells() {
    assert_eq!(
        texts(120, Glyphs::Unicode),
        [after(50, TOP), after(50, FOOT)]
    );
    assert_eq!(
        texts(60, Glyphs::Unicode),
        [after(20, TOP), after(20, FOOT)]
    );
}

/// In the regions of a screen 120, 80 and 60 columns wide, 118, 78 and 58 cells from its
/// second column, the rows are the targets' rows (`first-request` rows 5-6, 3-4 and 3-4) from
/// the region's first cell.
#[test]
fn the_rows_are_the_targets_rows() {
    for (cells, gap) in [(118, 49), (78, 29), (58, 19)] {
        assert_eq!(
            texts(cells, Glyphs::Unicode),
            [after(gap, TOP), after(gap, FOOT)],
            "{cells} cells"
        );
    }
}

/// ASCII has no block elements: the name stands in its own letters on the first row, centred,
/// the cell an odd room leaves standing before it, as the targets draw it at 120 columns
/// (`first-request` row 5, column 59). The second row stays blank, so what stands under the
/// name keeps its row.
#[test]
fn in_ascii_the_name_stands_in_its_own_letters_on_the_first_row() {
    assert_eq!(
        texts(118, Glyphs::Ascii),
        [after(57, "yunta"), String::new()]
    );
    assert_eq!(
        texts(120, Glyphs::Ascii),
        [after(58, "yunta"), String::new()]
    );
}

/// The letters stand strong, the blank cells before them unpainted.
#[test]
fn the_letters_are_strong() {
    for glyphs in [Glyphs::Unicode, Glyphs::Ascii] {
        let painted: Vec<Tone> = drawn(60, glyphs)
            .iter()
            .flat_map(Line::spans)
            .filter(|span| !span.text.trim().is_empty())
            .map(|span| span.tone)
            .collect();
        assert!(!painted.is_empty(), "{glyphs:?} draws no letter");
        assert!(
            painted.iter().all(|tone| *tone == Tone::Strong),
            "{glyphs:?}: {painted:?}"
        );
    }
}

/// Cut, the letters spell no name, and the trail names it: in fewer cells than they take, the
/// name draws no row.
#[test]
fn in_fewer_cells_than_its_letters_it_draws_nothing() {
    assert!(drawn(19, Glyphs::Unicode).is_empty());
    assert_eq!(drawn(20, Glyphs::Unicode).len(), 2);
    assert!(drawn(4, Glyphs::Ascii).is_empty());
}

/// A file and a page draw no picture: the name in block letters leaves them as they are.
#[test]
fn markdown_and_a_page_leave_it_out() {
    let named = Doc::new().with(Wordmark);
    assert_eq!(Markdown.draw(&named), Markdown.draw(&Doc::new()));
    assert_eq!(Html.body(&named), Html.body(&Doc::new()));
}

/// The name in every environment, and in the 120 columns of the screens that draw it.
#[test]
fn the_name_matches_its_goldens() {
    let wide = Environment {
        name: "unicode-120",
        unicode: true,
        paint: Paint::Plain,
        width: 120,
    };
    let doc = Doc::new().with(Wordmark);
    for environment in ENVIRONMENTS.iter().chain([&wide]) {
        let drawn = Terminal::on(Look::of(environment)).draw(&doc);
        assert_golden(&environment.golden(&goldens(), "name"), &drawn);
    }
}
