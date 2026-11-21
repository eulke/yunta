//! Surco's lines written into Ratatui's cells: each span in the style Surco's one table
//! of color gives its tone and fill on the stream's layer, and the rest of a filled row
//! erased with the fill's ground, so a row is tinted to its area's edge. Under plain paint
//! no cell gets a style. A line stops at its area's edge.
//!
//! A line replaces every cell of its row whole, its glyph, its tones and its ground, so a
//! box drawn over a screen shows nothing of what stood under it: Ratatui sets only what a
//! style names and keeps the rest of the cell, so each cell is emptied before its span is
//! written. A wide glyph just before the row reaches into its first cell and can no longer
//! stand whole: it is cut to a blank on its own ground, as a terminal cuts it, and Ratatui
//! never skips the row's first cell as that glyph's. Not a clearing before a box alone:
//! every line owns its row wherever it stands, so no drawing has to remember to clear first.
//!
//! No color is read anywhere else in the crate: `paint::style` and `paint::erase` are the
//! table, and this module only says each of their colors as a cell's.

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use surco::line::Line;
use surco::paint::{self, Ansi, Layer, Paint};
use surco::width::cell_width;

/// `line` written into `buffer` on `area`'s first row, painted by `paint`: every cell of
/// that row replaced whole.
pub(crate) fn write(line: &Line, area: Rect, buffer: &mut Buffer, paint: Paint) {
    let layer = match paint {
        Paint::Plain => None,
        Paint::Color { layer, .. } => Some(layer),
    };
    emptied(Rect::new(area.x, area.y, area.width, 1), buffer);
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

/// Every cell of `row` in `buffer` emptied, no glyph, tone or ground left of what stood
/// there; and a wide glyph just before it, which reaches into its first cell, cut to a blank
/// on its own ground.
fn emptied(row: Rect, buffer: &mut Buffer) {
    let row = row.intersection(buffer.area);
    if row.is_empty() {
        return;
    }
    for at in row.positions() {
        buffer[at].reset();
    }
    let before = row.x.checked_sub(1).map(|x| Position::new(x, row.y));
    if let Some(cell) = before.and_then(|at| buffer.cell_mut(at)) {
        if cell_width(cell.symbol()) > 1 {
            cell.set_symbol(" ");
        }
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
