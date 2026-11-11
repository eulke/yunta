//! Markdown as a terminal shows it, cut to the width it is read at.
//!
//! A diagram is read apart, before what is left reaches here. Prose is wrapped
//! here, under the indent of the block it sits in, its emphasis drawn strong
//! without its markers and its code in the tone a command is drawn in, inside
//! its backticks: a stream writes the same words painted or not. Code keeps its
//! spacing: re-flowing code changes what it says. Its fences and its code spans
//! are the ones the lexer reads, so a page and the colors of code agree on
//! where code starts and ends. It stands on the ground every block of code
//! takes, its blank lines included. Its words are colored in the language its
//! fence names. A line of code wider than the column breaks after a space and
//! goes on one step under where it starts, the way a formatter continues a
//! line, so what follows reads as the same line and not as one of its own.
//!
//! How prose reads, its emphasis and its code spans, and which lines are list
//! items and headings, is read in [`prose`].

pub(in crate::surface) mod prose;

use std::ops::Range;

use crate::glyphs::Glyphs;
use crate::line::Line;
use crate::look::Look;
use crate::width::{cell_width, cut, INDENT};
use prose::{heading_spans, list_item, marked_spans, spans};

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

/// `text` as a terminal shows it: paragraphs and list items wrapped to the
/// look's width under `indent`, and code as written, continued where it is
/// wider, never re-flowed.
pub fn markdown(text: &str, indent: &str, look: &Look) -> Vec<Line> {
    let mut page = Page {
        indent,
        width: look.width.cells(),
        glyphs: look.glyphs,
        lines: Vec::new(),
        paragraph: String::new(),
        fences: crate::code::Fences::of(text),
        fence: None,
    };
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        page.read(text, at, line.trim_end_matches(['\n', '\r']));
        at += line.len();
    }
    page.done()
}

/// A fenced block of code being read: the language its fence names, where
/// the marks of the line that closes it stand, and its lines so far, drawn
/// together once the fence closes.
struct Fenced {
    language: Option<crate::code::Language>,
    closing: Option<usize>,
    /// Where what follows it starts.
    next: usize,
    /// How many spaces each of its lines gives up: as many as stand before its marks.
    indent: usize,
    lines: Vec<String>,
}

/// The lines drawn so far, and what the next line of Markdown is read
/// as part of.
struct Page<'a> {
    indent: &'a str,
    width: usize,
    glyphs: Glyphs,
    lines: Vec<Line>,
    /// The paragraph gathered so far, drawn once it ends.
    paragraph: String,
    /// The fences of the text, read once for the whole of it.
    fences: crate::code::Fences,
    /// The fenced block of code being read, if a line is inside one.
    fence: Option<Fenced>,
}

impl Page<'_> {
    /// Reads `line`, which starts at offset `at` of `text`: a line of the
    /// fenced block being read, the opening line of a fence, a blank line, a
    /// heading, a list item, or a line of a paragraph.
    fn read(&mut self, text: &str, at: usize, line: &str) {
        if self.fence.as_ref().is_some_and(|fenced| at >= fenced.next) {
            self.close();
        }
        let trimmed = line.trim();
        if self.fence.is_some() {
            return self.code(at, line);
        }
        let marks = at + line.len() - line.trim_start().len();
        let opened = self.fences.at(marks).map(|fence| Fenced {
            language: crate::code::Language::named(&text[fence.info.clone()]),
            closing: fence.closing.as_ref().map(|closing| closing.start),
            next: fence.next,
            indent: fence.indent,
            lines: Vec::new(),
        });
        if let Some(fenced) = opened {
            self.flush();
            self.fence = Some(fenced);
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
                let words = heading_spans(trimmed, trimmed.len() - heading.len());
                self.lines
                    .extend(hanging(self.indent, "", &words, self.width));
            }
            Some((mark, _)) => {
                self.flush();
                let item = marked_spans(trimmed, mark.len());
                self.lines
                    .extend(hanging(self.indent, mark, &item, self.width));
            }
            None => {
                self.paragraph.push_str(trimmed);
                self.paragraph.push(' ');
            }
        }
    }

    /// A line inside a fenced block, which starts at offset `at`: the line
    /// that holds the fence's closing marks, which draws the block, or a line
    /// of its code, kept until then.
    fn code(&mut self, at: usize, line: &str) {
        let closes = self
            .fence
            .as_ref()
            .and_then(|fenced| fenced.closing)
            .is_some_and(|marks| (at..=at + line.len()).contains(&marks));
        if closes {
            self.close();
            return;
        }
        if let Some(fenced) = self.fence.as_mut() {
            let lead = line.len() - line.trim_start_matches(' ').len();
            fenced
                .lines
                .push(line[lead.min(fenced.indent)..].to_string());
        }
    }

    /// Draws the fenced block being read, if any: each line in the tones of
    /// what its words are in the fence's language, continued where it is
    /// wider than the room, and filled as code, its blank lines included.
    fn close(&mut self) {
        let Some(fence) = self.fence.take() else {
            return;
        };
        let code = format!("{}{INDENT}", self.indent);
        let room = self.width.saturating_sub(cell_width(&code));
        let text = fence.lines.join("\n");
        let colored = crate::code::highlighted(&text, fence.language);
        let mut start = 0;
        for line in &fence.lines {
            let range = start..start + line.len();
            start = range.end + 1;
            let painted = crate::code::painted(&text, range, &colored);
            let pieces = continued(&painted, room, self.glyphs);
            self.lines.extend(
                pieces
                    .into_iter()
                    .map(|piece| match piece.text().is_empty() {
                        true => super::document::as_code(Line::new()),
                        false => super::document::as_code(piece.under(&code)),
                    }),
            );
        }
    }

    /// The paragraph gathered so far, wrapped onto the page.
    fn flush(&mut self) {
        if !self.paragraph.trim().is_empty() {
            let paragraph = spans(self.paragraph.trim());
            self.lines
                .extend(hanging(self.indent, "", &paragraph, self.width));
        }
        self.paragraph.clear();
    }

    fn done(mut self) -> Vec<Line> {
        self.close();
        self.flush();
        while self.lines.last().is_some_and(|line| line.text().is_empty()) {
            self.lines.pop();
        }
        self.lines
    }
}

