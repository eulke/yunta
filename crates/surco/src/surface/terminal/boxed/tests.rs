//! What is open over a screen, boxed: its title in the top edge, its lines inside the
//! uprights, as tall as what it holds, and scrolled inside the box when it holds more.

use std::path::{Path, PathBuf};

use crate::components::{Legend, Overlay, Sight};
use crate::doc::Doc;
use crate::golden::{assert_golden, Environment, ENVIRONMENTS};
use crate::line::Line;
use crate::look::{Look, Width};
use crate::surface::Terminal;

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/boxed")
}

/// A box `cells` wide, in `environment`'s glyphs and paint.
fn on(environment: &Environment, cells: usize) -> Terminal {
    Terminal::on(Look {
        width: Width::exact(cells),
        ..Look::of(environment)
    })
}

/// What help lists over a run: four keys, each with what it does.
fn keys() -> Overlay {
    let lines = [
        "/      anything: only what can be done here",
        "n      start a new run, from anywhere",
        "esc    one step back; what it leaves stays waiting",
        "q      leave: every run keeps going",
    ]
    .map(|line| Line::new().plain(line));
    Overlay {
        title: "keys".to_string(),
        doc: Doc::new().with(lines.to_vec()),
        legend: Legend::new().key("esc", "back"),
        sight: Sight::default(),
    }
}

/// `count` lines to read, `line 1` to `line <count>`, from the line `top`.
fn many(count: usize, top: usize) -> Overlay {
    let lines: Vec<Line> = (1..=count)
        .map(|at| Line::new().plain(format!("line {at}")))
        .collect();
    Overlay {
        title: "palette".to_string(),
        doc: Doc::new().with(lines),
        legend: Legend::new().key("esc", "back"),
        sight: Sight::from_line(top),
    }
}

fn texts(lines: &[Line]) -> Vec<String> {
    lines.iter().map(Line::text).collect()
}

/// The reference's help box: 56 cells, the title in the top edge, a cell of room inside
/// each upright, as tall as its four keys and its two edges.
#[test]
fn the_box_holds_its_title_in_the_top_edge_and_its_lines_inside() {
    let boxed = on(&ENVIRONMENTS[2], 56).boxed(&keys(), 10);
    assert_eq!(
        texts(&boxed.lines),
        [
            "┌ keys ────────────────────────────────────────────────┐",
            "│ /      anything: only what can be done here          │",
            "│ n      start a new run, from anywhere                │",
            "│ esc    one step back; what it leaves stays waiting   │",
            "│ q      leave: every run keeps going                  │",
            "└──────────────────────────────────────────────────────┘",
        ]
    );
    assert_eq!(boxed.cursor, None);
}

/// In ASCII the box is drawn in its marks.
#[test]
fn in_ascii_the_box_is_drawn_in_its_marks() {
    let boxed = on(&ENVIRONMENTS[1], 20).boxed(&many(1, 0), 10);
    assert_eq!(
        texts(&boxed.lines),
        [
            "+ palette ---------+",
            "| line 1           |",
            "+------------------+",
        ]
    );
}

/// What holds more lines than the box has rows scrolls inside it, counting what is below.
#[test]
fn what_holds_more_than_the_box_scrolls_inside_it() {
    let boxed = on(&ENVIRONMENTS[2], 30).boxed(&many(20, 3), 8);
    let text = texts(&boxed.lines);
    assert_eq!(text.len(), 8, "{text:#?}");
    assert_eq!(text[1], "│ line 4                     │", "{text:#?}");
    assert!(text[6].contains("more lines"), "{text:#?}");
    assert_eq!(boxed.laid.top, 3);
    assert!(boxed.laid.scrolls());
}

/// Help boxed, and a palette scrolled inside its box, in every environment.
#[test]
fn a_box_matches_its_goldens_in_every_environment() {
    for environment in &ENVIRONMENTS {
        let cells = (environment.width - 4).min(60);
        let terminal = on(environment, cells);
        for (case, overlay, rows) in [("help", keys(), 10), ("scrolled", many(20, 3), 8)] {
            let boxed = terminal.boxed(&overlay, rows);
            let drawn: String = boxed
                .lines
                .iter()
                .map(|line| format!("{}\n", terminal.look.paint.line(line)))
                .collect();
            assert_golden(&environment.golden(&goldens(), case), &drawn);
        }
    }
}
