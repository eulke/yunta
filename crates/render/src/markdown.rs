//! Markdown as a terminal shows it, cut to the width it is read at.
//!
//! A terminal draws no emphasis, and re-flows nothing on its own — a
//! diagram is read apart, before what is left reaches here: prose is wrapped here, under the indent of the block it sits
//! in, while code keeps its spacing — re-flowing code changes what it
//! says. A line of code wider than the column breaks after a space and
//! goes on one step under where it starts, the way a formatter continues
//! a line, so what follows reads as the same line and not as one of its
//! own.

use crate::{cell_width, cut, wrap, Glyphs, INDENT};

/// `text` wrapped to `width` under `indent`, with `lead` before its first
/// line and the lines after it starting where the text did.
pub fn hanging(indent: &str, lead: &str, text: &str, width: usize) -> Vec<String> {
    let under = " ".repeat(cell_width(lead));
    let room = width.saturating_sub(cell_width(indent) + cell_width(lead));
    wrap(text, room)
        .into_iter()
        .enumerate()
        .map(|(at, line)| match at {
            0 => format!("{indent}{lead}{line}"),
            _ => format!("{indent}{under}{line}"),
        })
        .collect()
}

/// `text` as a terminal shows it: paragraphs and list items wrapped to
/// `width` under `indent`, and code as written — continued where it is
/// wider, never re-flowed.
pub fn markdown(text: &str, indent: &str, width: usize, glyphs: Glyphs) -> Vec<String> {
    let mut page = Page {
        indent,
        width,
        glyphs,
        lines: Vec::new(),
        paragraph: String::new(),
        fenced: false,
    };
    for line in text.lines() {
        page.read(line);
    }
    page.done()
}

/// The lines drawn so far, and what the next line of Markdown is read
/// as part of.
struct Page<'a> {
    indent: &'a str,
    width: usize,
    glyphs: Glyphs,
    lines: Vec<String>,
    /// The paragraph gathered so far, drawn once it ends.
    paragraph: String,
    /// Whether a line is inside a fenced block of code.
    fenced: bool,
}

impl Page<'_> {
    fn read(&mut self, line: &str) {
        let trimmed = line.trim();
        if self.fenced {
            return self.code(line, trimmed);
        }
        if trimmed.starts_with("```") {
            self.flush();
            self.fenced = true;
            return;
        }
        if trimmed.is_empty() {
            self.flush();
            if self.lines.last().is_some_and(|last| !last.is_empty()) {
                self.lines.push(String::new());
            }
            return;
        }
        match list_item(trimmed) {
            Some((mark, item)) => {
                self.flush();
                self.lines
                    .extend(hanging(self.indent, mark, item, self.width));
            }
            None => {
                self.paragraph.push_str(trimmed);
                self.paragraph.push(' ');
            }
        }
    }

    /// A line inside a fenced block: its closing fence, or a line of
    /// code.
    fn code(&mut self, line: &str, trimmed: &str) {
        if trimmed.starts_with("```") {
            self.fenced = false;
            return;
        }
        let code = format!("{}{INDENT}", self.indent);
        let room = self.width.saturating_sub(cell_width(&code));
        self.lines
            .extend(continued(line, room, self.glyphs).into_iter().map(|piece| {
                if piece.is_empty() {
                    String::new()
                } else {
                    format!("{code}{piece}")
                }
            }));
    }

    /// The paragraph gathered so far, wrapped onto the page.
    fn flush(&mut self) {
        if !self.paragraph.trim().is_empty() {
            self.lines
                .extend(hanging(self.indent, "", self.paragraph.trim(), self.width));
        }
        self.paragraph.clear();
    }

    fn done(mut self) -> Vec<String> {
        self.flush();
        while self.lines.last().is_some_and(String::is_empty) {
            self.lines.pop();
        }
        self.lines
    }
}

/// A list item's marker and its text — or a heading, as a line of its
/// own — for a line that is one.
fn list_item(line: &str) -> Option<(&str, &str)> {
    for mark in ["- ", "* ", "+ "] {
        if let Some(item) = line.strip_prefix(mark) {
            return Some((mark, item));
        }
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && line[digits..].starts_with(". ") {
        return Some(line.split_at(digits + 2));
    }
    if line.starts_with('#') {
        return Some(("", line.trim_start_matches('#').trim_start()));
    }
    None
}

/// `line` of code in pieces that each fit `width`: broken after the last
/// space that fits, and between characters only where no space does —
/// each piece after the first set under the line's own indent and opened
/// with the continuation glyph, so a reader never takes it for a line of
/// the code.
pub fn continued(line: &str, width: usize, glyphs: Glyphs) -> Vec<String> {
    if cell_width(line) <= width {
        return vec![line.to_string()];
    }
    let lead = &line[..line.len() - line.trim_start().len()];
    let under = format!("{lead}  {} ", glyphs.continued());
    let mut pieces = Vec::new();
    let mut rest = line.trim_end();
    let mut prefix = "";
    loop {
        let room = width.saturating_sub(cell_width(prefix)).max(1);
        if cell_width(rest) <= room {
            pieces.push(format!("{prefix}{rest}"));
            return pieces;
        }
        let (piece, after) = split_within(rest, room);
        pieces.push(format!("{prefix}{piece}"));
        rest = after;
        prefix = &under;
    }
}

/// The longest head of `text` that fits `room` and ends before a space,
/// and what follows that space; with no such space, the head cut between
/// characters.
fn split_within(text: &str, room: usize) -> (&str, &str) {
    let body_starts = text.len() - text.trim_start().len();
    let fits = text
        .char_indices()
        .filter(|(at, ch)| *at > body_starts && *ch == ' ')
        .map(|(at, _)| at)
        .take_while(|at| cell_width(text[..*at].trim_end()) <= room)
        .last();
    match fits {
        Some(at) => (text[..at].trim_end(), text[at..].trim_start()),
        None => {
            let head = cut(text, room).into_iter().next().unwrap_or_default();
            let at = head.len().min(text.len());
            (&text[..at], &text[at..])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_of_code_wider_than_the_column_goes_on_under_itself_after_a_space() {
        let drawn = markdown(
            "```rust\n    now.signed_duration_since(created_at) <= chrono::Duration::from_std(window)\n```",
            "",
            48,
            Glyphs::Unicode,
        );
        assert_eq!(
            drawn,
            [
                "      now.signed_duration_since(created_at) <=",
                "        ↪ chrono::Duration::from_std(window)",
            ]
        );
    }

    #[test]
    fn a_token_no_space_lets_fit_is_cut_and_still_hangs_under_its_line() {
        let drawn = markdown(
            "```\nabcdefghijklmnopqrstuvwxyz\n```",
            "",
            12,
            Glyphs::Ascii,
        );
        assert_eq!(
            drawn,
            ["  abcdefghij", "    > klmnop", "    > qrstuv", "    > wxyz"]
        );
        assert!(drawn.iter().all(|line| cell_width(line) <= 12));
    }
}
