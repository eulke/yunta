//! Surco, Yunta's design language: how Yunta tells a reader things.
//!
//! The words a run's and a node's state are called by, the marks that
//! repeat them for the eye, how a sentence counts, lists and quotes, how a
//! figure reads, the columns a line is built from, the blocks a document
//! is composed of, what a screen holds, and the surfaces that draw them: a
//! terminal, at its width and in its paint, a Markdown file and a page.
//!
//! One crate, below every other crate of the workspace and depending on
//! none of them, because two readers told the same fact in two words — a
//! person at a terminal, a file the engine writes, a pull request, an
//! agent reading a tool's reply — read it side by side. Yunta's own
//! documents are made of these blocks elsewhere; what depends on the
//! process — which stream is a terminal, how wide it measures, what the
//! environment asked for — is the caller's to settle and pass in.
//!
//! A block holds what is said and the role each part plays, never a
//! width, an escape or a glyph: a glyph is written only in [`glyphs`], an
//! escape only in [`paint`], and a state word only in [`words`]. Its map:
//!
//! - What is said: `words` and `figures`, `text` for the grammar and the layout
//!   of a sentence, `prose` for what a session wrote, and `paths`.
//! - What it is drawn with: `glyphs`, `line`, `bars`, `motion`, `paint` and
//!   `palette`, for a stream or a pane that `look` and `width` measure. A style
//!   in `paint` is the one table of color every engine that draws reads.
//! - What a document is made of: `doc`, `components`, `diagram`, and `code`, the
//!   one lexer every reader of code reads with. A screen, in `components`, holds
//!   documents with no cell, pixel or color: each interface lays it out.
//! - Who draws it: `surface`, and `golden` compares what a surface drew with what
//!   was written down for it.
//!
//! Not a module of `yunta-render` or a crate above `yunta-core`: a language that
//! shares a crate with its documents, or that can see the domain's types, holds
//! its rules by care, and core's refusals need its grammar from below.

// A panic is a bug, never a fallible path: production returns a typed
// error instead of unwrapping, expecting or panicking.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

pub mod bars;
pub mod code;
pub mod components;
pub mod diagram;
pub mod doc;
pub mod figures;
pub mod glyphs;
#[cfg(any(test, feature = "golden"))]
pub mod golden;
pub mod line;
pub mod look;
pub mod motion;
pub mod paint;
mod palette;
pub mod paths;
pub mod prose;
pub mod surface;
pub mod text;
pub mod width;
pub mod words;
