//! How a run looks to a person at a terminal: the columns a line is
//! built from, the step every block is indented by, the bars and
//! sparklines that carry a magnitude, the words a node's state and a
//! decision's options are called by, and the characters it is all drawn
//! with.
//!
//! One module, because two surfaces that size the same column
//! differently, or call the same state by two names, are read side by
//! side by the same person. Everything here is for a terminal: the
//! machine surfaces — `--json`, the Mermaid and DOT syntax `yunta graph`
//! emits — are shaped by their own formats and take only the words from
//! here, never the layout.
//!
//! How a *problem* looks is not decided here. One line, a hanging block,
//! an indented block and the block that lists what is wrong with a
//! document all live in [`yunta_core::text`], which is the single place
//! that decides it; this module calls into it and never writes a second
//! version of one.

pub(crate) mod bars;
pub(crate) mod blocks;
pub(crate) mod counter;
pub(crate) mod escalation;
mod findings;
pub(crate) mod glyphs;
pub(crate) mod ink;
pub(crate) mod line_width;
pub(crate) mod look;
pub(crate) mod markdown;
pub(crate) mod paths;
mod plan;
pub(crate) mod prose;
mod run_word;
pub(crate) mod shown;
mod spec;
pub(crate) mod state;
pub(crate) mod width;

pub(crate) use bars::{bar, bar_cells, sparkline};
pub(crate) use escalation::{evidence, label};
pub(crate) use glyphs::Glyphs;
pub(crate) use line_width::Width;
pub(crate) use look::Look;
pub(crate) use state::{Mark, NodeDisplay, StateWord, STATE_WIDTH};
pub(crate) use width::{
    cell_width, cut, id_column, indent, middle_cut, truncate, wrap, CHILD_DEPTH, INDENT,
    INDENT_WIDTH, LABEL_WIDTH, LINE_WIDTH,
};
pub(crate) use yunta_core::units::{duration, Ratio, Tokens};
