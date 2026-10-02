//! The blocks a document is built from beyond a surface's own: a titled
//! section of other blocks, a paragraph, a list whose items each carry
//! the same mark, and a file's code.

use std::path::Path;

use super::Drawn;
use crate::doc::Block;
use crate::ink::{Line, Tone};
use crate::{cell_width, wrap, Look, Mark, INDENT};

/// What follows a title, one step under it: a medium draws the title
/// and then each block the way it draws any other.
pub struct Section<'a> {
    /// What the title is marked with, when its subject carries a mark —
    /// a finding's severity.
    pub mark: Option<Mark>,
    pub title: Line,
    pub blocks: Vec<Block<'a>>,
}

/// A paragraph, wrapped to the line by whoever draws it.
pub struct Prose(pub String);

impl Drawn for Prose {
    fn lines(&self, look: &Look) -> Vec<Line> {
        let room = look.width.cells().saturating_sub(cell_width(INDENT));
        wrap(&self.0, room)
            .into_iter()
            .map(|part| Line::new().plain(INDENT).plain(part))
            .collect()
    }
}

/// Items that each carry the same mark: the risks a plan runs, what it
/// leaves out.
pub struct Marked {
    pub mark: Mark,
    pub items: Vec<String>,
}

impl Drawn for Marked {
    fn lines(&self, look: &Look) -> Vec<Line> {
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

/// A file's code, or what of it a reader is shown: where it is, what it
/// is there for, and its lines as written.
pub struct Code {
    /// Where the code is: a file, and what in it.
    pub at: String,
    /// What the code is there for, in words.
    pub what: Option<String>,
    /// The lines shown, as written.
    pub lines: Vec<String>,
    /// How many lines the whole file has, when more than are shown.
    pub whole: usize,
    /// Where the rest is read, when not every line is shown.
    pub rest: Option<String>,
}

impl Code {
    /// `code` whole, from the file at `at`.
    pub fn whole(at: impl Into<String>, what: Option<String>, code: &str) -> Self {
        let lines: Vec<String> = code.trim_end().lines().map(str::to_string).collect();
        Code {
            at: at.into(),
            what,
            whole: lines.len(),
            lines,
            rest: None,
        }
    }

    /// `code` with at most `shown` of its lines, and where the rest is.
    pub fn cut(mut self, shown: usize, rest: impl Into<String>) -> Self {
        if self.lines.len() > shown {
            self.lines.truncate(shown);
            self.rest = Some(rest.into());
        }
        self
    }

    /// The language a fence names for the file the code is in, by its
    /// extension, so a viewer that colors code colors it right.
    pub fn language(&self) -> &str {
        let extension = self
            .at
            .split(|c: char| c.is_whitespace() || c == ':')
            .find_map(|part| Path::new(part).extension().and_then(|ext| ext.to_str()));
        match extension {
            Some("rs") => "rust",
            Some("py") => "python",
            Some("ts") => "typescript",
            Some("js") => "javascript",
            Some("go") => "go",
            Some("yaml" | "yml") => "yaml",
            Some("toml") => "toml",
            Some("sh") => "sh",
            Some("md") => "markdown",
            _ => "",
        }
    }
}

impl Drawn for Code {
    fn lines(&self, look: &Look) -> Vec<Line> {
        let size = match self.whole {
            0 => String::new(),
            lines => yunta_core::text::counted(lines, "line"),
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
                false => crate::markdown::continued(line, code_room),
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
                    yunta_core::text::counted(self.whole - self.lines.len(), "line")
                ),
            ));
        }
        lines
    }
}
