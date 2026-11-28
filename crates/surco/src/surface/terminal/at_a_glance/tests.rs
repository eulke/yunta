//! A run at a glance, at a terminal: its facts, each value wrapped under its own first cell
//! and never inside a figure, and its acts, a key a row with the keys in one column. Every
//! expected row is written by hand.

use crate::components::{AtAGlance, Binding, Keys};
use crate::glyphs::Glyphs;
use crate::look::{Look, Width};
use crate::paint::Paint;
use crate::surface::Terminal;

/// `keys` and what each does, as the acts' bindings.
fn acts(keys: &[(&str, &str)]) -> Vec<Binding> {
    keys.iter()
        .map(|(keys, does)| Binding {
            keys: Keys::Typed(keys.to_string()),
            does: does.to_string(),
        })
        .collect()
}

/// `glance` drawn `cells` wide in `glyphs`, unpainted, a row a string.
fn drawn(glance: AtAGlance, cells: usize, glyphs: Glyphs) -> Vec<String> {
    let look = Look {
        glyphs,
        paint: Paint::Plain,
        width: Width::exact(cells),
        motion: crate::motion::Motion::Still,
    };
    Terminal::on(look)
        .block(&glance.into())
        .iter()
        .map(|line| line.text())
        .collect()
}

/// At 40 cells a value that passes its row wraps between its words, its later rows under
/// its first cell, never under its label, and every figure of it stands whole on one row.
#[test]
fn at_forty_cells_a_value_wraps_under_itself_with_no_figure_split() {
    let tokens = "27M in / 78.4k out (26M cached)";
    let glance = AtAGlance {
        facts: vec![("tokens", tokens.to_string())],
        acts: None,
    };
    let rows = drawn(glance, 40, Glyphs::Unicode);
    assert_eq!(
        rows,
        [
            "  tokens       27M in / 78.4k out (26M",
            "               cached)",
        ]
    );
    for figure in ["27M", "78.4k", "(26M"] {
        assert!(
            rows.iter()
                .any(|row| row.split_whitespace().any(|word| word == figure)),
            "{figure} is split: {rows:#?}"
        );
    }
}

/// The acts stand under their heading, a key a row: the keys in one column, so what each
/// does starts on one cell, `enter` the widest and two cells after it; with no `enter`
/// among them the column is the same, so the acts of two runs stand alike.
#[test]
fn the_acts_keys_align_with_enter_the_widest() {
    let waiting = AtAGlance {
        facts: Vec::new(),
        acts: Some((
            "what you can do".to_string(),
            acts(&[("enter", "read the decision"), ("x", "close it for good")]),
        )),
    };
    assert_eq!(
        drawn(waiting, 39, Glyphs::Unicode),
        [
            "what you can do",
            "enter  read the decision",
            "x      close it for good",
        ]
    );
    let asking = AtAGlance {
        facts: Vec::new(),
        acts: Some((
            "what you can do".to_string(),
            acts(&[
                ("c", "stop it and everything under it"),
                ("s", "what it cost"),
            ]),
        )),
    };
    assert_eq!(
        drawn(asking, 39, Glyphs::Unicode),
        [
            "what you can do",
            "c      stop it and everything under it",
            "s      what it cost",
        ]
    );
}

/// A fact with nothing to say is no row: the facts that say something stand in their order,
/// a blank row between them and the acts.
#[test]
fn a_fact_with_no_value_is_no_row() {
    let glance = AtAGlance {
        facts: vec![
            ("progress", String::new()),
            ("waiting", "for 11d".to_string()),
            ("tokens", "  ".to_string()),
            ("branch", "yunta/run/...EKPBGE off main".to_string()),
        ],
        acts: Some((
            "what you can do".to_string(),
            acts(&[("r", "continue it from its log")]),
        )),
    };
    assert_eq!(
        drawn(glance, 39, Glyphs::Unicode),
        [
            "  waiting      for 11d",
            "  branch       yunta/run/...EKPBGE off",
            "               main",
            "",
            "what you can do",
            "r      continue it from its log",
        ]
    );
}

/// An act's row that passes its edge is cut there with the ellipsis, its key whole, in each
/// glyph set's ellipsis; a glance with nothing to say draws no row.
#[test]
fn an_acts_row_is_cut_at_its_edge_its_key_whole() {
    let glance = || AtAGlance {
        facts: Vec::new(),
        acts: Some((
            "what you can do".to_string(),
            acts(&[("c", "stop it and everything under it")]),
        )),
    };
    assert_eq!(
        drawn(glance(), 20, Glyphs::Unicode),
        ["what you can do", "c      stop it and…"]
    );
    assert_eq!(
        drawn(glance(), 20, Glyphs::Ascii),
        ["what you can do", "c      stop it an..."]
    );
    let nothing = AtAGlance {
        facts: vec![("waiting", String::new())],
        acts: None,
    };
    assert!(drawn(nothing, 39, Glyphs::Unicode).is_empty());
}

/// The glance of the run the field's target points at, in the 39 cells the field's side
/// region gives it at 120 columns, draws the target's rows 7 to 20 from its column 81: each
/// value wrapped under its first cell, a blank row, then the acts.
#[test]
fn the_fields_glance_draws_its_targets_rows_in_thirty_nine_cells() {
    let glance = AtAGlance {
        facts: vec![
            ("progress", "12 of 15 nodes done, 1 reroute".to_string()),
            ("tokens", "26.1M in / 1.06M out (25M cached)".to_string()),
            ("time", "1h12m at work, 38m51s waiting on you".to_string()),
            (
                "slowest",
                "implement 41m05s, spec 13m53s, plan 6m25s".to_string(),
            ),
            ("branch", "yunta/run/...Q78GRR off main".to_string()),
        ],
        acts: Some((
            "what you can do".to_string(),
            acts(&[
                ("c", "stop it and everything under it"),
                ("s", "what it cost"),
            ]),
        )),
    };
    assert_eq!(
        drawn(glance, 39, Glyphs::Unicode),
        [
            "  progress     12 of 15 nodes done, 1",
            "               reroute",
            "  tokens       26.1M in / 1.06M out",
            "               (25M cached)",
            "  time         1h12m at work, 38m51s",
            "               waiting on you",
            "  slowest      implement 41m05s, spec",
            "               13m53s, plan 6m25s",
            "  branch       yunta/run/...Q78GRR off",
            "               main",
            "",
            "what you can do",
            "c      stop it and everything under it",
            "s      what it cost",
        ]
    );
}
