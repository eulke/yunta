//! Markdown as a terminal shows it, cut to the width it is read at.
//!
//! A terminal draws no emphasis and no diagram, and re-flows nothing on
//! its own: prose is wrapped here, under the indent of the block it sits
//! in, while code keeps its spacing and is only cut where it is wider
//! than the column — re-flowing code changes what it says.

use crate::render::{cell_width, cut, wrap, INDENT};

/// `text` wrapped to `width` under `indent`, with `lead` before its first
/// line and the lines after it starting where the text did.
pub(crate) fn hanging(indent: &str, lead: &str, text: &str, width: usize) -> Vec<String> {
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
/// `width` under `indent`, code as written — cut where it is wider, never
/// re-flowed — and each `mermaid` block named rather than drawn.
pub(crate) fn markdown(text: &str, indent: &str, width: usize) -> Vec<String> {
    let mut page = Page {
        indent,
        width,
        lines: Vec::new(),
        paragraph: String::new(),
        fence: None,
    };
    for line in text.lines() {
        page.read(line);
    }
    page.done()
}

/// What a fenced block is, which is what it is shown as.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fence {
    Code,
    Diagram,
}

/// The lines drawn so far, and what the next line of Markdown is read
/// as part of.
struct Page<'a> {
    indent: &'a str,
    width: usize,
    lines: Vec<String>,
    /// The paragraph gathered so far, drawn once it ends.
    paragraph: String,
    /// The fenced block a line is inside, when it is.
    fence: Option<Fence>,
}

impl Page<'_> {
    fn read(&mut self, line: &str) {
        let trimmed = line.trim();
        if let Some(open) = self.fence {
            return self.fenced(open, line, trimmed);
        }
        if trimmed.starts_with("```") {
            return self.open(trimmed);
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

    /// A line inside a fenced block: its closing fence, a line of code,
    /// or a line of a diagram nothing draws.
    fn fenced(&mut self, open: Fence, line: &str, trimmed: &str) {
        if trimmed.starts_with("```") {
            self.fence = None;
            return;
        }
        if open == Fence::Diagram {
            return;
        }
        let code = format!("{}{INDENT}", self.indent);
        let room = self.width.saturating_sub(cell_width(&code));
        self.lines.extend(cut(line, room).into_iter().map(|piece| {
            if piece.is_empty() {
                String::new()
            } else {
                format!("{code}{piece}")
            }
        }));
    }

    /// The fence that opens a block, and what it is named as when it is
    /// a diagram.
    fn open(&mut self, trimmed: &str) {
        self.flush();
        let mermaid = trimmed.trim_start_matches('`').trim() == "mermaid";
        if mermaid {
            self.lines
                .push(format!("{}(diagram: in the whole plan)", self.indent));
        }
        self.fence = Some(if mermaid { Fence::Diagram } else { Fence::Code });
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
        return Some(("", line));
    }
    None
}
