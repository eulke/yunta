//! Prose, marked items, code, and what a message says whole.

use crate::components::{Code, Marked, Paths, Prose, Said};
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::{cell_width, indent, wrap, INDENT};

impl Prose {
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let room = look.width.cells().saturating_sub(cell_width(INDENT));
        wrap(&self.0, room)
            .into_iter()
            .map(|part| Line::new().plain(INDENT).plain(part))
            .collect()
    }
}

impl Marked {
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let glyph = look.glyphs.mark(self.mark).to_string();
        let under = format!("{INDENT}{}", " ".repeat(cell_width(&glyph) + 1));
        let room = look.width.cells().saturating_sub(cell_width(&under));
        let mut lines = Vec::new();
        for item in &self.items {
            for (at, part) in wrap(item, room).into_iter().enumerate() {
                lines.push(match at {
                    0 => Line::new()
                        .plain(INDENT)
                        .push(Tone::of(self.mark), glyph.as_str())
                        .plain(" ")
                        .plain(part),
                    _ => Line::new().plain(under.as_str()).plain(part),
                });
            }
        }
        lines
    }
}

impl Code {
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let size = match self.whole {
            0 => String::new(),
            lines => crate::text::counted(lines, "line"),
        };
        let room = look.width.cells();
        let gap = room
            .saturating_sub(cell_width(INDENT) + cell_width(&self.at) + cell_width(&size))
            .max(2);
        let mut head = Line::new()
            .plain(INDENT)
            .push(Tone::Strong, self.at.as_str());
        if !size.is_empty() {
            head = head.plain(" ".repeat(gap)).push(Tone::Muted, size);
        }
        let mut lines = vec![head];
        let under = format!("{INDENT}{INDENT}");
        if let Some(what) = &self.what {
            let room = room.saturating_sub(cell_width(&under));
            lines.extend(
                wrap(what, room)
                    .into_iter()
                    .map(|part| Line::new().plain(under.as_str()).plain(part)),
            );
        }
        let gutter = format!("{} ", look.glyphs.gutter());
        let code_room = room.saturating_sub(cell_width(&under) + cell_width(&gutter));
        for line in &self.lines {
            let pieces = match line.is_empty() {
                true => vec![String::new()],
                false => super::markdown::continued(line, code_room, look.glyphs),
            };
            for piece in pieces {
                let line = Line::new().plain(under.as_str());
                lines.push(match piece.is_empty() {
                    true => line.push(Tone::Muted, gutter.trim_end()),
                    false => line.push(Tone::Muted, gutter.as_str()).plain(piece),
                });
            }
        }
        if let Some(rest) = &self.rest {
            lines.push(Line::new().plain(under.as_str()).push(
                Tone::Muted,
                format!(
                    "{} more — {rest}",
                    crate::text::counted(self.whole - self.lines.len(), "line")
                ),
            ));
        }
        lines
    }
}

impl Said {
    pub(in crate::surface) fn lines(&self, _look: &Look) -> Vec<Line> {
        self.0
            .iter()
            .map(|said| Line::new().plain(INDENT).plain(said.as_str()))
            .collect()
    }
}

impl Paths {
    pub(in crate::surface) fn lines(&self, _look: &Look) -> Vec<Line> {
        std::iter::once(Line::new().plain(INDENT).plain(self.heading.as_str()))
            .chain(self.paths.iter().map(|note| {
                let line = Line::new()
                    .plain(indent(2))
                    .plain(note.path.display().to_string());
                match &note.reason {
                    Some(reason) => line.plain(" — ").plain(reason.as_str()),
                    None => line,
                }
            }))
            .collect()
    }
}
