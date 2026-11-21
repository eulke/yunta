//! A cell's style is what Surco's one table says of its span, on every layer, for every
//! tone and fill; plain paint styles nothing; a filled row is tinted to its edge; a line
//! stops at its area's edge; a line replaces each cell of its row whole, and cuts a wide
//! glyph that reaches into it.

use ratatui::backend::TestBackend;
use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::Terminal;
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

/// Over a row in bold, a color and a ground of its own, a line in the terminal's own tone
/// with no fill leaves nothing of it in the cells of its area: its words in their own style,
/// every cell past them blank and unstyled; the cells outside its area keep what stood
/// there. On every layer.
#[test]
fn a_line_replaces_each_cell_of_its_row_whole() {
    for layer in LAYERS {
        let paint = Paint::Color {
            layer,
            links: false,
        };
        let under = Line::new()
            .push_filled(Tone::NeedsYou, Fill::Selection, "xxxxxxxxxx")
            .filled(Fill::Selection);
        let mut buffer = row(10);
        write(&under, Rect::new(0, 0, 10, 1), &mut buffer, paint);
        let stood = buffer[(0, 0)].clone();
        let over = Line::new().push(Tone::Plain, "ab");
        write(&over, Rect::new(2, 0, 6, 1), &mut buffer, paint);
        let own = kept(expected(layer, Tone::Plain, None, None));
        assert_eq!(buffer[(2, 0)].symbol(), "a");
        assert_eq!(buffer[(2, 0)].style(), own, "{layer:?}: the line's word");
        for x in 4..8 {
            assert_eq!(buffer[(x, 0)], Cell::EMPTY, "{layer:?}: cell {x}");
        }
        for x in [0, 1, 8, 9] {
            assert_eq!(
                buffer[(x, 0)],
                stood,
                "{layer:?}: cell {x}, outside the area"
            );
        }
    }
}

/// A line written where a wide glyph's second half stood cuts that glyph to a blank on its
/// own ground, so the terminal draws the line's first cell rather than the glyph across it.
#[test]
fn a_wide_glyph_a_line_reaches_into_is_cut_to_a_blank() {
    let layer = Layer::Truecolor(Ground::Dark);
    let paint = Paint::Color {
        layer,
        links: false,
    };
    let mut terminal = Terminal::new(TestBackend::new(6, 1)).expect("a test backend");
    terminal
        .draw(|frame| {
            let under = Line::new().push(Tone::Plain, "a界b").filled(Fill::Code);
            write(&under, Rect::new(0, 0, 6, 1), frame.buffer_mut(), paint);
            let over = Line::new().push(Tone::Muted, "│c");
            write(&over, Rect::new(2, 0, 4, 1), frame.buffer_mut(), paint);
        })
        .expect("a drawing");
    let shown = terminal.backend().buffer();
    let text: String = (0..6).map(|x| shown[(x, 0)].symbol().to_string()).collect();
    assert_eq!(text, "a │c  ");
    let ground = paint::erase(layer, Some(Fill::Code)).map(said);
    assert_eq!(Some(shown[(1, 0)].bg), ground, "the cut glyph's own ground");
}
