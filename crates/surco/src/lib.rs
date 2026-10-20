//! Surco, Yunta's design language: how Yunta tells a reader things.
//!
//! The words a run's and a node's state are called by, the marks that
//! repeat them for the eye, how a sentence counts, lists and quotes, how a
//! figure reads, the columns a line is built from, the blocks a document
//! is composed of, and the surfaces that draw a document: a terminal, at
//! its width and in its paint, and a Markdown file.
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
//! escape only in [`paint`], and a state word only in [`words`].

pub mod bars;
mod code;
pub mod components;
pub mod diagram;
pub mod doc;
pub mod figures;
pub mod glyphs;
#[cfg(any(test, feature = "golden"))]
pub mod golden;
pub mod line;
pub mod look;
pub mod markdown;
pub mod paint;
pub mod paths;
pub mod prose;
pub mod surface;
pub mod text;
pub mod width;
pub mod words;
