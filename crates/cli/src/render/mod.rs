//! How a run looks to a person at this terminal: what drawing Surco's
//! documents depends on in this process — the stream a line is written
//! to, the policy `main` settled, and the words of the engine's own
//! types.

pub(crate) mod counter;
mod stream;
mod words;

pub(crate) use stream::{
    glyphs, row_width, settle_color, settle_columns, settle_glyphs, stderr_paint, stderr_width,
    stdout_look,
};
pub(crate) use words::{engine, exit, node_display, RunExit};

use surco::doc::{Block, Doc};
use surco::line::Line;
use surco::look::Look;

/// `doc` laid out for a terminal stream whose lines look like `look`,
/// unpainted.
pub(crate) fn surface_lines(doc: Doc, look: &Look) -> Vec<Line> {
    surco::surface::Terminal::on(*look).lines(&doc)
}

/// `block` laid out for a terminal stream whose lines look like `look`,
/// unpainted.
pub(crate) fn block_lines(block: impl Into<Block>, look: &Look) -> Vec<Line> {
    surco::surface::Terminal::on(*look).block(&block.into())
}

/// `blocks`, in order, laid out the same way.
pub(crate) fn blocks_lines(blocks: &[Block], look: &Look) -> Vec<Line> {
    surco::surface::Terminal::on(*look).blocks(blocks)
}

/// `doc` drawn on a terminal stream whose lines look like `look`.
pub(crate) fn draw(doc: Doc, look: &Look) -> String {
    use surco::surface::Surface;
    surco::surface::Terminal::on(*look).draw(&doc)
}
