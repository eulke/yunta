//! Markdown as a terminal shows it, cut to the width it is read at.
//!
//! A diagram is read apart, before what is left reaches here. Prose is
//! wrapped here, under the indent of the block it sits in, its emphasis
//! drawn strong and its code in the tone a command is drawn in, the
//! markers that only meant something rendered dropped where a tone says
//! it. Code keeps its spacing: re-flowing code changes what it says. A line
//! of code wider than the column breaks after a space and goes on one step
//! under where it starts, the way a formatter continues a line, so what
//! follows reads as the same line and not as one of its own.

use crate::glyphs::Glyphs;
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::paint::Ink;
use crate::width::{cell_width, cut, INDENT};

/// `text` wrapped to `width` under `indent`, with `lead` before its first
/// line and the lines after it starting where the text did.
fn hanging(indent: &str, lead: &str, text: &Line, width: usize) -> Vec<Line> {
    let under = " ".repeat(cell_width(lead));
    let room = width.saturating_sub(cell_width(indent) + cell_width(lead));
    text.wrap(room)
        .into_iter()
        .enumerate()
        .map(|(at, line)| match at {
            0 => line.under(&format!("{indent}{lead}")),
            _ => line.under(&format!("{indent}{under}")),
        })
        .collect()
}

/// Prose as the spans it reads as: `**emphasis**` strong and `` `code` ``
/// in a command's tone. Where a stream paints tones the markers go;
/// where it does not, a code span keeps its backticks, since nothing else
/// would set it apart. A marker with no partner is text.
fn spans(text: &str, paints: bool) -> Line {
    let mut line = Line::new();
    let pieces: Vec<&str> = text.split('`').collect();
    // An odd count of pieces is backticks in pairs: every odd piece is code.
    let paired = pieces.len() % 2 == 1;
    for (at, piece) in pieces.iter().enumerate() {
        let code = at % 2 == 1;
        match (code, paired || at + 1 < pieces.len()) {
            (true, true) if paints => line = line.push(Tone::Command, *piece),
            (true, true) => line = line.push(Tone::Command, format!("`{piece}`")),
            (true, false) => line = line.plain(format!("`{piece}")),
            (false, _) => line = emphasis(line, piece),
        }
    }
    line
}

/// `text` onto `line`, its `**emphasis**` strong.
fn emphasis(mut line: Line, text: &str) -> Line {
    let parts: Vec<&str> = text.split("**").collect();
    let paired = parts.len() % 2 == 1;
    for (at, part) in parts.iter().enumerate() {
        line = match (at % 2 == 1, paired) {
            (true, true) => line.push(Tone::Strong, *part),
            (true, false) => line.plain(format!("**{part}")),
            (false, _) => line.plain(*part),
        };
    }
    line
}

/// `text` as a terminal shows it: paragraphs and list items wrapped to the
/// look's width under `indent`, and code as written — continued where it
/// is wider, never re-flowed.
pub fn markdown(text: &str, indent: &str, look: &Look) -> Vec<Line> {
    let mut page = Page {
        indent,
        width: look.width.cells(),
        glyphs: look.glyphs,
        paints: look.ink != Ink::Plain,
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
    /// Whether the stream paints tones.
    paints: bool,
    lines: Vec<Line>,
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
            if self
                .lines
                .last()
                .is_some_and(|last| !last.text().is_empty())
            {
                self.lines.push(Line::new());
            }
            return;
        }
        match list_item(trimmed) {
            Some(("", heading)) => {
                self.flush();
                let heading = Line::new().push(Tone::Strong, heading);
                self.lines
                    .extend(hanging(self.indent, "", &heading, self.width));
            }
            Some((mark, item)) => {
                self.flush();
                let item = spans(item, self.paints);
                self.lines
                    .extend(hanging(self.indent, mark, &item, self.width));
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
                    Line::new()
                } else {
                    Line::new().plain(format!("{code}{piece}"))
                }
            }));
    }

    /// The paragraph gathered so far, wrapped onto the page.
    fn flush(&mut self) {
        if !self.paragraph.trim().is_empty() {
            let paragraph = spans(self.paragraph.trim(), self.paints);
            self.lines
                .extend(hanging(self.indent, "", &paragraph, self.width));
        }
        self.paragraph.clear();
    }

    fn done(mut self) -> Vec<Line> {
        self.flush();
        while self.lines.last().is_some_and(|line| line.text().is_empty()) {
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
    use crate::look::Width;

    /// `text` drawn `width` wide with `glyphs`, unpainted, as the text of
    /// each line.
    fn drawn(text: &str, width: usize, glyphs: Glyphs) -> Vec<String> {
        let look = Look {
            glyphs,
            ink: Ink::Plain,
            width: Width::of(None, None).within(width),
        };
        markdown(text, "", &look).iter().map(Line::text).collect()
    }

    #[test]
    fn a_line_of_code_wider_than_the_column_goes_on_under_itself_after_a_space() {
        let drawn = drawn(
            "```rust\n    now.signed_duration_since(created_at) <= chrono::Duration::from_std(window)\n```",
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
        let drawn = drawn("```\nabcdefghijklmnopqrstuvwxyz\n```", 12, Glyphs::Ascii);
        assert_eq!(
            drawn,
            [
                "  abcdefghij",
                "    \\ klmnop",
                "    \\ qrstuv",
                "    \\ wxyz"
            ]
        );
        assert!(drawn.iter().all(|line| cell_width(line) <= 12));
    }

    #[test]
    fn emphasis_and_code_are_tones_and_code_keeps_its_ticks_where_nothing_paints() {
        let painted = spans("run **this** with `yunta status`", true);
        assert_eq!(painted.text(), "run this with yunta status");
        assert!(painted
            .spans()
            .iter()
            .any(|span| span.tone == Tone::Strong && span.text == "this"));
        assert!(painted
            .spans()
            .iter()
            .any(|span| span.tone == Tone::Command && span.text == "yunta status"));
        let plain = spans("run **this** with `yunta status`", false);
        assert_eq!(plain.text(), "run this with `yunta status`");
        assert_eq!(spans("a lone ` tick", true).text(), "a lone ` tick");
        assert_eq!(spans("a lone ** star", true).text(), "a lone ** star");
    }
}
