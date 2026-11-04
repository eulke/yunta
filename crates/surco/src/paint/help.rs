//! The styles `clap` draws its own help and usage errors in, in a paint's
//! colors and the tones every page takes.

#[cfg(test)]
mod tests;

use super::{ansi, Ansi, Ground, Layer, Paint};
use crate::line::Tone;
use crate::palette::{self, Rgb};

/// The styles `clap` draws its own help and usage errors in, in `paint`'s
/// colors and the tones every page takes: a heading and a value to fill in
/// muted, what a reader types strong, an error failed, a valid value done, an
/// invalid one a caution. A plain paint gets styles that write no escape, the
/// sixteen colors get the color `ansi` gives each tone, and 24 bits get each
/// tone's color on its ground, bold where the sixteen colors are bold.
pub fn clap_styles(paint: Paint) -> clap::builder::Styles {
    match paint {
        Paint::Plain => clap::builder::Styles::plain(),
        Paint::Color {
            layer: Layer::Ansi16,
            ..
        } => clap_roles(ansi16_style),
        Paint::Color {
            layer: Layer::Truecolor(ground),
            ..
        } => clap_roles(|tone| truecolor_style(ground, tone)),
    }
}

/// The styles `clap` draws its seven roles in: each role takes the style
/// `style` gives its tone, and the other roles keep the styles `clap` gives.
fn clap_roles(style: impl Fn(Tone) -> clap::builder::styling::Style) -> clap::builder::Styles {
    clap::builder::Styles::styled()
        .header(style(Tone::Muted))
        .usage(style(Tone::Muted))
        .literal(style(Tone::Strong))
        .placeholder(style(Tone::Muted))
        .error(style(Tone::Failed))
        .valid(style(Tone::Done))
        .invalid(style(Tone::Caution))
}

/// The style of `tone` in the sixteen colors: its ANSI color, bold and dim as
/// [`ansi`] says, so every terminal draws it in its own palette.
fn ansi16_style(tone: Tone) -> clap::builder::styling::Style {
    use clap::builder::styling::{AnsiColor, Style};
    let (bold, dim, color) = ansi(tone);
    let color = color.map(|code| match code {
        Ansi::Red => AnsiColor::Red,
        Ansi::Green => AnsiColor::Green,
        Ansi::Yellow => AnsiColor::Yellow,
        Ansi::Magenta => AnsiColor::Magenta,
        Ansi::Cyan => AnsiColor::Cyan,
    });
    let style = Style::new().fg_color(color.map(Into::into));
    let style = if bold { style.bold() } else { style };
    if dim {
        style.dimmed()
    } else {
        style
    }
}

/// The style of `tone` in 24 bits on `ground`: its color there, bold where the
/// sixteen colors are bold, and never dimmed.
fn truecolor_style(ground: Ground, tone: Tone) -> clap::builder::styling::Style {
    use clap::builder::styling::{Color, RgbColor, Style};
    let color = palette::tone(ground, tone)
        .map(|Rgb(red, green, blue)| Color::Rgb(RgbColor(red, green, blue)));
    let style = Style::new().fg_color(color);
    if ansi(tone).0 {
        style.bold()
    } else {
        style
    }
}
