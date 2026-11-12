//! Surco's lines written into Ratatui's cells: each span in the style Surco's one table
//! of color gives its tone and fill on the stream's layer, and the rest of a filled row
//! erased with the fill's ground, so a row is tinted to its area's edge. Under plain paint
//! no cell gets a style. A line stops at its area's edge.
//!
//! No color is read anywhere else in the crate: `paint::style` and `paint::erase` are the
//! table, and this module only says each of their colors as a cell's.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use surco::line::Line;
use surco::paint::{self, Ansi, Layer, Paint};

/// `line` written into `buffer` on `area`'s first row, painted by `paint`.
pub(crate) fn write(line: &Line, area: Rect, buffer: &mut Buffer, paint: Paint) {
    let layer = match paint {
        Paint::Plain => None,
        Paint::Color { layer, .. } => Some(layer),
    };
    let right = area.right();
    let mut x = area.x;
    for span in line.spans() {
        if x >= right {
            break;
        }
        let style = layer.map_or_else(Style::default, |layer| {
            cell_style(paint::style(layer, span.tone, span.fill, line.fill()))
        });
        let (after, _) = buffer.set_stringn(x, area.y, &span.text, usize::from(right - x), style);
        x = after;
    }
    if let Some(ground) = layer.and_then(|layer: Layer| paint::erase(layer, line.fill())) {
        let rest = Rect::new(x, area.y, right.saturating_sub(x), 1);
        buffer.set_style(rest, Style::default().bg(cell_color(ground)));
    }
}

/// A style of Surco's table, as a cell's: its colors, its bold and its dim.
fn cell_style(style: paint::Style) -> Style {
    let mut cell = Style::default();
    if let Some(fg) = style.fg {
        cell = cell.fg(cell_color(fg));
    }
    if let Some(bg) = style.bg {
        cell = cell.bg(cell_color(bg));
    }
    if style.bold {
        cell = cell.add_modifier(Modifier::BOLD);
    }
    if style.dim {
        cell = cell.add_modifier(Modifier::DIM);
    }
    cell
}

/// A color of Surco's table, as a cell's: each named color of the terminal's sixteen that
/// Surco paints with, which the terminal draws in its own theme, and 24 bits as they are.
fn cell_color(color: paint::Color) -> Color {
    match color {
        paint::Color::Ansi(Ansi::Red) => Color::Red,
        paint::Color::Ansi(Ansi::Green) => Color::Green,
        paint::Color::Ansi(Ansi::Yellow) => Color::Yellow,
        paint::Color::Ansi(Ansi::Magenta) => Color::Magenta,
        paint::Color::Ansi(Ansi::Cyan) => Color::Cyan,
        paint::Color::Rgb(red, green, blue) => Color::Rgb(red, green, blue),
    }
}

#[cfg(test)]
mod tests;
