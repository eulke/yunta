use super::*;
use crate::line::{Fill, Line, Syntax, Tone};
use crate::paint::{Ground, Layer, Paint};

/// Every layer a stream with color draws in.
const LAYERS: [Layer; 3] = [
    Layer::Ansi16,
    Layer::Truecolor(Ground::Dark),
    Layer::Truecolor(Ground::Light),
];

/// No fill, and every fill a span or a line can carry.
const FILLS: [Option<Fill>; 7] = [
    None,
    Some(Fill::Tint),
    Some(Fill::Selection),
    Some(Fill::Code),
    Some(Fill::Added),
    Some(Fill::Removed),
    Some(Fill::Pill),
];

/// Every tone a span can take, every word of code among them.
fn tones() -> Vec<Tone> {
    let mut tones = vec![
        Tone::Plain,
        Tone::Strong,
        Tone::Command,
        Tone::Muted,
        Tone::Faint,
        Tone::Done,
        Tone::Failed,
        Tone::Running,
        Tone::NeedsYou,
        Tone::Caution,
        Tone::Added,
        Tone::Removed,
    ];
    tones.extend(
        [
            Syntax::Keyword,
            Syntax::Type,
            Syntax::Function,
            Syntax::Text,
            Syntax::Number,
            Syntax::Comment,
            Syntax::Attribute,
            Syntax::Macro,
        ]
        .map(Tone::Code),
    );
    tones
}

/// The SGR parameter a terminal reads as one of its sixteen colors for text.
fn text_color(ansi: Ansi) -> u8 {
    match ansi {
        Ansi::Red => 31,
        Ansi::Green => 32,
        Ansi::Yellow => 33,
        Ansi::Magenta => 35,
        Ansi::Cyan => 36,
    }
}

/// `text` in `style`, written here apart from `Paint` as a terminal reads it:
/// bold, dim, the text's color and the background's, then a reset.
fn written(style: Style, text: &str) -> String {
    let mut parameters = Vec::new();
    if style.bold {
        parameters.push("1".to_string());
    }
    if style.dim {
        parameters.push("2".to_string());
    }
    match style.fg {
        Some(Color::Ansi(ansi)) => parameters.push(text_color(ansi).to_string()),
        Some(Color::Rgb(red, green, blue)) => parameters.push(format!("38;2;{red};{green};{blue}")),
        None => {}
    }
    if let Some(Color::Rgb(red, green, blue)) = style.bg {
        parameters.push(format!("48;2;{red};{green};{blue}"));
    }
    match parameters.is_empty() {
        true => text.to_string(),
        false => format!("\x1b[{}m{text}\x1b[0m", parameters.join(";")),
    }
}

/// The background a filled line is erased with to its edge, written as a
/// terminal reads it.
fn erased(color: Option<Color>) -> String {
    match color {
        Some(Color::Rgb(red, green, blue)) => {
            format!("\x1b[48;2;{red};{green};{blue}m\x1b[K\x1b[0m")
        }
        Some(Color::Ansi(_)) | None => String::new(),
    }
}

/// Every span `Paint` writes, in every layer, on every fill of its own and every
/// fill of its line, is the style the reading gives it: a color has one table.
#[test]
fn every_span_paints_as_its_style_reads() {
    for layer in LAYERS {
        let paint = Paint::Color {
            layer,
            links: false,
        };
        for tone in tones() {
            for own in FILLS {
                for under in FILLS {
                    let span = match own {
                        Some(fill) => Line::new().push_filled(tone, fill, "word"),
                        None => Line::new().push(tone, "word"),
                    };
                    let line = match under {
                        Some(fill) => span.filled(fill),
                        None => span,
                    };
                    let expected = format!(
                        "{}{}",
                        written(style(layer, tone, own, under), "word"),
                        erased(erase(layer, under))
                    );
                    assert_eq!(
                        paint.line(&line),
                        expected,
                        "{layer:?} {tone:?} on {own:?} over {under:?}"
                    );
                }
            }
        }
    }
}

/// In the sixteen colors a tone has its bold, its dim and its color, and no
/// fill: the layer paints none.
#[test]
fn the_sixteen_colors_give_a_tone_its_bold_dim_and_color_and_no_fill() {
    let needs_you = style(Layer::Ansi16, Tone::NeedsYou, None, None);
    assert_eq!(
        needs_you,
        Style {
            bold: true,
            dim: false,
            fg: Some(Color::Ansi(Ansi::Magenta)),
            bg: None,
        }
    );
    let muted = style(
        Layer::Ansi16,
        Tone::Muted,
        Some(Fill::Tint),
        Some(Fill::Code),
    );
    assert_eq!(
        muted,
        Style {
            bold: false,
            dim: true,
            fg: None,
            bg: None,
        }
    );
}

/// In 24 bits a tone has its color tuned to the ground, on its line's fill.
#[test]
fn twenty_four_bits_give_a_tone_its_tuned_color_on_its_lines_fill() {
    let done = style(
        Layer::Truecolor(Ground::Light),
        Tone::Done,
        None,
        Some(Fill::Tint),
    );
    assert_eq!(done.fg, Some(Color::Rgb(45, 134, 73)));
    assert_eq!(done.bg, Some(Color::Rgb(246, 233, 243)));
    assert!(!done.bold && !done.dim);
}

/// A pill is its tone's color with the pill's ink on it, in bold.
#[test]
fn a_pill_is_its_tones_color_under_the_pills_ink() {
    let pill = style(
        Layer::Truecolor(Ground::Dark),
        Tone::Done,
        Some(Fill::Pill),
        None,
    );
    assert_eq!(pill.fg, Some(Color::Rgb(28, 18, 34)));
    assert_eq!(pill.bg, Some(Color::Rgb(143, 206, 155)));
    assert!(pill.bold);
}

/// A filled line is erased with its fill in 24 bits and with nothing in the
/// sixteen colors, and a page has its own colors.
#[test]
fn a_line_is_erased_with_its_fill_and_a_page_has_its_own_colors() {
    assert_eq!(erase(Layer::Ansi16, Some(Fill::Tint)), None);
    assert_eq!(
        erase(Layer::Truecolor(Ground::Dark), Some(Fill::Tint)),
        Some(Color::Rgb(36, 26, 43))
    );
    assert_eq!(
        page(Ground::Light)[0],
        ("ground", Color::Rgb(251, 250, 246))
    );
}