/// `line` of code in pieces that each fit `width`: broken after the last
/// space that fits, and between characters only where no space does —
/// each piece after the first set under the line's own indent and opened
/// with the continuation glyph, so a reader never takes it for a line of
/// the code. Each piece keeps the tones of what it is cut from, so a word
/// a break cuts in two is painted on both sides of it.
pub fn continued(line: &Line, width: usize, glyphs: Glyphs) -> Vec<Line> {
    let text = line.text();
    let lead = &text[..text.len() - text.trim_start().len()];
    let under = format!("{lead}  {} ", glyphs.continued());
    breaks(&text, width, cell_width(&under))
        .into_iter()
        .enumerate()
        .map(|(at, piece)| match at {
            0 => line.slice(piece),
            _ => line.slice(piece).under(&under),
        })
        .collect()
}

/// Where `text`, a line of code, breaks to fit `width`: the bytes of each
/// piece, the first as wide as the line and each after it as wide as what
/// the `under` cells it hangs from leave.
fn breaks(text: &str, width: usize, under: usize) -> Vec<Range<usize>> {
    let whole = 0..text.len();
    if cell_width(text) <= width {
        return vec![whole];
    }
    let end = text.trim_end().len();
    let mut pieces = Vec::new();
    let mut start = 0;
    let mut prefix = 0;
    loop {
        let room = width.saturating_sub(prefix).max(1);
        let rest = &text[start..end];
        if cell_width(rest) <= room {
            pieces.push(start..end);
            return pieces;
        }
        let (head, next) = split_within(rest, room);
        pieces.push(start..start + head);
        start += next;
        prefix = under;
    }
}

