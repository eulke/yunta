//! The styles `clap` draws its own help and usage errors in, in a paint's
//! colors and the tones every page takes.

#[cfg(test)]
mod tests;

use super::{style, Ansi, Color, Layer, Paint, Style};
use crate::line::Tone;

/// The styles `clap` draws its own help and usage errors in, in `paint`'s
/// colors and the tones every page takes: a heading and a value to fill in
/// muted, what a reader types strong, an error failed, a valid value done, an
/// invalid one a caution. A plain paint gets styles that write no escape, the
/// sixteen colors get the color `ansi` gives each tone, and 24 bits get each
/// tone's color on its ground, bold where the sixteen colors are bold.
pub fn clap_styles(paint: Paint) -> clap::builder::Styles {
    match paint {
        Paint::Plain => clap::builder::Styles::plain(),
        Paint::Color { layer, .. } => clap_roles(|tone| clap_style(layer, tone)),
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

/// The style of `tone` in `layer`, as the style reading gives it with no fill:
/// in the sixteen colors its ANSI color, bold and dim, so every terminal draws
/// it in its own palette; in 24 bits its color on the ground, bold where the
/// sixteen colors are bold, and never dimmed.
fn clap_style(layer: Layer, tone: Tone) -> clap::builder::styling::Style {
    use clap::builder::styling::{AnsiColor, Color as ClapColor, RgbColor};
    let Style { bold, dim, fg, .. } = style(layer, tone, None, None);
    let color = fg.map(|color| match color {
        Color::Ansi(Ansi::Red) => ClapColor::Ansi(AnsiColor::Red),
        Color::Ansi(Ansi::Green) => ClapColor::Ansi(AnsiColor::Green),
        Color::Ansi(Ansi::Yellow) => ClapColor::Ansi(AnsiColor::Yellow),
        Color::Ansi(Ansi::Magenta) => ClapColor::Ansi(AnsiColor::Magenta),
        Color::Ansi(Ansi::Cyan) => ClapColor::Ansi(AnsiColor::Cyan),
        Color::Rgb(red, green, blue) => ClapColor::Rgb(RgbColor(red, green, blue)),
    });
    let clap = clap::builder::styling::Style::new().fg_color(color);
    let clap = if bold { clap.bold() } else { clap };
    if dim {
        clap.dimmed()
    } else {
        clap
    }
}
