//! Surco, how Yunta tells a reader things: the visual language every surface
//! draws with — its principles, its tokens (the columns a line is built from,
//! the step every block is indented by, the glyphs and the ink) and its
//! components (bars and sparklines, blocks, a decision's options, the sentences
//! a log's typed facts are read as).
//!
//! One crate, because two readers told the same fact in two words — a person at
//! a terminal, a file a run leaves, a pull request — read it side by side. It
//! sits on `yunta-core` alone: the engine hands core data, and the CLI and the
//! adapters draw it with the same words and blocks. A `Doc` says
//! what to tell, and a surface draws it for its medium. What depends on the
//! process — which stream is a terminal, how wide it measures, what the
//! environment asked for — is the caller's to settle and pass in. How a problem
//! looks is decided once, in [`yunta_core::text`], which this crate calls into.
//!
//! Its map:
//!
//! - What a surface says and draws: `doc`, `surface`, `blocks`, `markdown`,
//!   `diagram`, `look`, `line_width` and `width`.
//! - Tokens: `glyphs`, `ink`, `color` and `bars`.
//! - A run and its nodes: `run_word`, `state`, `pause`, `outcome`, `failure`,
//!   `spend`, `latency`, `caution`, `inputs`, `place`, `paths`, `target`,
//!   `prose` and `policy`.
//! - A decision a person meets: `escalation`, `claim`, `evidence`, `offers`,
//!   `shown`, `plan`, `spec`, `fold`, `findings`, `found`, `published` and
//!   `receipt`.

pub mod bars;
pub mod blocks;
pub mod caution;
pub mod claim;
pub mod color;
pub mod diagram;
pub mod doc;
pub mod escalation;
pub mod evidence;
pub mod failure;
mod findings;
mod fold;
pub mod found;
pub mod glyphs;
pub mod ink;
pub mod inputs;
mod latency;
pub mod line_width;
pub mod look;
pub mod markdown;
pub mod offers;
pub mod outcome;
pub mod paths;
pub mod pause;
pub mod place;
mod plan;
pub mod policy;
pub mod prose;
pub mod published;
pub mod receipt;
mod run_word;
pub mod shown;
mod spec;
pub mod spend;
pub mod state;
pub mod surface;
pub mod target;
pub mod width;

pub use bars::{bar, bar_cells, sparkline};
pub use escalation::evidence;
pub use glyphs::Glyphs;
pub use latency::latency;
pub use line_width::Width;
pub use look::Look;
pub use state::{Mark, NodeDisplay, RunWord, StateWord, STATE_WIDTH};
pub use width::{
    cell_width, cut, id_column, indent, middle_cut, truncate, wrap, CHILD_DEPTH, INDENT,
    INDENT_WIDTH, LABEL_WIDTH, LINE_WIDTH,
};
pub use yunta_core::units::{duration, Ratio, Tokens};

#[cfg(test)]
mod tests {
    mod cautions;
    mod claims;
    mod deciding;
    mod documents;
    mod failure;
    mod failure_families;
    mod found;
    mod grant;
    mod inputs;
    mod offers;
    mod retry;
}
