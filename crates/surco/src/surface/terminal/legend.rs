//! The legend on a terminal: the keys of what is on screen, in a row.

use crate::components::Legend;
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::cell_width;

impl Legend {
    /// Each key strong and what it does muted, a faint separator between two,
    /// on as many lines as the width takes: a key is never parted from what
    /// it does.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let width = look.width.cells();
        let sep = format!(" {} ", look.glyphs.sep());
        let mut lines = Vec::new();
        let mut line = Line::new();
        for binding in &self.0 {
            let keys = binding.keys.spelled(look.glyphs);
            let said = cell_width(keys) + 1 + cell_width(&binding.does);
            let used = cell_width(&line.text());
            if used > 0 && used + cell_width(&sep) + said > width {
                lines.push(std::mem::take(&mut line));
            }
            if !line.text().is_empty() {
                line = line.push(Tone::Faint, sep.as_str());
            }
            line = line
                .push(Tone::Strong, keys)
                .push(Tone::Muted, format!(" {}", binding.does));
        }
        if !line.text().is_empty() {
            lines.push(line);
        }
        lines
    }
}

#[cfg(test)]
mod tests;
