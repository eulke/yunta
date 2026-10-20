//! What a line says, apart from how it is painted.
//!
//! A line is built from spans that each carry a tone — what the text is
//! to a reader — and painted once, by the paint its stream gets
//! ([`crate::paint`]). Color only repeats what the words already say, so
//! every paint writes the same text.

use crate::glyphs::{Glyphs, Mark};
use crate::width::{cell_width, truncate};

/// What a span of text is to a reader, which decides its color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// The body of a line: the terminal's own color.
    Plain,
    /// What a line is about.
    Strong,
    /// A command to copy, drawn as strongly as what a line is about.
    Command,
    /// What a reader skims past: a label, a duration, a path.
    Muted,
    Done,
    Failed,
    Running,
    /// Something waits on a person. The one tone that means a person.
    NeedsYou,
    /// Worth a reader's attention and nothing anybody has to answer.
    Caution,
}

impl Tone {
    /// The tone a mark is painted in.
    pub fn of(mark: Mark) -> Self {
        match mark {
            Mark::Done => Tone::Done,
            Mark::Failed => Tone::Failed,
            Mark::Running => Tone::Running,
            Mark::NeedsYou => Tone::NeedsYou,
            Mark::Caution | Mark::Reroute => Tone::Caution,
            Mark::Skipped | Mark::Pending => Tone::Muted,
        }
    }
}

/// A run of text in one tone, and — for a path a reader opens — what it
/// links to where a terminal opens links.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub tone: Tone,
    pub link: Option<String>,
}

/// One line, as the spans it is built from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Line(Vec<Span>);

impl Line {
    pub fn new() -> Self {
        Line(Vec::new())
    }

    /// The line with `text` appended in `tone`.
    pub fn push(mut self, tone: Tone, text: impl Into<String>) -> Self {
        let text = text.into();
        if !text.is_empty() {
            self.0.push(Span {
                text,
                tone,
                link: None,
            });
        }
        self
    }

    /// The line with `shown` appended as a path, linking to the file at
    /// `target` on a terminal that opens links.
    pub fn path(mut self, shown: impl Into<String>, target: &std::path::Path) -> Self {
        let text = shown.into();
        if !text.is_empty() {
            self.0.push(Span {
                text,
                tone: Tone::Plain,
                link: Some(format!("file://{}", target.display())),
            });
        }
        self
    }

    /// The line with `text` appended in the terminal's own color.
    pub fn plain(self, text: impl Into<String>) -> Self {
        self.push(Tone::Plain, text)
    }

    /// The line moved right by `margin`, its spans and their tones kept.
    pub fn under(mut self, margin: &str) -> Self {
        if !margin.is_empty() {
            self.0.insert(
                0,
                Span {
                    text: margin.to_string(),
                    tone: Tone::Plain,
                    link: None,
                },
            );
        }
        self
    }

    /// The line inside `cells`, cut where it passes them and marked with
    /// the ellipsis `glyphs` draws, each span keeping its tone — so a row
    /// is cut on what a reader sees and painted after.
    pub fn cut(&self, cells: usize, glyphs: Glyphs) -> Line {
        let plain: String = self.0.iter().map(|span| span.text.as_str()).collect();
        if cell_width(&plain) <= cells {
            return self.clone();
        }
        let ellipsis = glyphs.ellipsis();
        let cut = truncate(&plain, cells, glyphs);
        let kept = cut.trim_end().strip_suffix(ellipsis).unwrap_or_default();
        let mut line = Line::new();
        let mut left = kept.len();
        for span in &self.0 {
            // The cut lands between characters of the whole line, so it
            // lands between characters of the span it falls in.
            let taken = span.text.len().min(left);
            left -= taken;
            if taken > 0 {
                line.0.push(Span {
                    text: span.text[..taken].to_string(),
                    ..span.clone()
                });
            }
            if left == 0 {
                return line.push(span.tone, ellipsis.to_string());
            }
        }
        line
    }

    pub fn spans(&self) -> &[Span] {
        &self.0
    }

    /// The line with `other`'s spans after its own.
    pub fn then(mut self, other: Line) -> Self {
        self.0.extend(other.0);
        self
    }

    /// What the line reads as, unpainted.
    pub fn text(&self) -> String {
        self.0.iter().map(|span| span.text.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::Ink;

    #[test]
    fn a_line_cut_to_fit_keeps_the_tone_of_every_span_it_keeps() {
        let line = Line::new()
            .push(Tone::NeedsYou, "needs you")
            .plain(": ")
            .push(Tone::Strong, "yunta resolve-gate T1Y0P5 <option>");
        let cut = line.cut(20, Glyphs::Ascii);
        assert_eq!(Ink::Plain.paint(&cut), "needs you: yunta re~");
        assert_eq!(cut.spans()[0], line.spans()[0]);
        assert_eq!(cut.spans().last().map(|span| span.tone), Some(Tone::Strong));
        assert_eq!(line.cut(80, Glyphs::Ascii), line);
    }

    #[test]
    fn a_line_moved_under_a_margin_keeps_its_tones() {
        let line = Line::new().push(Tone::Failed, "exit 1").plain(" — lint");
        let moved = line.clone().under("  ");
        assert_eq!(Ink::Plain.paint(&moved), "  exit 1 — lint");
        assert_eq!(&moved.spans()[1..], line.spans());
    }
}
