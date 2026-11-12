//! Yunta's application at a terminal: each screen `yunta-ui` makes, laid out in cells and
//! drawn in Surco's lines, the keys a person presses read as strokes, and the terminal held
//! while the application is open and given back on every way out.
//!
//! It composes no word, no tone and no mark. Every word reaches it through `yunta-ui`,
//! which says it in render's words; every line, mark and color is Surco's, so the
//! desktop finds nothing here it would have to decide again. It is the one crate that
//! knows Ratatui and crossterm.
//!
//! Its map:
//!
//! - `application`: the loop, as [`Application`]; how it ended, as [`Exit`], and how the
//!   terminal failed it, as [`TerminalFailure`]: the terminal held, what arrives handed
//!   in, the work carried out, the screen drawn.
//! - `layout`: where each part of a screen stands on a terminal, by the reference's rules,
//!   with Ratatui's `Layout`.
//! - `cells`: Surco's lines written into Ratatui's cells, in the colors of Surco's one
//!   table.
//! - `keys`: what a key or a paste means to the application, by the CLI's table: a
//!   stroke, or Ctrl-Z, the terminal's own.
//! - `full_screen`: the terminal held while the application is open, as `FullScreen`, and
//!   given back on every way out.
//! - `drawn`: a screen drawn into a Ratatui frame ([`drawn()`]), what any Ratatui program
//!   draws a Yunta screen with, and what this crate's tests and its walk draw through.

mod application;
mod cells;
mod drawn;
mod full_screen;
mod keys;
mod layout;

pub use application::{Application, Exit, TerminalFailure};
pub use drawn::drawn;
