//! Prose, marked items, code, and what a message says whole.

use crate::components::{Code, Marked, Note, Paths, Prose, Said};
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::{cell_width, indent, wrap, wrap_unbroken, INDENT};

impl Prose {
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        paragraph(&self.0, Tone::Plain, look)
    }
}

impl Note {
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        paragraph(&self.0, Tone::Muted, look)
    }
}

/// `text` in `tone`, wrapped one step in. A word is never cut: a path or a
/// command cut in two is two things nobody can copy.
fn paragraph(text: &str, tone: Tone, look: &Look) -> Vec<Line> {
    let room = look.width.cells().saturating_sub(cell_width(INDENT));
    wrap_unbroken(text, room)
        .into_iter()
        .map(|part| Line::new().plain(INDENT).push(tone, part))
        .collect()
}

impl Marked {
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let glyph = look.glyphs.mark(self.mark).to_string();
        let under = format!("{INDENT}{}", " ".repeat(cell_width(&glyph) + 1));
        let room = look.width.cells().saturating_sub(cell_width(&under));
        let mut lines = Vec::new();
        for item in &self.items {
            for (at, part) in wrap_unbroken(item, room).into_iter().enumerate() {
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
    /// Where the code is, on as many lines as the line's room asks: broken
    /// before a joiner, never inside the file's path, each part after the
    /// first under the file; then its `size` at the far edge, when the last
    /// part leaves room for it.
    fn titled(&self, size: &str, look: &Look) -> Vec<Line> {
        let room = look.width.cells();
        let under = format!("{INDENT}{INDENT}");
        let margin = |at: usize| match at {
            0 => INDENT,
            _ => under.as_str(),
        };
        let mut parts = self.title_parts(look.glyphs).into_iter();
        let mut rows: Vec<String> = parts.next().into_iter().collect();
        for part in parts {
            let at = rows.len() - 1;
            let row = &mut rows[at];
            match cell_width(margin(at)) + cell_width(row) + 1 + cell_width(&part) <= room {
                true => {
                    row.push(' ');
                    row.push_str(&part);
                }
                false => rows.push(part),
            }
        }
        let last = rows.len() - 1;
        rows.into_iter()
            .enumerate()
            .map(|(at, row)| {
                let used = cell_width(margin(at)) + cell_width(&row);
                let line = Line::new().plain(margin(at)).push(Tone::Strong, row);
                match at == last && !size.is_empty() && used + 2 + cell_width(size) <= room {
                    true => line
                        .plain(" ".repeat(room - used - cell_width(size)))
                        .push(Tone::Muted, size),
                    false => line,
                }
            })
            .collect()
    }

    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let size = match self.whole {
            0 => String::new(),
            lines => crate::text::counted(lines, "line"),
        };
        let room = look.width.cells();
        let mut lines = self.titled(&size, look);
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
                    "{} more: {rest}",
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
                    Some(reason) => line.plain(format!(" ({reason})")),
                    None => line,
                }
            }))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::components::Code;
    use crate::look::Look;

    /// A title too long for its line breaks before a joiner, never inside
    /// the file's path, and leaves out its size rather than run past the
    /// edge.
    #[test]
    fn a_code_title_too_long_for_its_line_breaks_at_its_joiner() {
        let code = Code::whole("crates/engine/src/frozen/provenance.rs", None, "fn a();")
            .within(["pack_provenance"]);
        let look = Look {
            width: Look::plain().width.within(44),
            ..Look::plain()
        };
        let drawn: Vec<String> = code.lines(&look).iter().map(|line| line.text()).collect();
        assert_eq!(drawn[0], "  crates/engine/src/frozen/provenance.rs");
        assert_eq!(
            drawn[1].trim_end(),
            "    :: pack_provenance                1 line"
        );
        assert!(
            drawn.iter().all(|line| line.chars().count() <= 44),
            "{drawn:?}"
        );
    }
}
