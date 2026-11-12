//! What a change's files look like on a terminal. Every expected line here is
//! written by hand: the two bars of a row share twenty cells, and each is
//! `moved / largest * 19` of them in eighths, since each rounds up to a whole
//! cell and the two together could otherwise take twenty-one.

use crate::components::{Diffstat, FileRow, MoreFiles};
use crate::glyphs::Glyphs;
use crate::line::Tone;
use crate::look::{Look, Width};
use crate::paint::Paint;
use crate::width::cell_width;

fn file(path: &str, added: u64, removed: u64) -> FileRow {
    FileRow {
        path: path.to_string(),
        added,
        removed,
    }
}

fn stat() -> Diffstat {
    Diffstat {
        files: vec![
            file("crates/cli/src/pack.rs", 142, 38),
            file("crates/engine/src/catalog.rs", 61, 12),
            file("docs/notes.md", 38, 0),
        ],
        more: Some(MoreFiles {
            files: 5,
            added: 123,
            removed: 16,
        }),
    }
}

fn look(glyphs: Glyphs, cells: usize) -> Look {
    Look {
        glyphs,
        paint: Paint::Plain,
        width: Width::row(cells, None),
        motion: crate::motion::Motion::Moves,
    }
}

fn drawn(stat: &Diffstat, look: &Look) -> Vec<String> {
    stat.lines(look).iter().map(|line| line.text()).collect()
}

#[test]
fn each_file_stands_with_its_figures_and_a_bar_of_what_it_moved() {
    assert_eq!(
        drawn(&stat(), &look(Glyphs::Unicode, 80)),
        [
            "  crates/cli/src/pack.rs        +142 −38  ███████████████████",
            "  crates/engine/src/catalog.rs   +61 −12  ██████▌█▎",
            "  docs/notes.md                  +38      ████",
            "  5 more files, +123 −16",
        ]
    );
}

#[test]
fn ascii_draws_whole_cells_and_its_own_minus() {
    assert_eq!(
        drawn(&stat(), &look(Glyphs::Ascii, 80)),
        [
            "  crates/cli/src/pack.rs        +142 -38  ###################",
            "  crates/engine/src/catalog.rs   +61 -12  #######",
            "  docs/notes.md                  +38      ####",
            "  5 more files, +123 -16",
        ]
    );
}

/// With no room for a bar the figures stay, and a path is cut in the middle, so
/// its head and its file name both stand.
#[test]
fn a_narrow_row_lets_the_bar_go_first_and_then_cuts_the_path() {
    let rows = drawn(&stat(), &look(Glyphs::Unicode, 40));
    assert_eq!(rows[0], "  crates/cli/src/pack.rs        +142 −38");
    let rows = drawn(&stat(), &look(Glyphs::Unicode, 30));
    assert_eq!(rows[1], "  crates/e…atalog.rs   +61 −12");
}

/// However a width falls, a row stays inside it and ends at its last word. Two
/// bars that each round up are what would take it past the edge.
#[test]
fn no_row_outruns_its_line_or_ends_in_spaces() {
    for cells in 24..=120 {
        for glyphs in [Glyphs::Unicode, Glyphs::Ascii] {
            for row in drawn(&stat(), &look(glyphs, cells)) {
                assert!(cell_width(&row) <= cells, "at {cells}: {row}");
                assert_eq!(row.trim_end(), row, "at {cells}");
            }
        }
    }
}

/// A file too small to win a step of a bar ends at its figure.
#[test]
fn a_file_too_small_for_a_bar_ends_at_its_figure() {
    let stat = Diffstat {
        files: vec![file("big.rs", 1000, 0), file("tiny.rs", 1, 0)],
        more: None,
    };
    assert_eq!(
        drawn(&stat, &look(Glyphs::Unicode, 80)),
        ["  big.rs   +1000   ███████████████████", "  tiny.rs     +1",]
    );
}

#[test]
fn a_file_with_no_lines_is_named_with_no_figure_and_no_bar() {
    let stat = Diffstat {
        files: vec![file("logo.png", 0, 0)],
        more: None,
    };
    assert_eq!(drawn(&stat, &look(Glyphs::Unicode, 80)), ["  logo.png"]);
}

/// A file with no lines holds to the line like any other row.
#[test]
fn a_long_file_with_no_lines_is_cut_to_the_line() {
    let long = "assets/images/of/every/size/and/every/kind/we/ever/shipped/logo-at-full-size.png";
    let stat = Diffstat {
        files: vec![file(long, 0, 0)],
        more: None,
    };
    let rows = drawn(&stat, &look(Glyphs::Unicode, 60));
    assert!(cell_width(&rows[0]) <= 60, "{rows:?}");
    assert!(
        rows[0].contains('…') && rows[0].ends_with(".png"),
        "{rows:?}"
    );
}

/// A control character in a file's name is drawn as a space, and measured as the
/// cell that space takes: the figures of every row stand in one column.
#[test]
fn a_control_character_in_a_path_takes_the_cell_it_is_drawn_in() {
    let stat = Diffstat {
        files: vec![file("a\u{1}b.rs", 5, 0), file("plain-name.rs", 5, 0)],
        more: None,
    };
    let painted: Vec<String> = stat
        .lines(&look(Glyphs::Unicode, 80))
        .iter()
        .map(|line| Paint::Plain.line(line))
        .collect();
    assert_eq!(
        painted,
        [
            "  a b.rs         +5   ███████████████████",
            "  plain-name.rs  +5   ███████████████████",
        ]
    );
}

/// The rest is a sentence: on a narrow line it wraps, and every figure stands.
#[test]
fn the_rest_wraps_on_a_narrow_line() {
    let stat = Diffstat {
        files: vec![file("a.rs", 1, 0)],
        more: Some(MoreFiles {
            files: 120,
            added: 12345,
            removed: 6789,
        }),
    };
    assert_eq!(
        drawn(&stat, &look(Glyphs::Unicode, 24))[1..],
        ["  120 more files, +12345", "  −6789"]
    );
}

#[test]
fn what_it_adds_and_what_it_takes_out_read_in_their_own_tones() {
    let lines = stat().lines(&look(Glyphs::Unicode, 80));
    let first = lines.first().expect("a row");
    let tone_of = |piece: &str| {
        first
            .spans()
            .iter()
            .find(|span| span.text.trim() == piece)
            .map(|span| span.tone)
    };
    assert_eq!(tone_of("+142"), Some(Tone::Done));
    assert_eq!(tone_of("−38"), Some(Tone::Failed));
    assert_eq!(tone_of("███████████████"), Some(Tone::Done));
    assert_eq!(tone_of("████"), Some(Tone::Failed));
    let last = lines.last().expect("the rest");
    assert_eq!(last.spans().last().map(|span| span.tone), Some(Tone::Muted));
}
