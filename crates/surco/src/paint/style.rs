//! What a span looks like in a layer, as data: bold or dim, the color of its
//! text and the color behind it.
//!
//! A color has one table. Every drawing reads it here: the escapes [`Paint`]
//! writes, the styles `clap` draws its help in, a page's colors, and any other
//! engine that draws Surco's lines in cells of its own. No color is chosen
//! outside this reading, and `every_span_paints_as_its_style_reads` holds that
//! the escapes are its style and nothing else. The values themselves stay in
//! `palette`, with the contrast floors its tests keep.
//!
//! [`Paint`]: super::Paint

use super::{Ground, Layer};
use crate::line::{Fill, Tone};
use crate::palette::{self, Rgb};

/// How a span is drawn in one layer: what an escape, a page's style or a cell
/// of another drawing engine sets. Not a [`Tone`], which is the span's role;
/// this is what that role looks like in a layer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Style {
    pub bold: bool,
    /// Drawn fainter than the terminal's own text: the sixteen colors' only
    /// way to recede.
    pub dim: bool,
    /// The text's color; `None` leaves it the terminal's own.
    pub fg: Option<Color>,
    /// The color behind the text; `None` leaves the ground showing.
    pub bg: Option<Color>,
}

/// A color as a layer has it: one of the terminal's sixteen, which it draws
/// in its own theme, or 24 bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Ansi(Ansi),
    Rgb(u8, u8, u8),
}

impl From<Rgb> for Color {
    fn from(Rgb(red, green, blue): Rgb) -> Self {
        Color::Rgb(red, green, blue)
    }
}

/// A color of the sixteen, which every terminal draws in its own theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ansi {
    Red,
    Green,
    Yellow,
    Magenta,
    Cyan,
}

/// The style of a span in `tone`, on the fill it carries (`own`) or else its
/// line's (`line`), in `layer`.
///
/// The sixteen colors give a tone its bold, its dim and its color, and paint
/// no fill. The 24 bits give it its tuned color on the ground, bold where the
/// sixteen colors are bold, on its own fill or else its line's. A pill is its
/// tone's color with the pill's ink on it, in bold; a pill in a tone the
/// palette leaves to the terminal has no color to stand on, so it is its word
/// on the line's fill.
pub fn style(layer: Layer, tone: Tone, own: Option<Fill>, line: Option<Fill>) -> Style {
    let (bold, dim, color) = ansi(tone);
    match layer {
        Layer::Ansi16 => Style {
            bold,
            dim,
            fg: color.map(Color::Ansi),
            bg: None,
        },
        Layer::Truecolor(ground) => truecolor(ground, tone, bold, own, line),
    }
}

/// The style of `tone` in 24 bits on `ground`, bold where the sixteen colors
/// are, on `own` or else `line`.
fn truecolor(
    ground: Ground,
    tone: Tone,
    bold: bool,
    own: Option<Fill>,
    line: Option<Fill>,
) -> Style {
    let text = palette::tone(ground, tone);
    match (own, text) {
        (Some(Fill::Pill), Some(color)) => Style {
            bold: true,
            dim: false,
            fg: Some(palette::pill_ink(ground).into()),
            bg: Some(color.into()),
        },
        _ => Style {
            bold,
            dim: false,
            fg: text.map(Color::from),
            bg: own
                .and_then(|fill| palette::fill(ground, fill))
                .or_else(|| line.and_then(|fill| palette::fill(ground, fill)))
                .map(Color::from),
        },
    }
}

/// The color a line on `line`'s fill is erased with to its edge, in `layer`:
/// none in the sixteen colors, which paint no fill, and none for a pill, which
/// has no background of its own.
pub fn erase(layer: Layer, line: Option<Fill>) -> Option<Color> {
    match layer {
        Layer::Ansi16 => None,
        Layer::Truecolor(ground) => palette::fill(ground, line?).map(Color::from),
    }
}

/// The colors only a page has on `ground`, each with the name a page's style
/// reads it by: its ground, a panel, its ink, a rule and a wash.
pub fn page(ground: Ground) -> [(&'static str, Color); 5] {
    palette::page(ground).map(|(name, color)| (name, color.into()))
}

/// How `tone` is drawn in the sixteen colors: bold, dim, and the ANSI color it
/// takes, so every terminal draws it in its own palette.
fn ansi(tone: Tone) -> (bool, bool, Option<Ansi>) {
    match tone {
        Tone::Plain => (false, false, None),
        Tone::Strong | Tone::Command => (true, false, None),
        Tone::Muted => (false, true, None),
        // In sixteen colors faint is dim, as muted is: the layer has no
        // step between them.
        Tone::Faint => (false, true, None),
        Tone::Done => (false, false, Some(Ansi::Green)),
        Tone::Failed => (false, false, Some(Ansi::Red)),
        Tone::Running => (false, false, Some(Ansi::Cyan)),
        Tone::NeedsYou => (true, false, Some(Ansi::Magenta)),
        Tone::Caution => (false, false, Some(Ansi::Yellow)),
        Tone::Added => (false, false, Some(Ansi::Green)),
        Tone::Removed => (false, false, Some(Ansi::Red)),
        // Code's colors are material of the 24-bit layer: in the sixteen
        // colors a word of code reads plain.
        Tone::Code(_) => (false, false, None),
    }
}

#[cfg(test)]
mod tests;
