//! How a run looks to a person at this terminal: the words and blocks of
//! [`yunta_render`], and what drawing them depends on in this process —
//! the stream a line is written to, the policy `main` settled, and the
//! words of the engine's own types.
//!
//! Everything a surface draws comes through here, so a surface names one
//! module whether what it draws is the shared vocabulary or this
//! process's view of a stream.

pub(crate) mod counter;
mod stream;
mod words;

pub(crate) use stream::{
    glyphs, row_width, settle_color, settle_columns, settle_glyphs, stderr_ink, stderr_width,
    stdout_look, stdout_width,
};
pub(crate) use words::{exit, observed_word, run_word, standing, RunExit};
#[cfg(test)]
pub(crate) use yunta_render::LINE_WIDTH;

/// `doc` drawn on a terminal stream whose lines look like `look`.
pub(crate) fn draw(doc: doc::Doc<'_>, look: &Look) -> String {
    use yunta_render::surface::Surface;
    yunta_render::surface::Terminal::on(*look).draw(&doc)
}
pub(crate) use yunta_render::{
    bar, bar_cells, blocks, cell_width, doc, duration, evidence, glyphs, id_column, indent, ink,
    label, middle_cut, paths, prose, shown, sparkline, state, truncate, wrap, Glyphs, Look, Mark,
    NodeDisplay, Ratio, StateWord, Tokens, Width, CHILD_DEPTH, INDENT, INDENT_WIDTH, LABEL_WIDTH,
    STATE_WIDTH,
};

/// The look of a terminal a golden is rendered for.
#[cfg(test)]
pub(crate) fn look_of(environment: &yunta_testkit::Environment) -> Look {
    Look {
        glyphs: match environment.unicode {
            true => Glyphs::Unicode,
            false => Glyphs::Ascii,
        },
        ink: match environment.color {
            true => ink::Ink::Ansi16,
            false => ink::Ink::Plain,
        },
        width: Width::of(Some(environment.width), None),
    }
}
