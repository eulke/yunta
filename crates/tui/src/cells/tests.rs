//! A cell's style is what Surco's one table says of its span, on every layer, for every
//! tone and fill; plain paint styles nothing; a filled row is tinted to its edge; a line
//! stops at its area's edge.

use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use surco::line::{Fill, Line, Syntax, Tone};
use surco::paint::{self, Ansi, Ground, Layer, Paint};

use super::write;

const TONES: [Tone; 20] = [
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
    Tone::Code(Syntax::Keyword),
    Tone::Code(Syntax::Type),
    Tone::Code(Syntax::Function),
    Tone::Code(Syntax::Text),
    Tone::Code(Syntax::Number),
    Tone::Code(Syntax::Comment),
    Tone::Code(Syntax::Attribute),
    Tone::Code(Syntax::Macro),
];

const FILLS: [Option<Fill>; 7] = [
    None,
    Some(Fill::Tint),
    Some(Fill::Selection),
    Some(Fill::Code),
    Some(Fill::Added),
    Some(Fill::Removed),
    Some(Fill::Pill),
];

const LAYERS: [Layer; 3] = [
    Layer::Ansi16,
    Layer::Truecolor(Ground::Dark),
    Layer::Truecolor(Ground::Light),
];

/// A color of Surco's table, as a cell says it: each named color of the terminal's sixteen
/// that Surco paints with, which the terminal draws in its own theme, and 24 bits as they
/// are.
fn said(color: paint::Color) -> Color {
    match color {
        paint::Color::Ansi(Ansi::Red) => Color::Red,
        paint::Color::Ansi(Ansi::Green) => Color::Green,
        paint::Color::Ansi(Ansi::Yellow) => Color::Yellow,
        paint::Color::Ansi(Ansi::Magenta) => Color::Magenta,
        paint::Color::Ansi(Ansi::Cyan) => Color::Cyan,
        paint::Color::Rgb(red, green, blue) => Color::Rgb(red, green, blue),
    }
}

/// The cell style Surco's table gives a span in `tone` on `own` fill, on a line filled
/// `line`, in `layer`.
fn expected(layer: Layer, tone: Tone, own: Option<Fill>, line: Option<Fill>) -> Style {
    let style = paint::style(layer, tone, own, line);
    let mut cell = Style::default();
    if let Some(fg) = style.fg {
        cell = cell.fg(said(fg));
    }
    if let Some(bg) = style.bg {
        cell = cell.bg(said(bg));
    }
    if style.bold {
        cell = cell.add_modifier(Modifier::BOLD);
    }
    if style.dim {
        cell = cell.add_modifier(Modifier::DIM);
    }
    cell
}

/// `style` as a cell keeps it: what it leaves unsaid is the terminal's own.
fn kept(style: Style) -> Style {
    let mut cell = Cell::default();
    cell.set_style(style);
    cell.style()
}

fn row(cells: u16) -> Buffer {
    Buffer::empty(Rect::new(0, 0, cells, 1))
}

#[test]
fn a_cells_style_is_what_surcos_table_says_of_its_span() {
    for layer in LAYERS {
        let paint = Paint::Color {
            layer,
            links: false,
        };
        for tone in TONES {
            for own in FILLS {
                for filled in FILLS {
                    let mut line = match own {
                        Some(fill) => Line::new().push_filled(tone, fill, "ab"),
                        None => Line::new().push(tone, "ab"),
                    };
                    if let Some(fill) = filled {
                        line = line.filled(fill);
                    }
                    let mut buffer = row(4);
                    write(&line, Rect::new(0, 0, 4, 1), &mut buffer, paint);
                    let cell = &buffer[(0, 0)];
                    assert_eq!(cell.symbol(), "a");
                    assert_eq!(
                        cell.style(),
                        kept(expected(layer, tone, own, filled)),
                        "{layer:?} {tone:?} own {own:?} line {filled:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn plain_paint_styles_no_cell() {
    let line = Line::new()
        .push(Tone::NeedsYou, "◆ needs you")
        .filled(Fill::Tint);
    let mut buffer = row(20);
    write(&line, Rect::new(0, 0, 20, 1), &mut buffer, Paint::Plain);
    for x in 0..20 {
        assert_eq!(buffer[(x, 0)].style(), kept(Style::default()), "cell {x}");
    }
    assert_eq!(buffer[(0, 0)].symbol(), "◆");
}

#[test]
fn a_filled_row_is_tinted_to_its_areas_edge() {
    let layer = Layer::Truecolor(Ground::Dark);
    let paint = Paint::Color {
        layer,
        links: false,
    };
    let line = Line::new().push(Tone::Plain, "ab").filled(Fill::Tint);
    let mut buffer = row(12);
    write(&line, Rect::new(0, 0, 10, 1), &mut buffer, paint);
    let ground = paint::erase(layer, Some(Fill::Tint)).map(said);
    assert!(ground.is_some(), "a tint has a ground on 24 bits");
    for x in 2..10 {
        assert_eq!(buffer[(x, 0)].style().bg, ground, "cell {x}");
    }
    assert_eq!(
        buffer[(10, 0)].style(),
        kept(Style::default()),
        "past the area"
    );
}

#[test]
fn a_line_stops_at_its_areas_edge() {
    let line = Line::new().plain("abcdefgh");
    let mut buffer = row(10);
    write(&line, Rect::new(2, 0, 4, 1), &mut buffer, Paint::Plain);
    let text: String = (0..10)
        .map(|x| buffer[(x, 0)].symbol().to_string())
        .collect();
    assert_eq!(text, "  abcd    ");
}