/// How long the longest head of `text` is that fits `room` and ends before
/// a space, and where what follows that space starts; with no such space,
/// the head cut between characters, and where it ends.
fn split_within(text: &str, room: usize) -> (usize, usize) {
    let body_starts = text.len() - text.trim_start().len();
    let fits = text
        .char_indices()
        .filter(|(at, ch)| *at > body_starts && *ch == ' ')
        .map(|(at, _)| at)
        .take_while(|at| cell_width(text[..*at].trim_end()) <= room)
        .last();
    match fits {
        Some(at) => (
            text[..at].trim_end().len(),
            text.len() - text[at..].trim_start().len(),
        ),
        None => {
            let head = cut(text, room).into_iter().next().unwrap_or_default();
            let at = head.len().min(text.len());
            (at, at)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::line::{Fill, Syntax, Tone};
    use crate::look::Width;
    use crate::paint::Paint;

    /// `text` drawn `width` wide with `glyphs`, unpainted, as the text of
    /// each line.
    fn drawn(text: &str, width: usize, glyphs: Glyphs) -> Vec<String> {
        let look = Look {
            glyphs,
            paint: Paint::Plain,
            width: Width::of(None, None).within(width),
            motion: crate::motion::Motion::Moves,
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

    /// Every line between a fence and its close, blank or not, is filled as
    /// code, and the prose before it is not.
    #[test]
    fn a_fenced_block_stands_on_the_code_ground_its_blank_lines_included() {
        let look = Look::plain();
        let lines = markdown(
            "Read this:\n\n```rust\nfn main() {}\n\nlet x = 1;\n```",
            "",
            &look,
        );
        assert_eq!(lines[0].text(), "Read this:");
        assert_eq!(lines[0].fill(), None);
        let block = &lines[2..5];
        assert!(block.iter().all(|line| line.fill() == Some(Fill::Code)));
    }

    /// A fence that names its language colors its code in that language, so
    /// `fn` reads as a keyword and the name after it as a function.
    #[test]
    fn a_fenced_block_is_colored_in_the_language_its_fence_names() {
        let look = Look::plain();
        let lines = markdown("```rust\nfn a() {}\n```", "", &look);
        let words = lines[0].spans();
        assert!(words
            .iter()
            .any(|span| span.text == "fn" && span.tone == Tone::Code(Syntax::Keyword)));
        assert!(words
            .iter()
            .any(|span| span.text == "a" && span.tone == Tone::Code(Syntax::Function)));
    }

    /// A fence that names no language leaves its code plain, so no word in it
    /// is colored as code.
    #[test]
    fn a_fence_that_names_no_language_draws_its_code_plain() {
        let look = Look::plain();
        let lines = markdown("```\nfn a() {}\n```", "", &look);
        assert!(lines[0]
            .spans()
            .iter()
            .all(|span| !matches!(span.tone, Tone::Code(_))));
    }

    /// A fence the text never closes is drawn to the end of the text, and its
    /// last line is filled as code.
    #[test]
    fn a_fence_left_open_is_drawn_where_the_text_ends() {
        let look = Look::plain();
        let lines = markdown("Read:\n\n```rust\nfn a() {}", "", &look);
        let last = lines.last().expect("the open fence draws its line");
        assert_eq!(last.text().trim(), "fn a() {}");
        assert_eq!(last.fill(), Some(Fill::Code));
    }

    /// A fence closes only at a line of its own marks, as many or more, so a
    /// shorter fence inside a longer one is its code, and a tilde fence is a
    /// fence.
    #[test]
    fn a_fence_closes_only_at_a_line_of_as_many_of_its_marks() {
        assert_eq!(
            drawn("````md\n```rust\nfn a() {}\n```\n````", 40, Glyphs::Unicode),
            ["  ```rust", "  fn a() {}", "  ```"]
        );
        assert_eq!(drawn("~~~\nx\n~~~", 40, Glyphs::Unicode), ["  x"]);
    }

    /// A fence is code at any depth of a list: its lines keep their breaks, and give up
    /// the indentation the list gives them.
    #[test]
    fn a_fence_deep_in_a_list_is_code() {
        assert_eq!(
            drawn(
                "- one\n  - two\n\n    ```sh\n    cargo test\n      --all\n    ```\n\nAfter.",
                40,
                Glyphs::Unicode
            ),
            [
                "- one",
                "- two",
                "",
                "  cargo test",
                "    --all",
                "",
                "After."
            ]
        );
    }

    /// A list item that ends ends the fence it holds, closed or not.
    #[test]
    fn a_fence_ends_with_the_list_item_that_holds_it() {
        assert_eq!(
            drawn(
                "- one\n\n  ```sh\n  cargo test\nAfter.",
                40,
                Glyphs::Unicode
            ),
            ["- one", "", "  cargo test", "After."]
        );
    }

    /// A list item and a heading read their marks in their whole line: a run
    /// of backticks or a tilde right after them is text, a code span is code,
    /// and a heading's other words are strong.
    #[test]
    fn a_list_item_and_a_heading_read_their_marks_in_their_line() {
        let lines = markdown(
            "- ```rust\n- ~~~ `a`\n# ```rust\n# `a` **b**",
            "",
            &Look::plain(),
        );
        assert_eq!(
            lines.iter().map(Line::text).collect::<Vec<_>>(),
            ["- ```rust", "- ~~~ `a`", "```rust", "`a` b"]
        );
        let code: Vec<&str> = lines
            .iter()
            .flat_map(|line| line.spans())
            .filter(|span| span.tone == Tone::Command)
            .map(|span| span.text.as_str())
            .collect();
        assert_eq!(code, ["`a`", "`a`"]);
    }

    /// A heading opens with one to six `#` that a space, a tab or the line's
    /// end follows; any other line that starts with `#` is prose.
    #[test]
    fn a_heading_opens_with_one_to_six_marks_and_a_blank() {
        assert_eq!(drawn("## Part", 40, Glyphs::Unicode), ["Part"]);
        assert_eq!(drawn("#\tTab", 40, Glyphs::Unicode), ["Tab"]);
        assert_eq!(drawn("###### six", 40, Glyphs::Unicode), ["six"]);
        assert_eq!(
            drawn("#123 is fixed", 40, Glyphs::Unicode),
            ["#123 is fixed"]
        );
        assert!(drawn("#", 40, Glyphs::Unicode).is_empty());
        assert_eq!(
            drawn("####### seven", 40, Glyphs::Unicode),
            ["####### seven"]
        );
    }
}
