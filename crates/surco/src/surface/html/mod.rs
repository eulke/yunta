//! A document as a page a browser reads: one file, which asks nothing of the
//! network, and whose head tells the browser so. A browser lays the words out,
//! so no line is wrapped here; a class names a tone, and the page's head sets
//! each from the palette, for a light ground and for a dark one. What a reader
//! chooses is a command, on a button that copies it: a page runs nothing.

mod asked;
mod blocks;
mod card;
mod code;
mod decision;
mod desk;
mod diagram;
mod markdown;
mod notices;
mod page;
mod screen;
mod style;
mod tables;
mod text;

use crate::components::Screen;
use crate::doc::Doc;
use crate::glyphs::Glyphs;
use crate::look::{Look, Width};
use crate::motion::Motion;
use crate::paint::Paint;
use crate::surface::Surface;

/// The set a page is drawn with: a browser draws any character.
const GLYPHS: Glyphs = Glyphs::Unicode;

/// The cells a builder that asks for a width is told a page has: the most a
/// line is laid out at, since a page wraps its own words.
const CELLS: usize = 120;

/// A page a browser reads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Html;

impl Html {
    /// What a page says, without what dresses it: what a golden holds, so a
    /// change to a color or a rule of the style does not move every page.
    pub fn body(&self, doc: &Doc) -> String {
        page::body(doc)
    }

    /// A screen as what a page's body holds, without what dresses it, as
    /// [`Html::body`] is: a golden holds it, and a page lays it out in a grid.
    pub fn screen(&self, screen: &Screen) -> String {
        screen::screen(screen)
    }

    /// The look a document is built with for a page, which is read rather
    /// than watched, so nothing on it turns.
    pub fn look(&self) -> Look {
        Look {
            glyphs: GLYPHS,
            paint: Paint::Plain,
            width: Width::of(Some(CELLS), None),
            motion: Motion::Still,
        }
    }
}

impl Surface for Html {
    /// The whole file.
    type Output = String;

    fn draw(&self, doc: &Doc) -> String {
        page::whole(doc)
    }
}

#[cfg(test)]
pub(super) mod tests;
