//! The legend on a terminal: the keys of what is on screen, in a row; on a screen, the
//! row a screen keeps for them.

use super::Terminal;
use crate::components::{Binding, Legend};
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::cell_width;

impl Legend {
    /// Each key strong and what it does muted, a faint separator between two,
    /// on as many lines as the width takes: a key is never parted from what
    /// it does.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let width = look.width.cells();
        let sep = separator(look);
        let mut lines = Vec::new();
        let mut line = Line::new();
        for binding in &self.0 {
            let said = spelled(binding, look);
            let used = cell_width(&line.text());
            if used > 0 && used + cell_width(&sep) + cell_width(&said.text()) > width {
                lines.push(std::mem::take(&mut line));
            }
            if !line.text().is_empty() {
                line = line.push(Tone::Faint, sep.as_str());
            }
            line = line.then(said);
        }
        if !line.text().is_empty() {
            lines.push(line);
        }
        lines
    }
}

impl Terminal {
    /// The legend row a screen holds, at this look's width: every key where they all fit.
    /// Where they do not, the first keys that fit, a mark that more stand between, and the
    /// last key, which leads to the rest: what a screen offers last is the key that lists
    /// every key. A last key too wide even for that leaves the first keys that fit, cut.
    pub fn legend(&self, legend: &Legend) -> Line {
        let look = &self.look;
        let cells = look.width.cells();
        let whole = legend.lines(look);
        if whole.len() <= 1 {
            return whole.into_iter().next().unwrap_or_default();
        }
        let sep = separator(look);
        let Some((last, first)) = legend.0.split_last() else {
            return Line::new();
        };
        let tail = Line::new()
            .push(Tone::Faint, sep.as_str())
            .push(Tone::Faint, look.glyphs.ellipsis())
            .push(Tone::Faint, sep.as_str())
            .then(spelled(last, look));
        let room = cells.saturating_sub(cell_width(&tail.text()));
        let mut row = Line::new();
        for binding in first {
            let said = spelled(binding, look);
            let joined = match row.text().is_empty() {
                true => said,
                false => row.clone().push(Tone::Faint, sep.as_str()).then(said),
            };
            if cell_width(&joined.text()) > room {
                break;
            }
            row = joined;
        }
        match row.text().is_empty() {
            true => whole
                .into_iter()
                .next()
                .unwrap_or_default()
                .cut(cells, look.glyphs),
            false => row.then(tail),
        }
    }
}

/// What stands between two keys.
fn separator(look: &Look) -> String {
    format!(" {} ", look.glyphs.sep())
}

/// One key, strong, and what it does, muted.
fn spelled(binding: &Binding, look: &Look) -> Line {
    Line::new()
        .push(Tone::Strong, binding.keys.spelled(look.glyphs))
        .push(Tone::Muted, format!(" {}", binding.does))
}

#[cfg(test)]
mod tests;
