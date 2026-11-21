//! The field in perspective on a terminal: furrows from near one point at its top to its
//! whole width at its foot, filling the cells and the rows a region gives it, in braille
//! cells; nothing where the glyphs draw none, or where it would stand too flat to read.

use std::path::{Path, PathBuf};

use crate::components::Horizon;
use crate::glyphs::Glyphs;
use crate::golden::assert_golden;
use crate::line::{Line, Tone};
use crate::look::{Look, Width};
use crate::motion::Motion;
use crate::paint::Paint;
use crate::surface::Terminal;
use crate::width::cell_width;

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/horizon")
}

/// A terminal `cells` cells across, drawing with `glyphs` in `paint`.
fn terminal(glyphs: Glyphs, paint: Paint, cells: usize) -> Terminal {
    Terminal::on(Look {
        glyphs,
        paint,
        width: Width::exact(cells),
        motion: Motion::Moves,
    })
}

/// Whether `cell` is a braille cell with a dot in it.
fn braille(cell: char) -> bool {
    ('\u{2801}'..='\u{28ff}').contains(&cell)
}

/// The first and the last cell of `row` a furrow stands in.
fn spread(row: &Line) -> (usize, usize) {
    let cells: Vec<char> = row.text().chars().collect();
    let first = cells.iter().position(|cell| braille(*cell));
    let last = cells.iter().rposition(|cell| braille(*cell));
    first.zip(last).expect("every row holds braille")
}

#[test]
fn every_row_holds_braille_and_the_rows_widen_toward_the_bottom() {
    let field = Horizon { tone: Tone::Done };
    let rows = terminal(Glyphs::Unicode, Paint::Plain, 40).horizon(&field, 8);
    assert_eq!(rows.len(), 8, "a row for every row given");
    for row in &rows {
        let text = row.text();
        assert!(
            text.chars().all(|cell| cell == ' ' || braille(cell)),
            "only furrows and the ground between them: {text}"
        );
        assert!(cell_width(&text) <= 40, "past the region's edge: {text}");
    }
    let spreads: Vec<(usize, usize)> = rows.iter().map(spread).collect();
    for (at, pair) in spreads.windows(2).enumerate() {
        let ((above_first, above_last), (first, last)) = (pair[0], pair[1]);
        assert!(
            first <= above_first && last >= above_last,
            "row {} spreads over {first}..={last}, narrower than {above_first}..={above_last} above it",
            at + 2
        );
    }
    let width = |(first, last): (usize, usize)| last - first + 1;
    assert!(
        width(spreads[7]) > width(spreads[0]),
        "the foot is wider than the top: {spreads:?}"
    );
}

#[test]
fn ascii_draws_no_row_of_it() {
    let field = Horizon { tone: Tone::Done };
    let rows = terminal(Glyphs::Ascii, Paint::Plain, 40).horizon(&field, 8);
    assert!(rows.is_empty(), "{} rows drawn", rows.len());
}

/// Two rows are too flat to read as furrows running away: the field stands from three.
#[test]
fn under_three_rows_it_draws_none() {
    let field = Horizon { tone: Tone::Done };
    let unicode = terminal(Glyphs::Unicode, Paint::Plain, 40);
    assert!(unicode.horizon(&field, 2).is_empty());
    assert_eq!(unicode.horizon(&field, 3).len(), 3);
}

/// A region a screen gives the field: the screen and the columns of its terminal, which
/// name its goldens, the cells across and the rows down it holds there, and its tone.
struct Region {
    screen: &'static str,
    columns: usize,
    cells: usize,
    rows: usize,
    tone: Tone,
}

/// Every region a screen gives the field: under the receipt and a report, harvested; under
/// the first request and a setup, the ground a page stands on.
const REGIONS: [Region; 8] = [
    Region {
        screen: "receipt",
        columns: 120,
        cells: 84,
        rows: 16,
        tone: Tone::Done,
    },
    Region {
        screen: "receipt",
        columns: 80,
        cells: 78,
        rows: 5,
        tone: Tone::Done,
    },
    Region {
        screen: "reported",
        columns: 120,
        cells: 84,
        rows: 12,
        tone: Tone::Done,
    },
    Region {
        screen: "first-request",
        columns: 120,
        cells: 118,
        rows: 20,
        tone: Tone::Faint,
    },
    Region {
        screen: "first-request",
        columns: 80,
        cells: 78,
        rows: 10,
        tone: Tone::Faint,
    },
    Region {
        screen: "setup",
        columns: 120,
        cells: 118,
        rows: 8,
        tone: Tone::Faint,
    },
    Region {
        screen: "setup-unreachable",
        columns: 120,
        cells: 118,
        rows: 11,
        tone: Tone::Faint,
    },
    Region {
        screen: "setup-changing",
        columns: 120,
        cells: 118,
        rows: 4,
        tone: Tone::Faint,
    },
];

/// The field `region` holds, drawn with `glyphs` in `paint`, each row painted and ended.
fn drawn(region: &Region, glyphs: Glyphs, paint: Paint) -> String {
    let field = Horizon { tone: region.tone };
    terminal(glyphs, paint, region.cells)
        .horizon(&field, region.rows)
        .iter()
        .map(|row| format!("{}\n", paint.line(row)))
        .collect()
}

/// The region's golden named for its glyphs and paint, `unicode`, `unicode-color` or `ascii`.
fn golden(region: &Region, drawn_as: &str) -> PathBuf {
    goldens().join(format!(
        "{}.{drawn_as}-{}.txt",
        region.screen, region.columns
    ))
}

#[test]
fn the_field_fills_every_region_a_screen_gives_it_cell_for_cell() {
    for region in &REGIONS {
        let plain = drawn(region, Glyphs::Unicode, Paint::Plain);
        assert_golden(&golden(region, "unicode"), &plain);
    }
}

/// Its tone paints the furrows and the ground between them, and leaves the ground before
/// the first furrow of a row unpainted.
#[test]
fn the_field_is_drawn_in_the_tone_it_is_given() {
    for region in REGIONS.iter().filter(|region| region.columns == 80) {
        let painted = drawn(region, Glyphs::Unicode, Paint::ANSI16);
        assert_golden(&golden(region, "unicode-color"), &painted);
    }
}

/// A layer without braille leaves the field's rows blank: it says nothing they lose.
#[test]
fn ascii_leaves_the_region_blank() {
    let receipt = &REGIONS[0];
    assert_golden(
        &golden(receipt, "ascii"),
        &drawn(receipt, Glyphs::Ascii, Paint::Plain),
    );
}
