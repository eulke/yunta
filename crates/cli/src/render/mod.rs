//! How a run looks to a person at a terminal: the columns a line is
//! built from, the bars and sparklines that carry a magnitude, the words
//! a node's state is called by, and the characters it is all drawn with.
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
pub(crate) mod glyphs;
pub(crate) mod state;
pub(crate) mod units;
pub(crate) mod width;

pub(crate) use bars::{bar, sparkline};
pub(crate) use glyphs::Glyphs;
pub(crate) use state::{NodeDisplay, StateWord, STATE_WIDTH};
pub(crate) use units::{format_duration, format_pct};
pub(crate) use width::{cell_width, truncate, LABEL_WIDTH, LINE_WIDTH};
