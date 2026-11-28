//! What a box needs over a screen: its widest row and the cells around it, every cell it is
//! given where a row stands on the selection or it is a confirmation, and as many rows as it
//! holds between its edges.

use super::super::tests::{closing, keys, many, over, palette};
use super::BoxSize;
use crate::components::Overlay;
use crate::golden::ENVIRONMENTS;

/// A box needs its widest row, a cell in from the left upright, two blank cells past it and
/// the far upright; a palette, whose pointed row stands on the selection to the far upright,
/// every cell it is given; a title wider than its rows, the title and the cells around it; a
/// confirmation every cell it is given. Each as tall as all it holds between its edges.
#[test]
fn a_box_needs_its_widest_row_and_the_cells_around_it() {
    let screen = over(&ENVIRONMENTS[2], 98);
    let widest = "esc    one step back; what it leaves stays waiting".len();
    assert_eq!(
        screen.box_size(&keys(), 84),
        BoxSize {
            cells: 1 + 1 + widest + 2 + 1,
            rows: 1 + 4 + 1
        }
    );
    assert_eq!(screen.box_size(&palette(), 84).cells, 84);
    assert_eq!(screen.box_size(&palette(), 84).rows, 1 + 2 + 11 + 1);
    let titled = Overlay {
        title: "a title wider than what the box holds".to_string(),
        ..many(1, 0)
    };
    assert_eq!(
        screen.box_size(&titled, 84).cells,
        2 + titled.title.len() + 2
    );
    assert_eq!(screen.box_size(&closing(), 64).cells, 64);
}
