//! Every block of the catalog in a pane narrower than any stream: a pane's own
//! edge bounds it, so a block drawn there takes the cells it is given and no
//! more.

use std::panic::{catch_unwind, AssertUnwindSafe};

use super::catalog;
use crate::glyphs::Glyphs;
use crate::look::{Look, Width};
use crate::paint::Paint;
use crate::surface::{Surface, Terminal};
use crate::width::cell_width;

/// The narrowest pane a screen gives a document: a side region beside a main
/// one of 40 cells on a terminal of 60.
const NARROWEST: usize = 20;

/// The narrowest a stream is drawn at; a pane at this width or wider is drawn
/// as a stream is.
const FLOOR: usize = 60;

#[test]
fn every_block_draws_within_an_exact_width() {
    let doc = catalog();
    for (set, glyphs) in [("Unicode", Glyphs::Unicode), ("ASCII", Glyphs::Ascii)] {
        for cells in NARROWEST..FLOOR {
            let look = Look {
                glyphs,
                paint: Paint::Plain,
                width: Width::exact(cells),
            };
            let drawn = catch_unwind(AssertUnwindSafe(|| Terminal::on(look).draw(&doc)))
                .unwrap_or_else(|_| panic!("{set} at {cells} cells: drawing panicked"));
            let past: Vec<&str> = drawn
                .lines()
                .filter(|line| cell_width(line) > cells)
                .collect();
            assert!(
                past.is_empty(),
                "{set} at {cells} cells, past the edge:\n{}",
                past.join("\n")
            );
        }
    }
}
