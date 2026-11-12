//! Yunta's application at a terminal: each screen `yunta-ui` makes, laid out in cells and
//! drawn in Surco's lines, the keys a person presses read as strokes, and the terminal held
//! while the application is open and given back on every way out.
//!
//! It composes no word, no tone and no mark. Every word reaches it through `yunta-ui`,
//! which says it in render's words; every line, mark and color is Surco's, so the
//! desktop finds nothing here it would have to decide again. It is the one crate that
//! knows Ratatui and crossterm.
