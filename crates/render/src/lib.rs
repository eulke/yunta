//! How Yunta tells a reader things: the columns a line is built from,
//! the step every block is indented by, the bars and sparklines that
//! carry a magnitude, the words a run's and a node's state and a
//! decision's options are called by, and the characters it is all drawn
//! with.
//!
//! One crate, because two readers told the same fact in two words — a
//! person at a terminal, a file the engine writes, a pull request — read
//! it side by side. It sits on `yunta-core` alone, below the engine, so
//! the engine writes for a person with the same words and blocks the
//! command line draws. What depends on the process — which stream is a
//! terminal, how wide it measures, what the environment asked for — is
//! the caller's to settle and pass in.
//!
//! How a *problem* looks is not decided here. One line, a hanging block,
//! an indented block and the block that lists what is wrong with a
//! document all live in [`yunta_core::text`], which is the single place
//! that decides it; this crate calls into it and never writes a second
//! version of one.

pub mod bars;
pub mod blocks;
pub mod color;
pub mod doc;
pub mod escalation;
mod findings;
pub mod glyphs;
pub mod ink;
pub mod line_width;
pub mod look;
pub mod markdown;
pub mod paths;
mod plan;
pub mod prose;
mod run_word;
pub mod shown;
mod spec;
pub mod state;
pub mod surface;
pub mod width;

pub use bars::{bar, bar_cells, sparkline};
pub use escalation::{evidence, label};
pub use glyphs::Glyphs;
pub use line_width::Width;
pub use look::Look;
pub use state::{Mark, NodeDisplay, RunWord, StateWord, STATE_WIDTH};
pub use width::{
    cell_width, cut, id_column, indent, middle_cut, truncate, wrap, CHILD_DEPTH, INDENT,
    INDENT_WIDTH, LABEL_WIDTH, LINE_WIDTH,
};
pub use yunta_core::units::{duration, Ratio, Tokens};
