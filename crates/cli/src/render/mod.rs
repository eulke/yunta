//! How a run looks to a person at this terminal: what drawing Surco's
//! documents depends on in this process — the stream a line is written
//! to, the policy `main` settled, and the words of the engine's own
//! types.

pub(crate) mod counter;
mod stream;
mod words;

pub(crate) use stream::{
    glyphs, row_width, settle_color, settle_columns, settle_glyphs, stderr_ink, stderr_width,
    stdout_look,
};
pub(crate) use words::{engine, exit, node_display, RunExit};

use surco::doc::Doc;
use surco::line::Line;
use surco::look::Look;

/// `doc` laid out for a terminal stream whose lines look like `look`,
/// unpainted.
pub(crate) fn surface_lines(doc: Doc, look: &Look) -> Vec<Line> {
    surco::surface::Terminal::on(*look).lines(&doc)
}

/// `doc` drawn on a terminal stream whose lines look like `look`.
pub(crate) fn draw(doc: Doc, look: &Look) -> String {
    use surco::surface::Surface;
    surco::surface::Terminal::on(*look).draw(&doc)
}
